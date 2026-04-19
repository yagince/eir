<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    isPermissionGranted,
    requestPermission,
    sendNotification,
  } from "@tauri-apps/plugin-notification";
  import { onDestroy, onMount } from "svelte";

  type WatchedItem = {
    id: number;
    kind: "pr" | "issue";
    title: string;
    number: number;
    repo: string;
    url: string;
    author: string;
    author_avatar: string;
    comments: number;
    updated_at: string;
    state: string;
  };

  type NotificationItem = {
    thread_id: number;
    reason: string;
    repo: string;
    kind: "pr" | "issue" | "commit" | "discussion" | "release" | "other";
    number: number | null;
    title: string;
    url: string;
    updated_at: string;
  };

  type DeviceCode = {
    user_code: string;
    verification_uri: string;
    device_code: string;
    interval: number;
    expires_in: number;
  };

  type Phase = "idle" | "pending" | "loaded";
  type Tab = "all" | "authored" | "review" | "mentions";

  const DEFAULT_REFRESH_MS = 60_000;
  const TAB_KEY = "eir.tab";
  const INTERVAL_KEY = "eir.refreshMs";
  const NOTIFY_KEY = "eir.notifyEnabled";
  const TABS: { id: Tab; label: string }[] = [
    { id: "all", label: "All" },
    { id: "authored", label: "Mine" },
    { id: "review", label: "Review" },
    { id: "mentions", label: "Mentions" },
  ];
  const REFRESH_OPTIONS: { value: number; label: string }[] = [
    { value: 30_000, label: "30 seconds" },
    { value: 60_000, label: "1 minute" },
    { value: 120_000, label: "2 minutes" },
    { value: 300_000, label: "5 minutes" },
  ];

  let phase = $state<Phase>("idle");
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

  let prevThreads = new Map<number, string>();
  let hasLoadedOnce = false;
  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  function loadTabFromStorage(): Tab {
    const raw = localStorage.getItem(TAB_KEY);
    if (raw === "authored" || raw === "review" || raw === "mentions") return raw;
    return "all";
  }

  function loadIntervalFromStorage(): number {
    const raw = localStorage.getItem(INTERVAL_KEY);
    const n = raw ? Number(raw) : NaN;
    return Number.isFinite(n) && n >= 5_000 ? n : DEFAULT_REFRESH_MS;
  }

  function loadNotifyFromStorage(): boolean {
    return localStorage.getItem(NOTIFY_KEY) !== "0";
  }

  function itemKey(i: Pick<WatchedItem, "repo" | "kind" | "number">): string {
    return `${i.repo}:${i.kind}:${i.number}`;
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
    const count = items.length;
    const hasUnread = items.some((i) => notificationsByKey.has(itemKey(i)));
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
    if (!silent) {
      loading = true;
      error = null;
    }
    try {
      const [fetchedItems, fetchedNotifs] = await Promise.all([
        invoke<WatchedItem[]>("fetch_watched", { tab: activeTab }),
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
        console.info(
          `[eir] refresh: ${fetchedNotifs.length} unread notifications, ${fresh.length} new or updated since last fetch`,
        );
        if (fresh.length > 0) {
          await notify(fresh);
        }
      } else {
        console.info(
          `[eir] initial load: ${fetchedNotifs.length} unread notifications (suppressed)`,
        );
      }

      items = fetchedItems;
      notifications = fetchedNotifs;
      prevThreads = nextThreads;
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
      if (!silent) loading = false;
    }
  }

  function resetToIdle() {
    stopRefresh();
    items = [];
    notifications = [];
    prevThreads = new Map();
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
      sendNotification({
        title: reasonLabel(n.reason),
        body: `${suffix} — ${n.title}`,
      });
    }
  }

  async function sendTestNotification() {
    if (!(await ensureNotificationPermission())) {
      error =
        "OS notification permission not granted. Check System Settings → Notifications → eir.";
      return;
    }
    sendNotification({
      title: "eir test notification",
      body: "If you see this, notifications are working.",
    });
  }

  async function ensureNotificationPermission(): Promise<boolean> {
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  }

  onMount(async () => {
    // Kick the permission dialog early so the first real notification isn't
    // also the first time the OS is asked — which silently denies in some
    // cases on macOS dev builds.
    try {
      await ensureNotificationPermission();
    } catch {
      // ignore
    }
    void loadItems({ silent: true });
  });

  onDestroy(stopRefresh);

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

  async function openItem(item: WatchedItem) {
    void openUrl(item.url);
    const matching = notificationsByKey.get(itemKey(item)) ?? [];
    if (matching.length === 0) return;
    const toClear = new Set(matching.map((n) => n.thread_id));
    notifications = notifications.filter((n) => !toClear.has(n.thread_id));
    updateBadge();
    await Promise.all(
      matching.map((n) =>
        invoke("mark_notification_read", { threadId: n.thread_id }).catch(
          () => {},
        ),
      ),
    );
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

  async function switchTab(tab: Tab) {
    if (tab === activeTab) return;
    activeTab = tab;
    localStorage.setItem(TAB_KEY, tab);
    items = [];
    await loadItems();
  }

  type RepoGroup = {
    repo: string;
    items: WatchedItem[];
    mostRecent: string;
    unreadCount: number;
  };

  const groups = $derived.by<RepoGroup[]>(() => {
    const byRepo = new Map<string, WatchedItem[]>();
    for (const item of items) {
      const bucket = byRepo.get(item.repo);
      if (bucket) {
        bucket.push(item);
      } else {
        byRepo.set(item.repo, [item]);
      }
    }
    const result: RepoGroup[] = [];
    for (const [repo, groupItems] of byRepo) {
      groupItems.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      result.push({
        repo,
        items: groupItems,
        mostRecent: groupItems[0].updated_at,
        unreadCount: groupItems.filter((i) => notificationsByKey.has(itemKey(i)))
          .length,
      });
    }
    result.sort((a, b) => b.mostRecent.localeCompare(a.mostRecent));
    return result;
  });

  function relativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const m = Math.floor(diff / 60000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h`;
    const d = Math.floor(h / 24);
    return `${d}d`;
  }
</script>

<main class="container">
  <header>
    <h1>eir</h1>
    <p class="subtitle">GitHub PR / Issue watcher</p>
  </header>

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
        <div class="setting-row">
          <span class="setting-label">Test notification</span>
          <button class="secondary" onclick={sendTestNotification}>Send</button>
        </div>
        <p class="setting-hint">
          Toggle popup: <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd>
        </p>
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
    {#if items.length === 0 && !loading}
      <section class="empty">
        <p>Nothing here.</p>
      </section>
    {:else}
      <ul class="list">
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
                <li>
                  <button
                    class="item"
                    class:unread={notificationsByKey.has(itemKey(item))}
                    onclick={() => openItem(item)}
                  >
                    <span class="badge" class:pr={item.kind === "pr"}>
                      {item.kind === "pr" ? "PR" : "IS"}
                    </span>
                    <span class="body">
                      <span class="title">{item.title}</span>
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
                      </span>
                    </span>
                  </button>
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
    <footer>
      <button class="refresh" onclick={() => loadItems()} disabled={loading}>
        {loading ? "Refreshing…" : "Refresh"}
      </button>
      <button
        class="icon-btn"
        onclick={() => (showingSettings = true)}
        title="Settings"
        aria-label="Settings"
      >
        ⚙
      </button>
      <button class="signout" onclick={signOut}>Sign out</button>
    </footer>
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

  .container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 16px;
    box-sizing: border-box;
  }

  header {
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    padding-bottom: 12px;
    margin-bottom: 12px;
  }

  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: 12px;
    color: rgba(27, 27, 31, 0.6);
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

  .icon-btn {
    padding: 0 10px;
    font-size: 16px;
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
    background: none;
    color: rgba(27, 27, 31, 0.6);
  }

  .signout:hover {
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
    gap: 2px;
  }

  .item {
    width: 100%;
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

  .item:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .item.unread .title {
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

  footer {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid rgba(0, 0, 0, 0.08);
    display: flex;
    gap: 8px;
  }

  .refresh {
    flex: 1;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      color: #ececef;
      background: rgba(30, 30, 32, 0.98);
    }
    header,
    footer,
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
    .subtitle,
    .meta,
    .hint,
    .waiting,
    .signout,
    .group-header,
    .back,
    .setting-hint {
      color: rgba(236, 236, 239, 0.6);
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
  }
</style>
