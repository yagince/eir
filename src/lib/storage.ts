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
  if (raw === "authored" || raw === "review" || raw === "mentions" || raw === "hidden") {
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
    for (const [repo, val] of Object.entries(newShape as Record<string, unknown>)) {
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

/// JSON.parse, or undefined when the slot is empty or unparseable.
///
/// Each key is parsed on its own so one corrupt value can't take another down
/// with it: both parses used to sit inside a single try, which meant leftover
/// garbage in the legacy key discarded every per-repo override the user had.
function parseStored(key: string): unknown {
  const raw = localStorage.getItem(key);
  if (!raw) return undefined;
  try {
    return JSON.parse(raw);
  } catch {
    return undefined;
  }
}

export function loadRepoSettings(): Record<string, RepoSetting> {
  return normalizeRepoSettingsInput(
    parseStored(REPO_SETTINGS_KEY),
    parseStored(EXCLUDED_REPOS_KEY),
  );
}

export function persistRepoSettings(settings: Record<string, RepoSetting>): void {
  localStorage.setItem(REPO_SETTINGS_KEY, JSON.stringify(settings));
}

/// Elements of the right type from a stored JSON array, or `[]`.
///
/// The type check is not decoration: these results go straight into
/// `new SvelteSet(...)` while the popup's module is initialising, so a stored
/// value that parses but isn't iterable — `5`, or an object — used to throw
/// there and leave the popup rendering nothing at all, with no way back short of
/// clearing localStorage by hand. Filtering per element also means one bad entry
/// costs only itself.
function loadTypedList<T>(key: string, keep: (v: unknown) => v is T): T[] {
  const parsed = parseStored(key);
  if (!Array.isArray(parsed)) return [];
  return parsed.filter(keep);
}

function isFiniteNumber(v: unknown): v is number {
  return typeof v === "number" && Number.isFinite(v);
}

function isNonEmptyString(v: unknown): v is string {
  return typeof v === "string" && v.length > 0;
}

export function loadHiddenItems(): number[] {
  return loadTypedList(HIDDEN_ITEMS_KEY, isFiniteNumber);
}

export function persistHiddenItems(values: Iterable<number>): void {
  localStorage.setItem(HIDDEN_ITEMS_KEY, JSON.stringify([...values]));
}

export function loadPinnedItems(): number[] {
  return loadTypedList(PINNED_ITEMS_KEY, isFiniteNumber);
}

export function persistPinnedItems(values: Iterable<number>): void {
  localStorage.setItem(PINNED_ITEMS_KEY, JSON.stringify([...values]));
}

export function loadWatchedOrgs(): string[] {
  return loadTypedList(WATCHED_ORGS_KEY, isNonEmptyString);
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
