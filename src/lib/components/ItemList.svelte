<script lang="ts">
  import { itemKey, relativeTime } from "$lib/list";
  import type {
    NotificationItem,
    RepoGroup,
    Tab,
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
    onRefresh: () => void;
    onMarkAllVisibleAsRead: () => void;
    onShowSettings: () => void;
    onSignOut: () => void;
    onSwitchTab: (tab: Tab) => void;
    onOpenItem: (item: WatchedItem) => void;
    onHideItem: (id: number) => void;
    onUnhideItem: (id: number) => void;
    onTogglePin: (id: number) => void;
  };

  const {
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
    onRefresh,
    onMarkAllVisibleAsRead,
    onShowSettings,
    onSignOut,
    onSwitchTab,
    onOpenItem,
    onHideItem,
    onUnhideItem,
    onTogglePin,
  }: Props = $props();
</script>

<header class="toolbar">
  <button class="refresh" onclick={onRefresh} disabled={loading}>
    {loading ? "Refreshing…" : "Refresh"}
  </button>
  {#if visibleUnreadCount > 0}
    <button
      class="icon-btn"
      onclick={onMarkAllVisibleAsRead}
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
    onclick={onShowSettings}
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
{#if visibleItemsCount === 0 && !loading}
  <section class="empty">
    <p>Nothing here.</p>
  </section>
{:else}
  <ul class="list" class:dim={loading}>
    {#each groups as group (group.kind === "pinned" ? "__pinned__" : group.repo)}
      <li class="group" class:pinned={group.kind === "pinned"}>
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
        <ul class="group-items">
          {#each group.items as item (item.id)}
            <li class="item-row" data-item-id={item.id}>
              <button
                class="item"
                class:unread={notificationsByKey.has(itemKey(item))}
                class:selected={item.id === selectedId}
                class:draft={item.is_draft}
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

  .tabs {
    display: flex;
    gap: 4px;
    padding: 0 0 8px;
    border-bottom: 1px solid var(--border-subtle);
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
</style>
