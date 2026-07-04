use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::session::{DEFAULT_MAX_SESSIONS, DEFAULT_SESSION_TTL};

#[derive(Parser, Debug)]
#[command(
    name = "crabridge",
    about = "Bridge Codex CLI (Responses API) to DeepSeek Chat Completions"
)]
pub struct Cli {
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
        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        model: String,
    },
    /// Print a Codex config.toml snippet for DeepSeek via CrabBridge
    PrintCodexConfig {
        #[arg(long, env = "DEEPSEEK_API_KEY")]
        api_key: String,
        #[arg(
            long,
            env = "DEEPSEEK_BASE_URL",
            default_value = "https://api.deepseek.com/v1"
        )]
        base_url: String,
        #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-chat")]
        model: String,
        #[arg(
            short = 'b',
            long,
            env = "BRIDGE_ADDR",
            default_value = "127.0.0.1:11435"
        )]
        bind_addr: SocketAddr,
    },
}

#[derive(Parser, Debug)]
pub struct ServeArgs {
    #[arg(long, env = "DEEPSEEK_API_KEY")]
    pub api_key: String,
    #[arg(
        long,
        env = "DEEPSEEK_BASE_URL",
        default_value = "https://api.deepseek.com/v1"
    )]
    pub base_url: String,
    #[arg(long, env = "DEEPSEEK_MODEL", default_value = "deepseek-v4-pro")]
    pub model: String,
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
