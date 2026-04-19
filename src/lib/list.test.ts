import { describe, expect, it } from "vitest";
import type { WatchedItem } from "./types";
import {
  computeItemChanges,
  describeItemChange,
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

describe("describeItemChange", () => {
  const base = makeItem({ id: 1, repo: "o/r", number: 1 });

  it("returns null when nothing interesting changed", () => {
    expect(describeItemChange(base, { ...base })).toBeNull();
  });

  it("reports state transitions first", () => {
    expect(
      describeItemChange(base, { ...base, state: "merged", updated_at: "t2" }),
    ).toBe("Now merged");
  });

  it("reports draft becoming ready", () => {
    const prev = { ...base, is_draft: true };
    const curr = { ...base, is_draft: false, updated_at: "t2" };
    expect(describeItemChange(prev, curr)).toBe("Ready for review");
  });

  it("reports ready becoming draft", () => {
    const prev = { ...base, is_draft: false };
    const curr = { ...base, is_draft: true, updated_at: "t2" };
    expect(describeItemChange(prev, curr)).toBe("Marked as draft");
  });

  it("reports CI status change", () => {
    const prev = { ...base, ci_status: "pending" as const };
    const curr = { ...base, ci_status: "failure" as const, updated_at: "t2" };
    expect(describeItemChange(prev, curr)).toBe("CI failure");
  });

  it("reports the first reviewer whose state shifted", () => {
    const prev = {
      ...base,
      reviewers: [
        { login: "alice", avatar_url: "", state: "pending" as const },
      ],
    };
    const curr = {
      ...base,
      updated_at: "t2",
      reviewers: [
        { login: "alice", avatar_url: "", state: "approved" as const },
      ],
    };
    expect(describeItemChange(prev, curr)).toBe("alice approved");
  });

  it("reports comment growth with pluralisation", () => {
    expect(
      describeItemChange(base, { ...base, comments: 1, updated_at: "t2" }),
    ).toBe("+1 comment");
    expect(
      describeItemChange(base, { ...base, comments: 3, updated_at: "t2" }),
    ).toBe("+3 comments");
  });

  it("falls back to Updated when only updated_at moved", () => {
    expect(describeItemChange(base, { ...base, updated_at: "t2" })).toBe(
      "Updated",
    );
  });
});

describe("computeItemChanges", () => {
  it("returns empty when nothing changed and prev is seeded", () => {
    const item = makeItem({ id: 1, repo: "o/r", number: 1 });
    const prev = new Map([[1, item]]);
    const out = computeItemChanges(prev, [item], new Set());
    expect(out).toEqual([]);
  });

  it("flags new items as 'New in list'", () => {
    const item = makeItem({ id: 1, repo: "o/r", number: 1 });
    const out = computeItemChanges(new Map(), [item], new Set());
    expect(out).toHaveLength(1);
    expect(out[0].reason).toBe("New in list");
    expect(out[0].item.id).toBe(1);
  });

  it("reuses describeItemChange reason for modified items", () => {
    const prev = makeItem({ id: 1, repo: "o/r", number: 1 });
    const curr = { ...prev, state: "merged", updated_at: "t2" };
    const out = computeItemChanges(
      new Map([[1, prev]]),
      [curr],
      new Set(),
    );
    expect(out).toHaveLength(1);
    expect(out[0].reason).toBe("Now merged");
  });

  it("skips items whose key is in notifiedKeys", () => {
    // Imagine a GitHub notification thread already covers this PR; we
    // should not fire a second "Updated" notification for the same event.
    const prev = makeItem({ id: 1, repo: "o/r", kind: "pr", number: 1 });
    const curr = { ...prev, updated_at: "t2" };
    const notifiedKeys = new Set(["o/r:pr:1"]);
    const out = computeItemChanges(
      new Map([[1, prev]]),
      [curr],
      notifiedKeys,
    );
    expect(out).toEqual([]);
  });

  it("notifiedKeys skip applies to new items too", () => {
    // A brand-new PR that GitHub already delivered via /notifications
    // should not also show up as "New in list".
    const item = makeItem({ id: 1, repo: "o/r", kind: "pr", number: 1 });
    const out = computeItemChanges(
      new Map(),
      [item],
      new Set(["o/r:pr:1"]),
    );
    expect(out).toEqual([]);
  });

  it("combines new + changed + untouched + notified in one pass", () => {
    const a = makeItem({ id: 1, repo: "o/r", kind: "pr", number: 1 });
    const b = makeItem({ id: 2, repo: "o/r", kind: "pr", number: 2 });
    const c = makeItem({ id: 3, repo: "o/r", kind: "pr", number: 3 });
    const d = makeItem({ id: 4, repo: "o/r", kind: "pr", number: 4 });

    const prev = new Map([
      [1, a],
      [2, b],
      [3, c],
      // #4 didn't exist yet
    ]);
    const curr = [
      { ...a }, // unchanged
      { ...b, state: "closed", updated_at: "t2" }, // changed
      { ...c, updated_at: "t2" }, // changed, but notified
      d, // new
    ];
    const notifiedKeys = new Set(["o/r:pr:3"]);

    const out = computeItemChanges(prev, curr, notifiedKeys);
    expect(out).toHaveLength(2);
    expect(out.find((c) => c.item.id === 2)?.reason).toBe("Now closed");
    expect(out.find((c) => c.item.id === 4)?.reason).toBe("New in list");
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
