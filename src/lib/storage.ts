import type { Tab } from "$lib/types";

export const TAB_KEY = "eir.tab";
export const INTERVAL_KEY = "eir.refreshMs";
export const NOTIFY_KEY = "eir.notifyEnabled";
export const EXCLUDED_REPOS_KEY = "eir.excludedRepos";
export const HIDDEN_ITEMS_KEY = "eir.hiddenItems";
export const WATCHED_ORGS_KEY = "eir.watchedOrgs";
export const THEME_KEY = "eir.theme";

export const DEFAULT_REFRESH_MS = 60_000;

export type Theme = "system" | "light" | "dark";

export function loadTab(): Tab {
  const raw = localStorage.getItem(TAB_KEY);
  if (
    raw === "authored" ||
    raw === "review" ||
    raw === "mentions" ||
    raw === "hidden"
  ) {
    return raw;
  }
  return "all";
}

export function loadInterval(): number {
  const raw = localStorage.getItem(INTERVAL_KEY);
  const n = raw ? Number(raw) : NaN;
  return Number.isFinite(n) && n >= 5_000 ? n : DEFAULT_REFRESH_MS;
}

export function persistInterval(value: number): void {
  localStorage.setItem(INTERVAL_KEY, String(value));
}

export function loadNotify(): boolean {
  return localStorage.getItem(NOTIFY_KEY) !== "0";
}

export function persistNotify(enabled: boolean): void {
  localStorage.setItem(NOTIFY_KEY, enabled ? "1" : "0");
}

export function loadTheme(): Theme {
  const raw = localStorage.getItem(THEME_KEY);
  if (raw === "light" || raw === "dark" || raw === "system") return raw;
  return "system";
}

export function persistTheme(value: Theme): void {
  localStorage.setItem(THEME_KEY, value);
}

export function persistTab(value: Tab): void {
  localStorage.setItem(TAB_KEY, value);
}

export function loadExcludedRepos(): string[] {
  try {
    const raw = localStorage.getItem(EXCLUDED_REPOS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function persistExcludedRepos(values: Iterable<string>): void {
  localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify([...values]));
}

export function loadHiddenItems(): number[] {
  try {
    const raw = localStorage.getItem(HIDDEN_ITEMS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function persistHiddenItems(values: Iterable<number>): void {
  localStorage.setItem(HIDDEN_ITEMS_KEY, JSON.stringify([...values]));
}

export function loadWatchedOrgs(): string[] {
  try {
    const raw = localStorage.getItem(WATCHED_ORGS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function persistWatchedOrgs(values: Iterable<string>): void {
  localStorage.setItem(WATCHED_ORGS_KEY, JSON.stringify([...values]));
}
