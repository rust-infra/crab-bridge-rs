//! `crabridge` server commands (`serve`, `prompt`).

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use reqwest::Client;

use crate::opts::{BridgeCli, Commands, ServeArgs};
use crate::prompt::ResponsesSseParser;
use crate::serve::start_serve;
use crabbridge_core::config::resolve_api_key;
use crabbridge_core::provider::ProviderKind;

/// Run `crabridge` after Clap parsing.
pub async fn run(cli: BridgeCli, config_path: Option<PathBuf>) -> Result<()> {
    match cli.command {
        Commands::Serve(serve) => run_serve(serve, config_path).await,
        Commands::Prompt {
            message,
            stream,
            bind_addr,
            model,
            provider,
        } => run_prompt(message, stream, bind_addr, model, provider).await,
    }
}

async fn run_serve(serve: ServeArgs, config_path: Option<PathBuf>) -> Result<()> {
    let mut handle = start_serve(serve, config_path, true).await?;

    tokio::select! {
        result = handle.wait() => return result,
        signal = tokio::signal::ctrl_c() => {
            signal.context("failed to listen for shutdown signal")?;
        }
    }

    tracing::info!("shutdown signal received; stopping bridge");
    handle.shutdown().await
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
