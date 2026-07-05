use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::config::DEFAULT_CONFIG_NAME;
use crate::session::{DEFAULT_MAX_SESSIONS, DEFAULT_SESSION_TTL};

fn default_config_file() -> PathBuf {
    PathBuf::from(DEFAULT_CONFIG_NAME)
}

#[derive(Parser, Debug)]
#[command(
    name = "crabridge",
    about = "Bridge Codex CLI (Responses API) to DeepSeek / Kimi Chat Completions"
)]
pub struct Cli {
    /// Path to crabbridge.toml (also `CRABRIDGE_CONFIG`)
    #[arg(
        short = 'c',
        long,
        global = true,
        env = "CRABRIDGE_CONFIG",
        value_name = "FILE",
        default_value_os_t = default_config_file()
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the bridge HTTP server
    Serve(ServeArgs),
    /// Send a test prompt to the local bridge via Responses API
    Prompt {
        #[arg(default_value = "Hello, who are you?")]
        message: String,
        #[arg(short = 's', long)]
        stream: bool,
        #[arg(
            short = 'b',
            long,
            env = "BRIDGE_ADDR",
            default_value = "127.0.0.1:11435"
        )]
        bind_addr: SocketAddr,
        #[arg(long, env = "UPSTREAM_MODEL", default_value = "deepseek-v4-pro")]
        model: String,
        #[arg(long, env = "CRABRIDGE_PROVIDER", default_value = "deepseek")]
        provider: String,
    },
    /// Print a Codex config.toml snippet for the configured upstream via CrabBridge
    PrintCodexConfig {
        #[arg(long, env = "UPSTREAM_API_KEY", default_value = "")]
        api_key: String,
        #[arg(
            long,
            env = "UPSTREAM_BASE_URL",
            default_value = "https://api.deepseek.com/v1"
        )]
        base_url: String,
        #[arg(long, env = "UPSTREAM_MODEL", default_value = "deepseek-v4-pro")]
        model: String,
        #[arg(
            short = 'b',
            long,
            env = "BRIDGE_ADDR",
            default_value = "127.0.0.1:11435"
        )]
        bind_addr: SocketAddr,
        #[arg(long, env = "CRABRIDGE_PROVIDER", default_value = "deepseek")]
        provider: String,
        #[arg(long, help = "Print Codex snippets for deepseek + kimi")]
        all_providers: bool,
        #[arg(
            long,
            value_delimiter = ',',
            conflicts_with = "all_providers",
            help = "Print Codex snippets for specific providers (e.g. kimi,deepseek)"
        )]
        providers: Option<Vec<String>>,
    },
    /// Write Codex config, model catalog, and optional bridge TOML config in one step
    Setup(SetupArgs),
}

#[derive(Parser, Debug)]
pub struct SetupArgs {
    /// Upstream provider preset: deepseek | kimi
    #[arg(long, env = "CRABRIDGE_PROVIDER", default_value = "deepseek")]
    pub provider: String,
    /// Upstream API key (optional; also reads DEEPSEEK_API_KEY / KIMI_API_KEY)
    #[arg(long, env = "UPSTREAM_API_KEY")]
    pub api_key: Option<String>,
    #[arg(long, env = "UPSTREAM_BASE_URL")]
    pub base_url: Option<String>,
    #[arg(long, env = "UPSTREAM_MODEL")]
    pub model: Option<String>,
    #[arg(
        short = 'b',
        long,
        env = "BRIDGE_ADDR",
        default_value = "127.0.0.1:11435"
    )]
    pub bind_addr: SocketAddr,
    /// Skip writing crabbridge.toml for `crabridge serve`
    #[arg(long)]
    pub codex_only: bool,
    /// Overwrite an existing bridge config.toml
    #[arg(long)]
    pub force_config: bool,
    /// Check current Codex + bridge configuration (read-only, no writes)
    #[arg(long)]
    pub docker: bool,
    /// Setup Codex entries for deepseek + kimi in one run
    #[arg(long, conflicts_with = "providers")]
    pub all_providers: bool,
    /// Setup specific providers in one run (e.g. kimi,deepseek)
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "all_providers",
        help = "Comma-separated provider slugs to configure (e.g. kimi,deepseek)"
    )]
    pub providers: Option<Vec<String>>,
}

#[derive(Parser, Debug)]
pub struct ServeArgs {
    #[arg(
        short = 'b',
        long,
        env = "BRIDGE_ADDR",
        default_value = "127.0.0.1:11435"
    )]
    pub bind_addr: SocketAddr,
    #[arg(long, env = "MAX_TOKENS")]
    pub max_tokens: Option<u32>,
    #[arg(long, env = "TEMPERATURE")]
    pub temperature: Option<f32>,
    #[arg(short = 'v', long, env = "LOG_LEVEL", default_value = "info")]
    pub log_level: String,
    #[arg(long, env = "CACHE_ENABLED", default_value_t = false)]
    pub cache_enabled: bool,
    #[arg(long, env = "CACHE_TTL_SECS", default_value_t = 300)]
    pub cache_ttl_secs: u64,
    #[arg(long, env = "CACHE_MAX_ENTRIES", default_value_t = 1000)]
    pub cache_max_entries: u64,
    #[arg(long, env = "RATE_LIMIT_RPS", default_value_t = 0)]
    pub rate_limit_rps: u64,
    #[arg(long, env = "MAX_SESSIONS", default_value_t = DEFAULT_MAX_SESSIONS)]
    pub max_sessions: usize,
    #[arg(
        long,
        env = "SESSION_TTL_HOURS",
        default_value_t = DEFAULT_SESSION_TTL.as_secs() / 60 / 60
    )]
    pub session_ttl_hours: u64,
    #[arg(long, env = "SESSION_DB", default_value = "data/crabbridge.db")]
    pub session_db: PathBuf,
    #[arg(long, env = "SESSION_MEMORY_ONLY", default_value_t = false)]
    pub session_memory_only: bool,
}
