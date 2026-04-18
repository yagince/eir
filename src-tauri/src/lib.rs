use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, PhysicalPosition, State, WindowEvent,
};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray-icon.png");
const CLIENT_ID: &str = "Ov23liRcswHdPlreAwk0";
const SCOPE: &str = "repo read:user";

#[derive(Default)]
struct AppState {
    token: Option<String>,
    suppress_auto_hide: bool,
}

#[derive(Serialize, Deserialize)]
struct DeviceCode {
    user_code: String,
    verification_uri: String,
    device_code: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success {
        access_token: String,
    },
    Error {
        error: String,
        #[allow(dead_code)]
        error_description: Option<String>,
    },
}

#[derive(Serialize)]
struct WatchedItem {
    id: u64,
    kind: &'static str,
    title: String,
    number: u64,
    repo: String,
    url: String,
    author: String,
    updated_at: String,
    state: String,
}

#[tauri::command]
async fn start_device_flow() -> Result<DeviceCode, String> {
    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    let body = res.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("device/code {status}: {body}"));
    }
    serde_json::from_str::<DeviceCode>(&body).map_err(|e| format!("parse: {e} body={body}"))
}

#[tauri::command]
async fn poll_device_flow(
    device_code: String,
    interval: u64,
    app: tauri::AppHandle,
    auth: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let mut wait = interval.max(5);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(900);

    loop {
        if std::time::Instant::now() >= deadline {
            return Err("device flow expired".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;

        let res = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json::<TokenResponse>()
            .await
            .map_err(|e| e.to_string())?;

        match res {
            TokenResponse::Success { access_token } => {
                auth.lock().unwrap().token = Some(access_token);
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                return Ok(());
            }
            TokenResponse::Error { error, .. } => match error.as_str() {
                "authorization_pending" => {}
                "slow_down" => wait += 5,
                "expired_token" | "access_denied" | "unsupported_grant_type" => {
                    return Err(error);
                }
                other => return Err(other.into()),
            },
        }
    }
}

#[tauri::command]
fn is_authenticated(auth: State<'_, Mutex<AppState>>) -> bool {
    auth.lock().unwrap().token.is_some()
}

#[tauri::command]
fn sign_out(auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().token = None;
}

#[tauri::command]
fn set_auto_hide(enabled: bool, auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().suppress_auto_hide = !enabled;
}

#[tauri::command]
async fn fetch_watched(
    auth: State<'_, Mutex<AppState>>,
) -> Result<Vec<WatchedItem>, String> {
    let token = auth
        .lock()
        .unwrap()
        .token
        .clone()
        .ok_or_else(|| "not authenticated".to_string())?;

    let octo = octocrab::OctocrabBuilder::new()
        .personal_token(token)
        .build()
        .map_err(|e| e.to_string())?;

    let page = octo
        .search()
        .issues_and_pull_requests("is:open involves:@me archived:false")
        .sort("updated")
        .order("desc")
        .per_page(50)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let items = page
        .items
        .into_iter()
        .map(|issue| {
            let repo = issue
                .repository_url
                .path()
                .trim_start_matches("/repos/")
                .to_string();
            let kind = if issue.pull_request.is_some() {
                "pr"
            } else {
                "issue"
            };
            WatchedItem {
                id: issue.id.0,
                kind,
                title: issue.title,
                number: issue.number,
                repo,
                url: issue.html_url.to_string(),
                author: issue.user.login,
                updated_at: issue.updated_at.to_rfc3339(),
                state: format!("{:?}", issue.state).to_lowercase(),
            }
        })
        .collect();

    Ok(items)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            start_device_flow,
            poll_device_flow,
            is_authenticated,
            sign_out,
            set_auto_hide,
            fetch_watched,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let quit_item = MenuItem::with_id(app, "quit", "Quit eir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            let tray_icon = Image::from_bytes(TRAY_ICON_BYTES)?;
            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let Some(window) = app.get_webview_window("main") else {
                            return;
                        };
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                            return;
                        }
                        let size = window.outer_size().unwrap_or_default();
                        let x = position.x as i32 - (size.width as i32) / 2;
                        let y = position.y as i32 + 8;
                        let _ = window.set_position(PhysicalPosition { x, y });
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::Focused(false) = event {
                let suppressed = window
                    .app_handle()
                    .state::<Mutex<AppState>>()
                    .lock()
                    .unwrap()
                    .suppress_auto_hide;
                if !suppressed {
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
