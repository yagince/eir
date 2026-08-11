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
/// Events may embed upstream error messages, so newlines are flattened to
/// keep the one-event-per-line format parseable.
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
        let _ = writeln!(file, "{ts}\t{}", flatten_to_one_line(event));
    }
}

fn flatten_to_one_line(event: &str) -> String {
    event.replace(['\n', '\r'], " ")
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

    // ---- isolation ---------------------------------------------------------

    /// Marker telling a re-executed test binary that it *is* the isolated
    /// child and should run the test body instead of spawning again.
    const CHILD_MARKER: &str = "EIR_DIAGNOSTICS_TEST_CHILD";

    #[cfg(not(windows))]
    const HOME_VAR: &str = "HOME";
    #[cfg(windows)]
    const HOME_VAR: &str = "APPDATA";

    /// Anything touching the flag file or the log needs `config_dir()` — i.e.
    /// `$HOME` / `%APPDATA%` — redirected, and `ENABLED` is a process-global
    /// static on top of that. An in-process mutex is *not* enough here: the
    /// `auth` and `shortcut` tests in this same binary redirect `HOME` under
    /// their own private locks and restore the developer's real value on drop,
    /// so a lock held here could not exclude them and a stray moment of the
    /// real `HOME` would let these tests append to the developer's real
    /// `~/.config/eir/auth-diagnostics.log`.
    ///
    /// So instead of mutating this process, each such test re-executes the test
    /// binary as a single-threaded child with the home dir already pointed at a
    /// fresh temp dir. The parent never touches the environment or `ENABLED`,
    /// the child cannot race anybody, and the tests stay deterministic no
    /// matter what the other modules' tests are doing.
    ///
    /// Returns the redirected config dir in the child, and `None` in the parent
    /// (whose only job is to run the child and report its result).
    fn isolated(test: &str) -> Option<PathBuf> {
        if std::env::var_os(CHILD_MARKER).is_some() {
            return Some(config_dir().expect("child is spawned with the home var set"));
        }
        run_isolated_child(test);
        None
    }

    fn run_isolated_child(test: &str) {
        let home = std::env::temp_dir().join(format!(
            "eir-diagnostics-test-{test}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let name = format!("diagnostics::tests::{test}");

        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([name.as_str(), "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_MARKER, "1")
            .env(HOME_VAR, &home)
            .output()
            .expect("re-exec the test binary");

        let _ = std::fs::remove_dir_all(&home);
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // A name that no longer matches would run zero tests and still exit 0,
        // which would silently "pass" forever — so require exactly one pass.
        assert!(
            out.status.success() && log.contains("1 passed"),
            "isolated run of {name} did not pass:\n{log}"
        );
    }

    fn log_lines(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    // ---- enable flag -------------------------------------------------------

    #[test]
    fn init_reads_the_enable_flag_from_disk() {
        let Some(cfg) = isolated("init_reads_the_enable_flag_from_disk") else {
            return;
        };
        std::fs::create_dir_all(&cfg).unwrap();
        let flag = cfg.join("diagnostics");
        // Only a trimmed "1" opts in. Everything else — including a missing
        // file, which is the state of a user who never touched the setting —
        // must leave capture off, because `log()` writes token-lifecycle
        // details nobody asked for otherwise.
        let cases = [
            (Some("1"), true),
            (Some("1\n"), true),
            (Some(" 1 \n"), true),
            (Some("0"), false),
            (Some(""), false),
            (Some("true"), false),
            (None, false),
        ];

        for (contents, expected) in cases {
            match contents {
                Some(c) => std::fs::write(&flag, c).unwrap(),
                None => drop(std::fs::remove_file(&flag)),
            }
            init();
            assert_eq!(is_enabled(), expected, "flag file {contents:?}");
        }
    }

    #[test]
    fn set_enabled_persists_the_flag_so_a_restart_keeps_the_setting() {
        let Some(cfg) = isolated("set_enabled_persists_the_flag_so_a_restart_keeps_the_setting")
        else {
            return;
        };
        let flag = cfg.join("diagnostics");

        // Enabling has to create the config dir as well as the file.
        set_enabled(true);
        assert_eq!(std::fs::read_to_string(&flag).unwrap(), "1");
        ENABLED.store(false, Ordering::Relaxed); // stand in for a fresh process
        init();
        assert!(is_enabled(), "enable flag should survive a restart");

        set_enabled(false);
        assert_eq!(std::fs::read_to_string(&flag).unwrap(), "0");
        ENABLED.store(true, Ordering::Relaxed);
        init();
        assert!(!is_enabled(), "disable flag should survive a restart");
    }

    #[test]
    fn command_wrappers_read_and_write_the_same_flag() {
        let Some(_cfg) = isolated("command_wrappers_read_and_write_the_same_flag") else {
            return;
        };
        // The settings panel toggles through these two commands only.
        assert!(!get_diagnostics_enabled(), "off by default");
        set_diagnostics_enabled(true);
        assert!(get_diagnostics_enabled() && is_enabled());
        set_diagnostics_enabled(false);
        assert!(!get_diagnostics_enabled() && !is_enabled());
    }

    // ---- writing -----------------------------------------------------------

    #[test]
    fn log_writes_nothing_at_all_while_disabled() {
        let Some(cfg) = isolated("log_writes_nothing_at_all_while_disabled") else {
            return;
        };
        assert!(!is_enabled(), "diagnostics must default to off");

        log("token cleared: 401 from search");

        // Not just an empty log — the directory itself must not be created, so
        // an opted-out user has no diagnostics footprint whatsoever.
        assert!(!cfg.exists(), "disabled logging created {}", cfg.display());
    }

    #[test]
    fn enabling_and_disabling_bracket_the_log_and_then_go_silent() {
        let Some(cfg) = isolated("enabling_and_disabling_bracket_the_log_and_then_go_silent")
        else {
            return;
        };
        let logfile = cfg.join("auth-diagnostics.log");

        set_enabled(true);
        log("startup: token probe");
        set_enabled(false);
        log("must not be recorded");

        let lines = log_lines(&logfile);
        assert_eq!(lines.len(), 3, "unexpected log contents: {lines:?}");
        // The "disabled" line is written *before* the flag flips, so the file
        // itself records when capture stopped.
        let tails = [
            "diagnostics enabled",
            "startup: token probe",
            "diagnostics disabled",
        ];
        for (line, tail) in lines.iter().zip(tails) {
            assert!(
                line.ends_with(tail),
                "expected {line:?} to end with {tail:?}"
            );
        }
    }

    #[test]
    fn logged_events_stay_one_line_even_with_embedded_newlines() {
        let Some(cfg) = isolated("logged_events_stay_one_line_even_with_embedded_newlines") else {
            return;
        };
        let logfile = cfg.join("auth-diagnostics.log");
        set_enabled(true);

        // Upstream octocrab / reqwest errors are routinely multi-line; a raw
        // newline here would split one event into several bogus records and
        // break the one-event-per-line format the log is read with.
        log("search failed: HTTP 502\r\n<html>\n  <body/>\n</html>");

        let lines = log_lines(&logfile);
        assert_eq!(lines.len(), 2, "enable marker + one event: {lines:?}");
        let (ts, event) = lines[1].split_once('\t').expect("timestamp<TAB>event");
        assert_eq!(event, "search failed: HTTP 502  <html>   <body/> </html>");
        assert!(
            ts.contains('T') && ts.ends_with('Z'),
            "expected an RFC3339-ish UTC timestamp, got {ts:?}"
        );
    }

    #[test]
    fn missing_home_leaves_diagnostics_off_and_logging_silent() {
        let Some(cfg) = isolated("missing_home_leaves_diagnostics_off_and_logging_silent") else {
            return;
        };
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("diagnostics"), "1").unwrap();
        // Safe in the isolated child: it is the only thread, and the parent's
        // environment is untouched. lib.rs has seen `HOME` resolve oddly across
        // launch contexts, which is the very thing this module exists to catch.
        std::env::remove_var(HOME_VAR);

        init();
        assert!(!is_enabled(), "an unresolvable config dir cannot opt in");
        ENABLED.store(true, Ordering::Relaxed);
        log("must not panic and must not be written anywhere");

        assert_eq!(
            std::fs::read_dir(&cfg).unwrap().count(),
            1,
            "only the flag file should exist"
        );
    }

    #[test]
    fn rotation_overwrites_the_single_backup_rather_than_stacking_more() {
        let Some(cfg) = isolated("rotation_overwrites_the_single_backup_rather_than_stacking_more")
        else {
            return;
        };
        let logfile = cfg.join("auth-diagnostics.log");
        set_enabled(true);

        // Drive two rotations through the real `log()` entry point: each pass
        // pre-fills the active log past the cap with an identifiable marker.
        for marker in ["first", "second"] {
            let filler = format!("{marker}{}", "x".repeat(MAX_LOG_BYTES as usize));
            std::fs::write(&logfile, filler).unwrap();
            log("event after rotation");
        }

        let backup = std::fs::read_to_string(cfg.join("auth-diagnostics.log.1")).unwrap();
        assert!(
            backup.starts_with("second"),
            "the single backup should hold the most recently rotated log"
        );
        assert!(
            !cfg.join("auth-diagnostics.log.2").exists(),
            "rotation must not stack a second backup"
        );
        // flag + active log + one backup: total on-disk size stays capped at ~2x.
        assert_eq!(std::fs::read_dir(&cfg).unwrap().count(), 3);
        assert_eq!(log_lines(&logfile).len(), 1, "rotation starts a fresh log");
    }

    // ---- pure helpers ------------------------------------------------------

    #[test]
    fn flatten_replaces_newlines_so_events_stay_one_line() {
        assert_eq!(
            flatten_to_one_line("graphql error:\r\nline two\nline three"),
            "graphql error:  line two line three"
        );
    }

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
