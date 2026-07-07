//! First-run Codex + bridge configuration for the desktop app.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Result;
use crabbridge_cli::setup::{SetupOptions, SetupResult, run_setup};
use crabbridge_core::provider::ProviderKind;

/// Configure Codex for all built-in providers and write a starter `crabbridge.toml`.
pub async fn run_desktop_setup(
    bind_addr: SocketAddr,
    bridge_config_path: PathBuf,
    force_config: bool,
) -> Result<Vec<SetupResult>> {
    let slugs: Vec<String> = ProviderKind::builtin_slugs()
        .iter()
        .map(|slug| (*slug).to_string())
        .collect();
    let mut results = Vec::with_capacity(slugs.len());

    for (idx, slug) in slugs.iter().enumerate() {
        let provider = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let result = run_setup(SetupOptions {
            provider,
            provider_slug: slug.clone(),
            api_key: None,
            base_url: None,
            model: None,
            bind_addr,
            write_bridge_config: false,
            write_multi_bridge_config: idx == 0,
            multi_provider_slugs: Some(slugs.clone()),
            bridge_config_path: bridge_config_path.clone(),
            force_bridge_config: force_config,
            set_active_codex_provider: idx + 1 == slugs.len(),
        })
        .await?;
        results.push(result);
    }

    Ok(results)
}
