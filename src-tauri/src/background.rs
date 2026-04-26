use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::Notify;

use crate::auth::{clear_stored_token, AppState};
use crate::diff::{compute_item_changes, fresh_notifications, item_key, removed_items};
use crate::github::{
    build_octocrab, fetch_item_states_with, fetch_notifications_with, fetch_watched_with, ItemRef,
    NotificationItem, WatchedItem,
};

pub const STATE_UPDATED_EVENT: &str = "state-updated";

pub const DEFAULT_INTERVAL_MS: u64 = 60_000;
pub const MIN_INTERVAL_MS: u64 = 5_000;

/// Tab the worker queries against. Matches the frontend's `Tab` type minus
/// `hidden` (that's a client-side filter and the frontend maps it to `All`
/// before pushing). Deserializing into an enum catches typos at the IPC
/// boundary rather than silently falling through to the `All` default.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    #[default]
    All,
    Authored,
    Review,
    Mentions,
}

impl Tab {
    pub fn as_query_key(self) -> &'static str {
        match self {
            Tab::All => "all",
            Tab::Authored => "authored",
            Tab::Review => "review",
            Tab::Mentions => "mentions",
        }
    }
}

#[derive(Default)]
pub struct BackgroundState {
    pub items: Vec<WatchedItem>,
    pub notifications: Vec<NotificationItem>,
    pub loading: bool,
    pub last_error: Option<String>,
    pub authenticated: bool,

    /// Flipped to true the first time the frontend pushes its persisted
    /// config. Until then the worker stays parked — kicking off a cycle
    /// with the default tab/orgs would fetch one set of items, then the
    /// real config arrives and overwrites tab/orgs; the next cycle would
    /// diff against the old snapshot and fire "Closed" notifications for
    /// every item the new filters dropped.
    pub config_applied: bool,

    pub has_loaded_once: bool,
    pub prev_items: HashMap<u64, WatchedItem>,
    pub prev_thread_updated_at: HashMap<u64, String>,

    pub tab: Tab,
    pub watched_orgs: Vec<String>,
    pub excluded_repos: HashSet<String>,
    pub hidden_items: HashSet<u64>,
    pub notify_enabled: bool,
    pub interval_ms: u64,
}

impl BackgroundState {
    pub fn new() -> Self {
        Self {
            notify_enabled: true,
            interval_ms: DEFAULT_INTERVAL_MS,
            ..Default::default()
        }
    }

    /// Drop cached items + diff anchors and mark the session unauthenticated.
    /// Used on sign-out, 401 responses, and the no-token branch of the worker.
    fn reset_session(&mut self, last_error: Option<String>) {
        self.items.clear();
        self.notifications.clear();
        self.prev_items.clear();
        self.prev_thread_updated_at.clear();
        self.has_loaded_once = false;
        self.last_error = last_error;
        self.authenticated = false;
        self.loading = false;
    }
}

#[derive(Clone)]
pub struct BackgroundHandle {
    state: Arc<Mutex<BackgroundState>>,
    wake: Arc<Notify>,
}

impl BackgroundHandle {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BackgroundState::new())),
            wake: Arc::new(Notify::new()),
        }
    }

    pub fn trigger_refresh(&self) {
        self.wake.notify_one();
    }

    fn with_state<R>(&self, f: impl FnOnce(&mut BackgroundState) -> R) -> R {
        let mut s = self.state.lock().expect("background state poisoned");
        f(&mut s)
    }

    /// Clear the worker's cached state and push the empty snapshot. Called
    /// from sign-out so the popup + tray drop back to the signed-out shell
    /// without waiting for the next worker tick.
    pub fn clear_and_notify(&self, app: &AppHandle) {
        self.with_state(|s| s.reset_session(None));
        emit_state(app, self);
        update_tray_badge(app, self);
    }
}

impl Default for BackgroundHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Clone)]
pub struct StatePayload {
    pub items: Vec<WatchedItem>,
    pub notifications: Vec<NotificationItem>,
    pub loading: bool,
    pub last_error: Option<String>,
    pub authenticated: bool,
}

