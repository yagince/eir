// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

function notif(overrides: Partial<NotificationItem> = {}): NotificationItem {
  return {
    thread_id: 1,
    reason: "mention",
    repo: "yagince/eir",
    kind: "pr",
    number: 42,
    title: "Fix the tray badge on Windows",
    url: "https://github.com/yagince/eir/pull/42",
    updated_at: "2026-08-11T00:00:00Z",
    ...overrides,
  };
}

// `notificationsByKey` is keyed by `repo:kind:number` (mirroring `itemKey`),
// never by item id — building the map from the item itself keeps these tests
// from silently asserting "no unread" because of a key typo.
function unreadFor(...items: WatchedItem[]): Map<string, NotificationItem[]> {
  return new Map(items.map((i) => [`${i.repo}:${i.kind}:${i.number}`, [notif()]]));
}

/// Row title is the only stable handle on the item button — its accessible
/// name is the whole row (badge + title + meta), so match on a fragment.
function row(title: string | RegExp = /Fix the tray badge/): HTMLElement {
  return screen.getByRole("button", { name: title });
}

/// Yields to the macrotask queue so the menu's `use:` action (which focuses
/// its first child inside a `queueMicrotask`) has run before we assert.
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/// Same clock the component snoozes against — unix seconds, floored.
const secondsNow = () => Math.floor(Date.now() / 1000);

describe("ItemList filters", () => {
  it("reports the unread-only toggle and mirrors its state to assistive tech", async () => {
    const on = mount();
    const chip = screen.getByRole("button", { name: "Unread only" });
    expect(chip).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(chip);
    expect(on.onToggleUnreadOnly).toHaveBeenCalledOnce();
  });

  it("tells the user how to undo the unread-only filter once it is on", () => {
    mount({ unreadOnly: true });
    const chip = screen.getByRole("button", { name: "Unread only" });
    expect(chip).toHaveAttribute("aria-pressed", "true");
    expect(chip).toHaveAttribute("title", expect.stringContaining("press U to show all"));
  });

  it("marks the active view mode and reports the other one when clicked", async () => {
    const on = mount({ viewMode: "grouped" });
    const grouped = screen.getByRole("button", { name: "Group by repository" });
    const recent = screen.getByRole("button", { name: "Sort by most recent activity" });
    expect(grouped).toHaveAttribute("aria-pressed", "true");
    expect(recent).toHaveAttribute("aria-pressed", "false");
    await fireEvent.click(recent);
    expect(on.onSetViewMode).toHaveBeenCalledWith("recent");
  });
});

