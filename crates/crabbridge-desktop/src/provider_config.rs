//! Per-provider configuration for the desktop settings UI.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use crabbridge_core::config::{self, BridgeConfigFile};
use crabbridge_core::provider::ProviderKind;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, value};
use tracing::warn;

use crate::codex_config::{catalog_path_for_slug, codex_home_dir};
use crate::secrets::{self, hydrate_api_keys};
use crate::setup::merge_codex_config;

const SETTINGS_FILE: &str = "provider-settings.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct ProviderSettingsFile {
    pub active_provider: Option<String>,
    pub providers: HashMap<String, StoredProviderSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredProviderSettings {
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderListItem {
    pub slug: String,
    pub label: String,
    pub env_key: String,
    pub is_active: bool,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderConfigView {
    pub slug: String,
    pub label: String,
    pub env_key: String,
    pub is_active: bool,
    pub base_url: String,
    pub default_base_url: String,
    pub api_key_source: String,
    pub api_key_masked: Option<String>,
    pub api_key_available: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigSaveRequest {
    pub slug: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub set_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProviderConfigSnapshot {
    pub active_provider: String,
    pub providers: Vec<ProviderListItem>,
    pub selected: ProviderConfigView,
}

fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE)
}

fn load_settings_file(config_dir: &Path) -> Result<ProviderSettingsFile> {
    let path = settings_path(config_dir);
    if !path.is_file() {
        return Ok(ProviderSettingsFile::default());
    }
    let body =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse {}", path.display()))
}

fn save_settings_file(config_dir: &Path, settings: &ProviderSettingsFile) -> Result<()> {
    let path = settings_path(config_dir);
    fs::create_dir_all(config_dir)
        .with_context(|| format!("failed to create {}", config_dir.display()))?;
    let body =
        serde_json::to_string_pretty(settings).context("failed to serialize provider settings")?;
    fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))
}

fn load_bridge_config(path: &Path) -> Result<Option<BridgeConfigFile>> {
    if path.is_file() {
        Ok(Some(config::load_config_file(path)?))
    } else {
        Ok(None)
    }
}

fn active_provider_slug(settings: &ProviderSettingsFile) -> String {
    settings
        .active_provider
        .clone()
        .filter(|slug| ProviderKind::from_route(slug).is_some())
        .unwrap_or_else(|| ProviderKind::DeepSeek.route_slug().to_string())
}

fn stored_base_url(
    slug: &str,
    bridge: Option<&BridgeConfigFile>,
    settings: &ProviderSettingsFile,
) -> Option<String> {
    settings
        .providers
        .get(slug)
        .and_then(|p| p.base_url.clone())
        .or_else(|| {
            bridge.and_then(|c| {
                c.providers
                    .get(slug)
                    .and_then(|section| section.base_url.clone())
            })
        })
}

fn resolve_base_url(
    slug: &str,
    bridge: Option<&BridgeConfigFile>,
    settings: &ProviderSettingsFile,
) -> String {
    stored_base_url(slug, bridge, settings).unwrap_or_else(|| {
        ProviderKind::from_route(slug)
            .unwrap_or(ProviderKind::Custom)
            .default_base_url()
            .to_string()
    })
}

fn provider_kind(slug: &str) -> Result<ProviderKind> {
    ProviderKind::from_route(slug).ok_or_else(|| anyhow::anyhow!("unknown provider slug: {slug}"))
}

fn api_key_view(env_key: &str) -> Result<(String, Option<String>, bool)> {
    let from_env = secrets::get_env_value(env_key);
    if let Some(value) = from_env {
        return Ok(("env".to_string(), Some(secrets::mask_api_key(&value)), true));
    }
    if let Some(value) = secrets::get_secret(env_key)? {
        return Ok((
            "keychain".to_string(),
            Some(secrets::mask_api_key(&value)),
            true,
        ));
    }
    Ok(("none".to_string(), None, false))
}

fn is_configured(
    slug: &str,
    bridge: Option<&BridgeConfigFile>,
    settings: &ProviderSettingsFile,
    env_key: &str,
) -> Result<bool> {
    if ProviderKind::builtin_slugs().contains(&slug) {
        return Ok(true);
    }
    let has_custom_url = stored_base_url(slug, bridge, settings).is_some();
    let in_bridge = bridge
        .map(|c| c.providers.contains_key(slug))
        .unwrap_or(false);
    let (_, _, key_ok) = api_key_view(env_key)?;
    Ok(has_custom_url || in_bridge || key_ok)
}

