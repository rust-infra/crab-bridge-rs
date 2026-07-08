//! First-run onboarding orchestration.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::bridge::BridgeManager;
use crate::env_export;
use crate::prefs;
use crate::secrets;
use crate::setup;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct OnboardingStatus {
    pub onboarding_complete: bool,
    pub bridge_config_exists: bool,
    pub bridge_running: bool,
    pub bind_addr: String,
    pub admin_url: Option<String>,
    pub secrets: Vec<secrets::SecretStatus>,
    pub env_script_path: String,
}

pub fn status(
    config_dir: &Path,
    config_path: &Path,
    manager: &BridgeManager,
) -> Result<OnboardingStatus> {
    Ok(OnboardingStatus {
        onboarding_complete: !prefs::needs_onboarding(config_dir)?,
        bridge_config_exists: config_path.is_file(),
        bridge_running: manager.status() == crate::bridge::BridgeStatus::Running,
        bind_addr: manager.bind_addr().to_string(),
        admin_url: manager.admin_url(),
        secrets: secrets::list_secret_status()?,
        env_script_path: env_export::env_script_path(config_dir)
            .display()
            .to_string(),
    })
}

pub async fn run_setup_and_export(
    bind_addr: SocketAddr,
    config_dir: &Path,
    config_path: PathBuf,
) -> Result<String> {
    secrets::hydrate_api_keys()?;
    setup::run_desktop_setup(bind_addr, config_path, false).await?;
    let env_path = env_export::write_env_script(config_dir)?;
    Ok(env_path.display().to_string())
}

pub async fn finish_onboarding(
    config_dir: &Path,
    manager: Arc<BridgeManager>,
) -> Result<OnboardingStatus> {
    manager
        .restart()
        .await
        .context("failed to restart bridge with updated configuration")?;
    prefs::mark_onboarding_complete(config_dir)?;
    status(config_dir, &manager.config_path(), manager.as_ref())
}
