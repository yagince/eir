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
  };

  const {
    loading,
    activeTab,
    visibleItemsCount,
    visibleUnreadCount,
    groups,
    selectedId,
    notificationsByKey,
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
                <button
                  class="row-action"
                  onclick={() => onHideItem(item.id)}
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
