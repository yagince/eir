<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    isPermissionGranted,
    requestPermission,
  } from "@tauri-apps/plugin-notification";
  import { check as checkForUpdate, Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { ask, message, open, save } from "@tauri-apps/plugin-dialog";
  import {
    disable as disableAutostart,
    enable as enableAutostart,
    isEnabled as isAutostartEnabled,
  } from "@tauri-apps/plugin-autostart";
  import { onDestroy, onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import {
    computeItemChanges,
    filterVisible,
    groupByRepo,
    itemKey,
    relativeTime,
    repoSuggestionsFrom,
  } from "$lib/list";
  import type { NotificationItem, Tab, WatchedItem } from "$lib/types";

  type DeviceCode = {
    user_code: string;
    verification_uri: string;
    device_code: string;
    interval: number;
    expires_in: number;
  };

  type Phase = "bootstrapping" | "idle" | "pending" | "loaded";

  const DEFAULT_REFRESH_MS = 60_000;
  const TAB_KEY = "eir.tab";
  const INTERVAL_KEY = "eir.refreshMs";
  const NOTIFY_KEY = "eir.notifyEnabled";
  const EXCLUDED_REPOS_KEY = "eir.excludedRepos";
  const HIDDEN_ITEMS_KEY = "eir.hiddenItems";
  const WATCHED_ORGS_KEY = "eir.watchedOrgs";
  const TABS: { id: Tab; label: string }[] = [
    { id: "all", label: "All" },
    { id: "authored", label: "Mine" },
    { id: "review", label: "Review" },
    { id: "mentions", label: "Mentions" },
    { id: "hidden", label: "Hidden" },
  ];
  const REFRESH_OPTIONS: { value: number; label: string }[] = [
    { value: 30_000, label: "30 seconds" },
    { value: 60_000, label: "1 minute" },
    { value: 120_000, label: "2 minutes" },
    { value: 300_000, label: "5 minutes" },
  ];

  let phase = $state<Phase>("bootstrapping");
  let deviceCode = $state<DeviceCode | null>(null);
  let items = $state<WatchedItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let activeTab = $state<Tab>(loadTabFromStorage());
  let showingSettings = $state(false);
  let refreshMs = $state<number>(loadIntervalFromStorage());
  let notifyEnabled = $state<boolean>(loadNotifyFromStorage());
  let notifications = $state<NotificationItem[]>([]);
  let toggleShortcut = $state<string>("Ctrl+Shift+E");
  let capturingShortcut = $state(false);
  let shortcutError = $state<string | null>(null);
  let selectedId = $state<number | null>(null);

  // The notification plugin's sendNotification() just invokes
  // `new window.Notification(title, options)` under the hood and throws away
  // the Notification instance — its desktop backend never emits the
  // `actionPerformed` event that `onAction` listens for (that wiring only
  // exists on iOS/Android). So to handle clicks we create the Notification
  // ourselves and attach `.onclick` directly.
  function showNotification(title: string, body: string, url?: string) {
    const n = new Notification(title, { body });
    if (url) {
      n.onclick = () => {
        void openUrl(url);
        n.close();
      };
    }
  }

  type UpdateStatus =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "up-to-date" }
    | { kind: "available"; update: Update }
    | { kind: "downloading" }
    | { kind: "installed" }
    | { kind: "error"; message: string };
  let updateStatus = $state<UpdateStatus>({ kind: "idle" });
  let appVersion = $state<string>("");
  let autostartEnabled = $state<boolean | null>(null);
  const excludedRepos = new SvelteSet<string>(loadExcludedRepos());
  const hiddenItems = new SvelteSet<number>(loadHiddenItems());
  const watchedOrgs = new SvelteSet<string>(loadWatchedOrgs());
  let newExcludedRepo = $state("");
  let newWatchedOrg = $state("");
  let settingsIoError = $state<string | null>(null);
  let settingsIoNotice = $state<string | null>(null);

  let prevThreads = new Map<number, string>();
  let prevItems = new Map<number, WatchedItem>();
  let hasLoadedOnce = false;
  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenPopupHidden: UnlistenFn | null = null;

  function loadTabFromStorage(): Tab {
    const raw = localStorage.getItem(TAB_KEY);
    if (
      raw === "authored" ||
      raw === "review" ||
      raw === "mentions" ||
      raw === "hidden"
    ) {
      return raw;
    }
    return "all";
  }

  function loadExcludedRepos(): string[] {
    try {
      const raw = localStorage.getItem(EXCLUDED_REPOS_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch {
      return [];
    }
  }

  function persistExcludedRepos() {
    localStorage.setItem(
      EXCLUDED_REPOS_KEY,
      JSON.stringify([...excludedRepos]),
    );
  }

  function loadHiddenItems(): number[] {
    try {
      const raw = localStorage.getItem(HIDDEN_ITEMS_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch {
      return [];
    }
  }

  function persistHiddenItems() {
    localStorage.setItem(HIDDEN_ITEMS_KEY, JSON.stringify([...hiddenItems]));
  }

  function loadWatchedOrgs(): string[] {
    try {
      const raw = localStorage.getItem(WATCHED_ORGS_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch {
      return [];
    }
  }

  function persistWatchedOrgs() {
    localStorage.setItem(WATCHED_ORGS_KEY, JSON.stringify([...watchedOrgs]));
  }

  function loadIntervalFromStorage(): number {
    const raw = localStorage.getItem(INTERVAL_KEY);
    const n = raw ? Number(raw) : NaN;
    return Number.isFinite(n) && n >= 5_000 ? n : DEFAULT_REFRESH_MS;
  }

  function loadNotifyFromStorage(): boolean {
    return localStorage.getItem(NOTIFY_KEY) !== "0";
  }

  const notificationsByKey = $derived.by(() => {
    const m = new Map<string, NotificationItem[]>();
    for (const n of notifications) {
      if (n.number == null) continue;
      const k = `${n.repo}:${n.kind}:${n.number}`;
      const existing = m.get(k);
      if (existing) existing.push(n);
      else m.set(k, [n]);
    }
    return m;
  });

  function updateBadge() {
    const count = visibleItems.length;
    const hasUnread = visibleItems.some((i) =>
      notificationsByKey.has(itemKey(i)),
    );
    console.info(`[eir] tray badge → count=${count} hasUnread=${hasUnread}`);
    void invoke("set_tray_badge", { count, hasUnread });
  }

  function startRefresh() {
    if (refreshTimer) return;
    refreshTimer = setInterval(() => {
      void loadItems({ silent: true });
    }, refreshMs);
  }

  function restartRefreshIfRunning() {
    if (!refreshTimer) return;
    stopRefresh();
    startRefresh();
  }

  function stopRefresh() {
    if (refreshTimer) {
      clearInterval(refreshTimer);
      refreshTimer = null;
    }
  }

  async function loadItems({ silent = false }: { silent?: boolean } = {}) {
    loading = true;
    if (!silent) {
      error = null;
    }
    try {
      // The "hidden" tab is purely a client-side filter; use the broadest
      // server query so it can surface anything the user has hidden.
      const serverTab = activeTab === "hidden" ? "all" : activeTab;
      const [fetchedItems, fetchedNotifs] = await Promise.all([
        invoke<WatchedItem[]>("fetch_watched", {
          tab: serverTab,
          watchedOrgs: [...watchedOrgs],
        }),
        invoke<NotificationItem[]>("fetch_notifications"),
      ]);

      const nextThreads = new Map(
        fetchedNotifs.map((n) => [n.thread_id, n.updated_at] as const),
      );

      if (hasLoadedOnce) {
        // A thread is "fresh" if we've never seen it before, or if its
        // updated_at has advanced — comments and other activity bump
        // updated_at on the same thread_id.
        const fresh = fetchedNotifs.filter(
          (n) => prevThreads.get(n.thread_id) !== n.updated_at,
        );

        // Item-level diff: catches changes GitHub doesn't generate a
        // notification thread for (another reviewer approved, CI status
        // flipped, PR merged upstream, etc.). Anything already covered by
        // the /notifications fresh set is dropped to avoid double-firing.
        const notifiedKeys = new Set(
          fresh
            .filter((n) => n.number != null)
            .map((n) => `${n.repo}:${n.kind}:${n.number}`),
        );
        const itemChanges = computeItemChanges(
          prevItems,
          fetchedItems,
          notifiedKeys,
        );

        console.info(
          `[eir] refresh: ${fetchedNotifs.length} unread notifications (${fresh.length} new), ${itemChanges.length} item-level changes`,
        );
        // Items in prev but missing from curr disappeared upstream. The
        // search query is `is:open`, so in practice this means the item was
        // closed / merged / archived (or moved out of scope, which is rare
        // enough to tolerate as a false-positive "Closed" ping).
        const currIds = new Set(fetchedItems.map((i) => i.id));
        const removed: WatchedItem[] = [];
        for (const [id, prev] of prevItems) {
          if (currIds.has(id)) continue;
          if (notifiedKeys.has(itemKey(prev))) continue;
          removed.push(prev);
        }

        if (fresh.length > 0) {
          await notify(fresh);
        }
        if (itemChanges.length > 0) {
          await notifyItemChanges(itemChanges);
        }
        if (removed.length > 0) {
          const entries = await resolveRemovedStates(removed);
          await notifyRemoved(entries);
        }
      } else {
        console.info(
          `[eir] initial load: ${fetchedNotifs.length} unread notifications, ${fetchedItems.length} items (notifications suppressed)`,
        );
      }

      items = fetchedItems;
      notifications = fetchedNotifs;
      prevThreads = nextThreads;
      prevItems = new Map(fetchedItems.map((i) => [i.id, i]));
      hasLoadedOnce = true;
      phase = "loaded";
      updateBadge();
      startRefresh();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("not_authenticated")) {
        resetToIdle();
      } else if (!silent) {
        error = msg;
      }
    } finally {
      loading = false;
    }
  }

  function resetToIdle() {
    stopRefresh();
    items = [];
    notifications = [];
    prevThreads = new Map();
    prevItems = new Map();
    hasLoadedOnce = false;
    phase = "idle";
    void invoke("set_tray_badge", { count: 0 });
  }

  function reasonLabel(reason: string): string {
    switch (reason) {
      case "review_requested":
        return "Review requested";
      case "mention":
        return "You were mentioned";
      case "team_mention":
        return "Your team was mentioned";
      case "comment":
        return "New comment";
      case "assign":
        return "Assigned to you";
      case "author":
        return "Activity on your PR";
      case "state_change":
        return "State changed";
      case "ci_activity":
        return "CI update";
      default:
        return "New activity";
    }
  }

  type RemovedEntry = { item: WatchedItem; state: string };

  async function notifyRemoved(entries: RemovedEntry[]) {
    if (!notifyEnabled) return;
    if (!(await ensureNotificationPermission())) return;
    for (const { item, state } of entries) {
      const title = state === "merged" ? "Merged" : "Closed";
      console.info(
        `[eir] sending removal notification: ${title} — ${item.repo}#${item.number}`,
      );
      showNotification(
        title,
        `${item.repo}#${item.number} — ${item.title}`,
        item.url,
      );
    }
  }

  async function resolveRemovedStates(
    removed: WatchedItem[],
  ): Promise<RemovedEntry[]> {
    if (removed.length === 0) return [];
    const refs = removed.map((i) => ({
      repo: i.repo,
      kind: i.kind,
      number: i.number,
    }));
    type StateRow = {
      repo: string;
      kind: "pr" | "issue";
      number: number;
      state: string;
    };
    // A failed lookup (network blip, rate limit, etc.) shouldn't suppress the
    // whole batch — fall back to an empty map so each item ends up labelled
    // "Closed" by default, which is still better than silence.
    const states = await invoke<StateRow[]>("fetch_item_states", {
      items: refs,
    }).catch((err) => {
      console.warn("[eir] fetch_item_states failed:", err);
      return [] as StateRow[];
    });
    const byKey = new Map<string, string>(
      states.map((s) => [`${s.repo}:${s.kind}:${s.number}`, s.state]),
    );
    return removed.map((item) => ({
      item,
      state: byKey.get(itemKey(item)) ?? "closed",
    }));
  }

  async function notifyItemChanges(
    changes: { item: WatchedItem; reason: string }[],
  ) {
    if (!notifyEnabled) return;
    if (!(await ensureNotificationPermission())) return;
    for (const { item, reason } of changes) {
      console.info(
        `[eir] sending item-change notification: ${reason} — ${item.repo}#${item.number}`,
      );
      showNotification(
        reason,
        `${item.repo}#${item.number} — ${item.title}`,
        item.url,
      );
    }
  }

  async function notify(fresh: NotificationItem[]) {
    if (!notifyEnabled) {
      console.info("[eir] notify skipped: notifications disabled in settings");
      return;
    }
    if (!(await ensureNotificationPermission())) {
      console.warn(
        "[eir] notify skipped: OS notification permission not granted",
      );
      return;
    }
    for (const n of fresh) {
      const suffix = n.number != null ? `${n.repo}#${n.number}` : n.repo;
      console.info(
        `[eir] sending notification: ${reasonLabel(n.reason)} — ${suffix}`,
      );
      showNotification(reasonLabel(n.reason), `${suffix} — ${n.title}`, n.url);
    }
  }

  async function toggleAutostart(enabled: boolean) {
    try {
      if (enabled) {
        await enableAutostart();
      } else {
        await disableAutostart();
      }
      autostartEnabled = await isAutostartEnabled();
    } catch (e) {
      console.warn("[eir] autostart toggle failed:", e);
      // Re-read from the system so the UI doesn't drift from reality.
      try {
        autostartEnabled = await isAutostartEnabled();
      } catch {
        autostartEnabled = false;
      }
    }
  }

  async function withPinnedWindow<T>(fn: () => Promise<T>): Promise<T> {
    // Pin the popup (so focus loss doesn't auto-hide it) AND drop always-on-top
    // so the native dialog can actually render above the popup instead of
    // getting buried underneath it.
    await invoke("set_dialog_mode", { enabled: true });
    try {
      return await fn();
    } finally {
      await invoke("set_dialog_mode", { enabled: false });
    }
  }

  async function runUpdateCheck(opts: { interactive: boolean }) {
    if (updateStatus.kind === "checking" || updateStatus.kind === "downloading")
      return;
    updateStatus = { kind: "checking" };
    try {
      const update = await checkForUpdate();
      if (!update) {
        updateStatus = { kind: "up-to-date" };
        console.info("[eir] update check: already latest");
        if (opts.interactive) {
          await withPinnedWindow(() =>
            message(
              `You're on the latest version${appVersion ? ` (v${appVersion})` : ""}.`,
              { title: "eir", kind: "info" },
            ),
          );
        }
        return;
      }
      console.info(
        `[eir] update check: ${update.version} available (current ${update.currentVersion})`,
      );
      updateStatus = { kind: "available", update };
      if (!opts.interactive && notifyEnabled) {
        if (await ensureNotificationPermission()) {
          showNotification(
            "eir update available",
            `Version ${update.version} is ready. Open Settings to install.`,
          );
        }
      }
    } catch (e) {
      const message = String(e);
      console.warn("[eir] update check failed:", message);
      updateStatus = { kind: "error", message };
    }
  }

  async function installPendingUpdate() {
    if (updateStatus.kind !== "available") return;
    const update = updateStatus.update;
    const ok = await withPinnedWindow(() =>
      ask(
        `Install eir v${update.version}?\n\nThe app will relaunch after the update is installed.`,
        { title: "Update available", kind: "info", okLabel: "Install" },
      ),
    );
    if (!ok) return;
    updateStatus = { kind: "downloading" };
    try {
      await update.downloadAndInstall();
      updateStatus = { kind: "installed" };
      await relaunch();
    } catch (e) {
      const message = String(e);
      console.warn("[eir] update install failed:", message);
      updateStatus = { kind: "error", message };
    }
  }

  async function sendTestNotification() {
    error = null;
    if (!(await ensureNotificationPermission())) {
      error =
        "OS notification permission not granted. Check System Settings → Notifications → eir.";
      return;
    }
    showNotification(
      "eir test notification",
      "Click to open the eir repo.",
      "https://github.com/yagince/eir",
    );
  }

  async function ensureNotificationPermission(): Promise<boolean> {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  }

  onMount(async () => {
    window.addEventListener("keydown", handleGlobalKey);
    // Reset list scroll on show, not on hide: a scroll-reset issued while
    // the popup is hidden gets overridden by the webview's scroll
    // restoration when it's shown again.
    document.addEventListener("visibilitychange", handleVisibilityChange);
    // When the popup is hidden (focus loss or tray re-click), Settings is
    // treated as transient: reopening should land back on the list with
    // the first item selected, so a fresh open feels like a fresh glance.
    void listen("popup-hidden", () => {
      showingSettings = false;
      // Clear selection so the $effect picks flatItems[0] on next render;
      // this also snaps keyboard nav back to the top.
      selectedId = null;
    }).then((fn) => {
      unlistenPopupHidden = fn;
    });
    void loadItems({ silent: true });
    // Silent update check on boot — if a new version is out, the Settings
    // button will show "Update available" and the user can choose to install.
    void runUpdateCheck({ interactive: false });

    // Kick the permission dialog early so the first real notification isn't
    // also the first time the OS is asked — which silently denies in some
    // cases on macOS dev builds.
    const [, shortcut, version, autostart] = await Promise.all([
      ensureNotificationPermission().catch(() => false),
      invoke<string>("get_toggle_shortcut").catch(() => null),
      getVersion().catch(() => ""),
      // Sync the autostart toggle with the actual system state — the plugin
      // reads the LaunchAgent plist, so this catches changes made outside
      // the app (e.g. the user disabled "Login Items" in System Settings).
      isAutostartEnabled().catch(() => false),
    ]);
    if (shortcut) toggleShortcut = shortcut;
    appVersion = version;
    autostartEnabled = autostart;
  });

  onDestroy(() => {
    stopRefresh();
    window.removeEventListener("keydown", handleGlobalKey);
    document.removeEventListener("visibilitychange", handleVisibilityChange);
    unlistenPopupHidden?.();
    unlistenPopupHidden = null;
  });

  function handleVisibilityChange() {
    if (document.visibilityState !== "visible") return;
    const list = document.querySelector<HTMLElement>(".list");
    if (list) list.scrollTop = 0;
  }

  function formatShortcut(e: KeyboardEvent): string | null {
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return null;
    const parts: string[] = [];
    if (e.metaKey) parts.push("Cmd");
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");
    // Accept single-character keys (letters/digits/symbols) and F-keys.
    let key: string;
    if (e.key.length === 1) {
      key = e.key.toUpperCase();
    } else if (/^F\d+$/.test(e.key)) {
      key = e.key.toUpperCase();
    } else {
      return null;
    }
    if (parts.length === 0) return null; // require at least one modifier
    parts.push(key);
    return parts.join("+");
  }

  async function handleGlobalKey(e: KeyboardEvent) {
    if (capturingShortcut) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        capturingShortcut = false;
        return;
      }
      const combo = formatShortcut(e);
      if (!combo) return;
      try {
        await invoke("set_toggle_shortcut", { shortcut: combo });
        toggleShortcut = combo;
        shortcutError = null;
      } catch (err) {
        shortcutError = String(err);
      } finally {
        capturingShortcut = false;
      }
      return;
    }

    if (e.key === "Backspace" && showingSettings) {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      e.preventDefault();
      showingSettings = false;
      return;
    }

    // macOS convention: Cmd+, opens the Preferences / Settings pane.
    if (e.metaKey && e.key === "," && !showingSettings && phase === "loaded") {
      e.preventDefault();
      showingSettings = true;
      return;
    }

    // Arrow keys walk the selection, Enter opens, Page/Home/End still scroll.
    if (phase === "loaded" && !showingSettings) {
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      switch (e.key) {
        case "ArrowDown":
          moveSelection(1);
          e.preventDefault();
          return;
        case "ArrowUp":
          moveSelection(-1);
          e.preventDefault();
          return;
        case "Enter":
          openSelected();
          e.preventDefault();
          return;
      }

      const list = document.querySelector<HTMLElement>(".list");
      if (!list) return;
      const page = Math.max(list.clientHeight - 40, 48);
      switch (e.key) {
        case "PageDown":
          list.scrollBy({ top: page });
          e.preventDefault();
          return;
        case "PageUp":
          list.scrollBy({ top: -page });
          e.preventDefault();
          return;
        case "Home":
          list.scrollTop = 0;
          e.preventDefault();
          return;
        case "End":
          list.scrollTop = list.scrollHeight;
          e.preventDefault();
          return;
      }
    }
  }

  function startCaptureShortcut() {
    shortcutError = null;
    capturingShortcut = true;
  }

  async function signIn() {
    error = null;
    await invoke("set_window_pinned", { pinned: true });
    try {
      const code = await invoke<DeviceCode>("start_device_flow");
      deviceCode = code;
      phase = "pending";
      try {
        await navigator.clipboard.writeText(code.user_code);
        copied = true;
        await invoke("set_window_pinned", { pinned: false });
      } catch {
        copied = false;
      }
      await openUrl(code.verification_uri);
      await invoke("poll_device_flow", {
        deviceCode: code.device_code,
        interval: code.interval,
      });
      deviceCode = null;
      copied = false;
      hasLoadedOnce = false;
      await loadItems();
    } catch (e) {
      error = String(e);
      phase = "idle";
      deviceCode = null;
      await invoke("set_window_pinned", { pinned: false });
    }
  }

  async function signOut() {
    await invoke("sign_out");
    resetToIdle();
  }

  async function copyCode() {
    if (!deviceCode) return;
    try {
      await navigator.clipboard.writeText(deviceCode.user_code);
      copied = true;
      await invoke("set_window_pinned", { pinned: false });
    } catch (e) {
      error = `copy failed: ${e}`;
    }
  }

  async function clearNotificationThreads(threadIds: Set<number>) {
    if (threadIds.size === 0) return;
    notifications = notifications.filter((n) => !threadIds.has(n.thread_id));
    updateBadge();
    await Promise.all(
      [...threadIds].map((threadId) =>
        invoke("mark_notification_read", { threadId }).catch(() => {}),
      ),
    );
  }

  async function openItem(item: WatchedItem) {
    void openUrl(item.url);
    const matching = notificationsByKey.get(itemKey(item)) ?? [];
    await clearNotificationThreads(
      new Set(matching.map((n) => n.thread_id)),
    );
  }

  async function markAllVisibleAsRead() {
    const threadIds = new Set<number>();
    for (const item of visibleItems) {
      const matching = notificationsByKey.get(itemKey(item));
      if (!matching) continue;
      for (const n of matching) threadIds.add(n.thread_id);
    }
    if (threadIds.size === 0) return;
    const ok = await withPinnedWindow(() =>
      ask(`Mark ${threadIds.size} notification(s) as read?`, {
        title: "eir",
        kind: "info",
        okLabel: "Mark as read",
      }),
    );
    if (!ok) return;
    await clearNotificationThreads(threadIds);
  }

  function onIntervalChange(value: number) {
    refreshMs = value;
    localStorage.setItem(INTERVAL_KEY, String(value));
    restartRefreshIfRunning();
  }

  function onNotifyChange(enabled: boolean) {
    notifyEnabled = enabled;
    localStorage.setItem(NOTIFY_KEY, enabled ? "1" : "0");
  }

  function hideItem(id: number) {
    hiddenItems.add(id);
    persistHiddenItems();
    updateBadge();
  }

  function unhideItem(id: number) {
    hiddenItems.delete(id);
    persistHiddenItems();
    updateBadge();
  }

  function addExcludedRepo() {
    const name = newExcludedRepo.trim();
    if (!name || !name.includes("/")) return;
    excludedRepos.add(name);
    persistExcludedRepos();
    newExcludedRepo = "";
    updateBadge();
  }

  function removeExcludedRepo(repo: string) {
    excludedRepos.delete(repo);
    persistExcludedRepos();
    updateBadge();
  }

  async function addWatchedOrg() {
    const raw = newWatchedOrg.trim();
    if (!raw) return;
    // GitHub logins are [A-Za-z0-9-] — keep only safe characters.
    const clean = raw.replace(/[^A-Za-z0-9_-]/g, "");
    if (!clean) return;
    watchedOrgs.add(clean);
    persistWatchedOrgs();
    newWatchedOrg = "";
    // Broaden the server-side query immediately by refetching.
    await loadItems({ silent: true });
  }

  async function removeWatchedOrg(org: string) {
    watchedOrgs.delete(org);
    persistWatchedOrgs();
    await loadItems({ silent: true });
  }

  async function switchTab(tab: Tab) {
    if (tab === activeTab) return;
    activeTab = tab;
    localStorage.setItem(TAB_KEY, tab);
    items = [];
    // The diff anchors are per-tab: a different query returns a different
    // set, so disappearing items aren't real closures and new items aren't
    // genuinely new. Reset before refetching.
    prevItems = new Map();
    prevThreads = new Map();
    hasLoadedOnce = false;
    await loadItems();
  }

  const SETTINGS_EXPORT_VERSION = 1;

  type SettingsExport = {
    version: number;
    refreshMs?: number;
    notifyEnabled?: boolean;
    excludedRepos?: string[];
    watchedOrgs?: string[];
    hiddenItems?: number[];
    toggleShortcut?: string;
  };

  function buildSettingsExport(): SettingsExport {
    return {
      version: SETTINGS_EXPORT_VERSION,
      refreshMs,
      notifyEnabled,
      excludedRepos: [...excludedRepos].sort(),
      watchedOrgs: [...watchedOrgs].sort(),
      hiddenItems: [...hiddenItems].sort((a, b) => a - b),
      toggleShortcut,
    };
  }

  async function exportSettings() {
    settingsIoError = null;
    settingsIoNotice = null;
    await withPinnedWindow(async () => {
      try {
        const stamp = new Date().toISOString().slice(0, 10);
        const path = await save({
          title: "Export eir settings",
          defaultPath: `eir-settings-${stamp}.json`,
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!path) return;
        const payload = JSON.stringify(buildSettingsExport(), null, 2);
        const written = await invoke<string>("write_text_file", {
          path,
          contents: payload,
        });
        settingsIoNotice = `Saved to ${written}`;
      } catch (err) {
        settingsIoError = `Export failed: ${err instanceof Error ? err.message : String(err)}`;
      }
    });
  }

  async function importSettings() {
    settingsIoError = null;
    settingsIoNotice = null;
    await withPinnedWindow(async () => {
      try {
        const selected = await open({
          title: "Import eir settings",
          multiple: false,
          directory: false,
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!selected || typeof selected !== "string") return;
        const text = await invoke<string>("read_text_file", { path: selected });
        const parsed = JSON.parse(text) as unknown;
        applyImportedSettings(parsed, selected);
      } catch (err) {
        settingsIoError = `Import failed: ${err instanceof Error ? err.message : String(err)}`;
      }
    });
  }

  function applyImportedSettings(raw: unknown, sourcePath?: string) {
    if (!raw || typeof raw !== "object") {
      throw new Error("not a JSON object");
    }
    const data = raw as Partial<SettingsExport>;
    if (data.version !== SETTINGS_EXPORT_VERSION) {
      throw new Error(
        `unsupported version: ${data.version ?? "missing"} (expected ${SETTINGS_EXPORT_VERSION})`,
      );
    }

    const applied: string[] = [];

    if (typeof data.refreshMs === "number" && data.refreshMs >= 5_000) {
      onIntervalChange(data.refreshMs);
      applied.push("refresh interval");
    }

    if (typeof data.notifyEnabled === "boolean") {
      onNotifyChange(data.notifyEnabled);
      applied.push("notifications");
    }

    if (Array.isArray(data.excludedRepos)) {
      const next = data.excludedRepos.filter(
        (r): r is string => typeof r === "string" && r.includes("/"),
      );
      excludedRepos.clear();
      for (const r of next) excludedRepos.add(r);
      persistExcludedRepos();
      applied.push("excluded repos");
    }

    if (Array.isArray(data.watchedOrgs)) {
      const next = data.watchedOrgs.filter(
        (o): o is string => typeof o === "string" && o.length > 0,
      );
      watchedOrgs.clear();
      for (const o of next) watchedOrgs.add(o);
      persistWatchedOrgs();
      applied.push("watched orgs");
    }

    if (Array.isArray(data.hiddenItems)) {
      const next = data.hiddenItems.filter(
        (n): n is number => typeof n === "number" && Number.isFinite(n),
      );
      hiddenItems.clear();
      for (const n of next) hiddenItems.add(n);
      persistHiddenItems();
      applied.push("hidden items");
    }

    if (typeof data.toggleShortcut === "string" && data.toggleShortcut) {
      void invoke("set_toggle_shortcut", { shortcut: data.toggleShortcut })
        .then(() => {
          toggleShortcut = data.toggleShortcut as string;
        })
        .catch((err) => {
          console.warn("[eir] import: shortcut rejected:", err);
        });
      applied.push("toggle shortcut");
    }

    updateBadge();
    // Re-fetch so excluded-repo/watched-org changes hit the server query.
    void loadItems({ silent: true });

    settingsIoError = null;
    const suffix = sourcePath ? ` from ${sourcePath}` : "";
    settingsIoNotice =
      applied.length > 0
        ? `Imported ${applied.join(", ")}${suffix}.`
        : `Nothing to import${suffix}.`;
  }

  const repoSuggestions = $derived(
    repoSuggestionsFrom(items, excludedRepos),
  );

  const orgSuggestions = $derived.by<string[]>(() => {
    const seen = new Set<string>();
    for (const item of items) {
      const owner = item.repo.split("/")[0];
      if (!owner) continue;
      if (watchedOrgs.has(owner)) continue;
      seen.add(owner);
    }
    return [...seen].sort();
  });

  const visibleItems = $derived(
    filterVisible(items, {
      tab: activeTab,
      excludedRepos,
      hiddenItems,
    }),
  );

  const visibleUnreadCount = $derived(
    visibleItems.filter((i) => notificationsByKey.has(itemKey(i))).length,
  );

  const groups = $derived(
    groupByRepo(visibleItems, (i) => notificationsByKey.has(itemKey(i))),
  );

  // Flat item order matching the rendered list (repo-groups preserved), so
  // ArrowUp/ArrowDown can walk items without tripping over the group headers.
  const flatItems = $derived(groups.flatMap((g) => g.items));

  $effect(() => {
    // Keep selectedId valid when the list changes (refresh, tab switch, etc.).
    if (flatItems.length === 0) {
      selectedId = null;
    } else if (
      selectedId == null ||
      !flatItems.some((i) => i.id === selectedId)
    ) {
      selectedId = flatItems[0].id;
    }
  });

  function moveSelection(delta: number) {
    if (flatItems.length === 0) return;
    const currentIdx =
      selectedId != null
        ? flatItems.findIndex((i) => i.id === selectedId)
        : -1;
    const nextIdx = Math.max(
      0,
      Math.min(flatItems.length - 1, (currentIdx < 0 ? 0 : currentIdx) + delta),
    );
    selectedId = flatItems[nextIdx].id;
    // Schedule scrollIntoView after Svelte re-renders the selected class.
    queueMicrotask(() => {
      const el = document.querySelector<HTMLElement>(
        `[data-item-id="${selectedId}"]`,
      );
      el?.scrollIntoView({ block: "nearest" });
    });
  }

  function openSelected() {
    if (selectedId == null) return;
    const item = flatItems.find((i) => i.id === selectedId);
    if (item) void openItem(item);
  }
</script>

<main class="container">
  <div class="progress-bar" class:visible={loading} aria-hidden="true"></div>
  {#if showingSettings}
    <section class="settings">
      <div class="settings-header">
        <button class="back" onclick={() => (showingSettings = false)}>
          ← Back
        </button>
      </div>
      <div class="settings-body">
        <label class="setting-row">
          <span class="setting-label">Refresh interval</span>
          <select
            value={refreshMs}
            onchange={(e) => onIntervalChange(Number(e.currentTarget.value))}
          >
            {#each REFRESH_OPTIONS as opt (opt.value)}
              <option value={opt.value}>{opt.label}</option>
            {/each}
          </select>
        </label>
        <label class="setting-row">
          <span class="setting-label">Desktop notifications</span>
          <input
            type="checkbox"
            checked={notifyEnabled}
            onchange={(e) => onNotifyChange(e.currentTarget.checked)}
          />
        </label>
        <label class="setting-row">
          <span class="setting-label">Start at login</span>
          <input
            type="checkbox"
            checked={autostartEnabled === true}
            disabled={autostartEnabled === null}
            onchange={(e) => toggleAutostart(e.currentTarget.checked)}
          />
        </label>
        <div class="setting-row">
          <span class="setting-label">Test notification</span>
          <button class="secondary" onclick={sendTestNotification}>Send</button>
        </div>

        <div class="setting-row">
          <span class="setting-label">
            Check for updates
            {#if appVersion}
              <span class="setting-hint-inline">— v{appVersion}</span>
            {/if}
            {#if updateStatus.kind === "up-to-date"}
              <span class="setting-hint-inline">· already latest</span>
            {:else if updateStatus.kind === "available"}
              <span class="setting-hint-inline update-available"
                >· v{updateStatus.update.version} available</span
              >
            {:else if updateStatus.kind === "installed"}
              <span class="setting-hint-inline">· installed, relaunching…</span>
            {:else if updateStatus.kind === "error"}
              <span class="setting-hint-inline error-inline"
                >· {updateStatus.message}</span
              >
            {/if}
          </span>
          <button
            class="secondary"
            disabled={updateStatus.kind === "checking" ||
              updateStatus.kind === "downloading"}
            onclick={() =>
              updateStatus.kind === "available"
                ? installPendingUpdate()
                : runUpdateCheck({ interactive: true })}
          >
            {#if updateStatus.kind === "checking"}
              Checking…
            {:else if updateStatus.kind === "downloading"}
              Installing…
            {:else if updateStatus.kind === "available"}
              Install v{updateStatus.update.version}
            {:else}
              Check
            {/if}
          </button>
        </div>
        <div class="setting-row">
          <span class="setting-label">Toggle popup shortcut</span>
          <button
            class="shortcut-capture"
            class:capturing={capturingShortcut}
            onclick={startCaptureShortcut}
          >
            {#if capturingShortcut}
              Press keys…
            {:else}
              {toggleShortcut}
            {/if}
          </button>
        </div>
        {#if shortcutError}
          <p class="error">{shortcutError}</p>
        {/if}

        <div class="setting-section">
          <span class="setting-label">Watched orgs / users</span>
          {#if watchedOrgs.size === 0}
            <p class="setting-hint">
              Only your personal repos and items you're involved with show up
              in All. Add an org login (e.g. <code>Lecto-inc</code>) to pull in
              all open PRs from that org — catches dependabot PRs in org repos.
            </p>
          {:else}
            <ul class="excluded-list">
              {#each [...watchedOrgs].sort() as org (org)}
                <li>
                  <span class="excluded-repo">{org}</span>
                  <button
                    class="row-action"
                    onclick={() => removeWatchedOrg(org)}
                    aria-label="Remove"
                    title="Remove"
                  >
                    ×
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="excluded-add">
            <input
              type="text"
              list="org-suggestions"
              placeholder="org login (e.g. Lecto-inc)"
              bind:value={newWatchedOrg}
              onkeydown={(e) => e.key === "Enter" && addWatchedOrg()}
            />
            <datalist id="org-suggestions">
              {#each orgSuggestions as org (org)}
                <option value={org}></option>
              {/each}
            </datalist>
            <button class="secondary" onclick={addWatchedOrg}>Add</button>
          </div>
        </div>

        <div class="setting-section">
          <span class="setting-label">Excluded repositories</span>
          {#if excludedRepos.size === 0}
            <p class="setting-hint">None. Items from all repos are shown.</p>
          {:else}
            <ul class="excluded-list">
              {#each [...excludedRepos].sort() as repo (repo)}
                <li>
                  <span class="excluded-repo">{repo}</span>
                  <button
                    class="row-action"
                    onclick={() => removeExcludedRepo(repo)}
                    aria-label="Remove"
                    title="Remove"
                  >
                    ×
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
          <div class="excluded-add">
            <input
              type="text"
              list="repo-suggestions"
              placeholder="owner/repo"
              bind:value={newExcludedRepo}
              onkeydown={(e) => e.key === "Enter" && addExcludedRepo()}
            />
            <datalist id="repo-suggestions">
              {#each repoSuggestions as repo (repo)}
                <option value={repo}></option>
              {/each}
            </datalist>
            <button class="secondary" onclick={addExcludedRepo}>Add</button>
          </div>
        </div>

        <div class="setting-section">
          <span class="setting-label">Backup settings</span>
          <p class="setting-hint">
            Export your configuration (refresh interval, notifications,
            watched orgs, excluded repos, hidden items, shortcut) as a JSON
            file, or import a previously saved file to restore it.
          </p>
          <div class="setting-buttons">
            <button class="secondary" onclick={exportSettings}>Export</button>
            <button class="secondary" onclick={importSettings}>Import</button>
          </div>
          {#if settingsIoNotice}
            <p class="setting-hint io-path" title={settingsIoNotice}>
              {settingsIoNotice}
            </p>
          {/if}
          {#if settingsIoError}
            <p class="error">{settingsIoError}</p>
          {/if}
        </div>

        <p class="setting-hint">
          <kbd>Cmd</kbd>+<kbd>,</kbd> opens settings ·
          <kbd>Backspace</kbd> goes back.
        </p>
        {#if error}
          <p class="error">{error}</p>
        {/if}
      </div>
    </section>
  {:else if phase === "bootstrapping"}
    <section class="auth" aria-busy="true">
      <div class="boot-logo-wrap">
        <svg
          class="boot-wave"
          viewBox="0 0 900 300"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <defs>
            <linearGradient id="boot-wave-gradient" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#cdece5" stop-opacity="0" />
              <stop offset="40%" stop-color="#b8e4da" stop-opacity="0.35" />
              <stop offset="100%" stop-color="#8fd0c4" stop-opacity="0.75" />
            </linearGradient>
          </defs>
          <path
            d="M 0,300 v -150 q 150,-50 300,0 t 300,0 q 150,-50 300,0 v 150 Z"
            fill="url(#boot-wave-gradient)"
          />
        </svg>
      </div>
    </section>
  {:else if phase === "idle"}
    <section class="auth">
      <p class="hint">Sign in to start tracking your PRs and Issues.</p>
      <button class="primary" onclick={signIn}>Sign in with GitHub</button>
      {#if error}
        <p class="error">{error}</p>
      {/if}
    </section>
  {:else if phase === "pending" && deviceCode}
    {@const dc = deviceCode}
    <section class="device">
      <p class="hint">Enter this code on GitHub:</p>
      <button class="code" onclick={copyCode} title="Click to copy">
        {dc.user_code}
      </button>
      <p class="copy-status" class:ok={copied}>
        {copied ? "✓ Copied to clipboard" : "Tap to copy"}
      </p>
      <button class="secondary" onclick={() => openUrl(dc.verification_uri)}>
        Open GitHub again
      </button>
      <p class="waiting">Waiting for authorization…</p>
    </section>
  {:else}
    <header class="toolbar">
      <button class="refresh" onclick={() => loadItems()} disabled={loading}>
        {loading ? "Refreshing…" : "Refresh"}
      </button>
      {#if visibleUnreadCount > 0}
        <button
          class="icon-btn"
          onclick={markAllVisibleAsRead}
          title="Mark {visibleUnreadCount} as read"
          aria-label="Mark all as read"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M18 6 7 17l-5-5" />
            <path d="m22 10-7.5 7.5L13 16" />
          </svg>
        </button>
      {/if}
      <button
        class="icon-btn"
        onclick={() => (showingSettings = true)}
        title="Settings"
        aria-label="Settings"
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path
            d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
          />
          <circle cx="12" cy="12" r="3" />
        </svg>
      </button>
      <button class="signout" onclick={signOut}>Sign out</button>
    </header>
    <nav class="tabs">
      {#each TABS as tab (tab.id)}
        <button
          class="tab"
          class:active={activeTab === tab.id}
          onclick={() => switchTab(tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </nav>
    {#if visibleItems.length === 0 && !loading}
      <section class="empty">
        <p>Nothing here.</p>
      </section>
    {:else}
      <ul class="list" class:dim={loading}>
        {#each groups as group (group.repo)}
          <li class="group">
            <div class="group-header">
              <span class="group-repo">{group.repo}</span>
              {#if group.unreadCount > 0}
                <span class="group-count">{group.unreadCount}</span>
              {/if}
            </div>
            <ul class="group-items">
              {#each group.items as item (item.id)}
                <li class="item-row" data-item-id={item.id}>
                  <button
                    class="item"
                    class:unread={notificationsByKey.has(itemKey(item))}
                    class:selected={item.id === selectedId}
                    class:draft={item.is_draft}
                    onclick={() => openItem(item)}
                  >
                    <span class="badge" class:pr={item.kind === "pr"}>
                      {item.kind === "pr" ? "PR" : "IS"}
                    </span>
                    <span class="body">
                      <span class="title">
                        {#if item.is_draft}
                          <span class="draft-label">DRAFT</span>
                        {/if}
                        <span class="title-text">{item.title}</span>
                      </span>
                      <span class="meta">
                        <img
                          class="avatar"
                          src={item.author_avatar}
                          alt=""
                          loading="lazy"
                        />
                        <span class="author">{item.author}</span>
                        <span class="sep">·</span>
                        <span>#{item.number}</span>
                        <span class="sep">·</span>
                        <span>{relativeTime(item.updated_at)}</span>
                        {#if item.comments > 0}
                          <span class="sep">·</span>
                          <span class="comments" title="Comments">
                            💬 {item.comments}
                          </span>
                        {/if}
                        {#if item.ci_status && item.ci_status !== "unknown"}
                          <span class="sep">·</span>
                          <span
                            class="ci ci-{item.ci_status}"
                            title="CI: {item.ci_status}"
                          >
                            {#if item.ci_status === "success"}✓{:else if item.ci_status === "failure" || item.ci_status === "error"}✗{:else}⏱{/if}
                          </span>
                        {/if}
                      </span>
                      {#if item.reviewers.length > 0}
                        <span class="reviewers">
                          {#each item.reviewers as r (r.login)}
                            <span class="reviewer-chip reviewer-{r.state}">
                              <img
                                class="reviewer-chip-avatar"
                                src={r.avatar_url}
                                alt=""
                                loading="lazy"
                              />
                              <span class="reviewer-chip-name">{r.login}</span>
                              <span class="reviewer-chip-state">
                                {#if r.state === "approved"}
                                  approved
                                {:else if r.state === "changes_requested"}
                                  changes
                                {:else if r.state === "commented"}
                                  commented
                                {:else if r.state === "dismissed"}
                                  dismissed
                                {:else}
                                  not yet
                                {/if}
                              </span>
                            </span>
                          {/each}
                        </span>
                      {/if}
                      {#if item.commenters.length > 0}
                        <span class="commenters">
                          {#each item.commenters as c (c.login)}
                            <span class="commenter-chip" title={c.login}>
                              <img
                                class="commenter-chip-avatar"
                                src={c.avatar_url}
                                alt=""
                                loading="lazy"
                              />
                              <span class="commenter-chip-name">{c.login}</span>
                            </span>
                          {/each}
                        </span>
                      {/if}
                    </span>
                  </button>
                  {#if activeTab === "hidden"}
                    <button
                      class="row-action"
                      onclick={() => unhideItem(item.id)}
                      title="Unhide"
                      aria-label="Unhide"
                    >
                      ↩
                    </button>
                  {:else}
                    <button
                      class="row-action"
                      onclick={() => hideItem(item.id)}
                      title="Hide"
                      aria-label="Hide"
                    >
                      ×
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
  {/if}
</main>

<style>
  :global(:root) {
    font-family: -apple-system, BlinkMacSystemFont, "Inter", system-ui, sans-serif;
    color: #1b1b1f;
    background: rgba(246, 246, 248, 0.98);
  }

  :global(body) {
    margin: 0;
    padding: 0;
  }

  :global(.list),
  :global(.settings) {
    scrollbar-width: thin;
    scrollbar-color: rgba(0, 0, 0, 0.25) transparent;
  }

  :global(.list::-webkit-scrollbar),
  :global(.settings::-webkit-scrollbar) {
    width: 8px;
  }

  :global(.list::-webkit-scrollbar-track),
  :global(.settings::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(.list::-webkit-scrollbar-thumb),
  :global(.settings::-webkit-scrollbar-thumb) {
    background-color: rgba(0, 0, 0, 0.2);
    border-radius: 4px;
    border: 2px solid transparent;
    background-clip: content-box;
  }

  :global(.list::-webkit-scrollbar-thumb:hover),
  :global(.settings::-webkit-scrollbar-thumb:hover) {
    background-color: rgba(0, 0, 0, 0.35);
    background-clip: content-box;
  }

  .container {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 16px;
    box-sizing: border-box;
  }

  .progress-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    overflow: hidden;
    opacity: 0;
    transition: opacity 0.15s;
    pointer-events: none;
    z-index: 2;
  }

  .progress-bar.visible {
    opacity: 1;
  }

  .progress-bar::before {
    content: "";
    position: absolute;
    top: 0;
    height: 100%;
    width: 30%;
    background: #0969da;
    animation: progress-slide 1.1s ease-in-out infinite;
  }

  @keyframes progress-slide {
    0% {
      left: -30%;
    }
    100% {
      left: 100%;
    }
  }

  .list.dim {
    opacity: 0.45;
    transition: opacity 0.15s;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  }

  .toolbar .refresh {
    flex: 1;
  }

  .auth,
  .device,
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    text-align: center;
    gap: 12px;
  }

  .boot-logo-wrap {
    position: relative;
    width: 160px;
    height: 160px;
    overflow: hidden;
    background: url(/icon.png) center / 100% 100% no-repeat;
    -webkit-mask: url(/icon.png) center / 100% 100% no-repeat;
    mask: url(/icon.png) center / 100% 100% no-repeat;
  }

  .boot-wave {
    position: absolute;
    inset: 0;
    width: 300%;
    height: 100%;
    pointer-events: none;
    animation: boot-wave 3s linear infinite;
  }

  @keyframes boot-wave {
    from {
      transform: translateX(0);
    }
    to {
      transform: translateX(-66.66%);
    }
  }

  .settings {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .settings-header {
    margin-bottom: 12px;
  }

  .back {
    background: none;
    border: none;
    padding: 4px 0;
    font-size: 12px;
    color: rgba(27, 27, 31, 0.6);
    cursor: pointer;
  }

  .back:hover {
    color: #0969da;
  }

  .settings-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    gap: 12px;
  }

  .setting-label {
    color: inherit;
  }

  .setting-row select {
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 5px;
    background: white;
    color: inherit;
  }

  .setting-row input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: #0969da;
  }

  .setting-hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: rgba(27, 27, 31, 0.55);
  }

  .setting-hint kbd {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 10px;
    padding: 1px 4px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.04);
  }

  .shortcut-capture {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    padding: 4px 10px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.04);
    color: inherit;
    cursor: pointer;
  }

  .shortcut-capture.capturing {
    background: rgba(9, 105, 218, 0.15);
    border-color: #0969da;
    color: #0969da;
  }

  .setting-hint-inline {
    font-size: 11px;
    color: rgba(27, 27, 31, 0.55);
    margin-left: 4px;
  }

  .setting-hint-inline.update-available {
    color: #1a7f37;
    font-weight: 500;
  }

  .setting-hint-inline.error-inline {
    color: #d1242f;
  }

  .setting-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid rgba(0, 0, 0, 0.06);
  }

  .setting-buttons {
    display: flex;
    gap: 8px;
  }

  .setting-hint.io-path {
    font-family: "SF Mono", Menlo, monospace;
    word-break: break-all;
  }

  .excluded-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .excluded-list li {
    display: flex;
    align-items: center;
    padding: 4px 6px;
    font-size: 12px;
    border-radius: 5px;
    background: rgba(0, 0, 0, 0.04);
  }

  .excluded-list .row-action {
    visibility: visible;
    pointer-events: auto;
    opacity: 0.6;
  }

  .excluded-list li:hover .row-action {
    opacity: 1;
  }

  .excluded-repo {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .excluded-add {
    display: flex;
    gap: 6px;
  }

  .excluded-add input {
    flex: 1;
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid rgba(0, 0, 0, 0.15);
    border-radius: 5px;
    background: white;
    color: inherit;
    min-width: 0;
  }

  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.05);
    color: inherit;
    cursor: pointer;
    line-height: 1;
  }

  .icon-btn:hover {
    background: rgba(0, 0, 0, 0.1);
  }

  .icon-btn svg {
    width: 16px;
    height: 16px;
    display: block;
  }

  .hint {
    margin: 0;
    font-size: 13px;
    color: rgba(27, 27, 31, 0.7);
  }

  .waiting {
    margin: 0;
    font-size: 12px;
    color: rgba(27, 27, 31, 0.5);
  }

  .code {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 26px;
    letter-spacing: 4px;
    font-weight: 600;
    background: rgba(0, 0, 0, 0.05);
    border: 1px dashed rgba(0, 0, 0, 0.2);
    border-radius: 8px;
    padding: 12px 18px;
    color: inherit;
    cursor: pointer;
  }

  .code:hover {
    background: rgba(0, 0, 0, 0.08);
  }

  .copy-status {
    margin: -6px 0 0;
    font-size: 11px;
    color: rgba(27, 27, 31, 0.5);
  }

  .copy-status.ok {
    color: #1a7f37;
    font-weight: 500;
  }

  button.primary,
  button.secondary,
  button.refresh,
  button.signout {
    padding: 8px 14px;
    font-size: 13px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
  }

  .primary,
  .refresh {
    background: #0969da;
    color: white;
  }

  .secondary {
    background: rgba(0, 0, 0, 0.06);
    color: inherit;
  }

  .signout {
    background: rgba(0, 0, 0, 0.05);
    color: inherit;
  }

  .signout:hover {
    background: rgba(209, 36, 47, 0.12);
    color: #d1242f;
  }

  .refresh:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    margin: 0;
    font-size: 12px;
    color: #d1242f;
  }

  .tabs {
    display: flex;
    gap: 4px;
    padding: 0 0 8px;
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    margin-bottom: 8px;
  }

  .tab {
    flex: 1;
    padding: 4px 8px;
    font-size: 11px;
    font-weight: 500;
    border: none;
    border-radius: 5px;
    background: none;
    color: rgba(27, 27, 31, 0.6);
    cursor: pointer;
  }

  .tab:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .tab.active {
    background: rgba(9, 105, 218, 0.12);
    color: #0969da;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .group {
    margin-bottom: 4px;
  }

  .group-header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 4px;
    font-size: 11px;
    font-weight: 600;
    color: rgba(27, 27, 31, 0.55);
    background: rgba(246, 246, 248, 0.98);
    backdrop-filter: blur(8px);
  }

  .group-repo {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-count {
    flex-shrink: 0;
    margin-left: 8px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 600;
    color: white;
    background: #0969da;
    border-radius: 8px;
  }

  .group-items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .item-row {
    display: flex;
    align-items: stretch;
    gap: 2px;
  }

  .item {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px;
    background: none;
    border: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }

  .row-action {
    flex-shrink: 0;
    width: 24px;
    padding: 0;
    background: none;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    color: rgba(27, 27, 31, 0.5);
    cursor: pointer;
    /* No fade transition: with a transition, a row that loses hover
       briefly overlaps a row that gains it, and several "×" glyphs
       stack up visually as the cursor travels down the list. */
    visibility: hidden;
    pointer-events: none;
  }

  .item-row:hover .row-action {
    visibility: visible;
    pointer-events: auto;
  }

  .row-action:hover {
    color: #d1242f;
    background: rgba(0, 0, 0, 0.05);
  }

  .item:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .item.selected {
    background: rgba(9, 105, 218, 0.12);
  }

  .item.selected:hover {
    background: rgba(9, 105, 218, 0.18);
  }

  .item.draft .title-text,
  .item.draft .meta,
  .item.draft .badge {
    opacity: 0.55;
  }

  .draft-label {
    display: inline-block;
    padding: 0 5px;
    margin-right: 6px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: #57606a;
    background: rgba(87, 96, 106, 0.15);
    border: 1px solid rgba(87, 96, 106, 0.45);
    border-radius: 3px;
    vertical-align: 1px;
  }

  .item.unread .title-text {
    font-weight: 600;
  }

  .item.unread::before {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #0969da;
    margin-top: 6px;
    flex-shrink: 0;
  }

  .item:not(.unread)::before {
    content: "";
    width: 6px;
    flex-shrink: 0;
  }

  .badge {
    flex: 0 0 auto;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: 3px;
    background: rgba(154, 103, 0, 0.15);
    color: #9a6700;
    margin-top: 1px;
  }

  .badge.pr {
    background: rgba(26, 127, 55, 0.15);
    color: #1a7f37;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .title {
    font-size: 13px;
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .meta {
    font-size: 11px;
    color: rgba(27, 27, 31, 0.55);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .avatar {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
    background: rgba(0, 0, 0, 0.05);
  }

  .author {
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
    min-width: 0;
  }

  .sep {
    flex-shrink: 0;
    opacity: 0.6;
  }

  .comments {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  .ci {
    flex-shrink: 0;
    font-weight: 600;
  }

  .ci-success {
    color: #1a7f37;
  }

  .ci-failure,
  .ci-error {
    color: #d1242f;
  }

  .ci-pending {
    color: #9a6700;
  }

  .reviewers {
    display: flex;
    gap: 4px;
    margin-top: 4px;
    flex-wrap: wrap;
  }

  .reviewer-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 500;
    max-width: 100%;
    min-width: 0;
  }

  .reviewer-chip-avatar {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .reviewer-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .reviewer-chip-state {
    flex-shrink: 0;
    opacity: 0.8;
    font-weight: 400;
  }

  .reviewer-approved {
    background: rgba(26, 127, 55, 0.15);
    color: #1a7f37;
  }

  .reviewer-changes_requested {
    background: rgba(209, 36, 47, 0.15);
    color: #d1242f;
  }

  .reviewer-pending {
    background: rgba(154, 103, 0, 0.15);
    color: #9a6700;
  }

  .reviewer-commented {
    background: rgba(87, 96, 106, 0.15);
    color: #57606a;
  }

  .reviewer-dismissed {
    background: rgba(87, 96, 106, 0.1);
    color: rgba(87, 96, 106, 0.7);
  }

  .reviewer-dismissed .reviewer-chip-name {
    text-decoration: line-through;
  }

  .commenters {
    display: flex;
    gap: 4px;
    margin-top: 4px;
    flex-wrap: wrap;
  }

  .commenter-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 500;
    max-width: 100%;
    min-width: 0;
    background: rgba(87, 96, 106, 0.15);
    color: #57606a;
  }

  .commenter-chip-avatar {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .commenter-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      color: #ececef;
      background: rgba(30, 30, 32, 0.98);
    }
    .toolbar,
    .tabs {
      border-color: rgba(255, 255, 255, 0.08);
    }
    .tab {
      color: rgba(236, 236, 239, 0.6);
    }
    .tab:hover {
      background: rgba(255, 255, 255, 0.06);
    }
    .tab.active {
      background: rgba(9, 105, 218, 0.2);
      color: #58a6ff;
    }
    .meta,
    .hint,
    .waiting,
    .group-header,
    .back,
    .setting-hint,
    .setting-hint-inline {
      color: rgba(236, 236, 239, 0.6);
    }
    .signout {
      background: rgba(255, 255, 255, 0.08);
      color: inherit;
    }
    .signout:hover {
      background: rgba(248, 81, 73, 0.2);
      color: #ff7b72;
    }
    .setting-hint-inline.update-available {
      color: #3fb950;
    }
    .setting-hint-inline.error-inline {
      color: #ff7b72;
    }
    .group-header {
      background: rgba(30, 30, 32, 0.98);
    }
    .code {
      background: rgba(255, 255, 255, 0.05);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .code:hover {
      background: rgba(255, 255, 255, 0.08);
    }
    .secondary {
      background: rgba(255, 255, 255, 0.08);
    }
    .item:hover {
      background: rgba(255, 255, 255, 0.06);
    }
    .setting-row select {
      background: rgba(255, 255, 255, 0.05);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .setting-hint kbd {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .icon-btn {
      background: rgba(255, 255, 255, 0.08);
    }
    .icon-btn:hover {
      background: rgba(255, 255, 255, 0.14);
    }
    .reviewer-approved {
      background: rgba(46, 160, 67, 0.2);
      color: #3fb950;
    }
    .reviewer-changes_requested {
      background: rgba(248, 81, 73, 0.2);
      color: #ff7b72;
    }
    .reviewer-pending {
      background: rgba(187, 128, 9, 0.2);
      color: #d29922;
    }
    .reviewer-commented {
      background: rgba(139, 148, 158, 0.2);
      color: #8b949e;
    }
    .reviewer-dismissed {
      background: rgba(139, 148, 158, 0.12);
      color: rgba(139, 148, 158, 0.7);
    }
    .commenter-chip {
      background: rgba(139, 148, 158, 0.2);
      color: #8b949e;
    }
    .row-action {
      color: rgba(236, 236, 239, 0.4);
    }
    .row-action:hover {
      background: rgba(255, 255, 255, 0.08);
    }
    .setting-section {
      border-top-color: rgba(255, 255, 255, 0.06);
    }
    .excluded-list li {
      background: rgba(255, 255, 255, 0.05);
    }
    .excluded-add input {
      background: rgba(255, 255, 255, 0.05);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .shortcut-capture {
      background: rgba(255, 255, 255, 0.06);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .shortcut-capture.capturing {
      background: rgba(88, 166, 255, 0.18);
      border-color: #58a6ff;
      color: #58a6ff;
    }
    .item.selected {
      background: rgba(88, 166, 255, 0.15);
    }
    .item.selected:hover {
      background: rgba(88, 166, 255, 0.22);
    }
    .draft-label {
      color: #8b949e;
      background: rgba(139, 148, 158, 0.18);
      border-color: rgba(139, 148, 158, 0.5);
    }
    :global(.list),
    :global(.settings) {
      scrollbar-color: rgba(255, 255, 255, 0.2) transparent;
    }
    :global(.list::-webkit-scrollbar-thumb),
    :global(.settings::-webkit-scrollbar-thumb) {
      background-color: rgba(255, 255, 255, 0.18);
    }
    :global(.list::-webkit-scrollbar-thumb:hover),
    :global(.settings::-webkit-scrollbar-thumb:hover) {
      background-color: rgba(255, 255, 255, 0.32);
    }
  }
</style>
