use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crabbridge_core::config::default_config_path;

#[derive(Parser, Debug)]
#[command(
    name = "crabridge-cli",
    about = "Codex config setup and snippet generator for CrabBridge"
)]
pub struct CrabridgeCli {
    /// Path to crabbridge.toml (also `CRABRIDGE_CONFIG`)
    #[arg(
        short = 'c',
        long,
        global = true,
        env = "CRABRIDGE_CONFIG",
        value_name = "FILE",
        default_value_os_t = default_config_path()
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Print a Codex config.toml snippet for the configured upstream via CrabBridge
    PrintCodexConfig(PrintCodexConfigArgs),
    /// Write Codex config, model catalog, and optional bridge TOML config in one step
    Setup(SetupArgs),
}

#[derive(Parser, Debug)]
pub struct PrintCodexConfigArgs {
    #[arg(long, env = "UPSTREAM_API_KEY", default_value = "")]
    pub api_key: String,
    #[arg(
        long,
        env = "UPSTREAM_BASE_URL",
        default_value = "https://api.deepseek.com/v1"
    )]
    pub base_url: String,
    #[arg(long, env = "UPSTREAM_MODEL", default_value = "deepseek-v4-pro")]
    pub model: String,
    #[arg(
        short = 'b',
        long,
        env = "BRIDGE_ADDR",
        default_value = "127.0.0.1:11435"
    )]
    pub bind_addr: SocketAddr,
    #[arg(long, env = "CRABRIDGE_PROVIDER", default_value = "deepseek")]
    pub provider: String,
    #[arg(long, help = "Print Codex snippets for deepseek + kimi")]
    pub all_providers: bool,
    #[arg(
        long,
        value_delimiter = ',',
        conflicts_with = "all_providers",
        help = "Print Codex snippets for specific providers (e.g. kimi,deepseek)"
    )]
    pub providers: Option<Vec<String>>,
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
    #[arg(long)]
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
