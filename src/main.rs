use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use clap::Parser;
use crab_bridge_rs::session::{DEFAULT_MAX_SESSION_BYTES, SessionStore};
use crab_bridge_rs::upstream_request::UpstreamRequestConfig;
use futures_util::StreamExt;
use reqwest::{Client, Url};
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor,
};
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crab_bridge_rs::cache::ResponseCache;
use crab_bridge_rs::codex_config::print_codex_config;
use crab_bridge_rs::config::{config_path_from_args, load_config_into_env};
use crab_bridge_rs::handlers::{
    api_root, handle_fallback, handle_models, handle_responses, health,
};
use crab_bridge_rs::opts::{Cli, Commands, ServeArgs, SetupArgs};
use crab_bridge_rs::prompt::ResponsesSseParser;
use crab_bridge_rs::provider::{bootstrap_upstream_env, ProviderKind};
use crab_bridge_rs::setup::{self, SetupOptions, print_setup_summary};
use crab_bridge_rs::state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    // Load crabbridge.toml (or --config / CRABRIDGE_CONFIG) into env before Clap.
    // Priority: CLI flags > existing env > TOML config > defaults.
    load_config_into_env(config_path_from_args())?;
    // Map DEEPSEEK_* / MOONSHOT_* / KIMI_* and CRABRIDGE_PROVIDER into UPSTREAM_*.
    bootstrap_upstream_env();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve(serve) => run_serve(serve).await,
        Commands::Prompt {
            message,
            stream,
            bind_addr,
            model,
        } => run_prompt(message, stream, bind_addr, model).await,
        Commands::PrintCodexConfig {
            api_key,
            base_url,
            model,
            bind_addr,
        } => run_print_codex_config(api_key, base_url, model, bind_addr).await,
        Commands::Setup(args) => run_setup(args).await,
    }
}

async fn run_serve(
    ServeArgs {
        api_key,
        base_url,
        model,
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
    }: ServeArgs,
) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&log_level).unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let upstream = validate_upstream(&base_url)?;
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

    let state = AppState {
        sessions: sessions.clone(),
        client,
        upstream: Arc::new(upstream),
        api_key: Arc::new(api_key),
        default_model: Arc::new(model.clone()),
        default_max_tokens: max_tokens,
        default_temperature: temperature,
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

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/v1", get(api_root))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/models", get(handle_models))
        .fallback(handle_fallback)
        .layer(DefaultBodyLimit::disable())
        .layer(CorsLayer::permissive())
        .with_state(state);

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

    let provider = ProviderKind::from_base_url(&base_url);
    info!(
        %bind_addr,
        provider = provider.label(),
        model = %model,
        upstream = %base_url,
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
) -> Result<()> {
    let request = serde_json::json!({
        "model": model,
        "input": message,
        "stream": stream,
    });

    let url = format!("http://{bind_addr}/v1/responses");
    let client = Client::new();

    if stream {
        let response = client
            .post(&url)
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
) -> Result<()> {
    let upstream = validate_upstream(&base_url)?;
    let client = Client::new();
    print_codex_config(&client, &upstream, &api_key, "crabbridge", &model).await;
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
        config,
        docker,
    }: SetupArgs,
) -> Result<()> {
    let provider_kind = ProviderKind::parse(&provider);

    if docker {
        let resolved_key = setup::resolve_api_key(provider_kind, api_key)?;
        setup::run_setup_check(setup::SetupCheckOptions {
            provider: provider_kind,
            api_key: resolved_key,
            bridge_config_path: config,
            bind_addr,
        })
        .await?;
        return Ok(());
    }

    let resolved_key = setup::resolve_api_key(provider_kind, api_key)?;

    let result = setup::run_setup(SetupOptions {
        provider: provider_kind,
        api_key: resolved_key,
        base_url,
        model,
        bind_addr,
        write_bridge_config: !codex_only,
        bridge_config_path: config,
        force_bridge_config: force_config,
    })
    .await?;

    print_setup_summary(&result);
    Ok(())
}

fn validate_upstream(raw: &str) -> Result<Url> {
    let url = Url::parse(raw.trim_end_matches('/'))?;
    match url.scheme() {
        "http" | "https" => {}
        s => bail!("upstream URL scheme must be http or https, got: {s}"),
    }
    if url.host_str().is_none() {
        bail!("upstream URL must have a host");
    }
    Ok(url)
}
