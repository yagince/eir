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
