mod auth;
mod github;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::auth::AppState;

/// Default hotkey to toggle the popup. Ctrl+Shift+E is uncommon on macOS
/// so it rarely clashes with app shortcuts. Plan to make this configurable.
fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyE)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // reqwest and other deps both pull in rustls with different crypto providers
    // (aws-lc-rs and ring). Picking one explicitly avoids the runtime panic when
    // rustls can't auto-select.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // Only one shortcut is registered; no need to compare.
                    if event.state() == ShortcutState::Pressed {
                        tray::toggle_popup(app);
                    }
                })
                .build(),
        )
        .manage(Mutex::new(AppState::with_stored_token()))
        .invoke_handler(tauri::generate_handler![
            auth::start_device_flow,
            auth::poll_device_flow,
            auth::sign_out,
            auth::set_window_pinned,
            github::fetch_watched,
            github::fetch_notifications,
            github::mark_notification_read,
            tray::set_tray_badge,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::setup(app)?;
            app.global_shortcut().register(toggle_shortcut())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::Focused(false) = event {
                let pinned = window
                    .app_handle()
                    .state::<Mutex<AppState>>()
                    .lock()
                    .unwrap()
                    .pinned;
                if !pinned {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
