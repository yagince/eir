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

/// Plain file under a per-user config directory, mode 0600 on unix.
///
/// - macOS / Linux: `$HOME/.config/eir/token`
/// - Windows: `%APPDATA%\eir\token`
///
/// We deliberately don't use the OS keychain. macOS Keychain ACLs bind to the
/// caller's cdhash, which changes on every rebuild / new release — so
/// "Always Allow" re-prompts the user on every update unless the app is
/// signed with a stable Apple Developer ID *and* the ACL is set up with a
/// Designated Requirement (which the `keyring` crate does not do). A
/// plain mode-0600 file behaves consistently across dev and release builds,
/// and matches what tools like `gh`, `git-credential-store`, and the `cargo`
/// registry credentials file do. Windows stores the file under `%APPDATA%`
/// where per-user ACLs already restrict access to the owning account.
mod token_store {
    use std::io::Write;
    use std::path::PathBuf;

    #[cfg(not(windows))]
    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("eir")
                .join("token"),
        )
    }

    #[cfg(windows)]
    fn path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("eir").join("token"))
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

    /// Non-sensitive snapshot of the token file for the diagnostics log: which
    /// base env var resolved, the resolved path, and whether the file exists /
    /// is non-empty. Never includes the token value itself.
    pub fn diagnostic_probe() -> String {
        #[cfg(not(windows))]
        let (env_name, env_val) = ("HOME", std::env::var_os("HOME"));
        #[cfg(windows)]
        let (env_name, env_val) = ("APPDATA", std::env::var_os("APPDATA"));

        let env_repr = match env_val {
            Some(v) => v.to_string_lossy().into_owned(),
            None => "<MISSING>".to_string(),
        };
        match path() {
            None => format!("{env_name}={env_repr} path=<unresolved>"),
            Some(p) => {
                let (exists, bytes) = match std::fs::metadata(&p) {
                    Ok(m) => (true, m.len()),
                    Err(_) => (false, 0),
                };
                format!(
                    "{env_name}={env_repr} path={} exists={exists} bytes={bytes}",
                    p.display()
                )
            }
        }
    }
}

/// One-line summary of the token-store + in-memory auth state for diagnostics.
/// `loaded_into_state` is the decisive bit: `exists=true` but
/// `loaded_into_state=false` means the file was there but we failed to read it.
pub fn token_probe(auth: &Mutex<AppState>) -> String {
    let loaded = auth.lock().unwrap().token.is_some();
    format!(
        "{}; loaded_into_state={loaded}",
        token_store::diagnostic_probe()
    )
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
                crate::diagnostics::log("device-flow: token obtained and stored");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
                app.state::<crate::background::BackgroundHandle>()
                    .trigger_refresh();
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

/// Clear the persisted token and the in-memory copy. `reason` is recorded in
/// the diagnostics log so a re-auth event can be traced back to its trigger
/// (a 401 from a specific call vs. an explicit sign-out).
pub fn clear_stored_token(auth: &Mutex<AppState>, reason: &str) {
    crate::diagnostics::log(&format!("token cleared: {reason}"));
    auth.lock().unwrap().token = None;
    token_store::delete();
}

#[tauri::command]
pub fn sign_out(
    auth: State<'_, Mutex<AppState>>,
    bg: State<'_, crate::background::BackgroundHandle>,
    app: tauri::AppHandle,
) {
    clear_stored_token(&auth, "sign_out (user action)");
    bg.clear_and_notify(&app);
}

#[tauri::command]
pub fn set_window_pinned(pinned: bool, auth: State<'_, Mutex<AppState>>) {
    auth.lock().unwrap().pinned = pinned;
}

/// Put the popup into "dialog mode": pinned (so focus loss won't auto-hide)
/// and not always-on-top (so a native dialog can actually appear above it).
/// Revert with `dialog_mode=false` when the dialog is dismissed.
#[tauri::command]
pub fn set_dialog_mode(
    enabled: bool,
    window: tauri::WebviewWindow,
    auth: State<'_, Mutex<AppState>>,
) {
    auth.lock().unwrap().pinned = enabled;
    let _ = window.set_always_on_top(!enabled);
}
