import type { RepoGroup, Tab, WatchedItem } from "./types";

export function relativeTime(iso: string, now: number = Date.now()): string {
  const diff = now - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  return `${d}d`;
}

export function itemKey(
  i: Pick<WatchedItem, "repo" | "kind" | "number">,
): string {
  return `${i.repo}:${i.kind}:${i.number}`;
}

export function filterVisible(
  items: WatchedItem[],
  opts: {
    tab: Tab;
    excludedRepos: ReadonlySet<string>;
    hiddenItems: ReadonlySet<number>;
  },
): WatchedItem[] {
  if (opts.tab === "hidden") {
    return items.filter((i) => opts.hiddenItems.has(i.id));
  }
  return items.filter(
    (i) => !opts.hiddenItems.has(i.id) && !opts.excludedRepos.has(i.repo),
  );
}

export function groupByRepo(
  items: WatchedItem[],
  isUnread: (i: WatchedItem) => boolean,
): RepoGroup[] {
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
      unreadCount: groupItems.filter(isUnread).length,
    });
  }
  result.sort((a, b) => b.mostRecent.localeCompare(a.mostRecent));
  return result;
}

export function repoSuggestionsFrom(
  items: WatchedItem[],
  excludedRepos: ReadonlySet<string>,
): string[] {
  const seen = new Set<string>();
  for (const item of items) {
    if (!excludedRepos.has(item.repo)) seen.add(item.repo);
  }
  return [...seen].sort();
}

export type ItemChange = {
  item: WatchedItem;
  reason: string;
};

/**
 * Compare a previous snapshot of the item list against a fresh fetch and
 * produce one ItemChange per item that meaningfully moved (new entries, or
 * anything describeItemChange cares about). Items whose (repo, kind, number)
 * is already covered by a GitHub notification thread in `notifiedKeys` are
 * skipped — the /notifications diff gets first dibs on surfacing them, so
 * we don't fire two desktop notifications for the same underlying event.
 */
export function computeItemChanges(
  prevItems: ReadonlyMap<number, WatchedItem>,
  currItems: readonly WatchedItem[],
  notifiedKeys: ReadonlySet<string>,
): ItemChange[] {
  const out: ItemChange[] = [];
  for (const item of currItems) {
    if (notifiedKeys.has(itemKey(item))) continue;
    const prev = prevItems.get(item.id);
    if (!prev) {
      out.push({ item, reason: "New in list" });
      continue;
    }
    const reason = describeItemChange(prev, item);
    if (reason) out.push({ item, reason });
  }
  return out;
}

/**
 * Describe what meaningfully changed between two snapshots of the same item,
 * as a short phrase suitable for a desktop-notification title. Returns null
 * when nothing interesting changed (i.e. not worth notifying about).
 *
 * Order matters — we return the first signal we find, roughly ranked by how
 * actionable the change is: CI failures are louder than a label tweak.
 */
export function describeItemChange(
  prev: WatchedItem,
  curr: WatchedItem,
): string | null {
  // PR state transitions (merged / closed / reopened) are the biggest news.
  if (prev.state !== curr.state) {
    return `Now ${curr.state}`;
  }

  // Draft ⇄ ready is a reviewer-relevant flip.
  if (prev.is_draft !== curr.is_draft) {
    return curr.is_draft ? "Marked as draft" : "Ready for review";
  }

  // CI state change (includes success after a red build — also worth hearing).
  if (prev.ci_status !== curr.ci_status) {
    return `CI ${curr.ci_status ?? "unknown"}`;
  }

  // Review-state change for any reviewer. Prefer the first non-matching one
  // to keep the output short; the full state lives in the app itself.
  const prevByLogin = new Map(
    prev.reviewers.map((r) => [r.login, r.state] as const),
  );
  for (const r of curr.reviewers) {
    if (prevByLogin.get(r.login) !== r.state) {
      return `${r.login} ${r.state.replace("_", " ")}`;
    }
  }

  // Comment count went up — someone commented.
  if (curr.comments > prev.comments) {
    const delta = curr.comments - prev.comments;
    return `+${delta} comment${delta === 1 ? "" : "s"}`;
  }

  // Nothing above matched but updated_at moved — some edit happened we don't
  // model (labels, assignees, etc). Worth a quiet catch-all notification so
  // the user knows something changed upstream.
  if (prev.updated_at !== curr.updated_at) {
    return "Updated";
  }

  return null;
}
