import { describe, expect, it } from "vitest";
import type { WatchedItem } from "./types";
import {
  filterBySearch,
  filterVisible,
  flattenCommentBody,
  groupByRepo,
  itemKey,
  relativeTime,
  repoSuggestionsFrom,
} from "./list";

function makeItem(
  overrides: Partial<WatchedItem> & Pick<WatchedItem, "id" | "repo" | "number">,
): WatchedItem {
  return {
    kind: "pr",
    title: "",
    url: "",
    author: "",
    author_avatar: "",
    comments: 0,
    updated_at: "2026-01-01T00:00:00Z",
    state: "open",
    is_draft: false,
    reviewers: [],
    commenters: [],
    ci_status: null,
    latest_comment: null,
    ...overrides,
  };
}

describe("relativeTime", () => {
  const now = Date.parse("2026-04-19T12:00:00Z");

  it("prints 'just now' under a minute", () => {
    expect(relativeTime("2026-04-19T11:59:30Z", now)).toBe("just now");
  });

  it("prints minutes under an hour", () => {
    expect(relativeTime("2026-04-19T11:55:00Z", now)).toBe("5m");
  });

  it("prints hours under a day", () => {
    expect(relativeTime("2026-04-19T09:00:00Z", now)).toBe("3h");
  });

  it("prints days otherwise", () => {
    expect(relativeTime("2026-04-17T12:00:00Z", now)).toBe("2d");
  });
});

describe("itemKey", () => {
  it("joins repo, kind and number", () => {
    expect(itemKey({ repo: "o/r", kind: "pr", number: 7 })).toBe("o/r:pr:7");
    expect(itemKey({ repo: "o/r", kind: "issue", number: 3 })).toBe(
      "o/r:issue:3",
    );
  });
});

describe("filterVisible", () => {
  const items = [
    makeItem({ id: 1, repo: "a/a", number: 1 }),
    makeItem({ id: 2, repo: "b/b", number: 2 }),
    makeItem({ id: 3, repo: "c/c", number: 3 }),
  ];

  it("hides individually hidden items on non-hidden tabs", () => {
    const out = filterVisible(items, {
      tab: "all",
      excludedRepos: new Set(),
      hiddenItems: new Set([2]),
    });
    expect(out.map((i) => i.id)).toEqual([1, 3]);
  });

  it("hides items from excluded repos on non-hidden tabs", () => {
    const out = filterVisible(items, {
      tab: "all",
      excludedRepos: new Set(["c/c"]),
      hiddenItems: new Set(),
    });
    expect(out.map((i) => i.id)).toEqual([1, 2]);
  });

  it("hidden tab shows only hidden items regardless of excluded repos", () => {
    const out = filterVisible(items, {
      tab: "hidden",
      excludedRepos: new Set(["c/c"]),
      hiddenItems: new Set([3]),
    });
    expect(out.map((i) => i.id)).toEqual([3]);
  });
});

describe("groupByRepo", () => {
  it("buckets by repo, sorts buckets by most recent update", () => {
    const items = [
      makeItem({
        id: 1,
        repo: "a/a",
        number: 1,
        updated_at: "2026-04-10T00:00:00Z",
      }),
      makeItem({
        id: 2,
        repo: "b/b",
        number: 2,
        updated_at: "2026-04-19T00:00:00Z",
      }),
      makeItem({
        id: 3,
        repo: "a/a",
        number: 3,
        updated_at: "2026-04-18T00:00:00Z",
      }),
    ];
    const groups = groupByRepo(items, () => false);
    expect(groups.map((g) => g.repo)).toEqual(["b/b", "a/a"]);
    // within a repo, newest first
    expect(groups[1].items.map((i) => i.id)).toEqual([3, 1]);
  });

  it("counts unread via predicate", () => {
    const items = [
      makeItem({ id: 1, repo: "a/a", number: 1 }),
      makeItem({ id: 2, repo: "a/a", number: 2 }),
      makeItem({ id: 3, repo: "a/a", number: 3 }),
    ];
    const unread = new Set([1, 3]);
    const groups = groupByRepo(items, (i) => unread.has(i.id));
    expect(groups[0].unreadCount).toBe(2);
  });

  it("places pinned items in a dedicated top group and removes them from their repo group", () => {
    const items = [
      makeItem({
        id: 1,
        repo: "a/a",
        number: 1,
        updated_at: "2026-04-10T00:00:00Z",
      }),
      makeItem({
        id: 2,
        repo: "b/b",
        number: 2,
        updated_at: "2026-04-19T00:00:00Z",
      }),
      makeItem({
        id: 3,
        repo: "a/a",
        number: 3,
        updated_at: "2026-04-18T00:00:00Z",
      }),
    ];
    const groups = groupByRepo(items, () => false, new Set([2]));
    expect(groups[0].kind).toBe("pinned");
    expect(groups[0].items.map((i) => i.id)).toEqual([2]);
    // b/b's sole item moved into the pinned group, so b/b disappears.
    const remaining = groups.slice(1);
    expect(remaining.map((g) => g.repo)).toEqual(["a/a"]);
    expect(remaining[0].items.map((i) => i.id)).toEqual([3, 1]);
  });

  it("sorts the pinned group by updated_at desc", () => {
    const items = [
      makeItem({
        id: 1,
        repo: "a/a",
        number: 1,
        updated_at: "2026-04-10T00:00:00Z",
      }),
      makeItem({
        id: 2,
        repo: "b/b",
        number: 2,
        updated_at: "2026-04-19T00:00:00Z",
      }),
    ];
    const groups = groupByRepo(items, () => false, new Set([1, 2]));
    expect(groups[0].kind).toBe("pinned");
    expect(groups[0].items.map((i) => i.id)).toEqual([2, 1]);
  });

  it("omits the pinned group entirely when nothing is pinned", () => {
    const items = [makeItem({ id: 1, repo: "a/a", number: 1 })];
    const groups = groupByRepo(items, () => false, new Set());
    expect(groups.some((g) => g.kind === "pinned")).toBe(false);
  });

  it("counts unread within the pinned group via predicate", () => {
    const items = [
      makeItem({ id: 1, repo: "a/a", number: 1 }),
      makeItem({ id: 2, repo: "b/b", number: 2 }),
    ];
    const groups = groupByRepo(items, (i) => i.id === 1, new Set([1, 2]));
    expect(groups[0].kind).toBe("pinned");
    expect(groups[0].unreadCount).toBe(1);
  });
});