describe("ItemList search", () => {
  it("keeps the search field out of the way until asked for", () => {
    mount();
    expect(screen.queryByRole("textbox", { name: "Search" })).not.toBeInTheDocument();
  });

  it("shows the field when the host reveals it", () => {
    mount({ searchVisible: true });
    expect(screen.getByRole("textbox", { name: "Search" })).toBeInTheDocument();
  });

  // A query keeps the field mounted even after the host stops "showing" it:
  // otherwise the list would stay filtered with no visible reason why, and no
  // way to clear it.
  it("stays mounted while a query is still filtering the list", () => {
    mount({ searchVisible: false, searchQuery: "tray" });
    expect(screen.getByRole("textbox", { name: "Search" })).toHaveValue("tray");
  });

  it("offers the clear button only while there is something to clear", () => {
    mount({ searchVisible: true, searchQuery: "" });
    expect(screen.queryByRole("button", { name: "Clear search" })).not.toBeInTheDocument();
  });

  it("feeds what is typed back out through the bound query", async () => {
    mount({ searchVisible: true, searchQuery: "" });
    const input = screen.getByRole("textbox", { name: "Search" });
    await fireEvent.input(input, { target: { value: "tray" } });
    // The clear affordance appearing is the observable proof that the keystroke
    // reached the query the host filters on, not just the DOM node.
    expect(screen.getByRole("button", { name: "Clear search" })).toBeInTheDocument();
  });

  it("clears the query on the clear button", async () => {
    const on = mount({ searchVisible: true, searchQuery: "tray" });
    await fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
    expect(on.onClearSearch).toHaveBeenCalledOnce();
    expect(on.onCloseSearch).not.toHaveBeenCalled();
  });

  it("makes Escape clear a non-empty query rather than closing the field", async () => {
    const on = mount({ searchVisible: true, searchQuery: "tray" });
    await fireEvent.keyDown(screen.getByRole("textbox", { name: "Search" }), { key: "Escape" });
    expect(on.onClearSearch).toHaveBeenCalledOnce();
    expect(on.onCloseSearch).not.toHaveBeenCalled();
  });

  it("makes Escape close the field once the query is already empty", async () => {
    const on = mount({ searchVisible: true, searchQuery: "" });
    await fireEvent.keyDown(screen.getByRole("textbox", { name: "Search" }), { key: "Escape" });
    expect(on.onCloseSearch).toHaveBeenCalledOnce();
    expect(on.onClearSearch).not.toHaveBeenCalled();
  });

  it("ignores other keys in the search field", async () => {
    const on = mount({ searchVisible: true, searchQuery: "tray" });
    await fireEvent.keyDown(screen.getByRole("textbox", { name: "Search" }), { key: "a" });
    expect(on.onClearSearch).not.toHaveBeenCalled();
    expect(on.onCloseSearch).not.toHaveBeenCalled();
  });

  it("names the query in the empty state so a typo is obvious", () => {
    mount({ groups: [], visibleItemsCount: 0, searchVisible: true, searchQuery: "tray" });
    expect(screen.getByText('No matches for "tray".')).toBeInTheDocument();
  });

  it("blames the unread filter when it is the reason nothing shows", () => {
    mount({ groups: [], visibleItemsCount: 0, unreadOnly: true });
    expect(screen.getByText("No unread items.")).toBeInTheDocument();
  });

  it("falls back to a plain empty message with no filter in play", () => {
    mount({ groups: [], visibleItemsCount: 0 });
    expect(screen.getByText("Nothing here.")).toBeInTheDocument();
  });
});

