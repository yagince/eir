// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import ItemList from "$lib/components/ItemList.svelte";
import type { NotificationItem, RepoGroup, Tab, ViewMode, WatchedItem } from "$lib/types";

const TABS: { id: Tab; label: string }[] = [
  { id: "all", label: "All" },
  { id: "authored", label: "Mine" },
  { id: "review", label: "Review" },
  { id: "mentions", label: "Mentions" },
  { id: "hidden", label: "Hidden" },
];

function item(overrides: Partial<WatchedItem> = {}): WatchedItem {
  return {
    id: 1,
    kind: "pr",
    title: "Fix the tray badge on Windows",
    number: 42,
    repo: "yagince/eir",
    url: "https://github.com/yagince/eir/pull/42",
    author: "yagince",
    author_avatar: "",
    comments: 0,
    updated_at: "2026-08-11T00:00:00Z",
    state: "open",
    is_draft: false,
    reviewers: [],
    commenters: [],
    ci_status: null,
    latest_comment: null,
    ...overrides,
  };
}

function group(overrides: Partial<RepoGroup> = {}): RepoGroup {
  const items = overrides.items ?? [item()];
  return {
    repo: "yagince/eir",
    items,
    mostRecent: items[0]?.updated_at ?? "2026-08-11T00:00:00Z",
    unreadCount: 0,
    ...overrides,
  };
}

function handlers() {
  return {
    onRefresh: vi.fn(),
    onMarkAllVisibleAsRead: vi.fn(),
    onShowSettings: vi.fn(),
    onSignOut: vi.fn(),
    onSwitchTab: vi.fn(),
    onOpenItem: vi.fn(),
    onHideItem: vi.fn(),
    onUnhideItem: vi.fn(),
    onTogglePin: vi.fn(),
    onSnoozeItem: vi.fn(),
    onUnsnoozeItem: vi.fn(),
    onClearSearch: vi.fn(),
    onCloseSearch: vi.fn(),
    onToggleUnreadOnly: vi.fn(),
    onSetViewMode: vi.fn(),
  };
}

function mount(overrides: Record<string, unknown> = {}) {
  const on = handlers();
  const groups = (overrides.groups as RepoGroup[]) ?? [group()];
  const count = groups.reduce((n, g) => n + g.items.length, 0);
  render(ItemList, {
    loading: false,
    activeTab: "all" as Tab,
    visibleItemsCount: count,
    visibleUnreadCount: 0,
    groups,
    selectedId: null,
    notificationsByKey: new Map<string, NotificationItem[]>(),
    pinnedItems: new Set<number>(),
    tabs: TABS,
    error: null,
    searchQuery: "",
    searchVisible: false,
    unreadOnly: false,
    viewMode: "grouped" as ViewMode,
    showLatestComment: true,
    snoozedUntil: {},
    nowSec: Math.floor(Date.parse("2026-08-11T00:00:00Z") / 1000),
    snoozeMenuOpenId: null,
    ...on,
    ...overrides,
  });
  return on;
}

describe("ItemList toolbar", () => {
  it("refreshes on demand and reports nothing else", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(on.onRefresh).toHaveBeenCalledOnce();
    expect(on.onSignOut).not.toHaveBeenCalled();
  });

  it("blocks a second refresh while one is running", () => {
    mount({ loading: true });
    expect(screen.getByRole("button", { name: "Refreshing…" })).toBeDisabled();
  });

  it("hides mark-all-as-read when there is nothing unread", () => {
    mount({ visibleUnreadCount: 0 });
    expect(screen.queryByRole("button", { name: "Mark all as read" })).not.toBeInTheDocument();
  });

  it("offers mark-all-as-read once something is unread", async () => {
    const on = mount({ visibleUnreadCount: 3 });
    const button = screen.getByRole("button", { name: "Mark all as read" });
    // The count belongs in the tooltip so the button stays icon-sized.
    expect(button).toHaveAttribute("title", "Mark 3 as read (⌘⇧A)");
    await fireEvent.click(button);
    expect(on.onMarkAllVisibleAsRead).toHaveBeenCalledOnce();
  });

  it("opens settings and signs out through their own callbacks", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(on.onShowSettings).toHaveBeenCalledOnce();
    expect(on.onSignOut).toHaveBeenCalledOnce();
  });
});

describe("ItemList tabs", () => {
  it("renders every tab and reports the id of the one clicked", async () => {
    const on = mount();
    for (const tab of TABS) {
      expect(screen.getByRole("button", { name: tab.label })).toBeInTheDocument();
    }
    await fireEvent.click(screen.getByRole("button", { name: "Review" }));
    expect(on.onSwitchTab).toHaveBeenCalledWith("review");
  });
});

describe("ItemList rows", () => {
  it("shows the repo, the title and the number", () => {
    mount();
    expect(screen.getByText("yagince/eir")).toBeInTheDocument();
    expect(screen.getByText("Fix the tray badge on Windows")).toBeInTheDocument();
    expect(screen.getByText(/#42/)).toBeInTheDocument();
  });

  it("groups items under their repo with an unread count", () => {
    mount({
      groups: [
        group({ repo: "yagince/eir", unreadCount: 2 }),
        group({
          repo: "yagince/other",
          unreadCount: 0,
          items: [item({ id: 2, number: 7, title: "Second repo item", repo: "yagince/other" })],
        }),
      ],
    });
    expect(screen.getByText("yagince/eir")).toBeInTheDocument();
    expect(screen.getByText("yagince/other")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("says so when the tab is empty rather than rendering a blank pane", () => {
    mount({ groups: [], visibleItemsCount: 0 });
    // Whatever the copy is, the empty state must render *something* — a silent
    // blank pane reads as a broken fetch.
    expect(document.querySelector(".empty")).toBeInTheDocument();
  });

  it("surfaces a fetch error", () => {
    mount({ error: "GitHub API rate limit exceeded" });
    expect(screen.getByText(/rate limit exceeded/)).toBeInTheDocument();
  });
});
