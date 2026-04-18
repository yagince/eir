use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

// Public OAuth App Client ID; device flow does not use a client secret.
const CLIENT_ID: &str = "Ov23liRcswHdPlreAwk0";
const SCOPE: &str = "repo read:user";
const DEVICE_FLOW_TIMEOUT: Duration = Duration::from_secs(900);

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(reqwest::Client::new)
}

#[derive(Default)]
pub struct AppState {
    pub(crate) token: Option<String>,
    pub(crate) pinned: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DeviceCode {
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

#[tauri::command]
pub async fn start_device_flow() -> Result<DeviceCode, String> {
    let res = http()
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("device/code {status}: {body}"));
    }
    res.json::<DeviceCode>().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn poll_device_flow(
    device_code: String,
    interval: u64,
    app: tauri::AppHandle,
    auth: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut wait = interval.max(5);
    let deadline = Instant::now() + DEVICE_FLOW_TIMEOUT;
    let mut first = true;

    loop {
        if Instant::now() >= deadline {
            return Err("device flow expired".into());
        }
        if !first {
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
        first = false;

        let res = http()
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
pub fn sign_out(auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().token = None;
}

#[tauri::command]
pub fn set_window_pinned(pinned: bool, auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().pinned = pinned;
}
