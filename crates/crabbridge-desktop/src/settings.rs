//! Settings and onboarding window helpers.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const SETTINGS_LABEL: &str = "settings";
pub const WELCOME_LABEL: &str = "welcome";

pub fn open_settings(app: &AppHandle) -> tauri::Result<()> {
    open_window(
        app,
        SETTINGS_LABEL,
        "CrabBridge Settings",
        "settings.html",
        520.0,
        720.0,
    )
}

pub fn open_welcome(app: &AppHandle) -> tauri::Result<()> {
    prepare_app_for_window(app);
    open_window(
        app,
        WELCOME_LABEL,
        "Welcome to CrabBridge",
        "welcome.html",
        560.0,
        760.0,
    )
}

pub fn hide_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(label) {
        window.hide()?;
    }
    Ok(())
}

pub fn back_to_home(app: &AppHandle) -> tauri::Result<()> {
    open_welcome(app)?;
    hide_window(app, SETTINGS_LABEL)
}

pub fn focus_existing_window(app: &AppHandle) {
    prepare_app_for_window(app);
    if let Some(window) = app.get_webview_window(WELCOME_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let _ = open_welcome(app);
}

pub fn prepare_app_for_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let _ = app.set_activation_policy(ActivationPolicy::Regular);
        if let Err(err) = crate::dock::apply_dock_icon() {
            tracing::warn!(error = %err, "failed to set macOS dock icon");
        }
    }
    let _ = app.show();
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
        window.unminimize()?;
        window.set_focus()?;
        return Ok(());
    }

    let icon = crate::tray::load_app_icon().map_err(tauri::Error::Anyhow)?;
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(true)
        .visible(true)
        .center()
        .focused(true)
        .icon(icon)?
        .build()?;
    window.show()?;
    window.set_focus()?;
    Ok(())
}