pub fn snapshot(
    config_dir: &Path,
    bridge_config_path: &Path,
    selected_slug: Option<&str>,
) -> Result<ProviderConfigSnapshot> {
    hydrate_api_keys()?;
    let settings = load_settings_file(config_dir)?;
    let bridge = load_bridge_config(bridge_config_path)?;
    let active = active_provider_slug(&settings);
    let selected_slug = selected_slug
        .map(str::to_string)
        .filter(|slug| ProviderKind::from_route(slug).is_some())
        .unwrap_or_else(|| active.clone());

    let mut providers = Vec::new();
    for slug in ProviderKind::builtin_slugs() {
        let kind = provider_kind(slug)?;
        let env_key = kind.codex_env_key();
        providers.push(ProviderListItem {
            slug: slug.to_string(),
            label: kind.label().to_string(),
            env_key: env_key.to_string(),
            is_active: *slug == active,
            configured: is_configured(slug, bridge.as_ref(), &settings, env_key)?,
        });
    }

    let selected = provider_view(&selected_slug, &active, bridge.as_ref(), &settings)?;

    Ok(ProviderConfigSnapshot {
        active_provider: active,
        providers,
        selected,
    })
}

fn provider_view(
    slug: &str,
    active: &str,
    bridge: Option<&BridgeConfigFile>,
    settings: &ProviderSettingsFile,
) -> Result<ProviderConfigView> {
    let kind = provider_kind(slug)?;
    let env_key = kind.codex_env_key();
    let (api_key_source, api_key_masked, api_key_available) = api_key_view(env_key)?;
    Ok(ProviderConfigView {
        slug: slug.to_string(),
        label: kind.label().to_string(),
        env_key: env_key.to_string(),
        is_active: slug == active,
        default_base_url: kind.default_base_url().to_string(),
        base_url: resolve_base_url(slug, bridge, settings),
        api_key_source,
        api_key_masked,
        api_key_available,
    })
}

pub fn save(
    config_dir: &Path,
    bridge_config_path: &Path,
    bind_addr: &str,
    request: ProviderConfigSaveRequest,
) -> Result<ProviderConfigSnapshot> {
    provider_kind(&request.slug)?;

    let base_url = request.base_url.trim();
    if base_url.is_empty() {
        bail!("base_url must not be empty");
    }
    config::validate_upstream_url(base_url)?;

    if let Some(api_key) = request.api_key.as_deref().filter(|v| !v.trim().is_empty()) {
        let env_key = provider_kind(&request.slug)?.codex_env_key();
        secrets::set_secret(env_key, api_key.trim())?;
    }

    let mut settings = load_settings_file(config_dir)?;
    settings
        .providers
        .entry(request.slug.clone())
        .or_default()
        .base_url = Some(base_url.to_string());
    if request.set_active {
        settings.active_provider = Some(request.slug.clone());
    }
    save_settings_file(config_dir, &settings)?;

    patch_bridge_config(bridge_config_path, bind_addr, &request.slug, base_url)?;

    if request.set_active {
        match codex_home_dir() {
            Some(codex_home) => {
                let codex_config_path = codex_home.join("config.toml");
                let codex_provider_name = ProviderKind::codex_provider_name(&request.slug);
                let catalog_path = catalog_path_for_slug(&request.slug);
                let bridge_base_url = format!("http://{}/{}/v1", bind_addr, request.slug);
                let kind = provider_kind(&request.slug)?;
                merge_codex_config(
                    &codex_config_path,
                    &codex_provider_name,
                    &catalog_path,
                    &bridge_base_url,
                    kind.codex_env_key(),
                    true,
                )?;
            }
            None => {
                warn!(
                    slug = %request.slug,
                    "Cannot update Codex config: home directory not resolved (HOME/CODEX_HOME unset)"
                );
            }
        }
    }

    snapshot(config_dir, bridge_config_path, Some(&request.slug))
}

fn patch_bridge_config(path: &Path, bind_addr: &str, slug: &str, base_url: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut doc = if path.is_file() {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        config::write_multi_bridge_config(path, ProviderKind::builtin_slugs(), bind_addr)?;
        fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", path.display()))?
    };

    if doc.get("providers").is_none() {
        doc["providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if let Some(providers) = doc["providers"].as_table_mut() {
        if providers.get(slug).is_none() {
            providers.insert(slug, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        if let Some(section) = providers.get_mut(slug).and_then(|item| item.as_table_mut()) {
            section.insert("base_url", value(base_url));
        }
    }

    fs::write(path, doc.to_string()).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_base_url_without_files() {
        let settings = ProviderSettingsFile::default();
        let url = resolve_base_url("deepseek", None, &settings);
        assert_eq!(url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn builtin_providers_are_configured_by_default() {
        let settings = ProviderSettingsFile::default();
        for slug in ProviderKind::builtin_slugs() {
            let kind = provider_kind(slug).unwrap();
            assert!(
                is_configured(slug, None, &settings, kind.codex_env_key()).unwrap(),
                "{slug} should be configured by default"
            );
        }
    }
}
