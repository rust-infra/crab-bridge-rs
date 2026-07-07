//! Clipboard helpers for onboarding copy actions.

use anyhow::{Context, Result};

pub fn copy_text(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open system clipboard")?;
    clipboard
        .set_text(text)
        .context("failed to write to clipboard")?;
    Ok(())
}
