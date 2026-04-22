import { describe, expect, it } from "vitest";
import type { WatchedItem } from "./types";
import {
  filterVisible,
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
