use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::Parser;
use crab_bridge_rs::session::{DEFAULT_MAX_SESSION_BYTES, SessionStore};
use crab_bridge_rs::upstream_request::UpstreamRequestConfig;
use futures_util::StreamExt;
use reqwest::Client;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crab_bridge_rs::app::build_router;
use crab_bridge_rs::cache::ResponseCache;
use crab_bridge_rs::codex_config::print_codex_config;
use crab_bridge_rs::config::{
    default_config_write_path, load_config_file, load_config_into_env, resolve_api_key,
    resolve_config_path, resolve_serve_providers, validate_upstream_url,
};
use crab_bridge_rs::opts::{Cli, Commands, ServeArgs, SetupArgs};
use crab_bridge_rs::prompt::ResponsesSseParser;
use crab_bridge_rs::provider::{ProviderKind, bootstrap_upstream_env};
use crab_bridge_rs::setup::{self, SetupOptions, print_setup_summary};
use crab_bridge_rs::state::{AppState, ProviderRuntime};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    load_config_into_env(cli.config.clone())?;
    bootstrap_upstream_env();

    match cli.command {
        Commands::Serve(serve) => run_serve(serve, cli.config.clone()).await,
        Commands::Prompt {
            message,
            stream,
            bind_addr,
            model,
            provider,
        } => run_prompt(message, stream, bind_addr, model, provider).await,
        Commands::PrintCodexConfig {
            api_key,
            base_url,
            model,
            bind_addr,
            provider,
            all_providers,
            providers,
        } => {
            run_print_codex_config(
                api_key,
                base_url,
                model,
                bind_addr,
                provider,
                all_providers,
                providers,
            )
            .await
        }
        Commands::Setup(args) => run_setup(args, cli.config).await,
    }
}

async fn run_serve(serve: ServeArgs, config_path: Option<std::path::PathBuf>) -> Result<()> {
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

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&log_level).unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let resolved_path = resolve_config_path(config_path);
    let cfg = resolved_path
        .as_ref()
        .and_then(|path| load_config_file(path).ok());
    if let Some(path) = &resolved_path {
        info!(config = %path.display(), "using bridge config");
    }
    let resolved = resolve_serve_providers(cfg.as_ref())?;

    let mut providers = HashMap::new();
    for (slug, entry) in &resolved.providers {
        let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let upstream = validate_upstream_url(kind.default_base_url())?;
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
            .next()
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
    };

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            interval.tick().await;
            sessions.cleanup();
        }
    });

    let mut app = build_router(state).layer(CorsLayer::permissive());

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
        "CrabBridge listening for Codex Responses API"
    );

    axum::serve(listener, app)
        .await
        .context("server exited with error")?;

    Ok(())
}

async fn run_prompt(
    message: String,
    stream: bool,
    bind_addr: SocketAddr,
    model: String,
    provider: String,
) -> Result<()> {
    let provider_kind = ProviderKind::parse(&provider);
    let api_key =
        resolve_api_key(provider_kind.route_slug(), provider_kind, None).context(format!(
            "no API key in environment — set {}",
            provider_kind.codex_env_key()
        ))?;

    let request = serde_json::json!({
        "model": model,
        "input": message,
        "stream": stream,
    });

    let url = format!("http://{bind_addr}/{provider}/v1/responses");
    let client = Client::new();

    if stream {
        let response = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("failed to connect to bridge at {bind_addr}"))?
            .error_for_status()
            .context("bridge returned an error status")?;

        let mut stream = response.bytes_stream();
        let mut parser = ResponsesSseParser::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed to read stream chunk")?;
            for content in parser.push_chunk(&chunk) {
                print!("{content}");
            }
        }
        println!();
    } else {
        let response: serde_json::Value = client
            .post(&url)
            .bearer_auth(api_key)
            .json(&request)
            .send()
            .await
            .with_context(|| format!("failed to connect to bridge at {bind_addr}"))?
            .error_for_status()
            .context("bridge returned an error status")?
            .json()
            .await
            .context("failed to decode bridge response")?;

        if let Some(text) = response["output"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["content"].as_array())
            .and_then(|parts| parts.first())
            .and_then(|part| part["text"].as_str())
        {
            println!("{text}");
        } else {
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    }

    Ok(())
}

