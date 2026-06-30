//! Opt-in diagnostics log for auth/token lifecycle events.
//!
//! The app occasionally drops back to the sign-in screen ("re-authentication")
//! and we want to know *why* — was the stored token actually rejected by GitHub
//! (a real 401), or did the app simply fail to read the token file at startup
//! (e.g. `HOME` resolving differently across launch contexts)? Those two look
//! identical to the user but have completely different fixes.
//!
//! This module appends timestamped one-line events to a log file so the next
//! occurrence is captured for inspection. It is **off by default** and gated
//! behind a user setting; nothing is written unless the user turns it on.
//!
//! - Enable flag: `<config>/diagnostics` (`1` / `0`), mirroring `shortcut.rs`.
//! - Log file: `<config>/auth-diagnostics.log`.
//! - `<config>` is `$HOME/.config/eir` (unix) or `%APPDATA%\eir` (Windows),
//!   the same directory the token itself lives in.
//!
//! Rotation keeps the log bounded: when the active file reaches
//! [`MAX_LOG_BYTES`] it is rotated to `*.log.1` (a single backup, overwritten
//! each rotation), capping total on-disk size at ~2×.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Rotate once the active log reaches this size; one backup is kept.
const MAX_LOG_BYTES: u64 = 256 * 1024;

static ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(not(windows))]
fn config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config").join("eir"))
}

#[cfg(windows)]
fn config_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("eir"))
}

fn flag_path() -> Option<PathBuf> {
    Some(config_dir()?.join("diagnostics"))
}

fn log_path() -> Option<PathBuf> {
    Some(config_dir()?.join("auth-diagnostics.log"))
}

/// Load the persisted enable flag into memory. Call once at startup, before
/// emitting any events.
pub fn init() {
    let enabled = flag_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim() == "1")
        .unwrap_or(false);
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Flip the flag, persist it, and bracket the change with a log line so the
/// file itself records when capture started or stopped.
pub fn set_enabled(enabled: bool) {
    // Record the "stopping" line while logging is still live.
    if !enabled {
        log("diagnostics disabled");
    }
    ENABLED.store(enabled, Ordering::Relaxed);
    if let Some(p) = flag_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, if enabled { "1" } else { "0" });
    }
    if enabled {
        log("diagnostics enabled");
    }
}

/// Append a timestamped event line. No-op unless diagnostics are enabled.
pub fn log(event: &str) {
    if !is_enabled() {
        return;
    }
    let Some(path) = log_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    rotate_if_needed(&path);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let _ = writeln!(file, "{ts}\t{event}");
    }
}

/// Rotate the active log to `*.log.1` once it grows past the cap, keeping a
/// single backup. `remove_file` first so the rename also succeeds on Windows
/// (where renaming onto an existing path errors).
fn rotate_if_needed(path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() < MAX_LOG_BYTES {
        return;
    }
    let mut backup = path.as_os_str().to_owned();
    backup.push(".1");
    let backup = PathBuf::from(backup);
    let _ = std::fs::remove_file(&backup);
    let _ = std::fs::rename(path, &backup);
}

#[tauri::command]
pub fn get_diagnostics_enabled() -> bool {
    is_enabled()
}

#[tauri::command]
pub fn set_diagnostics_enabled(enabled: bool) {
    set_enabled(enabled);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_renames_when_over_cap_and_keeps_single_backup() {
        let dir = std::env::temp_dir().join(format!("eir-diag-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("auth-diagnostics.log");

        // Over-cap file → rotates to .1
        std::fs::write(&log, vec![b'x'; (MAX_LOG_BYTES + 1) as usize]).unwrap();
        rotate_if_needed(&log);
        let backup = dir.join("auth-diagnostics.log.1");
        assert!(backup.exists(), "backup should be created");
        assert!(!log.exists(), "active log should have been renamed away");

        // A fresh, under-cap file is left untouched.
        std::fs::write(&log, b"small").unwrap();
        rotate_if_needed(&log);
        assert!(log.exists(), "small log should remain");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
