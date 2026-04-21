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
  import {
    loadExcludedRepos,
    loadHiddenItems,
    loadInterval,
    loadNotify,
    loadTab,
    loadTheme,
    loadWatchedOrgs,
    persistExcludedRepos,
    persistHiddenItems,
    persistInterval,
    persistNotify,
    persistTab,
    persistTheme,
    persistWatchedOrgs,
    type Theme,
  } from "$lib/storage";
  import type { NotificationItem, Tab, WatchedItem } from "$lib/types";
  import Auth from "$lib/components/Auth.svelte";
  import ItemList from "$lib/components/ItemList.svelte";
  import Settings from "$lib/components/Settings.svelte";

  type DeviceCode = {
    user_code: string;
    verification_uri: string;
    device_code: string;
    interval: number;
    expires_in: number;
  };

  type Phase = "bootstrapping" | "idle" | "pending" | "loaded";

  const THEME_OPTIONS: { value: Theme; label: string }[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
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
  let activeTab = $state<Tab>(loadTab());
  let showingSettings = $state(false);
  let refreshMs = $state<number>(loadInterval());
  let notifyEnabled = $state<boolean>(loadNotify());
  let theme = $state<Theme>(loadTheme());
  let systemDark = $state<boolean>(
    window.matchMedia("(prefers-color-scheme: dark)").matches,
  );
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
  let systemThemeMedia: MediaQueryList | null = null;
  let systemThemeListener: ((e: MediaQueryListEvent) => void) | null = null;

  $effect(() => {
    const resolved =
      theme === "system" ? (systemDark ? "dark" : "light") : theme;
    document.documentElement.setAttribute("data-theme", resolved);
  });

  function onThemeChange(value: Theme) {
    theme = value;
    persistTheme(value);
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
    systemThemeMedia = window.matchMedia("(prefers-color-scheme: dark)");
    systemThemeListener = (e) => {
      systemDark = e.matches;
    };
    systemThemeMedia.addEventListener("change", systemThemeListener);
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
    if (systemThemeMedia && systemThemeListener) {
      systemThemeMedia.removeEventListener("change", systemThemeListener);
    }
    systemThemeMedia = null;
    systemThemeListener = null;
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
    persistInterval(value);
    restartRefreshIfRunning();
  }

  function onNotifyChange(enabled: boolean) {
    notifyEnabled = enabled;
    persistNotify(enabled);
  }

  function hideItem(id: number) {
    hiddenItems.add(id);
    persistHiddenItems(hiddenItems);
    updateBadge();
  }

  function unhideItem(id: number) {
    hiddenItems.delete(id);
    persistHiddenItems(hiddenItems);
    updateBadge();
  }

  function addExcludedRepo() {
    const name = newExcludedRepo.trim();
    if (!name || !name.includes("/")) return;
    excludedRepos.add(name);
    persistExcludedRepos(excludedRepos);
    newExcludedRepo = "";
    updateBadge();
  }

  function removeExcludedRepo(repo: string) {
    excludedRepos.delete(repo);
    persistExcludedRepos(excludedRepos);
    updateBadge();
  }

  async function addWatchedOrg() {
    const raw = newWatchedOrg.trim();
    if (!raw) return;
    // GitHub logins are [A-Za-z0-9-] — keep only safe characters.
    const clean = raw.replace(/[^A-Za-z0-9_-]/g, "");
    if (!clean) return;
    watchedOrgs.add(clean);
    persistWatchedOrgs(watchedOrgs);
    newWatchedOrg = "";
    // Broaden the server-side query immediately by refetching.
    await loadItems({ silent: true });
  }

  async function removeWatchedOrg(org: string) {
    watchedOrgs.delete(org);
    persistWatchedOrgs(watchedOrgs);
    await loadItems({ silent: true });
  }

  async function switchTab(tab: Tab) {
    if (tab === activeTab) return;
    activeTab = tab;
    persistTab(tab);
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
      persistExcludedRepos(excludedRepos);
      applied.push("excluded repos");
    }

    if (Array.isArray(data.watchedOrgs)) {
      const next = data.watchedOrgs.filter(
        (o): o is string => typeof o === "string" && o.length > 0,
      );
      watchedOrgs.clear();
      for (const o of next) watchedOrgs.add(o);
      persistWatchedOrgs(watchedOrgs);
      applied.push("watched orgs");
    }

    if (Array.isArray(data.hiddenItems)) {
      const next = data.hiddenItems.filter(
        (n): n is number => typeof n === "number" && Number.isFinite(n),
      );
      hiddenItems.clear();
      for (const n of next) hiddenItems.add(n);
      persistHiddenItems(hiddenItems);
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
    <Settings
      {refreshMs}
      refreshOptions={REFRESH_OPTIONS}
      {notifyEnabled}
      {theme}
      themeOptions={THEME_OPTIONS}
      {autostartEnabled}
      {appVersion}
      {toggleShortcut}
      {capturingShortcut}
      {shortcutError}
      {updateStatus}
      {watchedOrgs}
      {excludedRepos}
      {orgSuggestions}
      {repoSuggestions}
      {error}
      {settingsIoNotice}
      {settingsIoError}
      bind:newWatchedOrg
      bind:newExcludedRepo
      onBack={() => (showingSettings = false)}
      {onIntervalChange}
      {onNotifyChange}
      {onThemeChange}
      onToggleAutostart={toggleAutostart}
      onSendTestNotification={sendTestNotification}
      onRunUpdateCheck={() => runUpdateCheck({ interactive: true })}
      onInstallUpdate={installPendingUpdate}
      onStartCaptureShortcut={startCaptureShortcut}
      onAddWatchedOrg={addWatchedOrg}
      onRemoveWatchedOrg={removeWatchedOrg}
      onAddExcludedRepo={addExcludedRepo}
      onRemoveExcludedRepo={removeExcludedRepo}
      onExportSettings={exportSettings}
      onImportSettings={importSettings}
    />
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
    <Auth phase="idle" {error} onSignIn={signIn} />
  {:else if phase === "pending" && deviceCode}
    <Auth
      phase="pending"
      deviceCode={deviceCode}
      {copied}
      {error}
      onCopyCode={copyCode}
      onReopenVerification={(url) => openUrl(url)}
    />
  {:else}
    <ItemList
      {loading}
      {activeTab}
      visibleItemsCount={visibleItems.length}
      {visibleUnreadCount}
      {groups}
      {selectedId}
      {notificationsByKey}
      tabs={TABS}
      {error}
      onRefresh={() => loadItems()}
      onMarkAllVisibleAsRead={markAllVisibleAsRead}
      onShowSettings={() => (showingSettings = true)}
      onSignOut={signOut}
      onSwitchTab={switchTab}
      onOpenItem={openItem}
      onHideItem={hideItem}
      onUnhideItem={unhideItem}
    />
  {/if}
</main>

<style>
  :global(:root) {
    font-family: -apple-system, BlinkMacSystemFont, "Inter", system-ui, sans-serif;
    color: var(--fg);
    background: var(--bg);

    --bg: rgba(246, 246, 248, 0.98);
    --fg: #1b1b1f;
    --fg-muted: rgba(27, 27, 31, 0.6);
    --fg-muted-strong: rgba(27, 27, 31, 0.7);
    --fg-subtle: rgba(27, 27, 31, 0.5);

    --border: rgba(0, 0, 0, 0.15);
    --border-subtle: rgba(0, 0, 0, 0.08);
    --border-faint: rgba(0, 0, 0, 0.06);

    --surface-1: rgba(0, 0, 0, 0.04);
    --surface-2: rgba(0, 0, 0, 0.06);
    --surface-2-hover: rgba(0, 0, 0, 0.1);
    --hover-bg: rgba(0, 0, 0, 0.05);

    --scrollbar-thumb: rgba(0, 0, 0, 0.22);
    --scrollbar-thumb-hover: rgba(0, 0, 0, 0.35);

    --accent: #0969da;
    --accent-bg: rgba(9, 105, 218, 0.12);
    --accent-bg-strong: rgba(9, 105, 218, 0.15);
    --accent-bg-hover: rgba(9, 105, 218, 0.18);
    --on-accent: white;

    --success: #1a7f37;
    --success-bg: rgba(26, 127, 55, 0.15);

    --danger: #d1242f;
    --danger-bg: rgba(209, 36, 47, 0.15);
    --danger-bg-faint: rgba(209, 36, 47, 0.12);

    --warning: #9a6700;
    --warning-bg: rgba(154, 103, 0, 0.15);

    --neutral: #57606a;
    --neutral-bg: rgba(87, 96, 106, 0.15);
    --neutral-bg-faint: rgba(87, 96, 106, 0.1);
    --neutral-border: rgba(87, 96, 106, 0.45);
    --neutral-dim: rgba(87, 96, 106, 0.7);
  }

  :global(:root[data-theme="dark"]) {
    --bg: rgba(30, 30, 32, 0.98);
    --fg: #ececef;
    --fg-muted: rgba(236, 236, 239, 0.6);
    --fg-muted-strong: rgba(236, 236, 239, 0.7);
    --fg-subtle: rgba(236, 236, 239, 0.4);

    --border: rgba(255, 255, 255, 0.15);
    --border-subtle: rgba(255, 255, 255, 0.08);
    --border-faint: rgba(255, 255, 255, 0.06);

    --surface-1: rgba(255, 255, 255, 0.05);
    --surface-2: rgba(255, 255, 255, 0.08);
    --surface-2-hover: rgba(255, 255, 255, 0.14);
    --hover-bg: rgba(255, 255, 255, 0.06);

    --scrollbar-thumb: rgba(255, 255, 255, 0.2);
    --scrollbar-thumb-hover: rgba(255, 255, 255, 0.32);

    --accent: #58a6ff;
    --accent-bg: rgba(88, 166, 255, 0.15);
    --accent-bg-strong: rgba(88, 166, 255, 0.18);
    --accent-bg-hover: rgba(88, 166, 255, 0.22);

    --success: #3fb950;
    --success-bg: rgba(46, 160, 67, 0.2);

    --danger: #ff7b72;
    --danger-bg: rgba(248, 81, 73, 0.2);
    --danger-bg-faint: rgba(248, 81, 73, 0.2);

    --warning: #d29922;
    --warning-bg: rgba(187, 128, 9, 0.2);

    --neutral: #8b949e;
    --neutral-bg: rgba(139, 148, 158, 0.2);
    --neutral-bg-faint: rgba(139, 148, 158, 0.12);
    --neutral-border: rgba(139, 148, 158, 0.5);
    --neutral-dim: rgba(139, 148, 158, 0.7);
  }

  :global(body) {
    margin: 0;
    padding: 0;
  }

  :global(.list),
  :global(.settings) {
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
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
    background-color: var(--scrollbar-thumb);
    border-radius: 4px;
    border: 2px solid transparent;
    background-clip: content-box;
  }

  :global(.list::-webkit-scrollbar-thumb:hover),
  :global(.settings::-webkit-scrollbar-thumb:hover) {
    background-color: var(--scrollbar-thumb-hover);
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
    background: var(--accent);
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

  :global(.list.dim) {
    opacity: 0.45;
    transition: opacity 0.15s;
  }

  :global(.toolbar) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  :global(.toolbar .refresh) {
    flex: 1;
  }

  :global(.auth),
  :global(.device),
  :global(.empty) {
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

  :global(.settings) {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  :global(.settings-header) {
    margin-bottom: 12px;
  }

  :global(.back) {
    background: none;
    border: none;
    padding: 4px 0;
    font-size: 12px;
    color: var(--fg-muted);
    cursor: pointer;
  }

  :global(.back:hover) {
    color: var(--accent);
  }

  :global(.settings-body) {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  :global(.setting-row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    gap: 12px;
  }

  :global(.setting-label) {
    color: inherit;
  }

  :global(.setting-row select) {
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
  }

  :global(.setting-row input[type="checkbox"]) {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }

  :global(.setting-hint) {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--fg-muted);
  }

  :global(.setting-hint kbd) {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 10px;
    padding: 1px 4px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--surface-1);
  }

  :global(.shortcut-capture) {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
    cursor: pointer;
  }

  :global(.shortcut-capture.capturing) {
    background: var(--accent-bg-strong);
    border-color: var(--accent);
    color: var(--accent);
  }

  :global(.setting-hint-inline) {
    font-size: 11px;
    color: var(--fg-muted);
    margin-left: 4px;
  }

  :global(.setting-hint-inline.update-available) {
    color: var(--success);
    font-weight: 500;
  }

  :global(.setting-hint-inline.error-inline) {
    color: var(--danger);
  }

  :global(.setting-section) {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--border-faint);
  }

  :global(.setting-buttons) {
    display: flex;
    gap: 8px;
  }

  :global(.setting-hint.io-path) {
    font-family: "SF Mono", Menlo, monospace;
    word-break: break-all;
  }

  :global(.excluded-list) {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  :global(.excluded-list li) {
    display: flex;
    align-items: center;
    padding: 4px 6px;
    font-size: 12px;
    border-radius: 5px;
    background: var(--surface-1);
  }

  :global(.excluded-list .row-action) {
    visibility: visible;
    pointer-events: auto;
    opacity: 0.6;
  }

  :global(.excluded-list li:hover .row-action) {
    opacity: 1;
  }

  :global(.excluded-repo) {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.excluded-add) {
    display: flex;
    gap: 6px;
  }

  :global(.excluded-add input) {
    flex: 1;
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
    min-width: 0;
  }

  :global(.icon-btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 10px;
    border: none;
    border-radius: 6px;
    background: var(--surface-2);
    color: inherit;
    cursor: pointer;
    line-height: 1;
  }

  :global(.icon-btn:hover) {
    background: var(--surface-2-hover);
  }

  :global(.icon-btn svg) {
    width: 16px;
    height: 16px;
    display: block;
  }

  :global(.hint) {
    margin: 0;
    font-size: 13px;
    color: var(--fg-muted-strong);
  }

  :global(.waiting) {
    margin: 0;
    font-size: 12px;
    color: var(--fg-subtle);
  }

  :global(.code) {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 26px;
    letter-spacing: 4px;
    font-weight: 600;
    background: var(--surface-2);
    border: 1px dashed var(--border);
    border-radius: 8px;
    padding: 12px 18px;
    color: inherit;
    cursor: pointer;
  }

  :global(.code:hover) {
    background: var(--surface-2-hover);
  }

  :global(.copy-status) {
    margin: -6px 0 0;
    font-size: 11px;
    color: var(--fg-subtle);
  }

  :global(.copy-status.ok) {
    color: var(--success);
    font-weight: 500;
  }

  :global(button.primary),
  :global(button.secondary),
  :global(button.refresh),
  :global(button.signout) {
    padding: 8px 14px;
    font-size: 13px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
  }

  :global(.primary),
  :global(.refresh) {
    background: var(--accent);
    color: var(--on-accent);
  }

  :global(.secondary) {
    background: var(--surface-2);
    color: inherit;
  }

  :global(.signout) {
    background: var(--surface-2);
    color: inherit;
  }

  :global(.signout:hover) {
    background: var(--danger-bg-faint);
    color: var(--danger);
  }

  :global(.refresh:disabled) {
    opacity: 0.5;
    cursor: default;
  }

  :global(.error) {
    margin: 0;
    font-size: 12px;
    color: var(--danger);
  }

  :global(.tabs) {
    display: flex;
    gap: 4px;
    padding: 0 0 8px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 8px;
  }

  :global(.tab) {
    flex: 1;
    padding: 4px 8px;
    font-size: 11px;
    font-weight: 500;
    border: none;
    border-radius: 5px;
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }

  :global(.tab:hover) {
    background: var(--hover-bg);
  }

  :global(.tab.active) {
    background: var(--accent-bg);
    color: var(--accent);
  }

  :global(.list) {
    flex: 1;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  :global(.group) {
    margin-bottom: 4px;
  }

  :global(.group-header) {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
    background: var(--bg);
    backdrop-filter: blur(8px);
  }

  :global(.group-repo) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.group-count) {
    flex-shrink: 0;
    margin-left: 8px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
    border-radius: 8px;
  }

  :global(.group-items) {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  :global(.item-row) {
    display: flex;
    align-items: stretch;
    gap: 2px;
  }

  :global(.item) {
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

  :global(.row-action) {
    flex-shrink: 0;
    width: 24px;
    padding: 0;
    background: none;
    border: none;
    border-radius: 6px;
    font-size: 14px;
    color: var(--fg-subtle);
    cursor: pointer;
    /* No fade transition: with a transition, a row that loses hover
       briefly overlaps a row that gains it, and several "×" glyphs
       stack up visually as the cursor travels down the list. */
    visibility: hidden;
    pointer-events: none;
  }

  :global(.item-row:hover .row-action) {
    visibility: visible;
    pointer-events: auto;
  }

  :global(.row-action:hover) {
    color: var(--danger);
    background: var(--hover-bg);
  }

  :global(.item:hover) {
    background: var(--hover-bg);
  }

  :global(.item.selected) {
    background: var(--accent-bg);
  }

  :global(.item.selected:hover) {
    background: var(--accent-bg-hover);
  }

  :global(.item.draft .title-text),
  :global(.item.draft .meta),
  :global(.item.draft .badge) {
    opacity: 0.55;
  }

  :global(.draft-label) {
    display: inline-block;
    padding: 0 5px;
    margin-right: 6px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.5px;
    color: var(--neutral);
    background: var(--neutral-bg);
    border: 1px solid var(--neutral-border);
    border-radius: 3px;
    vertical-align: 1px;
  }

  :global(.item.unread .title-text) {
    font-weight: 600;
  }

  :global(.item.unread::before) {
    content: "";
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    margin-top: 6px;
    flex-shrink: 0;
  }

  :global(.item:not(.unread)::before) {
    content: "";
    width: 6px;
    flex-shrink: 0;
  }

  :global(.badge) {
    flex: 0 0 auto;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: 3px;
    background: var(--warning-bg);
    color: var(--warning);
    margin-top: 1px;
  }

  :global(.badge.pr) {
    background: var(--success-bg);
    color: var(--success);
  }

  :global(.body) {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  :global(.title) {
    font-size: 13px;
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  :global(.meta) {
    font-size: 11px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  :global(.avatar) {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--hover-bg);
  }

  :global(.author) {
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 1;
    min-width: 0;
  }

  :global(.sep) {
    flex-shrink: 0;
    opacity: 0.6;
  }

  :global(.comments) {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  :global(.ci) {
    flex-shrink: 0;
    font-weight: 600;
  }

  :global(.ci-success) {
    color: var(--success);
  }

  :global(.ci-failure),
  :global(.ci-error) {
    color: var(--danger);
  }

  :global(.ci-pending) {
    color: var(--warning);
  }

  :global(.reviewers) {
    display: flex;
    gap: 4px;
    margin-top: 4px;
    flex-wrap: wrap;
  }

  :global(.reviewer-chip) {
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

  :global(.reviewer-chip-avatar) {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  :global(.reviewer-chip-name) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  :global(.reviewer-chip-state) {
    flex-shrink: 0;
    opacity: 0.8;
    font-weight: 400;
  }

  :global(.reviewer-approved) {
    background: var(--success-bg);
    color: var(--success);
  }

  :global(.reviewer-changes_requested) {
    background: var(--danger-bg);
    color: var(--danger);
  }

  :global(.reviewer-pending) {
    background: var(--warning-bg);
    color: var(--warning);
  }

  :global(.reviewer-commented) {
    background: var(--neutral-bg);
    color: var(--neutral);
  }

  :global(.reviewer-dismissed) {
    background: var(--neutral-bg-faint);
    color: var(--neutral-dim);
  }

  :global(.reviewer-dismissed .reviewer-chip-name) {
    text-decoration: line-through;
  }

  :global(.commenters) {
    display: flex;
    gap: 4px;
    margin-top: 4px;
    flex-wrap: wrap;
  }

  :global(.commenter-chip) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 2px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 500;
    max-width: 100%;
    min-width: 0;
    background: var(--neutral-bg);
    color: var(--neutral);
  }

  :global(.commenter-chip-avatar) {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  :global(.commenter-chip-name) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
</style>
