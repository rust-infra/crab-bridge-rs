use anyhow::Result;
use clap::Parser;
use crabbridge_cli::cli::run;
use crabbridge_cli::cli_opts::CrabridgeCli;
use crabbridge_core::runtime;

fn main() -> Result<()> {
    runtime::init()?;
    let cli = CrabridgeCli::parse();
    runtime::block_on(run(cli))
}
