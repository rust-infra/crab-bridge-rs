//! Programmatic HTTP server startup for desktop and embedded hosts.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::app::build_router;
use crate::cache::ResponseCache;
use crate::metrics::BridgeMetrics;
use crate::opts::ServeArgs;
use crate::session::{DEFAULT_MAX_SESSION_BYTES, SessionStore};
use crate::state::{AppState, ProviderRuntime};
use crate::upstream_request::UpstreamRequestConfig;
use crabbridge_core::config::{
    admin_enabled, load_config_file, resolve_api_key, resolve_config_path, resolve_serve_providers,
    validate_upstream_url,
};
use crabbridge_core::provider::ProviderKind;

/// Running bridge HTTP server; call [`ServeHandle::shutdown`] to stop gracefully.
pub struct ServeHandle {
    pub bind_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    join: JoinHandle<Result<()>>,
}

impl ServeHandle {
    pub fn admin_url(&self) -> String {
        format!("http://{}/admin", self.bind_addr)
    }

    pub fn health_url(&self) -> String {
        format!("http://{}/health", self.bind_addr)
    }

    pub fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    /// Collect the server task result after an unexpected exit.
    pub async fn take_join_result(mut self) -> Result<()> {
        self.shutdown.take();
        self.join.await.context("bridge server task panicked")?
    }

    /// Stop the server and wait for the background task to finish.
    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.join.await.context("bridge server task panicked")?
    }
}

/// Start the bridge HTTP server without blocking the caller.
///
/// When `init_logging` is true, installs a tracing subscriber (safe for CLI only;
/// desktop hosts should initialize logging once before calling this).
pub async fn start_serve(
    serve: ServeArgs,
    config_path: Option<PathBuf>,
    init_logging: bool,
) -> Result<ServeHandle> {
    let ServeArgs {
        bind_addr,
        max_tokens,
        temperature,
        log_level,
        cache_enabled,
        cache_ttl_secs,
        cache_max_entries,
        rate_limit_rps,
        max_sessions,
        session_ttl_hours,
        session_db,
        session_memory_only,
    } = serve;

    if init_logging {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_new(&log_level).unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .try_init()
            .ok();
    }

    let started_at = Instant::now();
    let metrics = BridgeMetrics::new();
    let resolved_path = resolve_config_path(config_path);
    let cfg = match &resolved_path {
        Some(path) => {
            info!(config = %path.display(), "using bridge config");
            Some(load_config_file(path)?)
        }
        None => None,
    };
    let resolved = resolve_serve_providers(cfg.as_ref())?;
    let admin_enabled = admin_enabled(cfg.as_ref());

    let mut providers = HashMap::new();
    let default_provider_slug = resolved.default_provider.as_str();
    for (slug, entry) in &resolved.providers {
        let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let slug_upper = slug.to_ascii_uppercase();
        let base_url = entry
            .base_url
            .clone()
            .filter(|u| !u.is_empty())
            .or_else(|| {
                std::env::var(format!("CRABRIDGE_{slug_upper}_BASE_URL"))
                    .ok()
                    .filter(|u| !u.is_empty())
            })
            .or_else(|| {
                (slug == default_provider_slug)
                    .then(|| {
                        std::env::var("UPSTREAM_BASE_URL")
                            .ok()
                            .filter(|u| !u.is_empty())
                    })
                    .flatten()
            })
            .unwrap_or_else(|| kind.default_base_url().to_string());
        let upstream = validate_upstream_url(&base_url)?;
        if resolve_api_key(slug, kind, None).is_none() {
            warn!(
                provider = %slug,
                env_key = kind.codex_env_key(),
                "no API key in environment; Codex must pass Authorization: Bearer on each request"
            );
        }
        providers.insert(
            slug.clone(),
            ProviderRuntime {
                upstream,
                default_max_tokens: max_tokens,
                default_temperature: temperature,
                model_map: entry.model_map.clone(),
            },
        );
    }

    if providers.is_empty() {
        bail!("no providers configured");
    }

    let default_provider = if providers.contains_key(&resolved.default_provider) {
        resolved.default_provider
    } else {
        providers
            .keys()
            .min()
            .cloned()
            .context("no default provider available")?
    };

    let upstream_request = Arc::new(UpstreamRequestConfig::default());
    let client = Client::new();
    let session_ttl = Duration::from_secs(session_ttl_hours.saturating_mul(60 * 60));
    let sessions = if session_memory_only {
        info!("session store: in-memory only");
        SessionStore::with_limits_and_ttl(max_sessions, DEFAULT_MAX_SESSION_BYTES, session_ttl)
    } else {
        info!(db = %session_db.display(), "session store: sqlite");
        SessionStore::with_sqlite_limits_and_ttl(
            &session_db,
            max_sessions,
            DEFAULT_MAX_SESSION_BYTES,
            session_ttl,
        )
        .with_context(|| {
            format!(
                "failed to open session database at {}",
                session_db.display()
            )
        })?
    };

    let cache = if cache_enabled {
        Some(Arc::new(ResponseCache::new(
            cache_max_entries,
            cache_ttl_secs,
        )))
    } else {
        None
    };

    let default_provider = Arc::new(default_provider);
    let state = AppState {
        sessions: sessions.clone(),
        client,
        providers: Arc::new(providers),
        default_provider: default_provider.clone(),
        upstream_request,
        cache,
        metrics,
        started_at,
    };

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            sessions.cleanup();
        }
    });

    let mut app = build_router(state, admin_enabled).layer(CorsLayer::permissive());

    if rate_limit_rps > 0 {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_second(rate_limit_rps)
                .burst_size(rate_limit_rps.clamp(1, u32::MAX as u64) as u32)
                .key_extractor(GlobalKeyExtractor)
                .finish()
                .context("failed to build rate limiter config")?,
        );
        app = app.layer(GovernorLayer::new(governor_conf));
        info!(rate_limit_rps, "global rate limiting enabled");
    }

    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind to {bind_addr}"))?;

    let provider_list: Vec<_> = resolved.providers.keys().cloned().collect();
    info!(
        %bind_addr,
        default_provider = %default_provider,
        providers = ?provider_list,
        cache_enabled,
        admin_enabled,
        "CrabBridge listening for Codex Responses API"
    );

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .context("server exited with error")?;
        Ok(())
    });

    Ok(ServeHandle {
        bind_addr,
        shutdown: Some(shutdown_tx),
        join,
    })
}
