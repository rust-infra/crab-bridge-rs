//! System keychain storage for upstream API keys.

use std::process::Command;

use anyhow::{Context, Result, bail};
use keyring::Entry;
use serde::Serialize;
use tracing::debug;

const SERVICE: &str = "crabbridge";

#[derive(Debug, Clone, Serialize)]
pub struct SecretStatus {
    pub env_key: String,
    pub provider: String,
    /// Present in this process (shell env or keychain fallback).
    pub in_process_env: bool,
    /// Stored in keychain (optional override; not required for Codex).
    pub stored: bool,
    /// Available for setup checks (process env or keychain).
    pub available: bool,
}

pub fn supported_env_keys() -> [&'static str; 2] {
    ["DEEPSEEK_API_KEY", "KIMI_API_KEY"]
}

pub fn provider_for_env_key(env_key: &str) -> Option<&'static str> {
    match env_key {
        "DEEPSEEK_API_KEY" => Some("deepseek"),
        "KIMI_API_KEY" => Some("kimi"),
        _ => None,
    }
}

fn entry_for(env_key: &str) -> Result<Entry> {
    Entry::new(SERVICE, env_key).context("failed to open keychain entry")
}

pub fn set_secret(env_key: &str, value: &str) -> Result<()> {
    if provider_for_env_key(env_key).is_none() {
        bail!("unsupported env key: {env_key}");
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("API key must not be empty");
    }
    entry_for(env_key)?
        .set_password(trimmed)
        .context("failed to store API key in keychain")
}

pub fn clear_secret(env_key: &str) -> Result<()> {
    if provider_for_env_key(env_key).is_none() {
        bail!("unsupported env key: {env_key}");
    }
    match entry_for(env_key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub fn get_secret(env_key: &str) -> Result<Option<String>> {
    if provider_for_env_key(env_key).is_none() {
        bail!("unsupported env key: {env_key}");
    }
    match entry_for(env_key)?.get_password() {
        Ok(value) if value.is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

pub fn mask_api_key(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 6 {
        return "••••".to_string();
    }
    let prefix: String = trimmed.chars().take(3).collect();
    let suffix: String = trimmed
        .chars()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

pub fn get_env_value(env_key: &str) -> Option<String> {
    std::env::var(env_key)
        .ok()
        .filter(|value| !value.is_empty())
}

pub fn resolve_env_value(env_key: &str) -> Result<Option<String>> {
    if let Some(value) = get_env_value(env_key) {
        return Ok(Some(value));
    }
    get_secret(env_key)
}

pub fn list_secret_status() -> Result<Vec<SecretStatus>> {
    supported_env_keys()
        .into_iter()
        .map(|env_key| {
            let stored = get_secret(env_key)?.is_some();
            let in_process_env = get_env_value(env_key).is_some();
            Ok(SecretStatus {
                env_key: (*env_key).to_string(),
                provider: provider_for_env_key(env_key)
                    .unwrap_or("unknown")
                    .to_string(),
                stored,
                in_process_env,
                available: in_process_env || stored,
            })
        })
        .collect()
}

pub fn hydrate_api_keys() -> Result<()> {
    load_login_shell_env()?;
    inject_stored_secrets()
}

/// Load `DEEPSEEK_API_KEY` / `KIMI_API_KEY` from the login shell when unset.
///
/// GUI apps launched from Finder do not inherit `~/.zshrc`; Codex terminals do.
/// Setup and config checks need the same variables Codex reads at runtime.
fn load_login_shell_env() -> Result<()> {
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

/// Load keychain secrets into the process environment when unset.
pub fn inject_stored_secrets() -> Result<()> {
    for env_key in supported_env_keys() {
        if get_env_value(env_key).is_some() {
            continue;
        }
        if let Some(value) = get_secret(env_key)? {
            // SAFETY: called during desktop startup before worker threads spawn.
            unsafe { std::env::set_var(env_key, value) };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_shows_prefix_and_suffix() {
        assert_eq!(mask_api_key("sk-abcdefghij98a"), "sk-...98a");
    }
}