describe("ItemList row detail", () => {
  it("flags a draft PR in both the label and the row styling", () => {
    mount({ groups: [group({ items: [item({ is_draft: true })] })] });
    expect(screen.getByText("DRAFT")).toBeInTheDocument();
    expect(row()).toHaveClass("draft");
  });

  it("renders a tick for green CI and a cross for red", () => {
    mount({ groups: [group({ items: [item({ ci_status: "success" })] })] });
    expect(screen.getByTitle("CI: success")).toHaveTextContent("✓");
    // Auto-cleanup only runs between tests, so a second render inside one test
    // has to tear the first one down or every query sees both.
    cleanup();
    mount({ groups: [group({ items: [item({ ci_status: "failure" })] })] });
    expect(screen.getByTitle("CI: failure")).toHaveTextContent("✗");
  });

  it("renders the pending glyph for an in-flight run", () => {
    mount({ groups: [group({ items: [item({ ci_status: "pending" })] })] });
    expect(screen.getByTitle("CI: pending")).toHaveTextContent("⏱");
  });

  // GitHub reports an infrastructure failure as "error" rather than "failure";
  // it has to read as broken CI too, not fall through to the neutral glyph.
  it("treats an errored run as a red cross", () => {
    mount({ groups: [group({ items: [item({ ci_status: "error" })] })] });
    expect(screen.getByTitle("CI: error")).toHaveTextContent("✗");
  });

  it("badges issues apart from pull requests", () => {
    mount({ groups: [group({ items: [item({ kind: "issue" })] })] });
    expect(screen.getByText("IS")).toBeInTheDocument();
    cleanup();
    mount({ groups: [group({ items: [item({ kind: "pr" })] })] });
    expect(screen.getByText("PR")).toBeInTheDocument();
  });

  // "unknown" is what the API returns for repos with no checks configured at
  // all — showing a neutral CI glyph there would imply a run that never exists.
  it("says nothing about CI when the status is unknown or absent", () => {
    mount({ groups: [group({ items: [item({ ci_status: "unknown" })] })] });
    expect(screen.queryByTitle(/^CI:/)).not.toBeInTheDocument();
    cleanup();
    mount({ groups: [group({ items: [item({ ci_status: null })] })] });
    expect(screen.queryByTitle(/^CI:/)).not.toBeInTheDocument();
  });

  it("shows a comment count only when there are comments", () => {
    mount({ groups: [group({ items: [item({ comments: 3 })] })] });
    expect(screen.getByTitle("Comments")).toHaveTextContent("💬 3");
    cleanup();
    mount({ groups: [group({ items: [item({ comments: 0 })] })] });
    expect(screen.queryByTitle("Comments")).not.toBeInTheDocument();
  });

  it("spells out every reviewer state", () => {
    mount({
      groups: [
        group({
          items: [
            item({
              reviewers: [
                { login: "alice", avatar_url: "", state: "approved" },
                { login: "bob", avatar_url: "", state: "changes_requested" },
                { login: "carol", avatar_url: "", state: "commented" },
                { login: "dave", avatar_url: "", state: "dismissed" },
                { login: "erin", avatar_url: "", state: "pending" },
              ],
            }),
          ],
        }),
      ],
    });
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("approved")).toBeInTheDocument();
    expect(screen.getByText("changes")).toBeInTheDocument();
    expect(screen.getByText("commented")).toBeInTheDocument();
    expect(screen.getByText("dismissed")).toBeInTheDocument();
    expect(screen.getByText("not yet")).toBeInTheDocument();
  });

  it("lists commenters and drops the section when there are none", () => {
    mount({
      groups: [
        group({
          items: [
            item({
              commenters: [
                { login: "alice", avatar_url: "" },
                { login: "bob", avatar_url: "" },
              ],
            }),
          ],
        }),
      ],
    });
    expect(screen.getByTitle("alice")).toHaveTextContent("alice");
    expect(screen.getByTitle("bob")).toHaveTextContent("bob");
  });

  it("marks a row unread only when a notification thread matches its key", () => {
    const target = item();
    mount({ groups: [group({ items: [target] })], notificationsByKey: unreadFor(target) });
    expect(row()).toHaveClass("unread");
    cleanup();
    mount({ groups: [group({ items: [item()] })], notificationsByKey: new Map() });
    expect(row()).not.toHaveClass("unread");
  });

  it("previews the latest comment on an unread row", () => {
    const target = item({
      latest_comment: {
        author: "alice",
        author_avatar: "",
        body_text: "Looks good, one nit about the badge",
        created_at: "2026-08-11T00:00:00Z",
        url: "https://github.com/yagince/eir/pull/42#issuecomment-1",
      },
    });
    mount({ groups: [group({ items: [target] })], notificationsByKey: unreadFor(target) });
    expect(screen.getByText("@alice")).toBeInTheDocument();
    expect(screen.getByText(/one nit about the badge/)).toBeInTheDocument();
  });

  it("drops the preview when the setting is off", () => {
    const target = item({
      latest_comment: {
        author: "alice",
        author_avatar: "",
        body_text: "Looks good",
        created_at: "2026-08-11T00:00:00Z",
        url: "https://github.com/yagince/eir/pull/42#issuecomment-1",
      },
    });
    mount({
      groups: [group({ items: [target] })],
      notificationsByKey: unreadFor(target),
      showLatestComment: false,
    });
    expect(screen.queryByText("@alice")).not.toBeInTheDocument();
  });

  // The preview is gated on an *active notification thread*, not merely on the
  // item having a comment — otherwise every already-read row in the list would
  // grow a three-line quote and the popup would be unreadable.
  it("drops the preview on a row with no live notification", () => {
    mount({
      groups: [
        group({
          items: [
            item({
              latest_comment: {
                author: "alice",
                author_avatar: "",
                body_text: "Looks good",
                created_at: "2026-08-11T00:00:00Z",
                url: "https://github.com/yagince/eir/pull/42#issuecomment-1",
              },
            }),
          ],
        }),
      ],
      notificationsByKey: new Map(),
    });
    expect(screen.queryByText("@alice")).not.toBeInTheDocument();
  });

  it("strips fenced code out of the preview and skips it entirely when nothing is left", () => {
    const withCode = item({
      latest_comment: {
        author: "alice",
        author_avatar: "",
        body_text: "see below\n```\nrm -rf /\n```",
        created_at: "2026-08-11T00:00:00Z",
        url: "https://github.com/yagince/eir/pull/42#issuecomment-1",
      },
    });
    mount({ groups: [group({ items: [withCode] })], notificationsByKey: unreadFor(withCode) });
    expect(screen.getByText(/see below/)).toBeInTheDocument();
    expect(screen.queryByText(/rm -rf/)).not.toBeInTheDocument();
    cleanup();

    const onlyCode = item({
      latest_comment: {
        author: "bob",
        author_avatar: "",
        body_text: "```\nrm -rf /\n```",
        created_at: "2026-08-11T00:00:00Z",
        url: "https://github.com/yagince/eir/pull/42#issuecomment-1",
      },
    });
    mount({ groups: [group({ items: [onlyCode] })], notificationsByKey: unreadFor(onlyCode) });
    expect(screen.queryByText("@bob")).not.toBeInTheDocument();
  });

  // In "recent" mode the rows aren't under a repo header any more, so the repo
  // has to move into the row itself or the list becomes ambiguous.
  it("moves the repo into the row when the list is sorted by recency", () => {
    const items = [item()];
    mount({
      viewMode: "recent",
      groups: [group({ repo: "Recent", kind: "flat", items })],
    });
    expect(screen.getByTitle("yagince/eir")).toBeInTheDocument();
  });

  // The recency view puts everything in one bucket whose `repo` is a synthetic
  // placeholder, so a flat group must not draw a header at all.
  it("renders no repo header for a flat group", () => {
    mount({ viewMode: "recent", groups: [group({ repo: "synthetic-bucket", kind: "flat" })] });
    expect(screen.queryByText("synthetic-bucket")).not.toBeInTheDocument();
  });

  it("labels the pinned group instead of showing its synthetic repo name", () => {
    mount({
      groups: [
        group({ repo: "Pinned", kind: "pinned", unreadCount: 1, items: [item()] }),
        group({
          repo: "yagince/other",
          items: [item({ id: 2, number: 7, repo: "yagince/other" })],
        }),
      ],
      pinnedItems: new Set([1]),
    });
    expect(screen.getByText("Pinned")).toBeInTheDocument();
    expect(screen.getByText("yagince/other")).toBeInTheDocument();
  });
});

