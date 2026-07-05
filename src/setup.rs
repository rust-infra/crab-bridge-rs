//! One-shot Codex + bridge configuration for CrabBridge.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::{Client, Url};
use serde_json::Value;
use toml_edit::{DocumentMut, Item, Table, value};
use tracing::info;

use crate::codex_config::{
    catalog_path_for_slug, codex_home_dir, prepare_model_catalog, write_model_catalog,
};
use crate::config::{self, BridgeConfigFile};
use crate::provider::ProviderKind;

const LEGACY_PROVIDER_NAME: &str = "crabbridge";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct ConfigCheck {
    pub label: String,
    pub status: CheckStatus,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct SetupCheckReport {
    pub checks: Vec<ConfigCheck>,
    pub in_docker: bool,
}

impl SetupCheckReport {
    pub fn has_failures(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Fail)
    }

    pub fn has_warnings(&self) -> bool {
        self.checks.iter().any(|c| c.status == CheckStatus::Warn)
    }
}

#[derive(Debug, Clone)]
pub struct SetupCheckOptions {
    pub provider_slugs: Vec<String>,
    pub api_key: Option<String>,
    pub bridge_config_path: PathBuf,
    pub bind_addr: SocketAddr,
}

/// Result of `crabridge-cli setup`.
#[derive(Debug, Clone)]
pub struct SetupResult {
    pub provider: ProviderKind,
    pub model: String,
    pub upstream_base_url: String,
    pub bridge_base_url: String,
    pub codex_config_path: PathBuf,
    pub catalog_path: PathBuf,
    pub codex_env_key: String,
    pub bridge_config_path: Option<PathBuf>,
    pub codex_config_created: bool,
    pub bridge_config_created: bool,
}

#[derive(Debug, Clone)]
pub struct SetupOptions {
    pub provider: ProviderKind,
    pub provider_slug: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub bind_addr: SocketAddr,
    pub write_bridge_config: bool,
    pub write_multi_bridge_config: bool,
    /// Slugs to include when writing multi-provider `crabbridge.toml`.
    pub multi_provider_slugs: Option<Vec<String>>,
    pub bridge_config_path: PathBuf,
    pub force_bridge_config: bool,
    pub set_active_codex_provider: bool,
}

pub async fn run_setup(opts: SetupOptions) -> Result<SetupResult> {
    let base_url = opts
        .base_url
        .unwrap_or_else(|| opts.provider.default_base_url().to_string());
    let model = opts
        .model
        .unwrap_or_else(|| opts.provider.default_model().to_string());
    let upstream = config::validate_upstream_url(&base_url).context("invalid upstream base URL")?;

    let client = Client::new();
    let models = prepare_model_catalog(
        &client,
        &upstream,
        opts.api_key.as_deref().unwrap_or(""),
        opts.provider,
        &model,
    )
    .await;

    let catalog_path = catalog_path_for_slug(&opts.provider_slug);
    write_model_catalog(&catalog_path, &models)
        .with_context(|| format!("failed to write {}", catalog_path.display()))?;

    let codex_home = codex_home_dir().context("could not resolve Codex home directory")?;
    fs::create_dir_all(&codex_home)
        .with_context(|| format!("failed to create {}", codex_home.display()))?;
    let codex_config_path = codex_home.join("config.toml");
    let codex_config_created = !codex_config_path.exists();

    let codex_provider_name = ProviderKind::codex_provider_name(&opts.provider_slug);
    let bridge_base_url = format!("http://{}/{}/v1", opts.bind_addr, opts.provider_slug);
    merge_codex_config(
        &codex_config_path,
        &codex_provider_name,
        &catalog_path,
        &bridge_base_url,
        opts.provider.codex_env_key(),
        opts.set_active_codex_provider,
    )?;

    let mut bridge_config_created = false;
    let bridge_config_path = if opts.write_multi_bridge_config {
        let path = opts.bridge_config_path.clone();
        if path.exists() && !opts.force_bridge_config {
            info!(path = %path.display(), "bridge config already exists (unchanged)");
            Some(path)
        } else {
            let slugs = opts
                .multi_provider_slugs
                .as_ref()
                .context("multi_provider_slugs required for multi bridge config")?;
            let slug_refs: Vec<&str> = slugs.iter().map(String::as_str).collect();
            let default_provider = slugs
                .last()
                .map(String::as_str)
                .unwrap_or_else(|| opts.provider_slug.as_str());
            config::write_multi_bridge_config(
                &path,
                default_provider,
                &slug_refs,
                &opts.bind_addr.to_string(),
            )?;
            bridge_config_created = true;
            Some(path)
        }
    } else if opts.write_bridge_config {
        let path = opts.bridge_config_path.clone();
        if path.exists() && !opts.force_bridge_config {
            info!(path = %path.display(), "bridge config already exists (unchanged)");
            Some(path)
        } else {
            config::write_bridge_config(&path, opts.provider, &opts.bind_addr.to_string())?;
            bridge_config_created = true;
            Some(path)
        }
    } else {
        None
    };

    Ok(SetupResult {
        provider: opts.provider,
        model,
        upstream_base_url: base_url,
        bridge_base_url,
        codex_config_path,
        catalog_path,
        codex_env_key: opts.provider.codex_env_key().to_string(),
        bridge_config_path,
        codex_config_created,
        bridge_config_created,
    })
}

