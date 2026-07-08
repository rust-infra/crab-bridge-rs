//! System tray menu and status labels.

use anyhow::{Context, Result};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Wry};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");
const APP_ICON_BYTES: &[u8] = include_bytes!("../icons/icon.png");

pub fn load_tray_icon() -> Result<Image<'static>> {
    Image::from_bytes(TRAY_ICON_BYTES).context("failed to decode tray icon (32x32.png)")
}

pub fn load_app_icon() -> Result<Image<'static>> {
    Image::from_bytes(APP_ICON_BYTES).context("failed to decode app icon (icon.png)")
}

pub const MENU_START: &str = "start";
pub const MENU_STOP: &str = "stop";
pub const MENU_ADMIN: &str = "admin";
pub const MENU_SETUP: &str = "setup";
pub const MENU_WELCOME: &str = "welcome";
pub const MENU_CHECK: &str = "check";
pub const MENU_SETTINGS: &str = "settings";
pub const MENU_QUIT: &str = "quit";

pub fn build_tray_menu(app: &App, running: bool) -> tauri::Result<Menu<Wry>> {
    build_tray_menu_for_handle(app.handle(), running)
}

pub fn build_tray_menu_for_handle(app: &AppHandle, running: bool) -> tauri::Result<Menu<Wry>> {
    let start = MenuItem::with_id(app, MENU_START, "Start Bridge", !running, None::<&str>)?;
    let stop = MenuItem::with_id(app, MENU_STOP, "Stop Bridge", running, None::<&str>)?;
    let admin = MenuItem::with_id(
        app,
        MENU_ADMIN,
        "Open Admin Dashboard",
        running,
        None::<&str>,
    )?;
    let setup = MenuItem::with_id(app, MENU_SETUP, "Run Codex Setup", true, None::<&str>)?;
    let welcome = MenuItem::with_id(app, MENU_WELCOME, "Quick Setup…", true, None::<&str>)?;
    let check = MenuItem::with_id(app, MENU_CHECK, "Check Configuration", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, MENU_SETTINGS, "Settings…", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "Quit", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &start, &stop, &admin, &welcome, &setup, &check, &settings, &separator, &quit,
        ],
    )
}

pub fn tray_tooltip(status: &str) -> String {
    format!("CrabBridge — {status}")
}

pub fn refresh_tray_menu(app: &AppHandle, running: bool) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        let menu = build_tray_menu_for_handle(app, running)?;
        tray.set_menu(Some(menu))?;
    }
    Ok(())
}

pub fn setup_tray(
    app: &App,
    on_menu_event: impl Fn(AppHandle, tauri::menu::MenuEvent) + Send + Sync + 'static,
) -> Result<()> {
    if app.tray_by_id("main").is_some() {
        return Ok(());
    }

    let menu = build_tray_menu(app, false)?;
    let icon = load_tray_icon()?;

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(false)
        .menu(&menu)
        .tooltip(tray_tooltip("stopped"))
        .on_menu_event(move |app, event| {
            on_menu_event(app.clone(), event);
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle().clone();
                let _ = crate::settings::open_welcome(&app);
            }
        })
        .build(app)
        .context("failed to create system tray icon")?;

    Ok(())
}
