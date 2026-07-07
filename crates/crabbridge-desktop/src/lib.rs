//! CrabBridge desktop tray application.

mod autostart;
mod bridge;
mod clipboard;
mod env_export;
mod health;
mod logs;
mod notify;
mod onboarding;
mod prefs;
mod provider_config;
mod secrets;
mod settings;
mod shell_env;
mod setup_wizard;
mod tray;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use bridge::{BridgeManager, BridgeStatus, default_config_path, ensure_config_parent};
use crabbridge_cli::setup::SetupCheckReport;
use logs::LogSnapshot;
use onboarding::OnboardingStatus;
use provider_config::{ProviderConfigSaveRequest, ProviderConfigSnapshot};
use secrets::{SecretStatus, hydrate_api_keys};
use serde::Serialize;
use tauri::menu::MenuEvent;
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BridgeStatusResponse {
    pub status: String,
    pub bind_addr: String,
    pub config_path: String,
    pub admin_url: Option<String>,
    pub last_error: Option<String>,
}

fn status_label(status: BridgeStatus) -> &'static str {
    match status {
        BridgeStatus::Running => "running",
        BridgeStatus::Stopped => "stopped",
        BridgeStatus::Error => "error",
    }
}

fn bridge_response(manager: &BridgeManager) -> BridgeStatusResponse {
    BridgeStatusResponse {
        status: status_label(manager.status()).to_string(),
        bind_addr: manager.bind_addr().to_string(),
        config_path: manager.config_path().display().to_string(),
        admin_url: manager.admin_url(),
        last_error: manager.last_error(),
    }
}

async fn refresh_tray_state(app: &AppHandle, manager: &BridgeManager) {
    let running = manager.status() == BridgeStatus::Running;
    let _ = tray::refresh_tray_menu(app, running);
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tray::tray_tooltip(status_label(manager.status()))));
    }
}

#[tauri::command]
async fn bridge_status(
    manager: State<'_, Arc<BridgeManager>>,
) -> Result<BridgeStatusResponse, String> {
    Ok(bridge_response(&manager))
}

#[tauri::command]
async fn bridge_start(
    manager: State<'_, Arc<BridgeManager>>,
) -> Result<BridgeStatusResponse, String> {
    manager.start().await.map_err(|err| err.to_string())?;
    Ok(bridge_response(&manager))
}

#[tauri::command]
async fn bridge_stop(
    manager: State<'_, Arc<BridgeManager>>,
) -> Result<BridgeStatusResponse, String> {
    manager.stop().await.map_err(|err| err.to_string())?;
    Ok(bridge_response(&manager))
}

#[tauri::command]
async fn bridge_open_admin(manager: State<'_, Arc<BridgeManager>>) -> Result<(), String> {
    let url = manager
        .admin_url()
        .ok_or_else(|| "bridge is not running".to_string())?;
    open::that(&url).map_err(|err| err.to_string())
}

#[tauri::command]
async fn bridge_run_setup(
    manager: State<'_, Arc<BridgeManager>>,
) -> Result<BridgeStatusResponse, String> {
    setup_wizard::run_desktop_setup(manager.bind_addr(), manager.config_path(), false)
        .await
        .map_err(|err| err.to_string())?;
    Ok(bridge_response(&manager))
}

#[tauri::command]
fn secrets_status() -> Result<Vec<SecretStatus>, String> {
    hydrate_api_keys().map_err(|err| err.to_string())?;
    secrets::list_secret_status().map_err(|err| err.to_string())
}

#[tauri::command]
fn secrets_set(env_key: String, value: String) -> Result<(), String> {
    secrets::set_secret(&env_key, &value).map_err(|err| err.to_string())
}

#[tauri::command]
fn secrets_clear(env_key: String) -> Result<(), String> {
    secrets::clear_secret(&env_key).map_err(|err| err.to_string())
}