describe("repoSuggestionsFrom", () => {
  it("returns unique sorted repos excluding already-excluded", () => {
    const items = [
      makeItem({ id: 1, repo: "b/b", number: 1 }),
      makeItem({ id: 2, repo: "a/a", number: 2 }),
      makeItem({ id: 3, repo: "b/b", number: 3 }),
      makeItem({ id: 4, repo: "c/c", number: 4 }),
    ];
    const out = repoSuggestionsFrom(items, new Set(["c/c"]));
    expect(out).toEqual(["a/a", "b/b"]);
  });
});

describe("filterBySearch", () => {
  const items = [
    makeItem({
      id: 1,
      repo: "acme/web",
      number: 42,
      title: "Fix login redirect loop",
      author: "alice",
    }),
    makeItem({
      id: 2,
      repo: "acme/api",
      number: 7,
      title: "Add rate limiting",
      author: "bob",
    }),
    makeItem({
      id: 3,
      repo: "other/cli",
      number: 123,
      title: "Upgrade CLI deps",
      author: "carol",
    }),
  ];

  it("returns input untouched when query is blank", () => {
    expect(filterBySearch(items, "")).toBe(items);
    expect(filterBySearch(items, "   ")).toBe(items);
  });

  it("matches case-insensitively against title", () => {
    const out = filterBySearch(items, "LOGIN");
    expect(out.map((i) => i.id)).toEqual([1]);
  });

  it("matches against repo name", () => {
    const out = filterBySearch(items, "acme");
    expect(out.map((i) => i.id)).toEqual([1, 2]);
  });

  it("matches against author", () => {
    const out = filterBySearch(items, "bob");
    expect(out.map((i) => i.id)).toEqual([2]);
  });

  it("matches against #number and bare number", () => {
    expect(filterBySearch(items, "#42").map((i) => i.id)).toEqual([1]);
    expect(filterBySearch(items, "123").map((i) => i.id)).toEqual([3]);
  });

  it("AND-combines whitespace-separated tokens", () => {
    const out = filterBySearch(items, "acme limit");
    expect(out.map((i) => i.id)).toEqual([2]);
  });

  it("returns empty array when no item matches all tokens", () => {
    const out = filterBySearch(items, "acme nothingmatches");
    expect(out).toEqual([]);
  });
});

describe("flattenCommentBody", () => {
  it("collapses runs of whitespace and embedded newlines", () => {
    expect(flattenCommentBody("first line\n\nsecond   line\n\tthird")).toBe(
      "first line second line third",
    );
  });

  it("strips fenced code blocks", () => {
    expect(flattenCommentBody("before\n```ts\ncode()\n```\nafter")).toBe(
      "before after",
    );
  });

  it("suppresses content after an unclosed fence", () => {
    // Mirrors the Rust side: an unclosed fence still hides everything after
    // it so a half-rendered code block doesn't leak into the preview.
    expect(flattenCommentBody("before\n```\ncode")).toBe("before");
  });

  it("returns an empty string for whitespace-only input", () => {
    expect(flattenCommentBody("   \n\t  ")).toBe("");
  });
});
