import type { Tab, ViewMode } from "$lib/types";

export const TAB_KEY = "eir.tab";
export const INTERVAL_KEY = "eir.refreshMs";
export const NOTIFY_KEY = "eir.notifyEnabled";
/// Legacy key — superseded by `eir.repoSettings`. Kept readable for one
/// release so a user who downgrades doesn't lose their excluded list, but
/// new code writes only to `eir.repoSettings`.
export const EXCLUDED_REPOS_KEY = "eir.excludedRepos";
export const REPO_SETTINGS_KEY = "eir.repoSettings";
export const HIDDEN_ITEMS_KEY = "eir.hiddenItems";
export const PINNED_ITEMS_KEY = "eir.pinnedItems";
export const WATCHED_ORGS_KEY = "eir.watchedOrgs";
export const THEME_KEY = "eir.theme";
export const UNREAD_ONLY_KEY = "eir.unreadOnly";
export const SHOW_LATEST_COMMENT_KEY = "eir.showLatestComment";
export const VIEW_MODE_KEY = "eir.viewMode";
export const INCLUDE_PRS_KEY = "eir.includePRs";
export const INCLUDE_ISSUES_KEY = "eir.includeIssues";

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

export type RepoSetting = { prs: boolean; issues: boolean };

export function isRepoSetting(value: unknown): value is RepoSetting {
  return (
    value !== null &&
    typeof value === "object" &&
    typeof (value as RepoSetting).prs === "boolean" &&
    typeof (value as RepoSetting).issues === "boolean"
  );
}

export function isValidRepoName(value: unknown): value is string {
  return typeof value === "string" && value.includes("/");
}

/// Coerce arbitrary input into a clean repoSettings map. Accepts either
/// the new shape (Record<repo, RepoSetting>) or the legacy excludedRepos
/// shape (string[] of fully-hidden repos), so a load from localStorage
/// and an import from a saved JSON file can share the same migration.
export function normalizeRepoSettingsInput(
  newShape: unknown,
  legacyExcluded: unknown,
): Record<string, RepoSetting> {
  const out: Record<string, RepoSetting> = {};
  if (newShape && typeof newShape === "object" && !Array.isArray(newShape)) {
    for (const [repo, val] of Object.entries(
      newShape as Record<string, unknown>,
    )) {
      if (isValidRepoName(repo) && isRepoSetting(val)) {
        out[repo] = { prs: val.prs, issues: val.issues };
      }
    }
    return out;
  }
  if (Array.isArray(legacyExcluded)) {
    for (const repo of legacyExcluded) {
      if (isValidRepoName(repo)) {
        out[repo] = { prs: false, issues: false };
      }
    }
  }
  return out;
}

export function loadRepoSettings(): Record<string, RepoSetting> {
  try {
    const raw = localStorage.getItem(REPO_SETTINGS_KEY);
    const legacy = localStorage.getItem(EXCLUDED_REPOS_KEY);
    return normalizeRepoSettingsInput(
      raw ? JSON.parse(raw) : undefined,
      legacy ? JSON.parse(legacy) : undefined,
    );
  } catch {
    return {};
  }
}

export function persistRepoSettings(
  settings: Record<string, RepoSetting>,
): void {
  localStorage.setItem(REPO_SETTINGS_KEY, JSON.stringify(settings));
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

export function loadPinnedItems(): number[] {
  try {
    const raw = localStorage.getItem(PINNED_ITEMS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function persistPinnedItems(values: Iterable<number>): void {
  localStorage.setItem(PINNED_ITEMS_KEY, JSON.stringify([...values]));
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

export function loadUnreadOnly(): boolean {
  return localStorage.getItem(UNREAD_ONLY_KEY) === "1";
}

export function persistUnreadOnly(enabled: boolean): void {
  localStorage.setItem(UNREAD_ONLY_KEY, enabled ? "1" : "0");
}

export function loadShowLatestComment(): boolean {
  return localStorage.getItem(SHOW_LATEST_COMMENT_KEY) !== "0";
}

export function persistShowLatestComment(enabled: boolean): void {
  localStorage.setItem(SHOW_LATEST_COMMENT_KEY, enabled ? "1" : "0");
}

export function loadViewMode(): ViewMode {
  const raw = localStorage.getItem(VIEW_MODE_KEY);
  return raw === "recent" ? "recent" : "grouped";
}

export function persistViewMode(value: ViewMode): void {
  localStorage.setItem(VIEW_MODE_KEY, value);
}

export function loadIncludePRs(): boolean {
  return localStorage.getItem(INCLUDE_PRS_KEY) !== "0";
}

export function persistIncludePRs(enabled: boolean): void {
  localStorage.setItem(INCLUDE_PRS_KEY, enabled ? "1" : "0");
}

export function loadIncludeIssues(): boolean {
  return localStorage.getItem(INCLUDE_ISSUES_KEY) !== "0";
}

export function persistIncludeIssues(enabled: boolean): void {
  localStorage.setItem(INCLUDE_ISSUES_KEY, enabled ? "1" : "0");
}
