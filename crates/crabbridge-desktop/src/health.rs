//! Configuration health checks for the desktop app.

use std::net::SocketAddr;
use std::path::PathBuf;

use crabbridge_cli::setup::{SetupCheckOptions, SetupCheckReport, collect_setup_check};
use crabbridge_core::provider::ProviderKind;

use crate::secrets;

pub async fn run_config_check(
    bind_addr: SocketAddr,
    bridge_config_path: PathBuf,
) -> SetupCheckReport {
    let _ = secrets::hydrate_api_keys();
    let slugs = ProviderKind::builtin_slugs()
        .iter()
        .map(|slug| (*slug).to_string())
        .collect();
    collect_setup_check(SetupCheckOptions {
        provider_slugs: slugs,
        api_key: None,
        bridge_config_path,
        bind_addr,
    })
    .await
}
