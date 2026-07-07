//! Login-item / launch-at-startup helpers.

use anyhow::{Context, Result};
use tauri::AppHandle;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;

pub fn install(app: &AppHandle) -> Result<()> {
    app.autolaunch()
        .enable()
        .context("failed to enable launch at login")
}

pub fn uninstall(app: &AppHandle) -> Result<()> {
    app.autolaunch()
        .disable()
        .context("failed to disable launch at login")
}

pub fn is_enabled(app: &AppHandle) -> Result<bool> {
    app.autolaunch()
        .is_enabled()
        .context("failed to read launch-at-login state")
}

pub fn macos_launcher() -> MacosLauncher {
    MacosLauncher::LaunchAgent
}