async fn run_print_codex_config(
    api_key: String,
    base_url: String,
    model: String,
    bind_addr: SocketAddr,
    provider: String,
    all_providers: bool,
    providers: Option<Vec<String>>,
) -> Result<()> {
    let client = Client::new();
    let slugs = ProviderKind::resolve_setup_slugs(all_providers, providers.as_deref(), &provider)
        .map_err(|e| anyhow::anyhow!(e))?;

    if slugs.len() > 1 || all_providers || providers.is_some() {
        for slug in &slugs {
            let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
            let upstream = validate_upstream_url(
                &std::env::var(format!("CRABRIDGE_{}_BASE_URL", slug.to_ascii_uppercase()))
                    .unwrap_or_else(|_| kind.default_base_url().to_string()),
            )?;
            let key = std::env::var(format!("CRABRIDGE_{}_API_KEY", slug.to_ascii_uppercase()))
                .or_else(|_| std::env::var("UPSTREAM_API_KEY"))
                .unwrap_or(api_key.clone());
            let model_name =
                std::env::var(format!("CRABRIDGE_{}_MODEL", slug.to_ascii_uppercase()))
                    .unwrap_or_else(|_| kind.default_model().to_string());
            print_codex_config(
                &client,
                &upstream,
                &key,
                &ProviderKind::codex_provider_name(slug),
                &model_name,
                &bind_addr,
                slug,
            )
            .await;
        }
    } else {
        let kind = ProviderKind::parse(&provider);
        let upstream = validate_upstream_url(&base_url)?;
        print_codex_config(
            &client,
            &upstream,
            &api_key,
            &ProviderKind::codex_provider_name(kind.route_slug()),
            &model,
            &bind_addr,
            kind.route_slug(),
        )
        .await;
    }

    eprintln!("// CrabBridge bind address: http://{bind_addr}");
    Ok(())
}

async fn run_setup(
    SetupArgs {
        provider,
        api_key,
        base_url,
        model,
        bind_addr,
        codex_only,
        force_config,
        docker,
        all_providers,
        providers,
    }: SetupArgs,
    config: Option<std::path::PathBuf>,
) -> Result<()> {
    let config = default_config_write_path(config);
    let slugs = if docker && providers.is_none() && !all_providers && config.is_file() {
        load_config_file(&config)
            .ok()
            .filter(|cfg| !cfg.providers.is_empty())
            .map(|cfg| cfg.providers.keys().cloned().collect())
            .unwrap_or_else(|| vec![ProviderKind::parse(&provider).route_slug().to_string()])
    } else {
        ProviderKind::resolve_setup_slugs(all_providers, providers.as_deref(), &provider)
            .map_err(|e| anyhow::anyhow!(e))?
    };
    let is_multi = slugs.len() > 1;

    if docker {
        setup::run_setup_check(setup::SetupCheckOptions {
            provider_slugs: slugs,
            api_key,
            bridge_config_path: config,
            bind_addr,
        })
        .await?;
        return Ok(());
    }

    for (idx, slug) in slugs.iter().enumerate() {
        let provider_kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let resolved_key = resolve_api_key(slug, provider_kind, api_key.clone());
        let result = setup::run_setup(SetupOptions {
            provider: provider_kind,
            provider_slug: slug.clone(),
            api_key: resolved_key,
            base_url: base_url.clone(),
            model: model.clone(),
            bind_addr,
            write_bridge_config: !codex_only && !is_multi,
            write_multi_bridge_config: !codex_only && is_multi && idx == 0,
            multi_provider_slugs: if is_multi { Some(slugs.clone()) } else { None },
            bridge_config_path: config.clone(),
            force_bridge_config: force_config,
            set_active_codex_provider: !is_multi || idx + 1 == slugs.len(),
        })
        .await?;
        print_setup_summary(&result);
    }

    Ok(())
}
