import { normalizeRepoSettingsInput, type RepoSetting, type Theme } from "$lib/storage";

/// Bumped when the on-disk shape changes incompatibly. An export carrying any
/// other value is rejected outright rather than partially applied.
export const SETTINGS_EXPORT_VERSION = 1;

/// Anything below this would hammer the GitHub API, so a shorter interval in an
/// imported file is treated as absent rather than clamped: the user keeps the
/// interval they already had.
const MIN_REFRESH_MS = 5_000;

export type SettingsExport = {
  version: number;
  refreshMs?: number;
  notifyEnabled?: boolean;
  showLatestComment?: boolean;
  includePRs?: boolean;
  includeIssues?: boolean;
  theme?: Theme;
  /// Pre-per-repo builds wrote this string array; still read on import so a
  /// round-trip doesn't silently drop someone's exclusion list.
  excludedRepos?: string[];
  repoSettings?: Record<string, RepoSetting>;
  watchedOrgs?: string[];
  hiddenItems?: number[];
  pinnedItems?: number[];
  toggleShortcut?: string;
};

/// A validated export. Every field is `undefined` unless the input carried a
/// usable value, so the caller applies exactly the settings that were present
/// and leaves the rest of its state alone.
export type ImportedSettings = {
  refreshMs?: number;
  notifyEnabled?: boolean;
  showLatestComment?: boolean;
  includePRs?: boolean;
  includeIssues?: boolean;
  theme?: Theme;
  repoSettings?: Record<string, RepoSetting>;
  watchedOrgs?: string[];
  hiddenItems?: number[];
  pinnedItems?: number[];
  toggleShortcut?: string;
};

function pickBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function pickRefreshMs(value: unknown): number | undefined {
  return typeof value === "number" && value >= MIN_REFRESH_MS ? value : undefined;
}

function pickTheme(value: unknown): Theme | undefined {
  return value === "system" || value === "light" || value === "dark" ? value : undefined;
}

function pickNonEmptyString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

/// Drops non-strings and blanks instead of rejecting the whole array, so one bad
/// entry in a hand-edited file doesn't cost the user the rest of the list.
function pickStrings(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter((v): v is string => typeof v === "string" && v.length > 0);
}

function pickFiniteNumbers(value: unknown): number[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter((v): v is number => typeof v === "number" && Number.isFinite(v));
}

/// Validate a parsed settings JSON blob. Throws when the file isn't a settings
/// export at all — a wrong shape or an unknown version is the user picking the
/// wrong file, which deserves an error rather than a silent no-op. Individual
/// fields are forgiving: anything unusable is reported as absent.
export function parseImportedSettings(raw: unknown): ImportedSettings {
  if (!raw || typeof raw !== "object") {
    throw new Error("not a JSON object");
  }
  const data = raw as Partial<SettingsExport>;
  if (data.version !== SETTINGS_EXPORT_VERSION) {
    throw new Error(
      `unsupported version: ${data.version ?? "missing"} (expected ${SETTINGS_EXPORT_VERSION})`,
    );
  }

  // Repo overrides come from either the current map or the legacy string array,
  // so absence has to be decided before normalizing: normalizeRepoSettingsInput
  // returns {} for "nothing here", which is indistinguishable from a deliberate
  // "clear all overrides".
  const hasRepoInput = data.repoSettings !== undefined || data.excludedRepos !== undefined;

  return {
    refreshMs: pickRefreshMs(data.refreshMs),
    notifyEnabled: pickBoolean(data.notifyEnabled),
    showLatestComment: pickBoolean(data.showLatestComment),
    includePRs: pickBoolean(data.includePRs),
    includeIssues: pickBoolean(data.includeIssues),
    theme: pickTheme(data.theme),
    repoSettings: hasRepoInput
      ? normalizeRepoSettingsInput(data.repoSettings, data.excludedRepos)
      : undefined,
    watchedOrgs: pickStrings(data.watchedOrgs),
    hiddenItems: pickFiniteNumbers(data.hiddenItems),
    pinnedItems: pickFiniteNumbers(data.pinnedItems),
    toggleShortcut: pickNonEmptyString(data.toggleShortcut),
  };
}
