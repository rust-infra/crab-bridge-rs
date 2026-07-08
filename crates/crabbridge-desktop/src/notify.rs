//! Native desktop notifications for tray actions.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn show_info(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn show_error(app: &AppHandle, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title("CrabBridge")
        .body(body)
        .show();
}
