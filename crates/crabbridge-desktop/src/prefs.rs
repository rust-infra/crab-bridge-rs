//! Desktop app preferences persisted beside the bridge config.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopPrefs {
    pub onboarding_complete: bool,
}

fn prefs_path(config_dir: &Path) -> PathBuf {
    config_dir.join("desktop-prefs.json")
}

pub fn load_prefs(config_dir: &Path) -> Result<DesktopPrefs> {
    let path = prefs_path(config_dir);
    if !path.is_file() {
        return Ok(DesktopPrefs::default());
    }
    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))
}

pub fn save_prefs(config_dir: &Path, prefs: &DesktopPrefs) -> Result<()> {
    let path = prefs_path(config_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(prefs).context("failed to serialize desktop prefs")?;
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))
}

pub fn mark_onboarding_complete(config_dir: &Path) -> Result<()> {
    let mut prefs = load_prefs(config_dir)?;
    prefs.onboarding_complete = true;
    save_prefs(config_dir, &prefs)
}

pub fn needs_onboarding(config_dir: &Path) -> Result<bool> {
    Ok(!load_prefs(config_dir)?.onboarding_complete)
}
