use anyhow::Result;
use clap::Parser;
use crab_bridge_rs::config::explicit_config_from_cli;
use crab_bridge_rs::opts::BridgeCli;
use crab_bridge_rs::runtime;
use crab_bridge_rs::server;

fn main() -> Result<()> {
    runtime::init()?;
    let cli = BridgeCli::parse();
    let config_path = explicit_config_from_cli(Some(cli.config.clone()));
    runtime::block_on(server::run(cli, config_path))
}