fn snapshot_payload(state: &BackgroundState) -> StatePayload {
    StatePayload {
        items: state.items.clone(),
        notifications: state.notifications.clone(),
        loading: state.loading,
        last_error: state.last_error.clone(),
        authenticated: state.authenticated,
    }
}

fn emit_state(app: &AppHandle, handle: &BackgroundHandle) {
    let payload = handle.with_state(|s| snapshot_payload(s));
    let _ = app.emit(STATE_UPDATED_EVENT, payload);
}

fn update_tray_badge(app: &AppHandle, handle: &BackgroundHandle) {
    let (count, has_unread) = handle.with_state(|s| {
        let notified_keys: HashSet<String> = s
            .notifications
            .iter()
            .filter_map(|n| n.number.map(|num| item_key(&n.repo, n.kind, num)))
            .collect();
        let visible = s
            .items
            .iter()
            .filter(|i| !s.hidden_items.contains(&i.id) && !s.excluded_repos.contains(&i.repo));
        let mut count = 0u32;
        let mut has_unread = false;
        for i in visible {
            count += 1;
            if !has_unread && notified_keys.contains(&item_key(&i.repo, i.kind, i.number)) {
                has_unread = true;
            }
        }
        (count, has_unread)
    });
    crate::tray::set_tray_badge(count, has_unread, app.clone());
}

fn reason_label(reason: &str) -> &'static str {
    match reason {
        "review_requested" => "Review requested",
        "mention" => "You were mentioned",
        "team_mention" => "Your team was mentioned",
        "comment" => "New comment",
        "assign" => "Assigned to you",
        "author" => "Activity on your PR",
        "state_change" => "State changed",
        "ci_activity" => "CI update",
        _ => "New activity",
    }
}

fn send_os_notification(app: &AppHandle, title: &str, body: &str) {
    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        eprintln!("[eir] notification failed: {err}");
    }
}