#[tauri::command]
fn provider_config_get(
    manager: State<'_, Arc<BridgeManager>>,
    slug: Option<String>,
) -> Result<ProviderConfigSnapshot, String> {
    provider_config::snapshot(
        manager.config_dir(),
        &manager.config_path(),
        slug.as_deref(),
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn provider_config_save(
    manager: State<'_, Arc<BridgeManager>>,
    request: ProviderConfigSaveRequest,
) -> Result<ProviderConfigSnapshot, String> {
    provider_config::save(
        manager.config_dir(),
        &manager.config_path(),
        &manager.bind_addr().to_string(),
        request,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn autostart_get(app: AppHandle) -> Result<bool, String> {
    autostart::is_enabled(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        autostart::install(&app).map_err(|err| err.to_string())
    } else {
        autostart::uninstall(&app).map_err(|err| err.to_string())
    }
}

#[tauri::command]
async fn config_check(manager: State<'_, Arc<BridgeManager>>) -> Result<SetupCheckReport, String> {
    Ok(health::run_config_check(manager.bind_addr(), manager.config_path()).await)
}

#[tauri::command]
fn logs_tail(limit: Option<usize>) -> Result<LogSnapshot, String> {
    logs::tail_logs(limit.unwrap_or(200)).map_err(|err| err.to_string())
}

#[tauri::command]
fn logs_reveal() -> Result<(), String> {
    let path = logs::log_path().ok_or_else(|| "log file not initialized".to_string())?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|err| err.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let parent = path
            .parent()
            .ok_or_else(|| "log file has no parent directory".to_string())?;
        open::that(parent).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_welcome(app: AppHandle) -> Result<(), String> {
    settings::open_welcome(&app).map_err(|err| err.to_string())
}

#[tauri::command]
fn onboarding_status(manager: State<'_, Arc<BridgeManager>>) -> Result<OnboardingStatus, String> {
    hydrate_api_keys().map_err(|err| err.to_string())?;
    onboarding::status(
        manager.config_dir(),
        &manager.config_path(),
        manager.as_ref(),
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn onboarding_run_setup(manager: State<'_, Arc<BridgeManager>>) -> Result<String, String> {
    onboarding::run_setup_and_export(
        manager.bind_addr(),
        manager.config_dir(),
        manager.config_path(),
    )
    .await
    .map_err(|err| err.to_string())
}

#[tauri::command]
async fn onboarding_finish(
    app: AppHandle,
    manager: State<'_, Arc<BridgeManager>>,
) -> Result<OnboardingStatus, String> {
    let config_dir = manager.config_dir().to_path_buf();
    let manager = manager.inner().clone();
    let status = onboarding::finish_onboarding(&config_dir, manager.clone())
        .await
        .map_err(|err| err.to_string())?;
    refresh_tray_state(&app, &manager).await;
    notify::show_info(&app, "CrabBridge", "Setup complete — bridge is running");
    Ok(status)
}

#[tauri::command]
fn install_shell_hook(manager: State<'_, Arc<BridgeManager>>) -> Result<String, String> {
    env_export::install_zsh_hook(manager.config_dir()).map_err(|err| err.to_string())
}

#[tauri::command]
fn codex_usage_hint(manager: State<'_, Arc<BridgeManager>>) -> Result<String, String> {
    Ok(env_export::codex_usage_hint(&manager.bind_addr().to_string()))
}

#[tauri::command]
fn copy_codex_hint(manager: State<'_, Arc<BridgeManager>>) -> Result<(), String> {
    let hint = env_export::codex_usage_hint(&manager.bind_addr().to_string());
    clipboard::copy_text(&hint).map_err(|err| err.to_string())
}

async fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    let manager = match app.try_state::<Arc<BridgeManager>>() {
        Some(state) => state.inner().clone(),
        None => return,
    };

    let result = match event.id.as_ref() {
        tray::MENU_START => manager.start().await.map(|_| ()),
        tray::MENU_STOP => manager.stop().await.map(|_| ()),
        tray::MENU_ADMIN => manager
            .admin_url()
            .context("bridge is not running")
            .and_then(|url| open::that(url).context("failed to open admin dashboard")),
        tray::MENU_WELCOME => settings::open_welcome(app)
            .map_err(|err| anyhow::anyhow!(err))
            .map(|_| ()),
        tray::MENU_SETUP => {
            setup_wizard::run_desktop_setup(manager.bind_addr(), manager.config_path(), false)
                .await
                .map(|_| ())
        }
        tray::MENU_CHECK => {
            let report = health::run_config_check(manager.bind_addr(), manager.config_path()).await;
            if report.has_failures() {
                if let Err(err) = settings::open_settings(app) {
                    Err(anyhow::anyhow!(err))
                } else {
                    Err(anyhow::anyhow!(
                        "configuration check failed — see Settings for details"
                    ))
                }
            } else {
                tracing::info!(
                    warnings = report.has_warnings(),
                    "configuration check passed"
                );
                Ok(())
            }
        }
        tray::MENU_SETTINGS => settings::open_settings(app)
            .map_err(|err| anyhow::anyhow!(err))
            .map(|_| ()),
        tray::MENU_QUIT => {
            let _ = manager.stop().await;
            app.exit(0);
            return;
        }
        _ => Ok(()),
    };

    match (event.id.as_ref(), &result) {
        (id, Ok(())) if id == tray::MENU_START => {
            notify::show_info(app, "CrabBridge", "Bridge started");
        }
        (id, Ok(())) if id == tray::MENU_STOP => {
            notify::show_info(app, "CrabBridge", "Bridge stopped");
        }
        (id, Ok(())) if id == tray::MENU_SETUP => {
            notify::show_info(app, "CrabBridge", "Codex setup completed");
        }
        (id, Ok(())) if id == tray::MENU_CHECK => {
            notify::show_info(app, "CrabBridge", "Configuration check passed");
        }
        (_, Err(err)) => {
            notify::show_error(app, &err.to_string());
        }
        _ => {}
    }

    if let Err(err) = result {
        tracing::error!(menu = event.id.as_ref(), error = %err, "tray action failed");
    }

    refresh_tray_state(app, &manager).await;
}

async fn bootstrap(app_handle: &AppHandle, manager: Arc<BridgeManager>) {
    let show_welcome = prefs::needs_onboarding(manager.config_dir()).unwrap_or(true);
    if show_welcome || !bridge::config_exists(&manager.config_path()) {
        tracing::info!("opening onboarding wizard");
        if let Err(err) = settings::open_welcome(app_handle) {
            tracing::error!(error = %err, "failed to open welcome window");
            let _ = settings::open_settings(app_handle);
        }
        return;
    }

    match manager.start().await {
        Ok(()) => {
            notify::show_info(app_handle, "CrabBridge", "Bridge started");
            refresh_tray_state(app_handle, &manager).await;
        }
        Err(err) => {
            tracing::error!(error = %err, "auto-start failed");
            notify::show_error(app_handle, &format!("Auto-start failed: {err}"));
        }
    }
}

pub fn run() -> Result<()> {
    let config_path = default_config_path();
    ensure_config_parent(&config_path)?;
    let config_dir = config_path
        .parent()
        .context("config path must have a parent directory")?;
    logs::init_tracing(config_dir)?;

    hydrate_api_keys()?;

    unsafe {
        std::env::set_var("CRABRIDGE_CONFIG", config_path.to_string_lossy().as_ref());
    }
    crabbridge_core::runtime::init()?;

    let bind_addr: SocketAddr = "127.0.0.1:11435"
        .parse()
        .context("default bind address must be valid")?;
    let manager = Arc::new(BridgeManager::new(config_path, bind_addr)?);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            autostart::macos_launcher(),
            Some(vec![]),
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            settings::focus_existing_window(app);
        }))
        .manage(manager.clone())
        .setup(move |app| {
            let menu = tray::build_tray_menu(app, false)?;
            let icon = app
                .default_window_icon()
                .context("missing default tray icon")?
                .clone();

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .tooltip(tray::tray_tooltip("stopped"))
                .on_menu_event(|app, event| {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        handle_menu_event(&app, event).await;
                    });
                })
                .build(app)?;

            let app_handle = app.handle().clone();
            let manager = manager.clone();
            tauri::async_runtime::spawn(async move {
                bootstrap(&app_handle, manager).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bridge_status,
            bridge_start,
            bridge_stop,
            bridge_open_admin,
            bridge_run_setup,
            secrets_status,
            secrets_set,
            secrets_clear,
            provider_config_get,
            provider_config_save,
            autostart_get,
            autostart_set,
            config_check,
            logs_tail,
            logs_reveal,
            onboarding_status,
            onboarding_run_setup,
            onboarding_finish,
            install_shell_hook,
            codex_usage_hint,
            copy_codex_hint,
            open_welcome,
        ])
        .build(tauri::generate_context!())
        .context("failed to build Tauri application")?
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event
                && let Some(manager) = app_handle.try_state::<Arc<BridgeManager>>()
            {
                let manager = manager.inner().clone();
                tauri::async_runtime::block_on(async move {
                    let _ = manager.stop().await;
                });
            }
        });

    Ok(())
}
