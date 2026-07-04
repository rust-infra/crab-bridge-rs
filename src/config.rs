//! CrabBridge TOML configuration (`crabbridge.toml`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Default config filename in the current working directory.
pub const DEFAULT_CONFIG_NAME: &str = "crabbridge.toml";

#[derive(Debug, Default, Clone, Deserialize)]
pub struct BridgeConfigFile {
    pub provider: Option<String>,
    pub upstream: Option<UpstreamSection>,
    pub server: Option<ServerSection>,
    pub session: Option<SessionSection>,
    pub cache: Option<CacheSection>,
    #[serde(default, alias = "rate_limit")]
    pub rate_limit: Option<RateLimitSection>,
    pub advanced: Option<AdvancedSection>,
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
    set_if_missing("CRABRIDGE_PROVIDER", cfg.provider.as_deref());

    if let Some(upstream) = &cfg.upstream {
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

/// Write a starter `crabbridge.toml` for `crabridge serve`.
pub fn write_bridge_config(
    path: &Path,
    provider: crate::provider::ProviderKind,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    bind_addr: &str,
) -> Result<()> {
    use crate::provider::ProviderKind;

    let provider_name = match provider {
        ProviderKind::DeepSeek => "deepseek",
        ProviderKind::Kimi => "kimi",
        ProviderKind::Custom => "custom",
    };

    let api_key_line = match api_key {
        Some(key) => format!("api_key = \"{key}\""),
        None => "# api_key = \"sk-your-key-here\"".to_string(),
    };

    let body = format!(
        "# Generated by: crabridge setup\n\
         # Priority: CLI flags > environment variables > this file > defaults\n\
         \n\
         provider = \"{provider_name}\"\n\
         \n\
         [upstream]\n\
         {api_key_line}\n\
         base_url = \"{base_url}\"\n\
         model = \"{model}\"\n\
         \n\
         [server]\n\
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
    );

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
    use crate::provider::ProviderKind;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_dir(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "crabridge-config-{name}-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
provider = "kimi"

[upstream]
api_key = "sk-test"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
log_level = "debug"
max_tokens = 2048
temperature = 0.2

[session]
db = "data/x.db"
memory_only = true
max_sessions = 10
ttl_hours = 24

[cache]
enabled = true
ttl_secs = 60
max_entries = 50

[rate_limit]
rps = 5

[advanced]
model_map = "gpt-5.4:kimi-for-coding"
tool_denylist = "spawn_agent"
"#;
        let cfg: BridgeConfigFile = toml::from_str(toml).unwrap();
        assert_eq!(cfg.provider.as_deref(), Some("kimi"));
        assert_eq!(cfg.upstream.unwrap().api_key.as_deref(), Some("sk-test"));
        assert_eq!(cfg.server.unwrap().log_level.as_deref(), Some("debug"));
        assert_eq!(cfg.session.unwrap().max_sessions, Some(10));
        assert_eq!(cfg.cache.unwrap().enabled, Some(true));
        assert_eq!(cfg.rate_limit.unwrap().rps, Some(5));
        assert_eq!(
            cfg.advanced.unwrap().model_map.as_deref(),
            Some("gpt-5.4:kimi-for-coding")
        );
    }

    #[test]
    fn write_and_reload_roundtrip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("crabbridge.toml");
        write_bridge_config(
            &path,
            ProviderKind::Kimi,
            "https://api.kimi.com/coding/v1",
            "kimi-for-coding",
            Some("sk-test"),
            "127.0.0.1:11435",
        )
        .unwrap();

        let cfg = load_config_file(&path).unwrap();
        assert_eq!(cfg.provider.as_deref(), Some("kimi"));
        let upstream = cfg.upstream.unwrap();
        assert_eq!(upstream.api_key.as_deref(), Some("sk-test"));
        assert_eq!(upstream.model.as_deref(), Some("kimi-for-coding"));
        assert_eq!(
            cfg.server.unwrap().bind_addr.as_deref(),
            Some("127.0.0.1:11435")
        );
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
