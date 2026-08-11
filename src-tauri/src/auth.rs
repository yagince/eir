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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::MutexGuard;

    const TOKEN: &str = "ghp_secret_value";

    /// `token_store` re-resolves its path from `HOME` (`APPDATA` on Windows) on
    /// every call, and env vars are process-global while `cargo test` runs test
    /// fns on parallel threads. Every test that redirects the home dir holds
    /// this lock for its whole body, so no test can observe another's `HOME` —
    /// and so none of them can reach the developer's real
    /// `~/.config/eir/token`.
    ///
    /// Any *other* test in this crate that mutates the same env var has to share
    /// **this** lock. A second, module-local lock elsewhere does not serialize
    /// against this one, and the two sets of tests will clobber each other's
    /// `HOME` intermittently.
    use crate::test_env::HOME_LOCK;

    #[cfg(not(windows))]
    const HOME_VAR: &str = "HOME";
    #[cfg(windows)]
    const HOME_VAR: &str = "APPDATA";

    /// Points the token store at a fresh temp dir for one test. The previous
    /// value is restored and the dir removed in `Drop` rather than at the end
    /// of the test body, so a failing assertion cannot leak a temp `HOME` into
    /// the tests queued behind the lock.
    struct TempHome {
        _lock: MutexGuard<'static, ()>,
        dir: PathBuf,
        previous: Option<OsString>,
    }

    impl TempHome {
        fn new(label: &str) -> Self {
            let lock = HOME_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dir =
                std::env::temp_dir().join(format!("eir-auth-test-{}-{label}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            let previous = std::env::var_os(HOME_VAR);
            std::env::set_var(HOME_VAR, &dir);
            Self {
                _lock: lock,
                dir,
                previous,
            }
        }

        /// The path `token_store` is documented to resolve to under this home.
        #[cfg(not(windows))]
        fn token_path(&self) -> PathBuf {
            self.dir.join(".config").join("eir").join("token")
        }

        #[cfg(windows)]
        fn token_path(&self) -> PathBuf {
            self.dir.join("eir").join("token")
        }

        /// Write the file directly, bypassing `save`, so `load` can be exercised
        /// against contents `save` itself would never produce.
        fn write_raw(&self, contents: &str) {
            let p = self.token_path();
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, contents).unwrap();
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => std::env::set_var(HOME_VAR, v),
                None => std::env::remove_var(HOME_VAR),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn state_with(token: Option<&str>) -> Mutex<AppState> {
        Mutex::new(AppState {
            token: token.map(str::to_string),
            pinned: false,
        })
    }

    #[test]
    fn save_writes_the_token_under_the_home_config_dir() {
        let home = TempHome::new("save-path");

        token_store::save(TOKEN);

        let p = home.token_path();
        assert!(p.exists(), "expected token at {}", p.display());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), TOKEN);
    }

    #[test]
    fn save_then_load_round_trips_the_token() {
        let _home = TempHome::new("round-trip");
        token_store::save(TOKEN);
        assert_eq!(token_store::load().as_deref(), Some(TOKEN));
    }

    /// The 0600 mode is the entire reason this module can skip the OS keychain
    /// (see the `token_store` doc comment), so pin the bits — including the
    /// overwrite case, where an older build may have left a laxer file behind.
    #[cfg(unix)]
    #[test]
    fn saved_token_file_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let home = TempHome::new("mode");

        for pre_existing in [None, Some(0o644), Some(0o666)] {
            if let Some(mode) = pre_existing {
                home.write_raw("stale");
                let perms = std::fs::Permissions::from_mode(mode);
                std::fs::set_permissions(home.token_path(), perms).unwrap();
            }
            token_store::save(TOKEN);
            let meta = std::fs::metadata(home.token_path()).unwrap();
            let actual = meta.permissions().mode() & 0o777;
            assert_eq!(actual, 0o600, "pre_existing mode {pre_existing:?}");
        }
    }

    #[test]
    fn load_returns_none_when_the_file_is_absent() {
        let _home = TempHome::new("absent");
        assert_eq!(token_store::load(), None);
    }

    /// A hand-edited or `echo`-written file carries a trailing newline, and an
    /// interrupted write can leave the file empty — the first must still read
    /// back as a usable token, the second must not.
    #[test]
    fn load_trims_whitespace_and_treats_blank_contents_as_no_token() {
        let home = TempHome::new("blank");
        let cases = [
            ("", None),
            ("   ", None),
            ("\n\t \r\n", None),
            ("ghp_token\n", Some("ghp_token")),
            ("  ghp_token  ", Some("ghp_token")),
        ];

        for (contents, expected) in cases {
            home.write_raw(contents);
            let got = token_store::load();
            assert_eq!(got.as_deref(), expected, "contents {contents:?}");
        }
    }

    #[test]
    fn delete_removes_the_token_file_and_is_idempotent() {
        let home = TempHome::new("delete");
        token_store::save(TOKEN);

        token_store::delete();
        assert!(!home.token_path().exists());
        assert_eq!(token_store::load(), None);

        // A second sign-out must not blow up on the now-missing file.
        token_store::delete();
    }

    #[test]
    fn clear_stored_token_clears_both_the_file_and_the_in_memory_copy() {
        let home = TempHome::new("clear");
        token_store::save(TOKEN);
        let auth = state_with(Some(TOKEN));

        clear_stored_token(&auth, "unit test");

        assert!(auth.lock().unwrap().token.is_none());
        assert!(!home.token_path().exists());
    }

    #[test]
    fn with_stored_token_restores_the_persisted_token() {
        let _home = TempHome::new("restore");
        token_store::save(TOKEN);
        assert_eq!(AppState::with_stored_token().token.as_deref(), Some(TOKEN));
    }

    #[test]
    fn with_stored_token_starts_signed_out_when_nothing_is_persisted() {
        let _home = TempHome::new("restore-empty");

        let state = AppState::with_stored_token();

        assert!(state.token.is_none());
        assert!(!state.pinned, "a restored session is never pinned");
    }

    /// The probe's exact field wording is what the diagnostics log is read for,
    /// so pin the fields rather than just "some string came back".
    #[test]
    fn token_probe_reports_a_present_and_loaded_token() {
        let home = TempHome::new("probe-present");
        token_store::save(TOKEN);

        let probe = token_probe(&state_with(Some(TOKEN)));

        let env = format!("{HOME_VAR}={}", home.dir.display());
        let path = format!("path={}", home.token_path().display());
        for expected in [
            env.as_str(),
            path.as_str(),
            "exists=true",
            "bytes=16",
            "loaded_into_state=true",
        ] {
            assert!(
                probe.contains(expected),
                "{expected:?} missing from {probe}"
            );
        }
        assert_eq!(TOKEN.len(), 16, "bytes= above assumes this token length");
    }

    #[test]
    fn token_probe_reports_an_absent_token() {
        let _home = TempHome::new("probe-absent");

        let probe = token_probe(&state_with(None));

        for expected in ["exists=false", "bytes=0", "loaded_into_state=false"] {
            assert!(
                probe.contains(expected),
                "{expected:?} missing from {probe}"
            );
        }
    }

    /// The case the probe exists to distinguish: the file is on disk but startup
    /// never got it into `AppState` (see the comment on `token_probe`).
    #[test]
    fn token_probe_separates_file_present_from_state_loaded() {
        let _home = TempHome::new("probe-unloaded");
        token_store::save(TOKEN);

        let probe = token_probe(&state_with(None));

        assert!(probe.contains("exists=true"), "{probe}");
        assert!(probe.contains("loaded_into_state=false"), "{probe}");
    }

    /// The probe lands in a log file the user may hand over verbatim, so it must
    /// never carry the token value itself.
    #[test]
    fn token_probe_never_includes_the_token_value() {
        let _home = TempHome::new("probe-secret");
        token_store::save(TOKEN);

        let probe = token_probe(&state_with(Some(TOKEN)));

        assert!(!probe.contains(TOKEN), "token leaked into probe: {probe}");
    }
}
