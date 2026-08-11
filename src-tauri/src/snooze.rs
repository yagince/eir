use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Snooze entry keyed by `WatchedItem::id`. `until_unix` is seconds since the
/// Unix epoch — i64 so a far-future timestamp comparing against `now_unix()`
/// can't silently wrap. RFC3339 strings would round-trip via the frontend
/// just as well; integer seconds keep the on-disk JSON compact and the
/// expiry check a trivial integer compare.
pub type SnoozedMap = HashMap<u64, i64>;

#[derive(Serialize, Deserialize, Default)]
struct StoredSnoozed {
    #[serde(flatten)]
    items: HashMap<String, i64>,
}

/// Seconds-since-epoch reading of the current wall clock. Used everywhere
/// expiry is checked so the same helper covers both the worker and the
/// initial purge-on-load. A clock running backwards (NTP correction) would
/// merely keep an entry alive a little longer — never resurrect a deleted
/// one.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// JSON file under a per-user config directory. Mirrors `auth::token_store` —
/// see that module's comment for why the OS keychain isn't used. This file
/// only ever contains item IDs + timestamps (no credentials), so default
/// per-user perms are fine; we still write 0600 on unix for symmetry.
///
/// - macOS / Linux: `$HOME/.config/eir/snoozed.json`
/// - Windows: `%APPDATA%\eir\snoozed.json`
mod snooze_store {
    use std::io::Write;
    use std::path::PathBuf;

    use super::{SnoozedMap, StoredSnoozed};

    #[cfg(not(windows))]
    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("eir")
                .join("snoozed.json"),
        )
    }

    #[cfg(windows)]
    fn path() -> Option<PathBuf> {
        let appdata = std::env::var_os("APPDATA")?;
        Some(PathBuf::from(appdata).join("eir").join("snoozed.json"))
    }

    pub fn load() -> SnoozedMap {
        let Some(p) = path() else {
            return SnoozedMap::new();
        };
        let Ok(raw) = std::fs::read_to_string(&p) else {
            return SnoozedMap::new();
        };
        let parsed: StoredSnoozed = serde_json::from_str(&raw).unwrap_or_default();
        parsed
            .items
            .into_iter()
            .filter_map(|(k, v)| k.parse::<u64>().ok().map(|id| (id, v)))
            .collect()
    }

    pub fn save(snoozed: &SnoozedMap) {
        let Some(p) = path() else { return };
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let stored = StoredSnoozed {
            items: snoozed.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        };
        let Ok(json) = serde_json::to_string(&stored) else {
            return;
        };
        if let Ok(mut file) = std::fs::File::create(&p) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            let _ = file.write_all(json.as_bytes());
        }
    }
}

/// Read the persisted snoozed map and immediately drop any already-expired
/// entries before returning it. Called once at startup so a worker that
/// boots after a long sleep doesn't have to re-fire the expiry purge.
pub fn load_active() -> SnoozedMap {
    let mut map = snooze_store::load();
    let now = now_unix();
    map.retain(|_, until| *until > now);
    // Persist the purge so a future load doesn't keep re-reading the same
    // already-expired entries.
    snooze_store::save(&map);
    map
}

pub fn save(snoozed: &SnoozedMap) {
    snooze_store::save(snoozed);
}

