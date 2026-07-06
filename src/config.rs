//! CrabBridge TOML configuration (`crabbridge.toml`).

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use reqwest::Url;
use serde::Deserialize;

use crate::provider::ProviderKind;

/// Default config filename in the current working directory.
pub const DEFAULT_CONFIG_NAME: &str = "crabbridge.toml";

/// Default `--config` path for Clap parsers.
pub fn default_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONFIG_NAME)
}

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
    pub admin: Option<AdminSection>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct ProviderSection {
    pub model_map: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct UpstreamSection {
    #[allow(dead_code)]
    pub api_key: Option<String>,
    #[allow(dead_code)]
    pub base_url: Option<String>,
    #[allow(dead_code)]
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

#[derive(Debug, Default, Clone, Deserialize)]
pub struct AdminSection {
    pub enabled: Option<bool>,
}

/// Whether `/admin` and `/metrics` routes are mounted (default: true).
pub fn admin_enabled(cfg: Option<&BridgeConfigFile>) -> bool {
    cfg.and_then(|c| c.admin.as_ref())
        .and_then(|a| a.enabled)
        .unwrap_or(true)
}

/// Resolved provider entry for one route slug.
#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub slug: String,
    pub model_map: Option<String>,
    pub base_url: Option<String>,
}

/// All providers served by one `crabridge serve` process.
#[derive(Debug, Clone)]
pub struct ServeProviders {
    pub default_provider: String,
    pub providers: HashMap<String, ProviderEntry>,
}

fn builtin_provider_entries(global_model_map: Option<String>) -> HashMap<String, ProviderEntry> {
    ProviderKind::builtin_slugs()
        .iter()
        .map(|slug| {
            (
                (*slug).to_string(),
                ProviderEntry {
                    slug: (*slug).to_string(),
                    model_map: global_model_map.clone(),
                    base_url: None,
                },
            )
        })
        .collect()
}

/// Path used when writing a new bridge config (`setup`) without `--config`.
pub fn default_config_write_path(explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_NAME))
}

/// Read `--config` / `-c` from argv before Clap runs.
///
/// Must stay in sync with the global `--config` flag on `Cli` in `opts.rs`.
pub fn explicit_config_from_argv() -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
        if let Some(value) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(value));
        }
        if arg == "-c" {
            return args.next().map(PathBuf::from);
        }
        if arg.starts_with("-c") && arg.len() > 2 {
            return Some(PathBuf::from(&arg[2..]));
        }
    }
    None
}

pub fn explicit_config_from_env() -> Option<PathBuf> {
    env::var("CRABRIDGE_CONFIG")
        .ok()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
}

/// Explicit config path before Clap parsing (`--config` / `-c` / `CRABRIDGE_CONFIG`).
pub fn explicit_config_before_cli() -> Option<PathBuf> {
    explicit_config_from_argv().or_else(explicit_config_from_env)
}

/// Explicit config path after Clap parsing (global `--config` on `Cli`).
pub fn explicit_config_from_cli(cli: Option<PathBuf>) -> Option<PathBuf> {
    cli.filter(|p| !p.as_os_str().is_empty())
}

/// Whether the user set `--config` / `-c` or `CRABRIDGE_CONFIG` (not Clap's default).
pub fn config_explicitly_requested() -> bool {
    explicit_config_from_argv().is_some() || explicit_config_from_env().is_some()
}

/// Resolve config path: `--config` / `CRABRIDGE_CONFIG`, then cwd, then user config dir.
pub fn resolve_config_path(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if config_explicitly_requested() {
        return explicit.or_else(explicit_config_from_env);
    }
    if let Some(path) = &explicit
        && path.as_os_str() != DEFAULT_CONFIG_NAME
    {
        return Some(path.clone());
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
        return PathBuf::from(appdata)
            .join("crabbridge")
            .join("config.toml");
    }
    PathBuf::from("config.toml")
}

