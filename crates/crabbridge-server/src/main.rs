use anyhow::Result;
use clap::Parser;
use crabbridge_core::config::explicit_config_from_cli;
use crabbridge_core::runtime;
use crabbridge_server::opts::BridgeCli;
use crabbridge_server::server;

fn main() -> Result<()> {
    runtime::init()?;
    let cli = BridgeCli::parse();
    let config_path = explicit_config_from_cli(Some(cli.config.clone()));
    runtime::block_on(server::run(cli, config_path))
}
