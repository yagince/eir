<script lang="ts">
  import { flattenCommentBody, itemKey, relativeTime } from "$lib/list";
  import type {
    NotificationItem,
    RepoGroup,
    Tab,
    ViewMode,
    WatchedItem,
  } from "$lib/types";

  type Props = {
    loading: boolean;
    activeTab: Tab;
    visibleItemsCount: number;
    visibleUnreadCount: number;
    groups: RepoGroup[];
    selectedId: number | null;
    notificationsByKey: Map<string, NotificationItem[]>;
    pinnedItems: ReadonlySet<number>;
    tabs: { id: Tab; label: string }[];
    error: string | null;
    searchQuery: string;
    searchVisible: boolean;
    unreadOnly: boolean;
    viewMode: ViewMode;
    showLatestComment: boolean;
    /// Item-id → unix epoch seconds the snooze expires at. Sourced from the
    /// Rust worker; the component only reads, mutations go through the
    /// onSnoozeItem / onUnsnoozeItem callbacks below.
    snoozedUntil: Record<number, number>;
    /// Current wall-clock in unix seconds, refreshed by the host so the
    /// countdown labels can re-render without each row owning a timer.
    nowSec: number;
    /// Item id whose Snooze menu should be open, owned by the host so a
    /// keyboard shortcut on the page can open the menu for the currently-
    /// selected row. Pass `null` to close.
    snoozeMenuOpenId: number | null;
    onRefresh: () => void;
    onMarkAllVisibleAsRead: () => void;
    onShowSettings: () => void;
    onSignOut: () => void;
    onSwitchTab: (tab: Tab) => void;
    onOpenItem: (item: WatchedItem) => void;
    onHideItem: (id: number) => void;
    onUnhideItem: (id: number) => void;
    onTogglePin: (id: number) => void;
    onSnoozeItem: (id: number, untilSec: number) => void;
    onUnsnoozeItem: (id: number) => void;
    onClearSearch: () => void;
    onCloseSearch: () => void;
    onToggleUnreadOnly: () => void;
    onSetViewMode: (mode: ViewMode) => void;
  };

  let {
    loading,
    activeTab,
    visibleItemsCount,
    visibleUnreadCount,
    groups,
    selectedId,
    notificationsByKey,
    pinnedItems,
    tabs,
    error,
    searchQuery = $bindable(),
    searchVisible,
    unreadOnly,
    viewMode,
    showLatestComment,
    snoozedUntil,
    nowSec,
    snoozeMenuOpenId = $bindable(),
    onRefresh,
    onMarkAllVisibleAsRead,
    onShowSettings,
    onSignOut,
    onSwitchTab,
    onOpenItem,
    onHideItem,
    onUnhideItem,
    onTogglePin,
    onSnoozeItem,
    onUnsnoozeItem,
    onClearSearch,
    onCloseSearch,
    onToggleUnreadOnly,
    onSetViewMode,
  }: Props = $props();

  // `snoozeMenuOpenId` is bindable from the host so a keyboard shortcut
  // can drive it. Custom-mode fields stay local since they're only
  // meaningful while a menu is open; a $effect below resets them whenever
  // the host closes the menu and seeds defaults when one opens.
  let customMode = $state(false);
  // Date stays as `<input type="date">` (browser handles locale fine).
  // Time is split into two `<input type="number">` fields because macOS
  // WebKit ignores the `lang` attribute on `<input type="time">` and
  // always renders the locale's 12h/24h widget — so a US-locale user
  // ends up with an "AM/PM" picker no matter what. Two number fields
  // give us a deterministic 24h interface that doesn't depend on the
  // user's OS settings.
  let customDate = $state("");
  let customHourStr = $state("");
  let customMinuteStr = $state("");
  let lastMenuId = $state<number | null>(null);
  /// Reference to the currently-rendered Snooze menu so the focus-management
  /// effect can query it for focusable children. Only one menu is open at a
  /// time, so a single ref is enough.
  let menuRef = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (snoozeMenuOpenId !== lastMenuId) {
      if (snoozeMenuOpenId != null) {
        customMode = false;
        seedCustomDefaults();
      } else {
        customMode = false;
        customDate = "";
        customHourStr = "";
        customMinuteStr = "";
      }
      lastMenuId = snoozeMenuOpenId ?? null;
    }
  });

  /// Action that focuses the first usable child of the menu when it
  /// mounts, and again when `customMode` flips. A `use:` action is more
  /// reliable here than a `$effect`: actions fire after the node and its
  /// children are fully attached to the DOM, whereas an effect that reads
  /// `menuRef` may run before `bind:this` has wired it up, leaving the
  /// `s` shortcut with no focus transfer.
  function focusMenuFirst(node: HTMLDivElement, mode: boolean) {
    let current = mode;
    function focusInner() {
      queueMicrotask(() => {
        if (!node.isConnected) return;
        const target = current
          ? node.querySelector<HTMLElement>("input")
          : node.querySelector<HTMLButtonElement>("button:not(:disabled)");
        target?.focus();
      });
    }
    focusInner();
    return {
      update(newMode: boolean) {
        current = newMode;
        focusInner();
      },
    };
  }

  /// Bubble-phase keydown handler on the menu container. Stops Enter and
  /// arrow keys from reaching the page-level shortcut handler (which would
  /// otherwise fire `openSelected` on Enter, or move the list selection on
  /// arrows). Arrow keys walk between preset buttons; the native time/date
  /// inputs handle their own arrow-key spinning.
  function handleMenuKey(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.stopPropagation();
      return;
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.stopPropagation();
      // Date/time inputs use arrows to change values; don't hijack.
      if (e.target instanceof HTMLInputElement) return;
      e.preventDefault();
      moveMenuFocus(e.key === "ArrowDown" ? 1 : -1);
    }
  }

  function moveMenuFocus(delta: 1 | -1) {
    if (!menuRef) return;
    const buttons = Array.from(
      menuRef.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
    );
    if (buttons.length === 0) return;
    const active = document.activeElement;
    const idx =
      active instanceof HTMLButtonElement ? buttons.indexOf(active) : -1;
    const next =
      idx === -1 ? 0 : (idx + delta + buttons.length) % buttons.length;
    buttons[next].focus();
  }

  function closeSnoozeMenu() {
    snoozeMenuOpenId = null;
  }

  function toggleSnoozeMenu(id: number) {
    if (snoozeMenuOpenId === id) {
      closeSnoozeMenu();
    } else {
      snoozeMenuOpenId = id;
    }
  }

  /// Seed both pickers with "1 hour from now" so opening custom mode lets
  /// the user nudge a sensible default rather than type from scratch.
  function seedCustomDefaults() {
    const d = new Date(Date.now() + 60 * 60 * 1000);
    const pad = (n: number) => n.toString().padStart(2, "0");
    customDate = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
    customHourStr = pad(d.getHours());
    customMinuteStr = pad(d.getMinutes());
  }

  /// Today's date as `YYYY-MM-DD`, used as the `min` on the date input so
  /// the native picker greys out past calendar days.
  function todayLocalIso(): string {
    const d = new Date();
    const pad = (n: number) => n.toString().padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  /// Combine date + hour + minute into unix seconds. Returns `null` for
  /// blank, out-of-range, or already-past values so the apply button can
  /// disable itself instead of silently no-opping on click. Hour and
  /// minute come in as strings so an empty input doesn't coerce to
  /// `NaN` mid-edit.
  function customUntilSec(): number | null {
    if (!customDate) return null;
    if (!customHourStr || !customMinuteStr) return null;
    const h = Number.parseInt(customHourStr, 10);
    const m = Number.parseInt(customMinuteStr, 10);
    if (!Number.isFinite(h) || h < 0 || h > 23) return null;
    if (!Number.isFinite(m) || m < 0 || m > 59) return null;
    const pad = (n: number) => n.toString().padStart(2, "0");
    const t = new Date(`${customDate}T${pad(h)}:${pad(m)}`).getTime();
    if (!Number.isFinite(t)) return null;
    const sec = Math.floor(t / 1000);
    if (sec <= Math.floor(Date.now() / 1000)) return null;
    return sec;
  }

  /// Next occurrence of 8 AM local time, used by the "Tomorrow 8 AM" preset.
  /// If it's currently before 8 AM the same calendar day's 8 AM wins; this
  /// matches what users expect when snoozing late at night.
  function nextMorningEightSec(): number {
    const d = new Date();
    const target = new Date(
      d.getFullYear(),
      d.getMonth(),
      d.getDate(),
      8,
      0,
      0,
      0,
    );
    if (target.getTime() <= d.getTime()) {
      target.setDate(target.getDate() + 1);
    }
    return Math.floor(target.getTime() / 1000);
  }

  function pickPreset(id: number, addSeconds: number) {
    const target = Math.floor(Date.now() / 1000) + addSeconds;
    onSnoozeItem(id, target);
    closeSnoozeMenu();
  }

  function pickMorning(id: number) {
    onSnoozeItem(id, nextMorningEightSec());
    closeSnoozeMenu();
  }

  function applyCustom(id: number) {
    const sec = customUntilSec();
    if (sec == null) return;
    onSnoozeItem(id, sec);
    closeSnoozeMenu();
  }

  function clearSnooze(id: number) {
    onUnsnoozeItem(id);
    closeSnoozeMenu();
  }

  /// Short, human-friendly remaining-time label for a snoozed row. We round
  /// down so a row labelled "1h 0m" doesn't disappear into the past while
  /// reading; the actual expiry is the Rust worker's responsibility.
  function snoozeLabel(untilSec: number, now: number): string {
    const delta = Math.max(0, untilSec - now);
    if (delta < 60) return `${delta}s`;
    const minutes = Math.floor(delta / 60);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const remMinutes = minutes % 60;
    if (hours < 24) {
      return remMinutes > 0 ? `${hours}h ${remMinutes}m` : `${hours}h`;
    }
    const days = Math.floor(hours / 24);
    const remHours = hours % 24;
    return remHours > 0 ? `${days}d ${remHours}h` : `${days}d`;
  }

  // Close the Snooze menu on Escape or a click outside its wrapper. Using a
  // global listener keeps the menu lifecycle out of every row's hover state
  // — opening one and clicking anywhere else just dismisses it.
  $effect(() => {
    if (snoozeMenuOpenId == null) return;
    function onDocMouseDown(e: MouseEvent) {
      const target = e.target as Element | null;
      if (target?.closest(".snooze-wrap")) return;
      closeSnoozeMenu();
    }
    function onDocKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        closeSnoozeMenu();
        e.stopPropagation();
      }
    }
    document.addEventListener("mousedown", onDocMouseDown);
    document.addEventListener("keydown", onDocKey, true);
    return () => {
      document.removeEventListener("mousedown", onDocMouseDown);
      document.removeEventListener("keydown", onDocKey, true);
    };
  });

  const VIEW_MODES: { id: ViewMode; label: string; title: string }[] = [
    { id: "grouped", label: "Repo", title: "Group by repository" },
    { id: "recent", label: "Recent", title: "Sort by most recent activity" },
  ];

  function onSearchKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (searchQuery !== "") {
        onClearSearch();
      } else {
        onCloseSearch();
      }
      e.stopPropagation();
      e.preventDefault();
    }
  }
