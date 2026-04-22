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
    filterVisible,
    groupByRepo,
    itemKey,
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

  let unlistenPopupHidden: UnlistenFn | null = null;
  let unlistenStateUpdated: UnlistenFn | null = null;
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
      const k = itemKey({ repo: n.repo, kind: n.kind, number: n.number });
      const existing = m.get(k);
      if (existing) existing.push(n);
      else m.set(k, [n]);
    }
    return m;
  });

  type StatePayload = {
    items: WatchedItem[];
    notifications: NotificationItem[];
    loading: boolean;
    last_error: string | null;
    authenticated: boolean;
  };

  function applyState(payload: StatePayload) {
    items = payload.items;
    notifications = payload.notifications;
    loading = payload.loading;
    if (payload.authenticated) {
      // Don't knock the UI out of the device-flow "pending" phase; the
      // worker briefly emits authenticated=false while the token is being
      // stored, and flipping to idle mid-flow would be jarring.
      if (phase !== "pending") {
        phase = "loaded";
      }
    } else if (phase !== "pending") {
      phase = "idle";
    }
    error =
      payload.last_error && payload.last_error !== "not_authenticated"
        ? payload.last_error
        : null;
  }

  // `hidden` is a client-side filter; the worker queries with "all" and the
  // frontend narrows down via `filterVisible`.
  function serverTab(tab: Tab): Tab | "all" {
    return tab === "hidden" ? "all" : tab;
  }

  async function pushBackgroundConfig(
    patch: {
      tab?: Tab;
      intervalMs?: number;
      notifyEnabled?: boolean;
      watchedOrgs?: string[];
      excludedRepos?: string[];
      hiddenItems?: number[];
    } = {},
  ) {
    await invoke("set_background_config", {
      config: {
        tab: patch.tab != null ? serverTab(patch.tab) : undefined,
        intervalMs: patch.intervalMs,
        notifyEnabled: patch.notifyEnabled,
        watchedOrgs: patch.watchedOrgs,
        excludedRepos: patch.excludedRepos,
        hiddenItems: patch.hiddenItems,
      },
    }).catch((e) => console.warn("[eir] set_background_config failed:", e));
  }

  async function pushFullConfig() {
    await pushBackgroundConfig({
      tab: activeTab,
      intervalMs: refreshMs,
      notifyEnabled,
      watchedOrgs: [...watchedOrgs],
      excludedRepos: [...excludedRepos],
      hiddenItems: [...hiddenItems],
    });
  }

  function triggerRefresh() {
    void invoke("trigger_refresh").catch((e) =>
      console.warn("[eir] trigger_refresh failed:", e),
    );
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
      // Clear selection so the $effect picks flatItems[0] on next render.
      selectedId = null;
    }).then((fn) => {
      unlistenPopupHidden = fn;
    });
    void listen<StatePayload>("state-updated", (evt) => {
      applyState(evt.payload);
    }).then((fn) => {
      unlistenStateUpdated = fn;
    });

    // Push persisted config into the worker so its filters match ours
    // before the first emit. A tab/orgs mismatch auto-triggers a refresh.
    await pushFullConfig();

    // Defensive read: if the worker's first emit fired between spawn_worker
    // and our `listen` attaching, we'd miss it — pull the current snapshot.
    try {
      const initial = await invoke<StatePayload>("get_background_state");
      applyState(initial);
    } catch (e) {
      console.warn("[eir] get_background_state failed:", e);
    }

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
    window.removeEventListener("keydown", handleGlobalKey);
    document.removeEventListener("visibilitychange", handleVisibilityChange);
    unlistenPopupHidden?.();
    unlistenPopupHidden = null;
    unlistenStateUpdated?.();
    unlistenStateUpdated = null;
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
    // Opening the popup is an implicit "what's new?" — the worker's interval
    // fires at most every refreshMs, which can be minutes.
    triggerRefresh();
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
        case "ArrowLeft":
          moveTab(-1);
          e.preventDefault();
          return;
        case "ArrowRight":
          moveTab(1);
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
      // `poll_device_flow` triggers a refresh on the Rust side; show the
      // list shell while the first fetch populates it via state-updated.
      phase = "loaded";
    } catch (e) {
      error = String(e);
      phase = "idle";
      deviceCode = null;
      await invoke("set_window_pinned", { pinned: false });
    }
  }

  async function signOut() {
    await invoke("sign_out");
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
    // Optimistic: drop threads locally so the unread dot disappears on
    // click. The next state-updated overwrites this, but only after the
    // mark-as-read calls below land on GitHub, so they stay dropped.
    notifications = notifications.filter((n) => !threadIds.has(n.thread_id));
    await Promise.all(
      [...threadIds].map((threadId) =>
        invoke("mark_notification_read", { threadId }).catch(() => {}),
      ),
    );
    triggerRefresh();
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
    void pushBackgroundConfig({ intervalMs: value });
  }

  function onNotifyChange(enabled: boolean) {
    notifyEnabled = enabled;
    persistNotify(enabled);
    void pushBackgroundConfig({ notifyEnabled: enabled });
  }

  function hideItem(id: number) {
    hiddenItems.add(id);
    persistHiddenItems(hiddenItems);
    void pushBackgroundConfig({ hiddenItems: [...hiddenItems] });
  }

  function unhideItem(id: number) {
    hiddenItems.delete(id);
    persistHiddenItems(hiddenItems);
    void pushBackgroundConfig({ hiddenItems: [...hiddenItems] });
  }

  function addExcludedRepo() {
    const name = newExcludedRepo.trim();
    if (!name || !name.includes("/")) return;
    excludedRepos.add(name);
    persistExcludedRepos(excludedRepos);
    newExcludedRepo = "";
    void pushBackgroundConfig({ excludedRepos: [...excludedRepos] });
  }

  function removeExcludedRepo(repo: string) {
    excludedRepos.delete(repo);
    persistExcludedRepos(excludedRepos);
    void pushBackgroundConfig({ excludedRepos: [...excludedRepos] });
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
    await pushBackgroundConfig({ watchedOrgs: [...watchedOrgs] });
  }

  async function removeWatchedOrg(org: string) {
    watchedOrgs.delete(org);
    persistWatchedOrgs(watchedOrgs);
    await pushBackgroundConfig({ watchedOrgs: [...watchedOrgs] });
  }

  async function switchTab(tab: Tab) {
    if (tab === activeTab) return;
    activeTab = tab;
    persistTab(tab);
    // Clear locally so the old tab's items don't linger until the worker's
    // emit arrives. The worker resets its diff anchors on tab change.
    items = [];
    await pushBackgroundConfig({ tab });
  }

  const SETTINGS_EXPORT_VERSION = 1;

  type SettingsExport = {
    version: number;
    refreshMs?: number;
    notifyEnabled?: boolean;
    theme?: Theme;
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
      theme,
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

    if (
      data.theme === "system" ||
      data.theme === "light" ||
      data.theme === "dark"
    ) {
      onThemeChange(data.theme);
      applied.push("theme");
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

    void pushFullConfig();

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

  function moveTab(delta: number) {
    const currentIdx = TABS.findIndex((t) => t.id === activeTab);
    if (currentIdx < 0) return;
    const nextIdx = (currentIdx + delta + TABS.length) % TABS.length;
    void switchTab(TABS[nextIdx].id);
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
      onRefresh={triggerRefresh}
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
</style>