/// Load config and apply values into the process environment (only for unset keys).
/// Priority after this: CLI flags > existing env > config file > defaults.
pub fn load_config_into_env(explicit: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let explicit_requested = config_explicitly_requested();
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
    set_if_missing(
        "CRABRIDGE_DEFAULT_PROVIDER",
        cfg.default_provider.as_deref(),
    );
    set_if_missing("CRABRIDGE_PROVIDER", cfg.provider.as_deref());

    if !cfg.providers.is_empty() {
        for (slug, section) in &cfg.providers {
            let prefix = slug.to_ascii_uppercase();
            set_if_missing(
                &format!("CRABRIDGE_{prefix}_MODEL_MAP"),
                section.model_map.as_deref(),
            );
            set_if_missing(
                &format!("CRABRIDGE_{prefix}_BASE_URL"),
                section.base_url.as_deref(),
            );
        }
    }

    if let Some(upstream) = &cfg.upstream {
        set_if_missing("UPSTREAM_BASE_URL", upstream.base_url.as_deref());
        set_if_missing("UPSTREAM_API_KEY", upstream.api_key.as_deref());
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
            set_if_missing(
                "SESSION_MEMORY_ONLY",
                Some(if v { "true" } else { "false" }),
            );
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
pub fn resolve_serve_providers(cfg: Option<&BridgeConfigFile>) -> Result<ServeProviders> {
    let global_model_map = cfg
        .and_then(|c| c.advanced.as_ref())
        .and_then(|a| a.model_map.clone());

    let mut providers = HashMap::new();

    if let Some(cfg) = cfg {
        if !cfg.providers.is_empty() {
            for (slug, section) in &cfg.providers {
                providers.insert(
                    slug.clone(),
                    ProviderEntry {
                        slug: slug.clone(),
                        model_map: section
                            .model_map
                            .clone()
                            .or_else(|| global_model_map.clone()),
                        base_url: section.base_url.clone(),
                    },
                );
            }
        } else if cfg.upstream.is_some() || cfg.provider.is_some() {
            let slug = legacy_provider_slug(cfg);
            providers.insert(
                slug.clone(),
                ProviderEntry {
                    slug,
                    model_map: global_model_map.clone(),
                    base_url: None,
                },
            );
        }
    }

    if providers.is_empty() {
        providers = builtin_provider_entries(global_model_map);
    }

    let default_provider = cfg
        .and_then(|c| c.default_provider.clone())
        .or_else(|| cfg.and_then(legacy_provider_slug_opt))
        .or_else(|| env::var("CRABRIDGE_DEFAULT_PROVIDER").ok())
        .or_else(|| env::var("CRABRIDGE_PROVIDER").ok())
        .filter(|slug| providers.contains_key(slug))
        .or_else(|| providers.keys().min().cloned());

    let default_provider = match default_provider {
        Some(slug) => slug,
        None => bail!("no providers configured"),
    };

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

/// Resolve an upstream API key for setup / CLI tools (Codex passes keys per request).
pub fn resolve_api_key(slug: &str, kind: ProviderKind, explicit: Option<String>) -> Option<String> {
    if let Some(key) = explicit.filter(|k| !k.is_empty()) {
        return Some(key);
    }
    let upper = slug.to_ascii_uppercase();
    if let Ok(key) = env::var(format!("CRABRIDGE_{upper}_API_KEY"))
        && !key.is_empty()
    {
        return Some(key);
    }
    for var in kind.preferred_api_key_vars() {
        if let Ok(key) = env::var(var)
            && !key.is_empty()
        {
            return Some(key);
        }
    }
    None
}

/// Parse and validate an upstream HTTP(S) base URL.
pub fn validate_upstream_url(raw: &str) -> Result<Url> {
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

/// Build provider slugs for multi-provider bridge TOML.
pub fn provider_bridge_slugs(slugs: &[String]) -> Vec<String> {
    slugs.to_vec()
}

/// Write a starter `crabbridge.toml` for `crabridge serve`.
pub fn write_bridge_config(path: &Path, provider: ProviderKind, bind_addr: &str) -> Result<()> {
    write_multi_bridge_config(
        path,
        provider.route_slug(),
        &[provider.route_slug()],
        bind_addr,
    )
}

/// Write a multi-provider `crabbridge.toml`.
pub fn write_multi_bridge_config(
    path: &Path,
    default_provider: &str,
    providers: &[&str],
    bind_addr: &str,
) -> Result<()> {
    let mut body = String::from(
        "# Generated by: crabridge-cli setup\n\
         # API keys and models come from Codex requests (env_key + request body).\n\
         # Upstream URLs are derived from provider route slugs.\n\
         \n",
    );
    body.push_str(&format!("default_provider = \"{default_provider}\"\n\n"));

    for slug in providers {
        body.push_str(&format!("[providers.{slug}]\n\n"));
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

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
fn set_if_missing(key: &str, value: Option<&str>) {
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return;
    };
    if env::var(key).is_ok_and(|v| !v.is_empty()) {
        return;
    }
    // SAFETY: called once at process start before other threads spawn.
    unsafe { env::set_var(key, value) };
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
[providers.kimi]

[server]
bind_addr = "127.0.0.1:11435"
"#;
        let cfg: BridgeConfigFile = toml::from_str(toml).unwrap();
        assert_eq!(cfg.default_provider.as_deref(), Some("deepseek"));
        assert_eq!(cfg.providers.len(), 2);
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
    }

    #[test]
    fn write_and_reload_multi_provider() {
        let dir = temp_dir("multi");
        let path = dir.join("crabbridge.toml");
        write_multi_bridge_config(&path, "deepseek", &["deepseek", "kimi"], "127.0.0.1:11435")
            .unwrap();

        let cfg = load_config_file(&path).unwrap();
        assert_eq!(cfg.default_provider.as_deref(), Some("deepseek"));
        assert_eq!(cfg.providers.len(), 2);
        let body = fs::read_to_string(&path).unwrap();
        assert!(!body.contains("api_key"));
    }

    #[test]
    fn resolve_serve_providers_from_multi_config() {
        let cfg: BridgeConfigFile = toml::from_str(
            r#"
default_provider = "kimi"
[providers.deepseek]
[providers.kimi]
"#,
        )
        .unwrap();
        let resolved = resolve_serve_providers(Some(&cfg)).unwrap();
        assert_eq!(resolved.default_provider, "kimi");
        assert_eq!(resolved.providers.len(), 2);
    }

    #[test]
    fn defaults_to_builtin_providers_without_config() {
        let resolved = resolve_serve_providers(None).unwrap();
        assert_eq!(resolved.providers.len(), 2);
        assert!(resolved.providers.contains_key("deepseek"));
        assert!(resolved.providers.contains_key("kimi"));
    }

    #[test]
    fn explicit_config_from_cli_rejects_empty() {
        assert!(explicit_config_from_cli(None).is_none());
        assert!(explicit_config_from_cli(Some(PathBuf::from(""))).is_none());
        assert_eq!(
            explicit_config_from_cli(Some(PathBuf::from("crabbridge.toml"))),
            Some(PathBuf::from("crabbridge.toml"))
        );
    }

    #[test]
    fn explicit_config_before_cli_prefers_argv_over_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("CRABRIDGE_CONFIG", "/from/env.toml");
        }
        // Cannot override process argv in unit tests; env-only fallback:
        assert_eq!(
            explicit_config_from_env(),
            Some(PathBuf::from("/from/env.toml"))
        );
        unsafe {
            env::remove_var("CRABRIDGE_CONFIG");
        }
    }

    #[test]
    fn admin_enabled_defaults_true() {
        assert!(admin_enabled(None));
        assert!(admin_enabled(Some(&BridgeConfigFile::default())));
        assert!(!admin_enabled(Some(&BridgeConfigFile {
            admin: Some(AdminSection {
                enabled: Some(false),
            }),
            ..Default::default()
        })));
    }

    #[test]
    fn apply_config_does_not_override_existing_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("CRABRIDGE_DEFAULT_PROVIDER", "from-env");
        }

        let cfg = BridgeConfigFile {
            default_provider: Some("kimi".into()),
            ..Default::default()
        };
        apply_config_to_env(&cfg);

        assert_eq!(env::var("CRABRIDGE_DEFAULT_PROVIDER").unwrap(), "from-env");

        unsafe {
            env::remove_var("CRABRIDGE_DEFAULT_PROVIDER");
        }
    }

    #[test]
    fn validate_upstream_url_rejects_bad_scheme() {
        assert!(validate_upstream_url("ftp://example.com/v1").is_err());
        assert!(validate_upstream_url("https://api.deepseek.com/v1").is_ok());
    }

    #[test]
    fn provider_bridge_slugs_respects_requested_list() {
        let slugs = provider_bridge_slugs(&["kimi".into(), "deepseek".into()]);
        assert_eq!(slugs, vec!["kimi", "deepseek"]);
    }

    #[test]
    fn resolve_api_key_prefers_slug_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            env::set_var("CRABRIDGE_KIMI_API_KEY", "slug-key");
            env::remove_var("KIMI_API_KEY");
        }
        assert_eq!(
            resolve_api_key("kimi", ProviderKind::Kimi, None),
            Some("slug-key".into())
        );
        unsafe {
            env::remove_var("CRABRIDGE_KIMI_API_KEY");
        }
    }
}