</script>

<header class="toolbar">
  <button
    class="refresh"
    onclick={onRefresh}
    disabled={loading}
    title="Refresh (⌘R)"
  >
    {loading ? "Refreshing…" : "Refresh"}
  </button>
  {#if visibleUnreadCount > 0}
    <button
      class="icon-btn"
      onclick={onMarkAllVisibleAsRead}
      title="Mark {visibleUnreadCount} as read (⌘⇧A)"
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
    onclick={onShowSettings}
    title="Settings (⌘,)"
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
  <button class="signout" onclick={onSignOut}>Sign out</button>
</header>
<nav class="tabs">
  {#each tabs as tab (tab.id)}
    <button
      class="tab"
      class:active={activeTab === tab.id}
      onclick={() => onSwitchTab(tab.id)}
    >
      {tab.label}
    </button>
  {/each}
</nav>
<div class="filters">
  <button
    class="filter-chip"
    class:active={unreadOnly}
    onclick={onToggleUnreadOnly}
    aria-pressed={unreadOnly}
    title={unreadOnly ? "Showing unread only — click or press U to show all" : "Show only unread items (U)"}
  >
    <span class="filter-dot" aria-hidden="true"></span>
    <span class="filter-label">Unread only</span>
    {#if unreadOnly}
      <span class="filter-x" aria-hidden="true">×</span>
    {/if}
  </button>
  <div
    class="view-toggle"
    role="group"
    aria-label="View mode"
    title="Toggle list view (R)"
  >
    {#each VIEW_MODES as mode (mode.id)}
      <button
        class="view-toggle-btn"
        class:active={viewMode === mode.id}
        onclick={() => onSetViewMode(mode.id)}
        aria-pressed={viewMode === mode.id}
        title={mode.title}
        aria-label={mode.title}
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
          {#if mode.id === "grouped"}
            <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
          {:else}
            <circle cx="12" cy="12" r="9" />
            <path d="M12 7v5l3 2" />
          {/if}
        </svg>
        <span>{mode.label}</span>
      </button>
    {/each}
  </div>
</div>
{#if searchVisible || searchQuery !== ""}
  <div class="search" class:active={searchQuery !== ""}>
    <svg
      class="search-icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7" />
      <path d="m21 21-4.3-4.3" />
    </svg>
    <input
      class="search-input"
      type="text"
      placeholder="Search title, repo, author, #number · Esc to close"
      aria-label="Search"
      bind:value={searchQuery}
      onkeydown={onSearchKeyDown}
    />
    {#if searchQuery !== ""}
      <button
        class="search-clear"
        onclick={onClearSearch}
        title="Clear search"
        aria-label="Clear search"
      >
        ×
      </button>
    {/if}
  </div>
{/if}
{#if visibleItemsCount === 0 && !loading}
  <section class="empty">
    {#if searchQuery !== ""}
      <p>No matches for "{searchQuery}".</p>
    {:else if unreadOnly}
      <p>No unread items.</p>
    {:else}
      <p>Nothing here.</p>
    {/if}
  </section>
{:else}
  <ul class="list" class:dim={loading}>
    {#each groups as group (group.kind ?? group.repo)}
      <li
        class="group"
        class:pinned={group.kind === "pinned"}
        class:flat={group.kind === "flat"}
      >
        {#if group.kind !== "flat"}
          <div class="group-header" class:pinned-header={group.kind === "pinned"}>
            <span class="group-repo">
              {#if group.kind === "pinned"}
                <svg
                  class="group-pin-icon"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  aria-hidden="true"
                >
                  <path
                    d="M16 4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v3.8l-2.4 2.4A2 2 0 0 0 5 11.6V13h6v8l1 1 1-1v-8h6v-1.4a2 2 0 0 0-.6-1.4L16 7.8z"
                  />
                </svg>
                Pinned
              {:else}
                {group.repo}
              {/if}
            </span>
            {#if group.unreadCount > 0}
              <span class="group-count">{group.unreadCount}</span>
            {/if}
          </div>
        {/if}
        <ul class="group-items">
          {#each group.items as item (item.id)}
            <li class="item-row" data-item-id={item.id}>
              <button
                class="item"
                class:unread={notificationsByKey.has(itemKey(item))}
                class:selected={item.id === selectedId}
                class:draft={item.is_draft}
                class:snoozed={snoozedUntil[item.id] != null}
                onclick={() => onOpenItem(item)}
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
                    {#if viewMode === "recent"}
                      <span class="repo-chip" title={item.repo}>{item.repo}</span>
                      <span class="sep">·</span>
                    {/if}
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
                    {#if snoozedUntil[item.id] != null}
                      <span class="sep">·</span>
                      <span
                        class="snooze-chip"
                        title={"Snoozed until " +
                          new Date(snoozedUntil[item.id] * 1000).toLocaleString(
                            undefined,
                            { hour12: false },
                          )}
                      >
                        💤 {snoozeLabel(snoozedUntil[item.id], nowSec)}
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
                  {#if showLatestComment && notificationsByKey.has(itemKey(item)) && item.latest_comment}
                    {@const flat = flattenCommentBody(
                      item.latest_comment.body_text,
                    )}
                    {#if flat.length > 0}
                      <span
                        class="latest-comment"
                        title={item.latest_comment.body_text}
                      >
                        <span class="latest-comment-clamp">
                          <span class="latest-comment-author"
                            >@{item.latest_comment.author}</span
                          >
                          {flat}
                        </span>
                      </span>
                    {/if}
                  {/if}
                </span>
              </button>
              {#if activeTab === "hidden"}
                <button
                  class="row-action"
                  onclick={() => onUnhideItem(item.id)}
                  title="Unhide"
                  aria-label="Unhide"
                >
                  ↩
                </button>
              {:else}
                <div class="row-actions">
                  <button
                    class="row-action pin-btn"
                    class:pinned={pinnedItems.has(item.id)}
                    onclick={() => onTogglePin(item.id)}
                    title={pinnedItems.has(item.id) ? "Unpin" : "Pin"}
                    aria-label={pinnedItems.has(item.id) ? "Unpin" : "Pin"}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      fill={pinnedItems.has(item.id)
                        ? "currentColor"
                        : "none"}
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      aria-hidden="true"
                    >
                      <path
                        d="M16 4a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v3.8l-2.4 2.4A2 2 0 0 0 5 11.6V13h6v8l1 1 1-1v-8h6v-1.4a2 2 0 0 0-.6-1.4L16 7.8z"
                      />
                    </svg>
                  </button>
                  <div class="snooze-wrap">
                    <button
                      class="row-action snooze-btn"
                      class:active={snoozedUntil[item.id] != null}
                      onclick={() => toggleSnoozeMenu(item.id)}
                      title={snoozedUntil[item.id] != null ? "Edit snooze" : "Snooze"}
                      aria-label={snoozedUntil[item.id] != null ? "Edit snooze" : "Snooze"}
                      aria-haspopup="menu"
                      aria-expanded={snoozeMenuOpenId === item.id}
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
                        <circle cx="12" cy="13" r="8" />
                        <path d="M12 9v4l2.5 2" />
                        <path d="M5 3 2 6" />
                        <path d="m22 6-3-3" />
                      </svg>
                    </button>
                    {#if snoozeMenuOpenId === item.id}
                      <div
                        bind:this={menuRef}
                        use:focusMenuFirst={customMode}
                        class="snooze-menu"
                        class:custom={customMode}
                        role="menu"
                        aria-label="Snooze options"
                        tabindex="-1"
                        onkeydown={handleMenuKey}
                      >
                        {#if !customMode}
                          <button
                            class="snooze-option"
                            role="menuitem"
                            onclick={() => pickPreset(item.id, 30 * 60)}
                          >
                            30 minutes
                          </button>
                          <button
                            class="snooze-option"
                            role="menuitem"
                            onclick={() => pickPreset(item.id, 60 * 60)}
                          >
                            1 hour
                          </button>
                          <button
                            class="snooze-option"
                            role="menuitem"
                            onclick={() => pickMorning(item.id)}
                          >
                            Tomorrow 8 AM
                          </button>
                          <button
                            class="snooze-option"
                            role="menuitem"
                            onclick={() => (customMode = true)}
                          >
                            Pick date &amp; time…
                          </button>
                          {#if snoozedUntil[item.id] != null}
                            <button
                              class="snooze-option danger"
                              role="menuitem"
                              onclick={() => clearSnooze(item.id)}
                            >
                              Clear snooze
                            </button>
                          {/if}
                        {:else}
                          <div class="snooze-custom-header">
                            Wake at
                          </div>
                          <div class="snooze-custom-fields">
                            <label class="snooze-custom-field">
                              <span>Date</span>
                              <input
                                type="date"
                                class="snooze-custom-input"
                                min={todayLocalIso()}
                                bind:value={customDate}
                              />
                            </label>
                            <label class="snooze-custom-field">
                              <span>Time (24h)</span>
                              <div class="snooze-time-pair">
                                <input
                                  type="number"
                                  class="snooze-custom-input snooze-time-input"
                                  min="0"
                                  max="23"
                                  step="1"
                                  inputmode="numeric"
                                  aria-label="Hour"
                                  bind:value={customHourStr}
                                />
                                <span class="snooze-time-sep">:</span>
                                <input
                                  type="number"
                                  class="snooze-custom-input snooze-time-input"
                                  min="0"
                                  max="59"
                                  step="1"
                                  inputmode="numeric"
                                  aria-label="Minute"
                                  bind:value={customMinuteStr}
                                />
                              </div>
                            </label>
                          </div>
                          {#if customUntilSec() == null && customDate && customHourStr && customMinuteStr}
                            <div class="snooze-custom-error">
                              Pick a future time.
                            </div>
                          {/if}
                          <div class="snooze-custom-actions">
                            <button
                              class="snooze-option subtle"
                              onclick={() => (customMode = false)}
                            >
                              Back
                            </button>
                            <button
                              class="snooze-option primary"
                              disabled={customUntilSec() == null}
                              onclick={() => applyCustom(item.id)}
                            >
                              Snooze
                            </button>
                          </div>
                        {/if}
                      </div>
                    {/if}
                  </div>
                  <button
                    class="row-action"
                    onclick={() => onHideItem(item.id)}
                    title="Hide"
                    aria-label="Hide"
                  >
                    ×
                  </button>
                </div>
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

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .toolbar .refresh {
    flex: 1;
  }

  .icon-btn {
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

  .icon-btn:hover {
    background: var(--surface-2-hover);
  }

  .icon-btn svg {
    width: 16px;
    height: 16px;
    display: block;
  }

  .filters {
    display: flex;
    gap: 6px;
    padding: 0 0 8px;
    margin-bottom: 4px;
  }

  .filter-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 500;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
    line-height: 1.5;
  }

  .filter-chip:hover {
    background: var(--hover-bg);
  }

  .filter-chip.active {
    border-color: var(--accent);
    background: var(--accent-bg);
    color: var(--accent);
  }

  .filter-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--fg-muted);
    flex-shrink: 0;
  }

  .filter-chip.active .filter-dot {
    background: var(--accent);
  }

  .filter-x {
    font-size: 13px;
    line-height: 1;
    opacity: 0.8;
  }

  .view-toggle {
    display: inline-flex;
    margin-left: auto;
    border: 1px solid var(--border-subtle);
    border-radius: 999px;
    overflow: hidden;
    background: none;
  }

  .view-toggle-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    font-size: 11px;
    font-weight: 500;
    line-height: 1.5;
    border: none;
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }

  .view-toggle-btn:hover {
    background: var(--hover-bg);
  }

  .view-toggle-btn.active {
    background: var(--accent-bg);
    color: var(--accent);
  }

  .view-toggle-btn svg {
    width: 11px;
    height: 11px;
    display: block;
  }

  .repo-chip {
    flex-shrink: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
    color: var(--fg);
    opacity: 0.85;
  }

  .tabs {
    display: flex;
    gap: 4px;
    padding: 0 0 8px;
    border-bottom: 1px solid var(--border-subtle);
    margin-bottom: 8px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    margin-bottom: 6px;
    background: var(--surface-2);
    border: 1px solid transparent;
    border-radius: 6px;
    transition: border-color 0.1s;
  }

  .search:focus-within,
  .search.active {
    border-color: var(--accent);
  }

  .search-icon {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--fg-muted);
  }

  .search-input {
    flex: 1;
    min-width: 0;
    padding: 0;
    border: none;
    outline: none;
    background: transparent;
    color: inherit;
    font-size: 12px;
    line-height: 1.6;
  }

  .search-input::placeholder {
    color: var(--fg-muted);
  }

  .search-clear {
    flex-shrink: 0;
    padding: 0 4px;
    font-size: 14px;
    line-height: 1;
    background: none;
    border: none;
    color: var(--fg-muted);
    cursor: pointer;
    border-radius: 3px;
  }

  .search-clear:hover {
    background: var(--hover-bg);
    color: inherit;
  }

  .tab {
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

  .tab:hover {
    background: var(--hover-bg);
  }

  .tab.active {
    background: var(--accent-bg);
    color: var(--accent);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list.dim {
    opacity: 0.45;
    transition: opacity 0.15s;
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
    color: var(--fg-muted);
    background: var(--bg);
    backdrop-filter: blur(8px);
  }

  .pinned-header {
    color: var(--accent);
  }

  .group-repo {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .group-pin-icon {
    width: 11px;
    height: 11px;
    flex-shrink: 0;
  }

  .group-count {
    flex-shrink: 0;
    margin-left: 8px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
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
    /* scrollIntoView doesn't know about the sticky .group-header, so without
       this margin ArrowUp to the top item leaves it tucked under the header. */
    scroll-margin-top: 28px;
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

  /* No fade transition: with a transition, a row that loses hover
     briefly overlaps a row that gains it, and several "×" glyphs
     stack up visually as the cursor travels down the list. */
  .row-action {
    visibility: hidden;
    pointer-events: none;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .row-action:hover {
    background: var(--hover-bg);
  }

  .item-row:hover .row-action {
    visibility: visible;
    pointer-events: auto;
  }

  .row-actions {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
  }

  .pin-btn svg {
    width: 12px;
    height: 12px;
    display: block;
  }

  /* A pinned item keeps its pin button visible even when the row isn't
     hovered — it's an "on/off" state indicator, not just an action. */
  .pin-btn.pinned {
    visibility: visible;
    pointer-events: auto;
    color: var(--accent);
  }

  .item:hover {
    background: var(--hover-bg);
  }

  .item.selected {
    background: var(--accent-bg);
  }

  .item.selected:hover {
    background: var(--accent-bg-hover);
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
    color: var(--neutral);
    background: var(--neutral-bg);
    border: 1px solid var(--neutral-border);
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
    background: var(--accent);
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
    background: var(--warning-bg);
    color: var(--warning);
    margin-top: 1px;
  }

  .badge.pr {
    background: var(--success-bg);
    color: var(--success);
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
    color: var(--fg-muted);
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
    background: var(--hover-bg);
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
    color: var(--success);
  }

  .ci-failure,
  .ci-error {
    color: var(--danger);
  }

  .ci-pending {
    color: var(--warning);
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
    background: var(--success-bg);
    color: var(--success);
  }

  .reviewer-changes_requested {
    background: var(--danger-bg);
    color: var(--danger);
  }

  .reviewer-pending {
    background: var(--warning-bg);
    color: var(--warning);
  }

  .reviewer-commented {
    background: var(--neutral-bg);
    color: var(--neutral);
  }

  .reviewer-dismissed {
    background: var(--neutral-bg-faint);
    color: var(--neutral-dim);
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
    background: var(--neutral-bg);
    color: var(--neutral);
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

  /* Quote-like preview of the latest unread comment. Only shown for items
     that still have an active notification thread, so the noise scales with
     the unread count rather than the entire list. Two lines are clamped via
     `-webkit-line-clamp` so the line count is bounded but the body wraps
     naturally instead of cutting mid-word. */
  /* Outer chrome: padding/border/background only. Padding+overflow on the
     same element as `-webkit-line-clamp` lets the third line bleed into the
     padding-bottom region (overflow:hidden clips at the padding-box, not the
     content-box), which is exactly the artefact users were seeing. */
  .latest-comment {
    display: block;
    margin-top: 6px;
    padding: 6px 8px 7px;
    border-left: 2px solid rgba(120, 130, 150, 0.45);
    background: rgba(120, 130, 150, 0.08);
    border-radius: 0 4px 4px 0;
    font-size: 0.78rem;
    line-height: 1.55;
    color: var(--muted, #aaa);
    word-break: break-word;
    overflow: hidden;
  }

  /* Inner clamp container: no padding, so its content-box and padding-box
     coincide. `max-height = 3 line boxes` and `overflow: hidden` together
     guarantee a hard 3-line cut even when the webkit clamp itself rounds
     fractionally. */
  .latest-comment-clamp {
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    max-height: calc(1.55em * 3);
  }

  .latest-comment-author {
    font-weight: 600;
    margin-right: 4px;
    opacity: 0.95;
  }

  /* Snoozed rows are still visible — just muted. The unread dot is already
     suppressed via notificationsByKey on the host side, so all we need
     here is to soften the text/badge so the eye skips past it until the
     timer fires. */
  .item.snoozed .title-text,
  .item.snoozed .meta,
  .item.snoozed .badge {
    opacity: 0.55;
  }

  .snooze-chip {
    flex-shrink: 0;
    padding: 0 4px;
    border-radius: 6px;
    background: var(--neutral-bg);
    color: var(--neutral);
    font-variant-numeric: tabular-nums;
  }

  /* Wrapper anchors the absolutely-positioned menu to the snooze button so
     dropping it open in the rightmost column doesn't reflow the row. */
  .snooze-wrap {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .snooze-btn svg {
    width: 12px;
    height: 12px;
    display: block;
  }

  .snooze-btn.active {
    visibility: visible;
    pointer-events: auto;
    color: var(--accent);
  }

  /* Menu hangs to the LEFT of the button so it doesn't run off the popup's
     right edge. The popup window is fixed at 440px wide; a menu opening
     rightward would be clipped. */
  .snooze-menu {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    min-width: 160px;
    padding: 4px;
    border: 1px solid var(--border-subtle);
    border-radius: 6px;
    background: var(--bg);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
    z-index: 10;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  /* Custom mode opens the menu wider so the date/time picker controls
     don't have to shrink — the native calendar dropdown that hangs off
     `<input type="date">` is already as small as WebKit allows. */
  .snooze-menu.custom {
    min-width: 260px;
    padding: 8px;
    gap: 6px;
  }

  .snooze-option {
    padding: 7px 10px;
    font-size: 12px;
    text-align: left;
    background: none;
    border: none;
    border-radius: 4px;
    color: inherit;
    cursor: pointer;
  }

  .snooze-option:hover {
    background: var(--hover-bg);
  }

  .snooze-option:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .snooze-option.danger {
    color: var(--danger);
    border-top: 1px solid var(--border-subtle);
    margin-top: 2px;
    padding-top: 6px;
  }

  .snooze-option.danger:hover {
    background: var(--danger-bg);
  }

  .snooze-option.primary {
    background: var(--accent-bg);
    color: var(--accent);
    font-weight: 600;
    text-align: center;
  }

  .snooze-option.primary:hover:not(:disabled) {
    background: var(--accent-bg-hover, var(--accent-bg));
  }

  .snooze-option.subtle {
    color: var(--fg-muted);
    text-align: center;
  }

  .snooze-custom-header {
    padding: 2px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .snooze-custom-fields {
    display: flex;
    gap: 8px;
  }

  .snooze-custom-field {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 10px;
    font-weight: 500;
    color: var(--fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  /* Bigger than the preset rows: this is the field the user is actively
     editing, and the native date/time controls render their internal
     spin/picker chrome at the input's font-size, so undersized fields
     make the popup widget tiny. */
  .snooze-custom-input {
    padding: 7px 8px;
    font-size: 13px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    background: var(--surface-2);
    color: inherit;
    font-family: inherit;
    width: 100%;
    box-sizing: border-box;
  }

  .snooze-custom-input:focus {
    outline: none;
    border-color: var(--accent);
  }

  /* Two-field 24h time picker. Each input is centred and roomy enough
     for two digits; the separator is purely visual. Keyboard up/down on
     a number input cycles its value, so we hide the native spinner
     chrome (it's tiny and ugly on WebKit) to keep the boxes clean. */
  .snooze-time-pair {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .snooze-time-input {
    flex: 1;
    min-width: 0;
    text-align: center;
    font-variant-numeric: tabular-nums;
    -moz-appearance: textfield;
    appearance: textfield;
  }

  .snooze-time-input::-webkit-inner-spin-button,
  .snooze-time-input::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }

  .snooze-time-sep {
    font-size: 14px;
    font-weight: 600;
    color: var(--fg-muted);
    flex-shrink: 0;
  }

  .snooze-custom-error {
    padding: 2px 4px;
    font-size: 11px;
    color: var(--danger);
  }

  .snooze-custom-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .snooze-custom-actions .snooze-option {
    flex: 1;
  }
</style>
