//! Settings and onboarding window helpers.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";
pub const WELCOME_LABEL: &str = "welcome";

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    open_window(app, SETTINGS_LABEL, "CrabBridge Settings", "settings.html", 520.0, 720.0)
}

pub fn open_welcome(app: &AppHandle) -> tauri::Result<()> {
    open_window(app, WELCOME_LABEL, "Welcome to CrabBridge", "welcome.html", 560.0, 760.0)
}

pub fn focus_existing_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WELCOME_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn open_window(
    app: &AppHandle,
    label: &str,
    title: &str,
    page: &str,
    width: f64,
    height: f64,
) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(label) {
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }

    WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(true)
        .build()?;
    Ok(())
}
