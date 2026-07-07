//! Import Codex-facing API keys from the user's login shell.

use std::process::Command;

use anyhow::{Context, Result};
use tracing::debug;

use crate::secrets::supported_env_keys;

/// Load `DEEPSEEK_API_KEY` / `KIMI_API_KEY` from the login shell when unset.
///
/// GUI apps launched from Finder do not inherit `~/.zshrc`; Codex terminals do.
/// Setup and config checks need the same variables Codex reads at runtime.
pub fn load_login_shell_env() -> Result<()> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    for env_key in supported_env_keys() {
        if std::env::var(env_key)
            .ok()
            .is_some_and(|value| !value.is_empty())
        {
            continue;
        }

        let Some(value) = read_shell_var(&shell, env_key)? else {
            continue;
        };

        debug!(env_key, "loaded API key from login shell");
        // SAFETY: called during desktop startup before worker threads spawn.
        unsafe { std::env::set_var(env_key, value) };
    }

    Ok(())
}

fn read_shell_var(shell: &str, env_key: &str) -> Result<Option<String>> {
    let script = format!("printf %s \"${{{env_key}}}\"");
    let output = Command::new(shell)
        .arg("-l")
        .arg("-c")
        .arg(&script)
        .output()
        .with_context(|| format!("failed to run login shell {shell}"))?;

    if !output.status.success() {
        return Ok(None);
    }

    let value = String::from_utf8(output.stdout)
        .with_context(|| format!("login shell returned non-UTF-8 for {env_key}"))?
        .trim()
        .to_string();

    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}