/// Merge CrabBridge keys into `~/.codex/config.toml`, preserving other tables.
pub fn merge_codex_config(
    path: &Path,
    provider_name: &str,
    catalog_path: &Path,
    bridge_base_url: &str,
    env_key: &str,
    set_active: bool,
) -> Result<()> {
    let existing = if path.exists() {
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
    } else {
        String::new()
    };

    let mut doc = existing
        .parse::<DocumentMut>()
        .with_context(|| format!("existing {} is not valid TOML", path.display()))?;

    if set_active {
        doc.remove("model");
        doc.insert("model_provider", value(provider_name));
        doc.insert(
            "model_catalog_json",
            value(catalog_path.display().to_string()),
        );
    }

    if let Some(providers) = doc
        .get_mut("model_providers")
        .and_then(|i| i.as_table_mut())
    {
        providers.remove(LEGACY_PROVIDER_NAME);
    }

    let mut provider_table = Table::new();
    provider_table.insert("name", value(provider_name));
    provider_table.insert("base_url", value(bridge_base_url));
    provider_table.insert("wire_api", value("responses"));
    provider_table.insert("env_key", value(env_key));

    let providers = doc
        .entry("model_providers")
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .with_context(|| "model_providers must be a TOML table")?;
    providers.insert(provider_name, Item::Table(provider_table));

    fs::write(path, doc.to_string())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn print_setup_summary(result: &SetupResult) {
    println!();
    println!("CrabBridge setup complete ({})", result.provider.label());
    println!();
    println!("  Codex config:    {}", result.codex_config_path.display());
    println!("  Model catalog:   {}", result.catalog_path.display());
    println!("  Model:           {}", result.model);
    println!("  Bridge URL:      {}", result.bridge_base_url);
    println!("  Upstream URL:    {}", result.upstream_base_url);
    if let Some(path) = &result.bridge_config_path {
        println!("  Bridge config:   {}", path.display());
    }
    println!();
    println!(
        "Codex expects {} in your shell environment.",
        result.codex_env_key
    );
    println!();
    println!("Next steps:");
    println!(
        "  1. export {}=sk-...   # if not already set",
        result.codex_env_key
    );
    if result.bridge_config_created {
        println!("  2. crabridge serve    # uses the generated TOML config");
    } else {
        println!("  2. crabridge serve");
    }
    println!(
        "  3. Restart Codex — it should show model: {}",
        result.model
    );
    println!();
}

pub fn running_in_docker() -> bool {
    if Path::new("/.dockerenv").exists() {
        return true;
    }
    std::fs::read_to_string("/proc/1/cgroup")
        .map(|s| {
            s.contains("docker")
                || s.contains("containerd")
                || s.contains("kubepods")
                || s.contains("podman")
        })
        .unwrap_or(false)
}

pub async fn run_setup_check(opts: SetupCheckOptions) -> Result<SetupCheckReport> {
    let in_docker = running_in_docker();
    let mut checks = Vec::new();

    let codex_home = match codex_home_dir() {
        Some(h) => {
            push_check(
                &mut checks,
                "Codex home",
                CheckStatus::Ok,
                h.display().to_string(),
            );
            h
        }
        None => {
            push_check(
                &mut checks,
                "Codex home",
                CheckStatus::Fail,
                "could not resolve ~/.codex (set CODEX_HOME?)".into(),
            );
            print_setup_check(&SetupCheckReport { checks, in_docker });
            bail!("configuration check failed");
        }
    };

    let config_path = codex_home.join("config.toml");
    let config_body = match fs::read_to_string(&config_path) {
        Ok(body) => {
            push_check(
                &mut checks,
                "Codex config",
                CheckStatus::Ok,
                config_path.display().to_string(),
            );
            body
        }
        Err(e) => {
            push_check(
                &mut checks,
                "Codex config",
                CheckStatus::Fail,
                format!("{} ({e})", config_path.display()),
            );
            print_setup_check(&SetupCheckReport { checks, in_docker });
            bail!("configuration check failed");
        }
    };

    let doc = match config_body.parse::<DocumentMut>() {
        Ok(d) => d,
        Err(e) => {
            push_check(
                &mut checks,
                "Codex config parse",
                CheckStatus::Fail,
                e.to_string(),
            );
            print_setup_check(&SetupCheckReport { checks, in_docker });
            bail!("configuration check failed");
        }
    };

    let model_provider = doc.get("model_provider").and_then(|v| v.as_str());
    match model_provider {
        Some(name) if name.starts_with("crabbridge-") => push_check(
            &mut checks,
            "model_provider",
            CheckStatus::Ok,
            name.to_string(),
        ),
        Some(LEGACY_PROVIDER_NAME) => push_check(
            &mut checks,
            "model_provider",
            CheckStatus::Warn,
            format!("legacy \"{LEGACY_PROVIDER_NAME}\" — run `crabridge-cli setup` to migrate"),
        ),
        Some(other) => push_check(
            &mut checks,
            "model_provider",
            CheckStatus::Warn,
            format!("expected crabbridge-* provider, got \"{other}\""),
        ),
        None => push_check(
            &mut checks,
            "model_provider",
            CheckStatus::Fail,
            "not set — run `crabridge-cli setup`".into(),
        ),
    }

    let active_model = doc
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    match &active_model {
        Some(m) => push_check(&mut checks, "model", CheckStatus::Ok, m.clone()),
        None => push_check(&mut checks, "model", CheckStatus::Fail, "not set".into()),
    }

    check_bridge_config_file(&mut checks, &opts.bridge_config_path);

    if in_docker {
        push_check(
            &mut checks,
            "runtime",
            CheckStatus::Ok,
            "running inside a container".into(),
        );
    }

    for slug in &opts.provider_slugs {
        let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let codex_name = ProviderKind::codex_provider_name(slug);
        let expected_base = format!("http://{}/{slug}/v1", opts.bind_addr);

        let catalog_path = catalog_path_for_slug(slug);
        let catalog_models = match fs::read_to_string(&catalog_path) {
            Ok(body) => match serde_json::from_str::<Value>(&body) {
                Ok(json) => {
                    let count = json
                        .get("models")
                        .and_then(|m| m.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    push_check(
                        &mut checks,
                        &format!("[{slug}] model catalog"),
                        CheckStatus::Ok,
                        format!("{} ({count} models)", catalog_path.display()),
                    );
                    catalog_model_slugs(&json)
                }
                Err(e) => {
                    push_check(
                        &mut checks,
                        &format!("[{slug}] model catalog"),
                        CheckStatus::Fail,
                        format!("{} invalid JSON: {e}", catalog_path.display()),
                    );
                    Vec::new()
                }
            },
            Err(e) => {
                push_check(
                    &mut checks,
                    &format!("[{slug}] model catalog"),
                    CheckStatus::Fail,
                    format!("{} ({e})", catalog_path.display()),
                );
                Vec::new()
            }
        };

        let provider_table = doc
            .get("model_providers")
            .and_then(|v| v.get(&codex_name))
            .and_then(|v| v.as_table())
            .or_else(|| {
                if opts.provider_slugs.len() == 1 {
                    doc.get("model_providers")
                        .and_then(|v| v.get(LEGACY_PROVIDER_NAME))
                        .and_then(|v| v.as_table())
                } else {
                    None
                }
            });

        let mut bridge_base_url = None;
        match provider_table {
            Some(table) => {
                let base_url = table.get("base_url").and_then(|v| v.as_str());
                let wire_api = table.get("wire_api").and_then(|v| v.as_str());
                let env_key = table.get("env_key").and_then(|v| v.as_str());

                match base_url {
                    Some(url) if url == expected_base => push_check(
                        &mut checks,
                        &format!("[{slug}] bridge base_url"),
                        CheckStatus::Ok,
                        url.to_string(),
                    ),
                    Some(url) => push_check(
                        &mut checks,
                        &format!("[{slug}] bridge base_url"),
                        CheckStatus::Warn,
                        format!("expected \"{expected_base}\", got \"{url}\""),
                    ),
                    None => push_check(
                        &mut checks,
                        &format!("[{slug}] bridge base_url"),
                        CheckStatus::Fail,
                        format!("missing in [model_providers.{codex_name}]"),
                    ),
                }

                match wire_api {
                    Some("responses") => push_check(
                        &mut checks,
                        &format!("[{slug}] wire_api"),
                        CheckStatus::Ok,
                        "responses".into(),
                    ),
                    Some(other) => push_check(
                        &mut checks,
                        &format!("[{slug}] wire_api"),
                        CheckStatus::Warn,
                        format!("expected \"responses\", got \"{other}\""),
                    ),
                    None => push_check(
                        &mut checks,
                        &format!("[{slug}] wire_api"),
                        CheckStatus::Fail,
                        "missing".into(),
                    ),
                }

                if let Some(key) = env_key {
                    push_check(
                        &mut checks,
                        &format!("[{slug}] env_key"),
                        CheckStatus::Ok,
                        key.to_string(),
                    );
                } else {
                    push_check(
                        &mut checks,
                        &format!("[{slug}] env_key"),
                        CheckStatus::Fail,
                        "missing".into(),
                    );
                }

                bridge_base_url = base_url.map(str::to_string);
            }
            None => {
                push_check(
                    &mut checks,
                    &format!("[{slug}] model_providers.{codex_name}"),
                    CheckStatus::Fail,
                    format!("section missing — run `crabridge-cli setup --provider {slug}`"),
                );
            }
        }

        let expected_env_key = kind.codex_env_key();
        match config::resolve_api_key(slug, kind, opts.api_key.clone()) {
            Some(_) => push_check(
                &mut checks,
                &format!("[{slug}] API key env"),
                CheckStatus::Ok,
                format!("{expected_env_key} is set"),
            ),
            None => push_check(
                &mut checks,
                &format!("[{slug}] API key env"),
                CheckStatus::Fail,
                format!("{expected_env_key} is not set — export it before running Codex"),
            ),
        }

        if let Some(m) = &active_model
            && model_provider == Some(codex_name.as_str())
        {
            if catalog_models.is_empty() {
                push_check(
                    &mut checks,
                    &format!("[{slug}] model in catalog"),
                    CheckStatus::Warn,
                    format!("cannot verify \"{m}\" — catalog empty or unreadable"),
                );
            } else if catalog_models.iter().any(|s| s == m) {
                push_check(
                    &mut checks,
                    &format!("[{slug}] model in catalog"),
                    CheckStatus::Ok,
                    format!("\"{m}\" found"),
                );
            } else {
                push_check(
                    &mut checks,
                    &format!("[{slug}] model in catalog"),
                    CheckStatus::Fail,
                    format!("\"{m}\" missing from catalog (causes metadata warnings)"),
                );
            }
        }

        let route_url = bridge_base_url.as_deref().unwrap_or(expected_base.as_str());
        check_bridge_route(&mut checks, slug, route_url, in_docker, opts.bind_addr).await;
        check_docker_url_hints(&mut checks, route_url, in_docker, slug);
    }

    let report = SetupCheckReport { checks, in_docker };
    print_setup_check(&report);

    if report.has_failures() {
        bail!("configuration check failed");
    }
    Ok(report)
}

fn push_check(checks: &mut Vec<ConfigCheck>, label: &str, status: CheckStatus, detail: String) {
    checks.push(ConfigCheck {
        label: label.to_string(),
        status,
        detail,
    });
}

fn catalog_model_slugs(catalog: &Value) -> Vec<String> {
    catalog
        .get("models")
        .and_then(|m| m.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|entry| entry.get("slug").and_then(|s| s.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn check_bridge_config_file(checks: &mut Vec<ConfigCheck>, path: &Path) {
    if !path.exists() {
        // Also accept the auto-discovered user config path.
        let discovered = config::resolve_config_path(None);
        if let Some(found) = discovered.filter(|p| p != path && p.is_file()) {
            check_bridge_config_file(checks, &found);
            return;
        }
        push_check(
            checks,
            "bridge config",
            CheckStatus::Warn,
            format!(
                "{} not found (optional — run `crabridge-cli setup` to generate)",
                path.display()
            ),
        );
        return;
    }

    match config::load_config_file(path) {
        Ok(cfg) => {
            let summary = bridge_config_summary(&cfg);
            push_check(
                checks,
                "bridge config",
                CheckStatus::Ok,
                format!("{} ({summary})", path.display()),
            );
        }
        Err(e) => push_check(
            checks,
            "bridge config",
            CheckStatus::Fail,
            format!("{} ({e})", path.display()),
        ),
    }
}

fn bridge_config_summary(cfg: &BridgeConfigFile) -> String {
    let mut parts = Vec::new();
    if let Some(p) = &cfg.default_provider {
        parts.push(format!("default={p}"));
    }
    if !cfg.providers.is_empty() {
        let slugs: Vec<_> = cfg.providers.keys().cloned().collect();
        parts.push(format!("providers={}", slugs.join(",")));
    } else if let Some(p) = &cfg.provider {
        parts.push(format!("provider={p}"));
    }
    if let Some(upstream) = &cfg.upstream {
        if let Some(m) = &upstream.model {
            parts.push(format!("model={m}"));
        }
        if let Some(u) = &upstream.base_url {
            parts.push(format!("upstream={u}"));
        }
    }
    if parts.is_empty() {
        "ok".into()
    } else {
        parts.join(", ")
    }
}

async fn check_bridge_route(
    checks: &mut Vec<ConfigCheck>,
    slug: &str,
    base_url: &str,
    in_docker: bool,
    bind_addr: SocketAddr,
) {
    let trimmed = base_url.trim_end_matches('/');
    let route_probe = trimmed.to_string();
    let health_urls = bridge_health_urls(base_url, in_docker, bind_addr);
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_else(|_| Client::new());

    let mut last_result = None;
    for url in [route_probe].into_iter().chain(health_urls) {
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                push_check(
                    checks,
                    &format!("[{slug}] bridge reachability"),
                    CheckStatus::Ok,
                    format!("GET {url} → {}", resp.status()),
                );
                return;
            }
            Ok(resp) => {
                last_result = Some((
                    CheckStatus::Warn,
                    format!("GET {url} → HTTP {}", resp.status()),
                ));
            }
            Err(e) => {
                last_result = Some((CheckStatus::Fail, format!("GET {url} failed: {e}")));
            }
        }
    }

    if let Some((status, detail)) = last_result {
        push_check(
            checks,
            &format!("[{slug}] bridge reachability"),
            status,
            detail,
        );
    }
}

fn bridge_health_urls(base_url: &str, in_docker: bool, bind_addr: SocketAddr) -> Vec<String> {
    let trimmed = base_url.trim_end_matches('/');
    let mut root = trimmed.strip_suffix("/v1").unwrap_or(trimmed);
    if let Some((without_slug, slug)) = root.rsplit_once('/')
        && ProviderKind::from_route(slug).is_some()
    {
        root = without_slug;
    }
    let mut urls = vec![format!("{root}/health")];

    if in_docker
        && bind_addr.ip().is_unspecified()
        && let Ok(parsed) = Url::parse(trimmed)
        && (parsed.host_str() == Some("127.0.0.1") || parsed.host_str() == Some("localhost"))
    {
        let port = parsed.port().unwrap_or(11435);
        urls.push(format!("http://127.0.0.1:{port}/health"));
    }

    urls
}

fn check_docker_url_hints(
    checks: &mut Vec<ConfigCheck>,
    base_url: &str,
    in_docker: bool,
    slug: &str,
) {
    let Ok(parsed) = Url::parse(base_url) else {
        return;
    };
    let host = parsed.host_str().unwrap_or("");
    let is_loopback = host == "127.0.0.1" || host == "localhost";

    if in_docker && is_loopback {
        push_check(
            checks,
            "docker networking",
            CheckStatus::Warn,
            "Codex on the host cannot reach 127.0.0.1 inside this container — \
             publish port 11435 and point Codex at host.docker.internal:11435 (macOS/Windows) \
             or the host gateway IP (Linux)"
                .into(),
        );
    } else if !in_docker && (host == "host.docker.internal" || host.ends_with(".docker.internal")) {
        push_check(
            checks,
            "docker networking",
            CheckStatus::Warn,
            format!(
                "base_url targets Docker host DNS — use http://127.0.0.1:11435/{slug}/v1 when Codex runs on the same host"
            ),
        );
    } else if !in_docker && is_loopback {
        push_check(
            checks,
            "docker networking",
            CheckStatus::Ok,
            "local loopback — fine when Codex and CrabBridge run on the same host".into(),
        );
    }
}

pub fn print_setup_check(report: &SetupCheckReport) {
    println!();
    println!("CrabBridge configuration check");
    if report.in_docker {
        println!("(running inside Docker — networking hints enabled)");
    }
    println!();

    for check in &report.checks {
        let icon = match check.status {
            CheckStatus::Ok => "✓",
            CheckStatus::Warn => "!",
            CheckStatus::Fail => "✗",
        };
        println!("  {icon} {}: {}", check.label, check.detail);
    }

    println!();
    if report.has_failures() {
        println!("Result: FAILED — fix the items above, then run `crabridge-cli setup` if needed.");
    } else if report.has_warnings() {
        println!("Result: OK with warnings.");
    } else {
        println!("Result: OK — configuration looks good.");
    }
    println!();
    if report.has_failures() {
        println!("Quick fix: crabridge-cli setup --provider deepseek   # or --providers kimi,deepseek");
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_dir(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "crabridge-setup-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn merge_codex_config_preserves_other_tables() {
        let dir = temp_dir("merge");
        let path = dir.join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "openai"
model = "gpt-5.4"

[projects."/tmp/foo"]
trust_level = "trusted"

[hooks.state]
"#,
        )
        .unwrap();

        let catalog = dir.join("catalog.json");
        fs::write(&catalog, "{}").unwrap();

        merge_codex_config(
            &path,
            "crabbridge-kimi",
            &catalog,
            "http://127.0.0.1:11435/kimi/v1",
            "KIMI_API_KEY",
            true,
        )
        .unwrap();

        let merged = fs::read_to_string(&path).unwrap();
        assert!(merged.contains("model_provider = \"crabbridge-kimi\""));
        assert!(!merged.contains("model = "));
        assert!(merged.contains("[model_providers.crabbridge-kimi]"));
        assert!(merged.contains("env_key = \"KIMI_API_KEY\""));
        assert!(merged.contains("[projects.\"/tmp/foo\"]"));
        assert!(!merged.contains("model_provider = \"openai\""));
    }

    #[test]
    fn write_bridge_config_for_kimi() {
        let dir = temp_dir("toml");
        let path = dir.join("crabbridge.toml");
        config::write_bridge_config(&path, ProviderKind::Kimi, "127.0.0.1:11435").unwrap();
        let body = fs::read_to_string(&path).unwrap();
        assert!(body.contains("default_provider = \"kimi\""));
        assert!(body.contains("[providers.kimi]"));
        assert!(!body.contains("api_key"));
    }

    #[test]
    fn bridge_health_url_strips_provider_path() {
        let urls = bridge_health_urls(
            "http://127.0.0.1:11435/kimi/v1",
            false,
            "127.0.0.1:11435".parse().unwrap(),
        );
        assert_eq!(urls[0], "http://127.0.0.1:11435/health");
    }

    #[test]
    fn bridge_health_url_strips_v1_suffix() {
        let urls = bridge_health_urls(
            "http://127.0.0.1:11435/v1",
            false,
            "127.0.0.1:11435".parse().unwrap(),
        );
        assert_eq!(urls[0], "http://127.0.0.1:11435/health");
    }

    #[test]
    fn catalog_model_slugs_reads_slug_field() {
        let json: Value = serde_json::json!({
            "models": [
                { "slug": "deepseek-v4-pro" },
                { "slug": "kimi-for-coding" }
            ]
        });
        let slugs = catalog_model_slugs(&json);
        assert_eq!(slugs, vec!["deepseek-v4-pro", "kimi-for-coding"]);
    }
}