async fn run_cycle(app: &AppHandle, handle: &BackgroundHandle) {
    // Pull the current token off AppState. No token = sign-in required, and
    // we clear cached state without emitting the "loading" prelude because
    // there's nothing to load.
    let token = {
        let auth = app.state::<Mutex<AppState>>();
        let guard = auth.lock().expect("AppState poisoned");
        guard.token.clone()
    };
    let Some(token) = token else {
        handle.with_state(|s| s.reset_session(None));
        emit_state(app, handle);
        update_tray_badge(app, handle);
        return;
    };

    // Take diff anchors under the lock (move, not clone — they're about to
    // be overwritten anyway) along with the config snapshot. Drop the lock
    // before any await so the state stays contended-free while fetching.
    let (tab, watched_orgs, has_loaded_once, prev_items, prev_threads, notify_enabled) = handle
        .with_state(|s| {
            s.loading = true;
            (
                s.tab,
                s.watched_orgs.clone(),
                s.has_loaded_once,
                std::mem::take(&mut s.prev_items),
                std::mem::take(&mut s.prev_thread_updated_at),
                s.notify_enabled,
            )
        });
    emit_state(app, handle);

    let octo = match build_octocrab(&token) {
        Ok(o) => o,
        Err(err) => {
            handle.with_state(|s| {
                s.last_error = Some(err);
                s.loading = false;
            });
            emit_state(app, handle);
            return;
        }
    };

    // Run both fetches concurrently and emit partial state the moment either
    // returns — the list is what the user stares at, so pushing it as soon as
    // it arrives lets the popup refresh before notifications finish loading.
    // The tray badge and diff-driven OS notifications still wait for both.
    let (items_res, notifs_res) = drive_progressive_fetches(
        fetch_watched_with(&octo, tab.as_query_key(), &watched_orgs),
        fetch_notifications_with(&octo),
        |items| {
            handle.with_state(|s| s.items = items.clone());
            emit_state(app, handle);
        },
        |notifs| {
            handle.with_state(|s| s.notifications = notifs.clone());
            emit_state(app, handle);
        },
    )
    .await;

    // 401 on either call → clear the persisted token and bail.
    let unauthorized = matches!(&items_res, Err(e) if e.is_unauthorized)
        || matches!(&notifs_res, Err(e) if e.is_unauthorized);
    if unauthorized {
        let auth = app.state::<Mutex<AppState>>();
        clear_stored_token(&auth);
        handle.with_state(|s| s.reset_session(Some("not_authenticated".into())));
        emit_state(app, handle);
        update_tray_badge(app, handle);
        return;
    }

    let items = match items_res {
        Ok(v) => v,
        Err(err) => return fail_cycle(app, handle, err.message),
    };
    let notifs = match notifs_res {
        Ok(v) => v,
        Err(err) => return fail_cycle(app, handle, err.message),
    };

    let fresh = fresh_notifications(&prev_threads, &notifs);
    let notified_keys: HashSet<String> = fresh
        .iter()
        .filter_map(|n| n.number.map(|num| item_key(&n.repo, n.kind, num)))
        .collect();
    let item_changes = compute_item_changes(&prev_items, &items, &notified_keys);
    let removed = removed_items(&prev_items, &items, &notified_keys);

    if has_loaded_once && notify_enabled {
        for n in &fresh {
            let suffix = match n.number {
                Some(num) => format!("{}#{}", n.repo, num),
                None => n.repo.clone(),
            };
            let title = reason_label(&n.reason);
            let body = format!("{suffix} — {}", n.title);
            eprintln!("[eir] notify fresh: {title} — {suffix}");
            send_os_notification(app, title, &body);
        }
        for ic in &item_changes {
            let body = format!("{}#{} — {}", ic.item.repo, ic.item.number, ic.item.title);
            eprintln!(
                "[eir] notify item-change: {} — {}#{}",
                ic.reason, ic.item.repo, ic.item.number
            );
            send_os_notification(app, &ic.reason, &body);
        }
        if !removed.is_empty() {
            let refs: Vec<ItemRef> = removed
                .iter()
                .map(|i| ItemRef {
                    repo: i.repo.clone(),
                    kind: i.kind.to_string(),
                    number: i.number,
                })
                .collect();
            // A failed lookup falls back to "Closed" for every item rather
            // than silencing the whole batch — silence is worse than an
            // occasionally-wrong "Closed" vs "Merged" label.
            let states = fetch_item_states_with(&octo, &refs)
                .await
                .unwrap_or_else(|err| {
                    eprintln!("[eir] fetch_item_states failed: {}", err.message);
                    Vec::new()
                });
            let by_key: HashMap<String, &'static str> = states
                .into_iter()
                .map(|s| (item_key(&s.repo, s.kind, s.number), s.state))
                .collect();
            for item in &removed {
                let state = by_key
                    .get(&item_key(&item.repo, item.kind, item.number))
                    .copied()
                    .unwrap_or("closed");
                let title = if state == "merged" {
                    "Merged"
                } else {
                    "Closed"
                };
                let body = format!("{}#{} — {}", item.repo, item.number, item.title);
                eprintln!(
                    "[eir] notify removed: {title} — {}#{}",
                    item.repo, item.number
                );
                send_os_notification(app, title, &body);
            }
        }
    }

    let next_threads: HashMap<u64, String> = notifs
        .iter()
        .map(|n| (n.thread_id, n.updated_at.clone()))
        .collect();
    let next_prev_items: HashMap<u64, WatchedItem> =
        items.iter().map(|i| (i.id, i.clone())).collect();

    handle.with_state(|s| {
        s.items = items;
        s.notifications = notifs;
        s.prev_items = next_prev_items;
        s.prev_thread_updated_at = next_threads;
        s.has_loaded_once = true;
        s.last_error = None;
        s.authenticated = true;
        s.loading = false;
    });
    emit_state(app, handle);
    update_tray_badge(app, handle);
}

