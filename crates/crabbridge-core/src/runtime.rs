//! Shared runtime bootstrap and Tokio helpers.

use std::future::Future;

use anyhow::Result;

use crate::config::{explicit_config_before_cli, load_config_into_env};
use crate::provider::bootstrap_upstream_env;

/// Load config from env/argv and bootstrap upstream env vars before host startup.
pub fn init() -> Result<()> {
    load_config_into_env(explicit_config_before_cli())?;
    bootstrap_upstream_env();
    Ok(())
}

/// Run an async handler on a multi-thread Tokio runtime.
pub fn block_on<F>(future: F) -> Result<()>
where
    F: Future<Output = Result<()>>,
{
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(future)
}