describe("ItemList row actions", () => {
  it("hands the whole item to the open callback", async () => {
    const target = item();
    const on = mount({ groups: [group({ items: [target] })] });
    await fireEvent.click(row());
    expect(on.onOpenItem).toHaveBeenCalledWith(expect.objectContaining({ id: 1, number: 42 }));
  });

  it("pins an unpinned row and reports its id", async () => {
    const on = mount();
    const pin = screen.getByRole("button", { name: "Pin" });
    await fireEvent.click(pin);
    expect(on.onTogglePin).toHaveBeenCalledWith(1);
  });

  it("offers Unpin for a row that is already pinned", async () => {
    const on = mount({ pinnedItems: new Set([1]) });
    expect(screen.queryByRole("button", { name: "Pin" })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Unpin" }));
    expect(on.onTogglePin).toHaveBeenCalledWith(1);
  });

  it("hides a row by id", async () => {
    const on = mount({
      groups: [group({ items: [item(), item({ id: 2, number: 7, title: "Other" })] })],
    });
    await fireEvent.click(screen.getAllByRole("button", { name: "Hide" })[1]);
    expect(on.onHideItem).toHaveBeenCalledWith(2);
  });

  // On the Hidden tab the only sensible action is putting the row back, so the
  // pin/snooze/hide column is replaced wholesale rather than extended.
  it("swaps the whole action column for Unhide on the hidden tab", async () => {
    const on = mount({ activeTab: "hidden" });
    expect(screen.queryByRole("button", { name: "Hide" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pin" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Snooze" })).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Unhide" }));
    expect(on.onUnhideItem).toHaveBeenCalledWith(1);
  });
});

describe("ItemList snooze menu", () => {
  // Fake only Date: the menu's focus transfer rides on `queueMicrotask` and
  // Svelte flushes through microtasks too, so faking the whole timer surface
  // would deadlock rendering.
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 7, 11, 12, 0, 0));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  async function openMenu(index = 0) {
    const triggers = screen.getAllByRole("button", { name: /Snooze$|Edit snooze/ });
    await fireEvent.click(triggers[index]);
    await settle();
    return screen.getByRole("menu", { name: "Snooze options" });
  }

  it("opens the menu for the row whose trigger was clicked", async () => {
    mount({
      groups: [group({ items: [item(), item({ id: 2, number: 7, title: "Other" })] })],
    });
    const triggers = screen.getAllByRole("button", { name: "Snooze" });
    expect(triggers[0]).toHaveAttribute("aria-expanded", "false");
    await openMenu(1);
    expect(triggers[0]).toHaveAttribute("aria-expanded", "false");
    expect(triggers[1]).toHaveAttribute("aria-expanded", "true");
  });

  it("closes again when the same trigger is clicked twice", async () => {
    mount();
    await openMenu();
    await fireEvent.click(screen.getByRole("button", { name: "Snooze" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("renders the menu the host opened without any click", () => {
    mount({ snoozeMenuOpenId: 1 });
    expect(screen.getByRole("menu", { name: "Snooze options" })).toBeInTheDocument();
  });

  it("offers the presets and nothing to clear on a row that is not snoozed", async () => {
    mount();
    const menu = await openMenu();
    for (const label of ["30 minutes", "1 hour", "Tomorrow 8 AM", "Pick date & time…"]) {
      expect(within(menu).getByRole("menuitem", { name: label })).toBeInTheDocument();
    }
    expect(within(menu).queryByRole("menuitem", { name: "Clear snooze" })).not.toBeInTheDocument();
  });

  it("snoozes 30 minutes out and closes the menu", async () => {
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "30 minutes" }));
    expect(on.onSnoozeItem).toHaveBeenCalledWith(1, secondsNow() + 30 * 60);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("snoozes an hour out", async () => {
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "1 hour" }));
    expect(on.onSnoozeItem).toHaveBeenCalledWith(1, secondsNow() + 60 * 60);
  });

  it("snoozes to the next 8 AM", async () => {
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Tomorrow 8 AM" }));
    expect(on.onSnoozeItem).toHaveBeenCalledWith(
      1,
      Math.floor(new Date(2026, 7, 12, 8, 0, 0).getTime() / 1000),
    );
  });

  // Snoozing at 2 AM should wake you at 8 AM the *same* morning, not 30 hours
  // later — the preset is "next 8 o'clock", not "tomorrow" literally.
  it("uses this morning's 8 AM when it is still before 8", async () => {
    vi.setSystemTime(new Date(2026, 7, 11, 2, 0, 0));
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Tomorrow 8 AM" }));
    expect(on.onSnoozeItem).toHaveBeenCalledWith(
      1,
      Math.floor(new Date(2026, 7, 11, 8, 0, 0).getTime() / 1000),
    );
  });

  it("offers Clear snooze on an already-snoozed row and unsnoozes it", async () => {
    const on = mount({ snoozedUntil: { 1: secondsNow() + 3600 } });
    expect(screen.getByRole("button", { name: "Edit snooze" })).toBeInTheDocument();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Clear snooze" }));
    expect(on.onUnsnoozeItem).toHaveBeenCalledWith(1);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("dismisses the menu on Escape", async () => {
    mount();
    await openMenu();
    await fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("dismisses the menu on a click anywhere outside it", async () => {
    mount();
    await openMenu();
    await fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("survives a mousedown on its own contents", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.mouseDown(within(menu).getByRole("menuitem", { name: "1 hour" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  // The host can open this menu from a keyboard shortcut with no pointer
  // involved, so focus has to land inside it or the arrow/Enter handling below
  // has nothing to act on.
  it("moves focus onto the first option as soon as it opens", async () => {
    mount({ snoozeMenuOpenId: 1 });
    await settle();
    expect(screen.getByRole("menuitem", { name: "30 minutes" })).toHaveFocus();
  });

  it("walks the options with the arrow keys and wraps at the ends", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "1 hour" })).toHaveFocus();
    await fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(screen.getByRole("menuitem", { name: "30 minutes" })).toHaveFocus();
    await fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(screen.getByRole("menuitem", { name: "Pick date & time…" })).toHaveFocus();
  });

  it("starts arrow navigation at the first option when focus sits on the menu itself", async () => {
    mount();
    const menu = await openMenu();
    menu.focus();
    await fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "30 minutes" })).toHaveFocus();
  });

  // Enter inside the menu must not bubble: the page-level shortcut handler
  // would read it as "open the selected row" and yank the popup to GitHub
  // while the user is only confirming a snooze.
  it("keeps Enter and arrows from reaching the page shortcut handler", async () => {
    mount();
    const menu = await openMenu();
    const pageKey = vi.fn();
    document.addEventListener("keydown", pageKey);
    try {
      await fireEvent.keyDown(menu, { key: "Enter" });
      await fireEvent.keyDown(menu, { key: "ArrowDown" });
      expect(pageKey).not.toHaveBeenCalled();
      await fireEvent.keyDown(menu, { key: "x" });
      expect(pageKey).toHaveBeenCalledOnce();
    } finally {
      document.removeEventListener("keydown", pageKey);
    }
  });

  it("seeds the custom pickers with one hour from now and can back out", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await settle();
    expect(screen.getByLabelText("Date")).toHaveValue("2026-08-11");
    expect(screen.getByLabelText("Hour")).toHaveValue(13);
    expect(screen.getByLabelText("Minute")).toHaveValue(0);
    // Focus follows the mode switch so the date field is typeable immediately.
    expect(screen.getByLabelText("Date")).toHaveFocus();
    await fireEvent.click(within(menu).getByRole("button", { name: "Back" }));
    expect(within(menu).getByRole("menuitem", { name: "30 minutes" })).toBeInTheDocument();
  });

  it("applies a custom future date and time", async () => {
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Date"), { target: { value: "2026-08-12" } });
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "9" } });
    await fireEvent.input(screen.getByLabelText("Minute"), { target: { value: "30" } });
    const apply = within(menu).getByRole("button", { name: "Snooze" });
    expect(apply).toBeEnabled();
    await fireEvent.click(apply);
    expect(on.onSnoozeItem).toHaveBeenCalledWith(
      1,
      Math.floor(new Date(2026, 7, 12, 9, 30, 0).getTime() / 1000),
    );
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("explains a past time picked on the hour instead of just disabling", async () => {
    // The hour and minute inputs are type="number", so Svelte binds them as
    // numbers — and a legitimate 0 is falsy. Gating the message on truthiness
    // meant :00 (or midnight) read as "still blank", leaving the user with a
    // disabled Snooze button and no reason for it.
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Date"), { target: { value: "2020-01-01" } });
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "11" } });
    await fireEvent.input(screen.getByLabelText("Minute"), { target: { value: "0" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
    expect(screen.getByText("Pick a future time.")).toBeInTheDocument();
    expect(on.onSnoozeItem).not.toHaveBeenCalled();
  });

  it("stays quiet while the fields are still empty", async () => {
    // The flip side: the message must not accuse the user of picking a past time
    // before they have entered anything.
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "" } });
    expect(screen.queryByText("Pick a future time.")).not.toBeInTheDocument();
  });

  it("refuses an hour outside 0–23 and says why", async () => {
    const on = mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "99" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
    expect(screen.getByText("Pick a future time.")).toBeInTheDocument();
    await fireEvent.click(within(menu).getByRole("button", { name: "Snooze" }));
    expect(on.onSnoozeItem).not.toHaveBeenCalled();
  });

  // `min="0"` only constrains the spinner, not typing, so a negative hour has
  // to be rejected in the parse as well.
  it("refuses a negative hour", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "-5" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
  });

  it("refuses a minute outside 0–59", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Minute"), { target: { value: "75" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
  });

  // A half-typed field must read as "not a number yet", not as 0 — coercing a
  // blank hour to midnight would silently snooze into the past.
  it("refuses a blank time field without shouting at a mid-edit user", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
    expect(screen.queryByText("Pick a future time.")).not.toBeInTheDocument();
  });

  it("refuses a blank date", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Date"), { target: { value: "" } });
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
  });

  it("refuses a time that has already passed", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    await fireEvent.input(screen.getByLabelText("Date"), { target: { value: "2020-01-01" } });
    await fireEvent.input(screen.getByLabelText("Hour"), { target: { value: "10" } });
    await fireEvent.input(screen.getByLabelText("Minute"), { target: { value: "30" } });
    expect(screen.getByText("Pick a future time.")).toBeInTheDocument();
    expect(within(menu).getByRole("button", { name: "Snooze" })).toBeDisabled();
  });

  // Arrows inside the number/date inputs belong to the input (they spin the
  // value); hijacking them for menu navigation would make the fields uneditable.
  it("leaves arrow keys alone inside the custom time fields", async () => {
    mount();
    const menu = await openMenu();
    await fireEvent.click(within(menu).getByRole("menuitem", { name: "Pick date & time…" }));
    const hour = screen.getByLabelText("Hour");
    hour.focus();
    await fireEvent.keyDown(hour, { key: "ArrowDown" });
    expect(hour).toHaveFocus();
  });
});