/// Drive two fetch futures concurrently and invoke the matching callback as
/// soon as each resolves — without waiting for the other. The callbacks run
/// only on `Ok`; errors are returned to the caller so the standard 401 /
/// fail-cycle branches can handle them. Extracted from `run_cycle` so the
/// progressive ordering behaviour can be unit-tested without a Tauri runtime.
async fn drive_progressive_fetches<I, N, E, FI, FN, OI, ON>(
    items_fut: FI,
    notifs_fut: FN,
    mut on_items: OI,
    mut on_notifs: ON,
) -> (Result<I, E>, Result<N, E>)
where
    FI: Future<Output = Result<I, E>>,
    FN: Future<Output = Result<N, E>>,
    OI: FnMut(&I),
    ON: FnMut(&N),
{
    tokio::pin!(items_fut);
    tokio::pin!(notifs_fut);
    let mut items_res: Option<Result<I, E>> = None;
    let mut notifs_res: Option<Result<N, E>> = None;
    while items_res.is_none() || notifs_res.is_none() {
        tokio::select! {
            r = &mut items_fut, if items_res.is_none() => {
                if let Ok(ref items) = r {
                    on_items(items);
                }
                items_res = Some(r);
            }
            r = &mut notifs_fut, if notifs_res.is_none() => {
                if let Ok(ref notifs) = r {
                    on_notifs(notifs);
                }
                notifs_res = Some(r);
            }
        }
    }
    (
        items_res.expect("items future awaited"),
        notifs_res.expect("notifications future awaited"),
    )
}

fn fail_cycle(app: &AppHandle, handle: &BackgroundHandle, message: String) {
    handle.with_state(|s| {
        s.last_error = Some(message);
        s.loading = false;
    });
    emit_state(app, handle);
}

pub fn spawn_worker(app: AppHandle, handle: BackgroundHandle) {
    // Seed `authenticated` from the persisted token so the frontend can
    // render the list shell while the first fetch is in flight — otherwise
    // `get_background_state` would flash the sign-in view on boot.
    let has_token = app
        .state::<Mutex<AppState>>()
        .lock()
        .expect("AppState poisoned")
        .token
        .is_some();
    handle.with_state(|s| s.authenticated = has_token);

    tauri::async_runtime::spawn(async move {
        // Park until the frontend has pushed its persisted config at
        // least once. See BackgroundState::config_applied for why.
        while !handle.with_state(|s| s.config_applied) {
            handle.wake.notified().await;
        }
        loop {
            run_cycle(&app, &handle).await;
            let interval_ms = handle.with_state(|s| s.interval_ms.max(MIN_INTERVAL_MS));
            tokio::select! {
                _ = handle.wake.notified() => {},
                _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {},
            }
        }
    });
}

#[tauri::command]
pub fn get_background_state(handle: tauri::State<'_, BackgroundHandle>) -> StatePayload {
    handle.with_state(|s| snapshot_payload(s))
}

