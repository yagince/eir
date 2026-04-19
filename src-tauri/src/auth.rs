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

/// Plain file under `$HOME/.config/eir/token` with mode 0600.
///
/// We deliberately don't use the OS keychain. macOS Keychain ACLs bind to the
/// caller's cdhash, which changes on every rebuild / new release — so
/// "Always Allow" re-prompts the user on every update unless the app is
/// signed with a stable Apple Developer ID *and* the ACL is set up with a
/// Designated Requirement (which the `keyring` crate does not do). A
/// mode-0600 file behaves consistently across dev and release builds, and
/// matches what tools like `gh`, `git-credential-store`, and the `cargo`
/// registry credentials file do.
mod token_store {
    use std::io::Write;
    use std::path::PathBuf;

    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".config/eir/token"))
    }

    pub fn load() -> Option<String> {
        let raw = std::fs::read_to_string(path()?).ok()?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn save(token: &str) {
        let Some(p) = path() else { return };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::File::create(&p) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            let _ = file.write_all(token.as_bytes());
        }
    }

    pub fn delete() {
        if let Some(p) = path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn load_stored_token() -> Option<String> {
    token_store::load()
}

#[derive(Default)]
pub struct AppState {
    pub(crate) token: Option<String>,
    pub(crate) pinned: bool,
}

impl AppState {
    pub fn with_stored_token() -> Self {
        Self {
            token: load_stored_token(),
            pinned: false,
        }
    }
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
                token_store::save(&access_token);
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

pub fn clear_stored_token(auth: &Mutex<AppState>) {
    auth.lock().unwrap().token = None;
    token_store::delete();
}

#[tauri::command]
pub fn sign_out(auth: State<'_, Mutex<AppState>>) {
    clear_stored_token(&auth);
}

#[tauri::command]
pub fn set_window_pinned(pinned: bool, auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().pinned = pinned;
}
