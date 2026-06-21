use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

// Public OAuth App Client ID; device flow does not use a client secret.
const CLIENT_ID: &str = "Ov23liRcswHdPlreAwk0";
const SCOPE: &str = "repo read:user";
const DEVICE_FLOW_TIMEOUT: Duration = Duration::from_secs(900);

/// Refresh the access token this many seconds before it actually expires, so a
/// fetch never goes out with a token that's about to lapse mid-flight.
const TOKEN_REFRESH_SKEW_SECS: u64 = 300;

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(reqwest::Client::new)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Persisted token bundle. The access token expires after a few hours when the
/// GitHub OAuth App has "Expire user authorization tokens" enabled; the refresh
/// token (valid ~6 months) is what lets us mint a fresh access token without
/// dragging the user back through the device flow.
///
/// When token expiration is *disabled* on the OAuth App, GitHub returns only an
/// `access_token` with no `refresh_token` / `expires_in` — `refresh_token` and
/// `expires_at` stay `None` and we simply keep using the long-lived token.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct StoredToken {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Absolute unix time (seconds) at which `access_token` expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    /// Absolute unix time (seconds) at which `refresh_token` expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token_expires_at: Option<u64>,
}

impl StoredToken {
    fn from_grant(g: TokenGrant) -> Self {
        let now = now_unix();
        Self {
            access_token: g.access_token,
            refresh_token: g.refresh_token,
            expires_at: g.expires_in.map(|s| now + s),
            refresh_token_expires_at: g.refresh_token_expires_in.map(|s| now + s),
        }
    }
}