describe("ItemList snooze countdown", () => {
  const base = Math.floor(Date.parse("2026-08-11T00:00:00Z") / 1000);

  it("renders a coarse remaining-time label per row from the nowSec prop", () => {
    const cases: [number, string][] = [
      [30, "💤 30s"],
      [45 * 60, "💤 45m"],
      [2 * 3600, "💤 2h"],
      [2 * 3600 + 30 * 60, "💤 2h 30m"],
      [26 * 3600, "💤 1d 2h"],
      [48 * 3600, "💤 2d"],
    ];
    const items = cases.map((_, i) => item({ id: i + 1, number: i + 1, title: `row ${i + 1}` }));
    const snoozedUntil = Object.fromEntries(cases.map(([delta], i) => [i + 1, base + delta]));
    mount({ groups: [group({ items })], snoozedUntil, nowSec: base });
    for (const [, label] of cases) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
  });

  // A snooze that expired between worker ticks must read "0s", not a negative
  // countdown — the row stays visible until the Rust side clears it.
  it("clamps an expired snooze to zero instead of counting backwards", () => {
    mount({ snoozedUntil: { 1: base - 500 }, nowSec: base });
    expect(screen.getByText("💤 0s")).toBeInTheDocument();
  });

  it("mutes a snoozed row and leaves an unsnoozed one alone", () => {
    mount({ snoozedUntil: { 1: base + 3600 }, nowSec: base });
    expect(row()).toHaveClass("snoozed");
    cleanup();
    mount({ snoozedUntil: {}, nowSec: base });
    expect(row()).not.toHaveClass("snoozed");
  });
});
