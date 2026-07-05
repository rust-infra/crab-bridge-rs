use anyhow::Result;
use clap::Parser;
use crab_bridge_rs::cli::run;
use crab_bridge_rs::cli_opts::CrabridgeCli;
use crab_bridge_rs::runtime;

fn main() -> Result<()> {
    runtime::init()?;
    let cli = CrabridgeCli::parse();
    runtime::block_on(run(cli))
}