#[tauri::command]
pub fn trigger_refresh(handle: tauri::State<'_, BackgroundHandle>) {
    handle.trigger_refresh();
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundConfig {
    pub tab: Option<Tab>,
    pub watched_orgs: Option<Vec<String>>,
    pub excluded_repos: Option<Vec<String>>,
    pub hidden_items: Option<Vec<u64>>,
    pub notify_enabled: Option<bool>,
    pub interval_ms: Option<u64>,
}

#[tauri::command]
pub fn set_background_config(
    config: BackgroundConfig,
    handle: tauri::State<'_, BackgroundHandle>,
    app: AppHandle,
) {
    // `trigger` means the item set is about to change (tab / watched_orgs),
    // so we reset the diff anchors and wake the worker. `badge_dirty` is
    // the filter-only path that doesn't need a refetch.
    let (trigger, badge_dirty) = handle.with_state(|s| {
        // First push from the frontend always wakes the worker — the
        // main loop is parked on `config_applied` and the tab/orgs may
        // happen to equal the defaults, which would leave `trigger`
        // false and strand the worker forever.
        let first_apply = !s.config_applied;
        s.config_applied = true;
        let mut trigger = first_apply;
        let mut badge_dirty = false;
        if let Some(tab) = config.tab {
            if tab != s.tab {
                s.tab = tab;
                s.has_loaded_once = false;
                s.prev_items.clear();
                s.prev_thread_updated_at.clear();
                trigger = true;
            }
        }
        if let Some(orgs) = config.watched_orgs {
            if orgs != s.watched_orgs {
                s.watched_orgs = orgs;
                s.has_loaded_once = false;
                s.prev_items.clear();
                s.prev_thread_updated_at.clear();
                trigger = true;
            }
        }
        if let Some(excluded) = config.excluded_repos {
            let next: HashSet<String> = excluded.into_iter().collect();
            if next != s.excluded_repos {
                s.excluded_repos = next;
                badge_dirty = true;
            }
        }
        if let Some(hidden) = config.hidden_items {
            let next: HashSet<u64> = hidden.into_iter().collect();
            if next != s.hidden_items {
                s.hidden_items = next;
                badge_dirty = true;
            }
        }
        if let Some(notify) = config.notify_enabled {
            s.notify_enabled = notify;
        }
        if let Some(ms) = config.interval_ms {
            s.interval_ms = ms.max(MIN_INTERVAL_MS);
        }
        (trigger, badge_dirty)
    });
    if trigger {
        handle.trigger_refresh();
    } else if badge_dirty {
        update_tray_badge(&app, &handle);
    }
}

#[cfg(test)]
mod drive_progressive_fetches_tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::time::Duration;
    use tokio::sync::oneshot;

    // Tests must not hang the suite if the ordering guarantee regresses — the
    // progressive-emit tests deliberately deadlock on a bug, so they need a
    // fuse. 2s is plenty for in-memory futures on any runner.
    const TEST_TIMEOUT: Duration = Duration::from_secs(2);

    #[tokio::test]
    async fn returns_both_ok_results_and_invokes_each_callback_once() {
        let items_seen: Cell<usize> = Cell::new(0);
        let notifs_seen: Cell<usize> = Cell::new(0);

        let fut = drive_progressive_fetches(
            async { Ok::<Vec<u32>, String>(vec![1, 2, 3]) },
            async { Ok::<Vec<&'static str>, String>(vec!["a", "b"]) },
            |items: &Vec<u32>| {
                items_seen.set(items_seen.get() + 1);
                assert_eq!(items, &vec![1u32, 2, 3]);
            },
            |notifs: &Vec<&'static str>| {
                notifs_seen.set(notifs_seen.get() + 1);
                assert_eq!(notifs, &vec!["a", "b"]);
            },
        );

        let (items_res, notifs_res) = tokio::time::timeout(TEST_TIMEOUT, fut)
            .await
            .expect("test timed out");

        assert_eq!(items_res.unwrap(), vec![1u32, 2, 3]);
        assert_eq!(notifs_res.unwrap(), vec!["a", "b"]);
        assert_eq!(items_seen.get(), 1);
        assert_eq!(notifs_seen.get(), 1);
    }

    #[tokio::test]
    async fn errors_skip_callbacks_and_propagate_to_caller() {
        let calls: Cell<usize> = Cell::new(0);

        let fut = drive_progressive_fetches(
            async { Err::<Vec<u32>, String>("items failed".into()) },
            async { Err::<Vec<&'static str>, String>("notifs failed".into()) },
            |_: &Vec<u32>| calls.set(calls.get() + 1),
            |_: &Vec<&'static str>| calls.set(calls.get() + 1),
        );

        let (items_res, notifs_res) = tokio::time::timeout(TEST_TIMEOUT, fut)
            .await
            .expect("test timed out");

        assert_eq!(items_res.unwrap_err(), "items failed");
        assert_eq!(notifs_res.unwrap_err(), "notifs failed");
        assert_eq!(calls.get(), 0);
    }

    #[tokio::test]
    async fn items_ok_and_notifs_err_invokes_only_items_callback() {
        let items_called: Cell<usize> = Cell::new(0);
        let notifs_called: Cell<usize> = Cell::new(0);

        let fut = drive_progressive_fetches(
            async { Ok::<Vec<u32>, String>(vec![42]) },
            async { Err::<Vec<&'static str>, String>("boom".into()) },
            |_: &Vec<u32>| items_called.set(items_called.get() + 1),
            |_: &Vec<&'static str>| notifs_called.set(notifs_called.get() + 1),
        );

        let (items_res, notifs_res) = tokio::time::timeout(TEST_TIMEOUT, fut)
            .await
            .expect("test timed out");

        assert_eq!(items_res.unwrap(), vec![42]);
        assert_eq!(notifs_res.unwrap_err(), "boom");
        assert_eq!(items_called.get(), 1);
        assert_eq!(notifs_called.get(), 0);
    }

    #[tokio::test]
    async fn items_callback_fires_while_notifs_are_still_pending() {
        // The notifs future is gated on a oneshot that the controller only
        // releases AFTER the items callback has signalled. A non-progressive
        // implementation (one that waits for both futures before invoking
        // either callback) deadlocks — the controller waits on `fired_rx`,
        // the driver waits on `notifs_rx`, and TEST_TIMEOUT trips the fuse.
        let (notifs_tx, notifs_rx) = oneshot::channel::<()>();
        let (fired_tx, fired_rx) = oneshot::channel::<()>();
        let fired_slot: RefCell<Option<oneshot::Sender<()>>> = RefCell::new(Some(fired_tx));

        let driver = drive_progressive_fetches(
            async { Ok::<Vec<u32>, String>(vec![7]) },
            async move {
                notifs_rx.await.expect("notifs_tx dropped");
                Ok::<Vec<&'static str>, String>(vec!["done"])
            },
            |_: &Vec<u32>| {
                if let Some(tx) = fired_slot.borrow_mut().take() {
                    tx.send(()).unwrap();
                }
            },
            |_: &Vec<&'static str>| {},
        );
        let controller = async move {
            fired_rx.await.expect("fired_tx dropped");
            notifs_tx.send(()).unwrap();
        };

        let joined = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(driver, controller) })
            .await
            .expect("items callback never fired while notifs were pending");

        let ((items_res, notifs_res), _) = joined;
        assert_eq!(items_res.unwrap(), vec![7]);
        assert_eq!(notifs_res.unwrap(), vec!["done"]);
    }

    #[tokio::test]
    async fn notifs_callback_fires_while_items_are_still_pending() {
        let (items_tx, items_rx) = oneshot::channel::<()>();
        let (fired_tx, fired_rx) = oneshot::channel::<()>();
        let fired_slot: RefCell<Option<oneshot::Sender<()>>> = RefCell::new(Some(fired_tx));

        let driver = drive_progressive_fetches(
            async move {
                items_rx.await.expect("items_tx dropped");
                Ok::<Vec<u32>, String>(vec![9])
            },
            async { Ok::<Vec<&'static str>, String>(vec!["hi"]) },
            |_: &Vec<u32>| {},
            |_: &Vec<&'static str>| {
                if let Some(tx) = fired_slot.borrow_mut().take() {
                    tx.send(()).unwrap();
                }
            },
        );
        let controller = async move {
            fired_rx.await.expect("fired_tx dropped");
            items_tx.send(()).unwrap();
        };

        let joined = tokio::time::timeout(TEST_TIMEOUT, async { tokio::join!(driver, controller) })
            .await
            .expect("notifs callback never fired while items were pending");

        let ((items_res, notifs_res), _) = joined;
        assert_eq!(items_res.unwrap(), vec![9]);
        assert_eq!(notifs_res.unwrap(), vec!["hi"]);
    }
}
