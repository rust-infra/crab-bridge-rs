//! CrabBridge TOML configuration (`crabbridge.toml`).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::provider::ProviderKind;

/// Default config filename in the current working directory.
pub const DEFAULT_CONFIG_NAME: &str = "crabbridge.toml";

#[derive(Debug, Default, Clone, Deserialize)]
pub struct BridgeConfigFile {
    pub default_provider: Option<String>,
    /// Legacy single-provider preset (`deepseek` | `kimi`).
    pub provider: Option<String>,
    /// Legacy single upstream block.
    pub upstream: Option<UpstreamSection>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderSection>,
    pub server: Option<ServerSection>,
    pub session: Option<SessionSection>,
    pub cache: Option<CacheSection>,
    #[serde(default, alias = "rate_limit")]
    pub rate_limit: Option<RateLimitSection>,
    pub advanced: Option<AdvancedSection>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ProviderSection {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub model_map: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct UpstreamSection {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ServerSection {
    pub bind_addr: Option<String>,
    pub log_level: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct SessionSection {
    pub db: Option<String>,
    pub memory_only: Option<bool>,
    pub max_sessions: Option<usize>,
    pub ttl_hours: Option<u64>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct CacheSection {
    pub enabled: Option<bool>,
    pub ttl_secs: Option<u64>,
    pub max_entries: Option<u64>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct RateLimitSection {
    pub rps: Option<u64>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct AdvancedSection {
    pub model_map: Option<String>,
    pub tool_denylist: Option<String>,
}

/// Resolved upstream entry for one route slug.
#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub slug: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub model_map: Option<String>,
}

/// All providers served by one `crabridge serve` process.
#[derive(Debug, Clone)]
pub struct ServeProviders {
    pub default_provider: String,
    pub providers: HashMap<String, ProviderEntry>,
}

/// CLI/env overrides applied to the default provider when resolving `serve`.
#[derive(Debug, Default, Clone)]
pub struct ServeOverrides {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

fn discover_providers_from_env(model_map: Option<String>) -> HashMap<String, ProviderEntry> {
    let mut providers = HashMap::new();
    for slug in ProviderKind::builtin_slugs() {
        let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
        let api_key = resolve_provider_api_key(slug, kind, None);
        if api_key.is_empty() {
            continue;
        }
        providers.insert(
            slug.to_string(),
            ProviderEntry {
                slug: slug.to_string(),
                api_key,
                base_url: env::var(format!("CRABRIDGE_{}_BASE_URL", slug.to_ascii_uppercase()))
                    .or_else(|_| env::var(format!("{}_BASE_URL", kind_label_env(slug))))
                    .unwrap_or_else(|_| kind.default_base_url().to_string()),
                model: env::var(format!("CRABRIDGE_{}_MODEL", slug.to_ascii_uppercase()))
                    .or_else(|_| env::var(format!("{}_MODEL", kind_label_env(slug))))
                    .unwrap_or_else(|_| kind.default_model().to_string()),
                model_map: model_map.clone(),
            },
        );
    }
    providers
}

fn kind_label_env(slug: &str) -> String {
    match slug {
        "deepseek" => "DEEPSEEK".to_string(),
        "kimi" => "KIMI".to_string(),
        _ => slug.to_ascii_uppercase(),
    }
}

/// Resolve config path: `--config` / `CRABRIDGE_CONFIG`, then cwd, then user config dir.
pub fn resolve_config_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path);
    }
    if let Ok(path) = env::var("CRABRIDGE_CONFIG") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    let cwd = PathBuf::from(DEFAULT_CONFIG_NAME);
    if cwd.is_file() {
        return Some(cwd);
    }
    let user = user_config_path();
    if user.is_file() {
        return Some(user);
    }
    None
}

pub fn user_config_path() -> PathBuf {
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("crabbridge").join("config.toml");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("crabbridge")
            .join("config.toml");
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("crabbridge").join("config.toml");
    }
    PathBuf::from("config.toml")
}

/// Load config and apply values into the process environment (only for unset keys).
/// Priority after this: CLI flags > existing env > config file > defaults.
pub fn load_config_into_env(explicit: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let explicit_requested = explicit.is_some() || env::var_os("CRABRIDGE_CONFIG").is_some();
    let Some(path) = resolve_config_path(explicit) else {
        return Ok(None);
    };

    if !path.is_file() {
        // `setup --config PATH` may target a file that does not exist yet.
        let is_setup = env::args().any(|a| a == "setup");
        if explicit_requested && !is_setup {
            bail!("config file not found: {}", path.display());
        }
        return Ok(None);
    }

    let cfg = load_config_file(&path)?;
    apply_config_to_env(&cfg);
    Ok(Some(path))
}

pub fn load_config_file(path: &Path) -> Result<BridgeConfigFile> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    toml::from_str(&body).with_context(|| format!("failed to parse config {}", path.display()))
}

pub fn apply_config_to_env(cfg: &BridgeConfigFile) {
    set_if_missing("CRABRIDGE_DEFAULT_PROVIDER", cfg.default_provider.as_deref());
    set_if_missing("CRABRIDGE_PROVIDER", cfg.provider.as_deref());

    if !cfg.providers.is_empty() {
        for (slug, section) in &cfg.providers {
            let prefix = slug.to_ascii_uppercase();
            set_if_missing(
                &format!("CRABRIDGE_{prefix}_API_KEY"),
                section.api_key.as_deref(),
            );
            set_if_missing(
                &format!("CRABRIDGE_{prefix}_BASE_URL"),
                section.base_url.as_deref(),
            );
            set_if_missing(
                &format!("CRABRIDGE_{prefix}_MODEL"),
                section.model.as_deref(),
            );
        }
    } else if let Some(upstream) = &cfg.upstream {
        set_if_missing("UPSTREAM_API_KEY", upstream.api_key.as_deref());
        set_if_missing("UPSTREAM_BASE_URL", upstream.base_url.as_deref());
        set_if_missing("UPSTREAM_MODEL", upstream.model.as_deref());
    }

    if let Some(server) = &cfg.server {
        set_if_missing("BRIDGE_ADDR", server.bind_addr.as_deref());
        set_if_missing("LOG_LEVEL", server.log_level.as_deref());
        if let Some(v) = server.max_tokens {
            set_if_missing("MAX_TOKENS", Some(&v.to_string()));
        }
        if let Some(v) = server.temperature {
            set_if_missing("TEMPERATURE", Some(&v.to_string()));
        }
    }

    if let Some(session) = &cfg.session {
        set_if_missing("SESSION_DB", session.db.as_deref());
        if let Some(v) = session.memory_only {
            set_if_missing("SESSION_MEMORY_ONLY", Some(if v { "true" } else { "false" }));
        }
        if let Some(v) = session.max_sessions {
            set_if_missing("MAX_SESSIONS", Some(&v.to_string()));
        }
        if let Some(v) = session.ttl_hours {
            set_if_missing("SESSION_TTL_HOURS", Some(&v.to_string()));
        }
    }

    if let Some(cache) = &cfg.cache {
        if let Some(v) = cache.enabled {
            set_if_missing("CACHE_ENABLED", Some(if v { "true" } else { "false" }));
        }
        if let Some(v) = cache.ttl_secs {
            set_if_missing("CACHE_TTL_SECS", Some(&v.to_string()));
        }
        if let Some(v) = cache.max_entries {
            set_if_missing("CACHE_MAX_ENTRIES", Some(&v.to_string()));
        }
    }

    if let Some(rate) = &cfg.rate_limit
        && let Some(v) = rate.rps
    {
        set_if_missing("RATE_LIMIT_RPS", Some(&v.to_string()));
    }

    if let Some(advanced) = &cfg.advanced {
        set_if_missing("CRABRIDGE_MODEL_MAP", advanced.model_map.as_deref());
        set_if_missing("CRABRIDGE_TOOL_DENYLIST", advanced.tool_denylist.as_deref());
    }
}

/// Build the provider map used by `crabridge serve`.
pub fn resolve_serve_providers(
    cfg: Option<&BridgeConfigFile>,
    overrides: &ServeOverrides,
) -> Result<ServeProviders> {
    let global_model_map = cfg
        .and_then(|c| c.advanced.as_ref())
        .and_then(|a| a.model_map.clone());

    let mut providers = HashMap::new();

    if let Some(cfg) = cfg {
        if !cfg.providers.is_empty() {
            for (slug, section) in &cfg.providers {
                let kind = ProviderKind::from_route(slug).unwrap_or(ProviderKind::Custom);
                let api_key = resolve_provider_api_key(slug, kind, section.api_key.clone());
                if api_key.is_empty() {
                    continue;
                }
                let entry = ProviderEntry {
                    slug: slug.clone(),
                    api_key,
                    base_url: section
                        .base_url
                        .clone()
                        .unwrap_or_else(|| kind.default_base_url().to_string()),
                    model: section
                        .model
                        .clone()
                        .unwrap_or_else(|| kind.default_model().to_string()),
                    model_map: section
                        .model_map
                        .clone()
                        .or_else(|| global_model_map.clone()),
                };
                providers.insert(slug.clone(), entry);
            }
        } else if cfg.upstream.is_some() || cfg.provider.is_some() {
            let slug = legacy_provider_slug(cfg);
            let kind = ProviderKind::parse(&slug);
            let upstream = cfg.upstream.as_ref();
            let entry = ProviderEntry {
                slug: slug.clone(),
                api_key: resolve_provider_api_key(
                    &slug,
                    kind,
                    upstream.and_then(|u| u.api_key.clone()),
                ),
                base_url: upstream
                    .and_then(|u| u.base_url.clone())
                    .unwrap_or_else(|| kind.default_base_url().to_string()),
                model: upstream
                    .and_then(|u| u.model.clone())
                    .unwrap_or_else(|| kind.default_model().to_string()),
                model_map: global_model_map.clone(),
            };
            providers.insert(slug, entry);
        }
    }

    if providers.is_empty() {
        providers = discover_providers_from_env(global_model_map.clone());
    }

    if providers.is_empty() {
        let slug = env::var("CRABRIDGE_PROVIDER")
            .or_else(|_| env::var("PROVIDER"))
            .unwrap_or_else(|_| "deepseek".to_string());
        let kind = ProviderKind::parse(&slug);
        let api_key = resolve_provider_api_key(&slug, kind, env::var("UPSTREAM_API_KEY").ok());
        if api_key.is_empty() {
            bail!(
                "no upstream API key configured — set DEEPSEEK_API_KEY / KIMI_API_KEY \
                 or [providers.*] in crabbridge.toml"
            );
        }
        let entry = ProviderEntry {
            slug: slug.clone(),
            api_key,
            base_url: env::var("UPSTREAM_BASE_URL")
                .unwrap_or_else(|_| kind.default_base_url().to_string()),
            model: env::var("UPSTREAM_MODEL").unwrap_or_else(|_| kind.default_model().to_string()),
            model_map: global_model_map,
        };
        providers.insert(slug, entry);
    }

    let default_provider = cfg
        .and_then(|c| c.default_provider.clone())
        .or_else(|| cfg.and_then(legacy_provider_slug_opt))
        .or_else(|| env::var("CRABRIDGE_DEFAULT_PROVIDER").ok())
        .or_else(|| env::var("CRABRIDGE_PROVIDER").ok())
        .filter(|slug| providers.contains_key(slug))
        .or_else(|| providers.keys().next().cloned());

    let default_provider = match default_provider {
        Some(slug) => slug,
        None => bail!("no providers configured"),
    };

    if let Some(entry) = providers.get_mut(&default_provider) {
        if let Some(key) = overrides.api_key.as_ref().filter(|k| !k.is_empty()) {
            entry.api_key = key.clone();
        }
        if let Some(url) = overrides.base_url.as_ref().filter(|u| !u.is_empty()) {
            entry.base_url = url.clone();
        }
        if let Some(model) = overrides.model.as_ref().filter(|m| !m.is_empty()) {
            entry.model = model.clone();
        }
    }

    Ok(ServeProviders {
        default_provider,
        providers,
    })
}

fn legacy_provider_slug(cfg: &BridgeConfigFile) -> String {
    legacy_provider_slug_opt(cfg).unwrap_or_else(|| "deepseek".to_string())
}

fn legacy_provider_slug_opt(cfg: &BridgeConfigFile) -> Option<String> {
    cfg.provider
        .as_ref()
        .map(|p| ProviderKind::parse(p).route_slug().to_string())
}

fn resolve_provider_api_key(
    slug: &str,
    kind: ProviderKind,
    explicit: Option<String>,
) -> String {
    if let Some(key) = explicit.filter(|k| !k.is_empty()) {
        return key;
    }
    let upper = slug.to_ascii_uppercase();
    if let Ok(key) = env::var(format!("CRABRIDGE_{upper}_API_KEY"))
        && !key.is_empty()
    {
        return key;
    }
    for var in kind.preferred_api_key_vars() {
        if let Ok(key) = env::var(var)
            && !key.is_empty()
        {
            return key;
        }
    }
    String::new()
}

/// Write a starter `crabbridge.toml` for `crabridge serve`.
pub fn write_bridge_config(
    path: &Path,
    provider: ProviderKind,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    bind_addr: &str,
) -> Result<()> {
    write_multi_bridge_config(
        path,
        provider.route_slug(),
        &[(provider.route_slug(), base_url, model, api_key)],
        bind_addr,
    )
}

/// Write a multi-provider `crabbridge.toml`.
pub fn write_multi_bridge_config(
    path: &Path,
    default_provider: &str,
    providers: &[(&str, &str, &str, Option<&str>)],
    bind_addr: &str,
) -> Result<()> {
    let mut body = String::from(
        "# Generated by: crabridge setup\n\
         # Priority: CLI flags > environment variables > this file > defaults\n\
         \n",
    );
    body.push_str(&format!("default_provider = \"{default_provider}\"\n\n"));

    for (slug, base_url, model, api_key) in providers {
        body.push_str(&format!("[providers.{slug}]\n"));
        match api_key {
            Some(key) => body.push_str(&format!("api_key = \"{key}\"\n")),
            None => body.push_str("# api_key = \"sk-your-key-here\"\n"),
        }
        body.push_str(&format!("base_url = \"{base_url}\"\n"));
        body.push_str(&format!("model = \"{model}\"\n\n"));
    }

    body.push_str(&format!(
        "[server]\n\
         bind_addr = \"{bind_addr}\"\n\
         log_level = \"info\"\n\
         \n\
         [session]\n\
         db = \"data/crabbridge.db\"\n\
         memory_only = false\n\
         \n\
         [cache]\n\
         enabled = false\n\
         ttl_secs = 300\n\
         max_entries = 1000\n\
         \n\
         [rate_limit]\n\
         rps = 0\n\
         \n\
         # [advanced]\n\
         # model_map = \"gpt-5.4:deepseek-v4-pro\"\n\
         # tool_denylist = \"spawn_agent,wait_agent\"\n"
    ));

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
    }
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Scan argv for `--config` / `-c` before full Clap parsing.
pub fn config_path_from_args() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" || arg == "-c" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn set_if_missing(key: &str, value: Option<&str>) {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return;
    };
    if env::var_os(key).is_none() {
        // SAFETY: called once at process start before other threads spawn.
        unsafe { env::set_var(key, value) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "crabbridge-config-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn parses_multi_provider_config() {
        let toml = r#"
default_provider = "deepseek"

[providers.deepseek]
api_key = "sk-ds"
base_url = "https://api.deepseek.com/v1"
model = "deepseek-v4-pro"

[providers.kimi]
api_key = "sk-kimi"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
"#;
        let cfg: BridgeConfigFile = toml::from_str(toml).unwrap();
        assert_eq!(cfg.default_provider.as_deref(), Some("deepseek"));
        assert_eq!(cfg.providers.len(), 2);
        assert_eq!(
            cfg.providers["kimi"].model.as_deref(),
            Some("kimi-for-coding")
        );
    }

    #[test]
    fn parses_legacy_config() {
        let toml = r#"
provider = "kimi"

[upstream]
api_key = "sk-test"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"
"#;
        let cfg: BridgeConfigFile = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider.as_deref(), Some("kimi"));
        assert_eq!(cfg.upstream.unwrap().api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn write_and_reload_multi_provider() {
        let dir = temp_dir("multi");
        let path = dir.join("crabbridge.toml");
        write_multi_bridge_config(
            &path,
            "deepseek",
            &[
                (
                    "deepseek",
                    "https://api.deepseek.com/v1",
                    "deepseek-v4-pro",
                    Some("sk-ds"),
                ),
                (
                    "kimi",
                    "https://api.kimi.com/coding/v1",
                    "kimi-for-coding",
                    Some("sk-kimi"),
                ),
            ],
            "127.0.0.1:11435",
        )
        .unwrap();

        let cfg = load_config_file(&path).unwrap();
        assert_eq!(cfg.default_provider.as_deref(), Some("deepseek"));
        assert_eq!(cfg.providers.len(), 2);
    }

    #[test]
    fn resolve_serve_providers_from_multi_config() {
        let cfg: BridgeConfigFile = toml::from_str(
            r#"
default_provider = "kimi"
[providers.deepseek]
api_key = "a"
base_url = "https://api.deepseek.com/v1"
model = "deepseek-v4-pro"
[providers.kimi]
api_key = "b"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"
"#,
        )
        .unwrap();
        let resolved = resolve_serve_providers(Some(&cfg), &ServeOverrides::default()).unwrap();
        assert_eq!(resolved.default_provider, "kimi");
        assert_eq!(resolved.providers.len(), 2);
        assert_eq!(resolved.providers["kimi"].api_key, "b");
    }

    #[test]
    fn discovers_multiple_providers_from_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::remove_var("DEEPSEEK_API_KEY");
            env::remove_var("KIMI_API_KEY");
            env::remove_var("UPSTREAM_API_KEY");
            env::set_var("DEEPSEEK_API_KEY", "ds-key");
            env::set_var("KIMI_API_KEY", "kimi-key");
        }

        let providers = discover_providers_from_env(None);
        assert_eq!(providers.len(), 2);
        assert_eq!(providers["deepseek"].api_key, "ds-key");
        assert_eq!(providers["kimi"].api_key, "kimi-key");

        unsafe {
            env::remove_var("DEEPSEEK_API_KEY");
            env::remove_var("KIMI_API_KEY");
        }
    }

    #[test]
    fn apply_config_does_not_override_existing_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("UPSTREAM_API_KEY", "from-env");
            env::remove_var("UPSTREAM_MODEL");
        }

        let cfg = BridgeConfigFile {
            provider: Some("deepseek".into()),
            upstream: Some(UpstreamSection {
                api_key: Some("from-toml".into()),
                base_url: None,
                model: Some("deepseek-v4-pro".into()),
            }),
            ..Default::default()
        };
        apply_config_to_env(&cfg);

        assert_eq!(env::var("UPSTREAM_API_KEY").unwrap(), "from-env");
        assert_eq!(env::var("UPSTREAM_MODEL").unwrap(), "deepseek-v4-pro");

        unsafe {
            env::remove_var("UPSTREAM_API_KEY");
            env::remove_var("UPSTREAM_MODEL");
            env::remove_var("CRABRIDGE_PROVIDER");
        }
    }
}