/// Pull out the entries whose `until` has passed. The mutation happens on
/// the caller's owned map so the worker can immediately reach for the
/// returned IDs to fire "Snooze ended" notifications without re-acquiring
/// the state lock.
pub fn drain_expired(map: &mut SnoozedMap, now: i64) -> Vec<u64> {
    let expired: Vec<u64> = map
        .iter()
        .filter_map(|(id, until)| (*until <= now).then_some(*id))
        .collect();
    for id in &expired {
        map.remove(id);
    }
    expired
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ---- isolation ---------------------------------------------------------

    /// Marker telling a re-executed test binary that it *is* the isolated
    /// child and should run the test body instead of spawning again.
    const CHILD_MARKER: &str = "EIR_SNOOZE_TEST_CHILD";

    #[cfg(not(windows))]
    const HOME_VAR: &str = "HOME";
    #[cfg(windows)]
    const HOME_VAR: &str = "APPDATA";

    /// `snooze_store::path()` re-resolves `$HOME` / `%APPDATA%` on every call
    /// and has no injection seam, and an in-process mutex would not help: the
    /// `auth` and `shortcut` tests in this same binary redirect `HOME` under
    /// their own private locks and restore the developer's real value on drop,
    /// so a race could point `save()` at the developer's real
    /// `~/.config/eir/snoozed.json` and overwrite it.
    ///
    /// So instead of mutating this process, each filesystem test re-executes the
    /// test binary as a single-threaded child with the home dir already pointed
    /// at a fresh temp dir. The parent's environment is never touched, and the
    /// child has no other threads to race. Mirrors `diagnostics::tests`.
    ///
    /// Returns the redirected `snoozed.json` path in the child, `None` in the
    /// parent (whose only job is to run the child and report its result).
    fn isolated(test: &str) -> Option<PathBuf> {
        if std::env::var_os(CHILD_MARKER).is_some() {
            return Some(snoozed_path());
        }
        run_isolated_child(test);
        None
    }

    /// The path `snooze_store` is documented to resolve to. Spelled out here
    /// rather than reused, so a change to the private resolver has to be a
    /// deliberate one.
    #[cfg(not(windows))]
    fn snoozed_path() -> PathBuf {
        let home = std::env::var_os(HOME_VAR).expect("child is spawned with the home var set");
        PathBuf::from(home)
            .join(".config")
            .join("eir")
            .join("snoozed.json")
    }

    #[cfg(windows)]
    fn snoozed_path() -> PathBuf {
        let home = std::env::var_os(HOME_VAR).expect("child is spawned with the home var set");
        PathBuf::from(home).join("eir").join("snoozed.json")
    }

    fn run_isolated_child(test: &str) {
        let home =
            std::env::temp_dir().join(format!("eir-snooze-test-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let name = format!("snooze::tests::{test}");

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

    fn write_raw(path: &PathBuf, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn sorted_ids(map: SnoozedMap) -> Vec<u64> {
        let mut ids: Vec<u64> = map.into_keys().collect();
        ids.sort_unstable();
        ids
    }

    // ---- persistence -------------------------------------------------------

    #[test]
    fn save_then_load_active_round_trips_future_entries() {
        let Some(path) = isolated("save_then_load_active_round_trips_future_entries") else {
            return;
        };
        let until = now_unix() + 3600;
        // `u64::MAX` proves the id survives the trip through a JSON *string*
        // key, which is where an `i64`/`u32` slip would show up.
        let map = SnoozedMap::from([(1, until), (u64::MAX, until + 60)]);

        save(&map);

        assert!(path.exists(), "save should create {}", path.display());
        assert_eq!(load_active(), map);
    }

    #[test]
    fn load_active_drops_expired_entries_and_rewrites_the_file() {
        let Some(path) = isolated("load_active_drops_expired_entries_and_rewrites_the_file") else {
            return;
        };
        let now = now_unix();
        // `until == now` is not in the future, so it goes too — the same
        // boundary `drain_expired` uses.
        save(&SnoozedMap::from([
            (1, now - 60),
            (2, now),
            (3, now + 3600),
        ]));

        assert_eq!(sorted_ids(load_active()), vec![3]);

        // The purge is persisted, so the next load can't resurrect them.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("\"1\"") && !raw.contains("\"2\""),
            "expired entries still on disk: {raw}"
        );
        assert!(raw.contains("\"3\""), "live entry was dropped: {raw}");
    }

    #[test]
    fn load_active_returns_empty_when_nothing_is_stored() {
        let Some(path) = isolated("load_active_returns_empty_when_nothing_is_stored") else {
            return;
        };
        assert!(!path.exists(), "precondition: a first-run machine");

        assert!(load_active().is_empty());

        // Startup leaves a valid empty document rather than a stray file.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn load_active_falls_back_to_empty_for_unusable_contents() {
        let Some(path) = isolated("load_active_falls_back_to_empty_for_unusable_contents") else {
            return;
        };
        let until = now_unix() + 3600;
        // A snooze map is disposable state, so anything unreadable degrades to
        // "nothing is snoozed" instead of failing startup.
        let cases: Vec<(String, Vec<u64>)> = vec![
            (String::new(), vec![]),
            ("not json at all".to_string(), vec![]),
            ("{}".to_string(), vec![]),
            (format!("[1,{until}]"), vec![]),
            (format!("{{\"7\":\"{until}\"}}"), vec![]),
            (format!("{{\"abc\":{until}}}"), vec![]),
            // A single unparseable key is skipped without taking its siblings
            // down with it.
            (format!("{{\"-1\":{until},\"7\":{until}}}"), vec![7]),
        ];

        for (raw, expected) in cases {
            write_raw(&path, &raw);
            assert_eq!(sorted_ids(load_active()), expected, "contents {raw:?}");
        }
    }

    #[test]
    fn save_replaces_the_previous_document_rather_than_appending() {
        let Some(path) = isolated("save_replaces_the_previous_document_rather_than_appending")
        else {
            return;
        };
        let until = now_unix() + 3600;
        save(&SnoozedMap::from([(11, until), (22, until), (33, until)]));

        // Un-snoozing shrinks the document; leftover bytes would make the file
        // unparseable and silently drop the remaining snoozes.
        save(&SnoozedMap::from([(11, until)]));

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{{\"11\":{until}}}")
        );
    }

    #[test]
    #[cfg(unix)]
    fn saved_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let Some(path) = isolated("saved_file_is_owner_only") else {
            return;
        };
        write_raw(&path, "{}");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        save(&SnoozedMap::from([(1, now_unix() + 60)]));

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "documented as 0600 for symmetry with the token"
        );
    }

    #[test]
    fn load_and_save_do_nothing_when_the_home_dir_is_unset() {
        let Some(path) = isolated("load_and_save_do_nothing_when_the_home_dir_is_unset") else {
            return;
        };
        // Safe in the isolated child: it is the only thread and the parent's
        // environment is untouched.
        std::env::remove_var(HOME_VAR);

        save(&SnoozedMap::from([(1, now_unix() + 60)]));

        assert!(load_active().is_empty(), "no config dir, nothing to load");
        assert!(!path.exists(), "save must not guess a path");
    }

    // ---- expiry ------------------------------------------------------------

    #[test]
    fn drain_expired_removes_only_past_entries() {
        let mut map: SnoozedMap = HashMap::new();
        map.insert(1, 100);
        map.insert(2, 200);
        map.insert(3, 300);

        let expired = drain_expired(&mut map, 200);

        let mut sorted = expired.clone();
        sorted.sort();
        assert_eq!(sorted, vec![1, 2]);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key(&3));
    }

    #[test]
    fn drain_expired_no_op_when_nothing_due() {
        let mut map: SnoozedMap = HashMap::new();
        map.insert(1, 500);
        map.insert(2, 600);

        let expired = drain_expired(&mut map, 100);

        assert!(expired.is_empty());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn drain_expired_empty_map_returns_empty() {
        let mut map: SnoozedMap = HashMap::new();
        let expired = drain_expired(&mut map, 100);
        assert!(expired.is_empty());
    }
}