/// Plain file under a per-user config directory, mode 0600 on unix.
///
/// - macOS / Linux: `$HOME/.config/eir/token`
/// - Windows: `%APPDATA%\eir\token`
///
/// The file holds a JSON [`StoredToken`]. Older builds wrote the bare access
/// token as plain text; [`load`] transparently upgrades that legacy format so
/// existing installs keep working across the update (the next refresh / sign-in
/// rewrites it as JSON).
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
    use super::StoredToken;
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

    pub fn load() -> Option<StoredToken> {
        let raw = std::fs::read_to_string(path()?).ok()?;
        parse(&raw)
    }

    /// New format: a JSON [`StoredToken`]. Legacy format (older builds): the
    /// bare access token as plain text. Empty content → no token.
    pub(super) fn parse(raw: &str) -> Option<StoredToken> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(stored) = serde_json::from_str::<StoredToken>(trimmed) {
            if !stored.access_token.is_empty() {
                return Some(stored);
            }
        }
        Some(StoredToken {
            access_token: trimmed.to_string(),
            ..Default::default()
        })
    }

    pub fn save(token: &StoredToken) {
        let Some(p) = path() else { return };
        let Ok(json) = serde_json::to_string(token) else {
            return;
        };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::File::create(&p) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            let _ = file.write_all(json.as_bytes());
        }
    }

    pub fn delete() {
        if let Some(p) = path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[derive(Default)]
pub struct AppState {
    /// Current access token. Consumers (`github.rs`, `background.rs`) read this.
    pub(crate) token: Option<String>,
    pub(crate) refresh_token: Option<String>,
    /// Absolute unix time (seconds) at which `token` expires, if known.
    pub(crate) access_token_expires_at: Option<u64>,
    pub(crate) pinned: bool,
}

impl AppState {
    pub fn with_stored_token() -> Self {
        let stored = token_store::load();
        Self {
            token: stored.as_ref().map(|s| s.access_token.clone()),
            refresh_token: stored.as_ref().and_then(|s| s.refresh_token.clone()),
            access_token_expires_at: stored.as_ref().and_then(|s| s.expires_at),
            pinned: false,
        }
    }
}

/// Persist a freshly-minted token bundle to disk and mirror it into `AppState`.
fn apply_stored_token(auth: &Mutex<AppState>, stored: &StoredToken) {
    token_store::save(stored);
    let mut guard = auth.lock().unwrap();
    guard.token = Some(stored.access_token.clone());
    guard.refresh_token = stored.refresh_token.clone();
    guard.access_token_expires_at = stored.expires_at;
}

/// Exchange a refresh token for a fresh access (and rotated refresh) token.
async fn request_refresh(refresh_token: &str) -> Result<StoredToken, String> {
    let res = http()
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json::<TokenResponse>()
        .await
        .map_err(|e| e.to_string())?;
    match res {
        TokenResponse::Success(grant) => Ok(StoredToken::from_grant(grant)),
        TokenResponse::Error { error, .. } => Err(error),
    }
}

/// Returns a usable access token, refreshing proactively when the current one
/// is within [`TOKEN_REFRESH_SKEW_SECS`] of expiry. Falls back to the existing
/// token if there's nothing to refresh with or the refresh call fails — the
/// caller's fetch will then surface a 401 and hit the reactive path.
///
/// The `AppState` lock is only held to read/write fields, never across the
/// network round-trip.
pub async fn valid_access_token(auth: &Mutex<AppState>) -> Option<String> {
    let (access, refresh, expires_at) = {
        let guard = auth.lock().unwrap();
        (
            guard.token.clone(),
            guard.refresh_token.clone(),
            guard.access_token_expires_at,
        )
    };
    let access = access?;

    let near_expiry = expires_at.is_some_and(|exp| now_unix() + TOKEN_REFRESH_SKEW_SECS >= exp);
    if !near_expiry {
        return Some(access);
    }
    let Some(refresh) = refresh else {
        return Some(access);
    };
    match request_refresh(&refresh).await {
        Ok(stored) => {
            let token = stored.access_token.clone();
            apply_stored_token(auth, &stored);
            Some(token)
        }
        Err(_) => Some(access),
    }
}

/// One-shot refresh attempt after a fetch came back unauthorized. Returns the
/// new access token on success; `None` means there's no usable refresh token
/// (or it too has lapsed) and the caller should clear state and re-auth.
pub async fn refresh_after_unauthorized(auth: &Mutex<AppState>) -> Option<String> {
    let refresh = auth.lock().unwrap().refresh_token.clone()?;
    match request_refresh(&refresh).await {
        Ok(stored) => {
            let token = stored.access_token.clone();
            apply_stored_token(auth, &stored);
            Some(token)
        }
        Err(_) => None,
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

/// Token-grant payload returned by both the device-flow `access_token` poll and
/// the `refresh_token` exchange. `refresh_token` / `expires_in` are only present
/// when the OAuth App has token expiration enabled.
#[derive(Deserialize)]
struct TokenGrant {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token_expires_in: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    Success(TokenGrant),
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
            TokenResponse::Success(grant) => {
                let stored = StoredToken::from_grant(grant);
                apply_stored_token(&auth, &stored);
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

pub fn clear_stored_token(auth: &Mutex<AppState>) {
    {
        let mut guard = auth.lock().unwrap();
        guard.token = None;
        guard.refresh_token = None;
        guard.access_token_expires_at = None;
    }
    token_store::delete();
}

#[tauri::command]
pub fn sign_out(
    auth: State<'_, Mutex<AppState>>,
    bg: State<'_, crate::background::BackgroundHandle>,
    app: tauri::AppHandle,
) {
    clear_stored_token(&auth);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_legacy_plain_token_keeps_access_token_only() {
        let stored = token_store::parse("  ghu_legacytoken123\n").expect("token");
        assert_eq!(stored.access_token, "ghu_legacytoken123");
        assert!(stored.refresh_token.is_none());
        assert!(stored.expires_at.is_none());
    }

    #[test]
    fn parse_empty_is_none() {
        assert!(token_store::parse("   \n").is_none());
    }

    #[test]
    fn parse_json_round_trips_all_fields() {
        let original = StoredToken {
            access_token: "ghu_access".into(),
            refresh_token: Some("ghr_refresh".into()),
            expires_at: Some(1_700_000_000),
            refresh_token_expires_at: Some(1_715_000_000),
        };
        let json = serde_json::to_string(&original).unwrap();
        let stored = token_store::parse(&json).expect("token");
        assert_eq!(stored.access_token, "ghu_access");
        assert_eq!(stored.refresh_token.as_deref(), Some("ghr_refresh"));
        assert_eq!(stored.expires_at, Some(1_700_000_000));
        assert_eq!(stored.refresh_token_expires_at, Some(1_715_000_000));
    }

    #[test]
    fn from_grant_with_no_expiry_leaves_expiry_unset() {
        // OAuth App with token expiration disabled: only an access token comes
        // back, so we keep using it indefinitely.
        let grant = TokenGrant {
            access_token: "ghu_longlived".into(),
            refresh_token: None,
            expires_in: None,
            refresh_token_expires_in: None,
        };
        let stored = StoredToken::from_grant(grant);
        assert_eq!(stored.access_token, "ghu_longlived");
        assert!(stored.expires_at.is_none());
        assert!(stored.refresh_token.is_none());
    }

    #[test]
    fn from_grant_with_expiry_sets_absolute_deadlines() {
        let before = now_unix();
        let grant = TokenGrant {
            access_token: "ghu_access".into(),
            refresh_token: Some("ghr_refresh".into()),
            expires_in: Some(28_800),
            refresh_token_expires_in: Some(15_897_600),
        };
        let stored = StoredToken::from_grant(grant);
        let after = now_unix();
        let exp = stored.expires_at.expect("expires_at");
        assert!(exp >= before + 28_800 && exp <= after + 28_800);
        let rexp = stored.refresh_token_expires_at.expect("refresh expires_at");
        assert!(rexp >= before + 15_897_600 && rexp <= after + 15_897_600);
    }
}
