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

export function itemKey(i: {
  repo: string;
  kind: string;
  number: number;
}): string {
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
