// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import type { Tab } from "./types";
import {
  DEFAULT_REFRESH_MS,
  EXCLUDED_REPOS_KEY,
  HIDDEN_ITEMS_KEY,
  INCLUDE_ISSUES_KEY,
  INCLUDE_PRS_KEY,
  INTERVAL_KEY,
  isRepoSetting,
  isValidRepoName,
  loadHiddenItems,
  loadIncludeIssues,
  loadIncludePRs,
  loadInterval,
  loadNotify,
  loadPinnedItems,
  loadRepoSettings,
  loadShowLatestComment,
  loadTab,
  loadTheme,
  loadUnreadOnly,
  loadViewMode,
  loadWatchedOrgs,
  normalizeRepoSettingsInput,
  NOTIFY_KEY,
  persistHiddenItems,
  persistIncludeIssues,
  persistIncludePRs,
  persistInterval,
  persistNotify,
  persistPinnedItems,
  persistRepoSettings,
  persistShowLatestComment,
  persistTab,
  persistTheme,
  persistUnreadOnly,
  persistViewMode,
  persistWatchedOrgs,
  PINNED_ITEMS_KEY,
  REPO_SETTINGS_KEY,
  SHOW_LATEST_COMMENT_KEY,
  TAB_KEY,
  type Theme,
  THEME_KEY,
  UNREAD_ONLY_KEY,
  VIEW_MODE_KEY,
  WATCHED_ORGS_KEY,
} from "./storage";

beforeEach(() => {
  localStorage.clear();
});

describe("storage keys", () => {
  it("keeps the on-disk key names stable", () => {
    // These strings are the upgrade contract. Renaming one doesn't migrate
    // anything — it orphans whatever the previous build wrote, so the setting
    // silently resets to its default on the user's next launch.
    expect({
      TAB_KEY,
      INTERVAL_KEY,
      NOTIFY_KEY,
      EXCLUDED_REPOS_KEY,
      REPO_SETTINGS_KEY,
      HIDDEN_ITEMS_KEY,
      PINNED_ITEMS_KEY,
      WATCHED_ORGS_KEY,
      THEME_KEY,
      UNREAD_ONLY_KEY,
      SHOW_LATEST_COMMENT_KEY,
      VIEW_MODE_KEY,
      INCLUDE_PRS_KEY,
      INCLUDE_ISSUES_KEY,
    }).toEqual({
      TAB_KEY: "eir.tab",
      INTERVAL_KEY: "eir.refreshMs",
      NOTIFY_KEY: "eir.notifyEnabled",
      EXCLUDED_REPOS_KEY: "eir.excludedRepos",
      REPO_SETTINGS_KEY: "eir.repoSettings",
      HIDDEN_ITEMS_KEY: "eir.hiddenItems",
      PINNED_ITEMS_KEY: "eir.pinnedItems",
      WATCHED_ORGS_KEY: "eir.watchedOrgs",
      THEME_KEY: "eir.theme",
      UNREAD_ONLY_KEY: "eir.unreadOnly",
      SHOW_LATEST_COMMENT_KEY: "eir.showLatestComment",
      VIEW_MODE_KEY: "eir.viewMode",
      INCLUDE_PRS_KEY: "eir.includePRs",
      INCLUDE_ISSUES_KEY: "eir.includeIssues",
    });
  });
});

describe("loadTab / persistTab", () => {
  it("defaults to the all tab when nothing is stored", () => {
    expect(loadTab()).toBe("all");
  });

  it("round-trips every tab, including hidden", () => {
    const tabs: Tab[] = ["all", "authored", "review", "mentions", "hidden"];
    for (const tab of tabs) {
      persistTab(tab);
      expect(loadTab()).toBe(tab);
    }
  });

  it("falls back to all for a tab name this build doesn't know", () => {
    // A downgrade can find a tab written by a newer build; landing on an empty
    // unknown tab would look like "eir lost all my PRs".
    localStorage.setItem(TAB_KEY, "starred");
    expect(loadTab()).toBe("all");
    localStorage.setItem(TAB_KEY, "");
    expect(loadTab()).toBe("all");
  });
});

describe("loadInterval / persistInterval", () => {
  it("defaults to DEFAULT_REFRESH_MS when nothing is stored", () => {
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
  });

  it("round-trips a valid interval", () => {
    persistInterval(300_000);
    expect(loadInterval()).toBe(300_000);
  });

  it("accepts the 5s floor exactly", () => {
    persistInterval(5_000);
    expect(loadInterval()).toBe(5_000);
  });

  it("falls back to the default below the floor rather than clamping", () => {
    // Anything under 5s would hammer the GitHub API. The stored value is
    // discarded outright, not raised to the floor, so a hand-edited 100 gives
    // the normal 60s cadence instead of a near-floor 5s one.
    persistInterval(4_999);
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
    persistInterval(0);
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
    persistInterval(-60_000);
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
  });

  it("falls back to the default for values that aren't finite numbers", () => {
    localStorage.setItem(INTERVAL_KEY, "60s");
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
    localStorage.setItem(INTERVAL_KEY, "");
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
    localStorage.setItem(INTERVAL_KEY, String(Infinity));
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
    localStorage.setItem(INTERVAL_KEY, JSON.stringify({ ms: 60_000 }));
    expect(loadInterval()).toBe(DEFAULT_REFRESH_MS);
  });
});

describe("loadNotify / persistNotify", () => {
  it("defaults to enabled when nothing is stored", () => {
    expect(loadNotify()).toBe(true);
  });

  it("round-trips both states", () => {
    persistNotify(false);
    expect(loadNotify()).toBe(false);
    persistNotify(true);
    expect(loadNotify()).toBe(true);
  });

  it("treats anything other than the exact opt-out marker as enabled", () => {
    // Notifications are the point of the app, so an unreadable value must fail
    // open rather than silently leave the user with no alerts.
    localStorage.setItem(NOTIFY_KEY, "false");
    expect(loadNotify()).toBe(true);
    localStorage.setItem(NOTIFY_KEY, "");
    expect(loadNotify()).toBe(true);
    localStorage.setItem(NOTIFY_KEY, "0");
    expect(loadNotify()).toBe(false);
  });
});

describe("loadTheme / persistTheme", () => {
  it("defaults to system when nothing is stored", () => {
    expect(loadTheme()).toBe("system");
  });

  it("round-trips all three themes", () => {
    const themes: Theme[] = ["system", "light", "dark"];
    for (const theme of themes) {
      persistTheme(theme);
      expect(loadTheme()).toBe(theme);
    }
  });

  it("falls back to system for an unknown theme", () => {
    localStorage.setItem(THEME_KEY, "solarized");
    expect(loadTheme()).toBe("system");
  });
});

describe("isValidRepoName", () => {
  it("requires an owner/name separator", () => {
    expect(isValidRepoName("yagince/eir")).toBe(true);
    // A bare repo name can't be resolved to a GitHub repo, and would silently
    // never match any item's `repo` field if it were kept as an override key.
    expect(isValidRepoName("eir")).toBe(false);
    expect(isValidRepoName("")).toBe(false);
  });

  it("rejects non-strings", () => {
    expect(isValidRepoName(null)).toBe(false);
    expect(isValidRepoName(undefined)).toBe(false);
    expect(isValidRepoName(42)).toBe(false);
    expect(isValidRepoName(["a/b"])).toBe(false);
  });
});

describe("isRepoSetting", () => {
  it("accepts a pair of booleans", () => {
    expect(isRepoSetting({ prs: true, issues: false })).toBe(true);
  });

  it("rejects partial, mistyped and non-object values", () => {
    expect(isRepoSetting({ prs: true })).toBe(false);
    expect(isRepoSetting({ prs: "true", issues: "false" })).toBe(false);
    expect(isRepoSetting({ prs: 1, issues: 0 })).toBe(false);
    expect(isRepoSetting(null)).toBe(false);
    expect(isRepoSetting(undefined)).toBe(false);
    expect(isRepoSetting("prs")).toBe(false);
    expect(isRepoSetting([true, false])).toBe(false);
  });

  it("tolerates extra keys", () => {
    // A newer build may add a third toggle; an older build reading that file
    // should keep the two flags it understands instead of dropping the repo.
    expect(isRepoSetting({ prs: true, issues: true, discussions: true })).toBe(true);
  });
});

describe("normalizeRepoSettingsInput", () => {
  it("copies only the two known flags", () => {
    expect(
      normalizeRepoSettingsInput(
        { "o/r": { prs: false, issues: true, discussions: true } },
        undefined,
      ),
    ).toEqual({ "o/r": { prs: false, issues: true } });
  });

  it("returns a defensive copy of each setting", () => {
    // The caller feeds the result straight into a live SvelteMap and mutates
    // it as the user toggles checkboxes; sharing objects with the parsed input
    // would let those edits leak back into the source blob.
    const input = { "o/r": { prs: true, issues: true } };
    const out = normalizeRepoSettingsInput(input, undefined);
    out["o/r"].prs = false;
    expect(input["o/r"].prs).toBe(true);
  });

  it("drops entries whose value isn't a repo setting", () => {
    expect(
      normalizeRepoSettingsInput(
        { "keep/me": { prs: true, issues: true }, "drop/me": { prs: true }, "also/drop": null },
        undefined,
      ),
    ).toEqual({ "keep/me": { prs: true, issues: true } });
  });

  it("ignores the legacy list once the new shape is present, even if it yields nothing", () => {
    // The new shape winning is what makes the migration one-way: after the
    // first save, deleting an override must not be undone by the stale
    // excludedRepos array still sitting in localStorage.
    expect(
      normalizeRepoSettingsInput({ "not a repo": { prs: true, issues: true } }, ["legacy/repo"]),
    ).toEqual({});
    expect(normalizeRepoSettingsInput({}, ["legacy/repo"])).toEqual({});
  });

  it("treats an array or null in the new-shape slot as absent and migrates the legacy list", () => {
    expect(normalizeRepoSettingsInput([], ["legacy/repo"])).toEqual({
      "legacy/repo": { prs: false, issues: false },
    });
    expect(normalizeRepoSettingsInput(null, ["legacy/repo"])).toEqual({
      "legacy/repo": { prs: false, issues: false },
    });
    expect(normalizeRepoSettingsInput(undefined, ["legacy/repo"])).toEqual({
      "legacy/repo": { prs: false, issues: false },
    });
  });

  it("filters unusable legacy entries instead of rejecting the whole list", () => {
    expect(
      normalizeRepoSettingsInput(undefined, ["ok/one", "noslash", "", 7, null, "ok/two"]),
    ).toEqual({
      "ok/one": { prs: false, issues: false },
      "ok/two": { prs: false, issues: false },
    });
  });

  it("returns an empty map when neither shape is usable", () => {
    expect(normalizeRepoSettingsInput(undefined, undefined)).toEqual({});
    expect(normalizeRepoSettingsInput("nope", "nope")).toEqual({});
    expect(normalizeRepoSettingsInput(42, { "o/r": true })).toEqual({});
  });
});

describe("loadRepoSettings / persistRepoSettings", () => {
  it("returns an empty map when nothing is stored", () => {
    expect(loadRepoSettings()).toEqual({});
  });

  it("round-trips a map of overrides", () => {
    const settings = {
      "yagince/eir": { prs: true, issues: false },
      "acme/web": { prs: false, issues: true },
    };
    persistRepoSettings(settings);
    expect(loadRepoSettings()).toEqual(settings);
  });

  it("returns an empty map for unparseable JSON", () => {
    localStorage.setItem(REPO_SETTINGS_KEY, "{not json");
    expect(loadRepoSettings()).toEqual({});
  });

  it("returns an empty map when the stored value parses to the wrong type", () => {
    localStorage.setItem(REPO_SETTINGS_KEY, JSON.stringify("yagince/eir"));
    expect(loadRepoSettings()).toEqual({});
    localStorage.setItem(REPO_SETTINGS_KEY, "42");
    expect(loadRepoSettings()).toEqual({});
  });

  it("migrates the legacy excludedRepos array when the new key is absent", () => {
    localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify(["yagince/eir", "bogus"]));
    expect(loadRepoSettings()).toEqual({ "yagince/eir": { prs: false, issues: false } });
  });

  it("prefers the new key over a legacy list that is still present", () => {
    persistRepoSettings({ "acme/web": { prs: true, issues: false } });
    localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify(["yagince/eir"]));
    expect(loadRepoSettings()).toEqual({ "acme/web": { prs: true, issues: false } });
  });

  it("does not resurrect the legacy list after the user clears every override", () => {
    // persistRepoSettings({}) is what "I un-excluded everything" writes. The
    // stale legacy key must stay ignored or the exclusions come back on restart.
    persistRepoSettings({});
    localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify(["yagince/eir"]));
    expect(loadRepoSettings()).toEqual({});
  });

  it("falls back to the legacy list when the new key holds a JSON null", () => {
    localStorage.setItem(REPO_SETTINGS_KEY, "null");
    localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify(["yagince/eir"]));
    expect(loadRepoSettings()).toEqual({ "yagince/eir": { prs: false, issues: false } });
  });

  it("keeps valid new-shape overrides when the legacy key is corrupt", () => {
    // Each key is parsed on its own. Sharing one try meant leftover garbage in
    // the legacy key threw before normalization and took every per-repo override
    // down with it.
    persistRepoSettings({ "yagince/eir": { prs: true, issues: false } });
    localStorage.setItem(EXCLUDED_REPOS_KEY, "[oops");
    expect(loadRepoSettings()).toEqual({ "yagince/eir": { prs: true, issues: false } });
  });

  it("falls back to the legacy list when the new key is the corrupt one", () => {
    localStorage.setItem(REPO_SETTINGS_KEY, "{oops");
    localStorage.setItem(EXCLUDED_REPOS_KEY, JSON.stringify(["yagince/eir"]));
    expect(loadRepoSettings()).toEqual({ "yagince/eir": { prs: false, issues: false } });
  });
});

describe("loadHiddenItems / persistHiddenItems", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadHiddenItems()).toEqual([]);
  });

  it("round-trips an iterable, deduplicating via the caller's Set", () => {
    persistHiddenItems(new Set([3, 1, 3, 2]));
    expect(loadHiddenItems()).toEqual([3, 1, 2]);
  });

  it("round-trips an empty selection", () => {
    persistHiddenItems(new Set<number>());
    expect(loadHiddenItems()).toEqual([]);
  });

  it("returns an empty list for unparseable JSON", () => {
    localStorage.setItem(HIDDEN_ITEMS_KEY, "[1, 2,");
    expect(loadHiddenItems()).toEqual([]);
  });

  it("rejects a parseable non-array instead of handing back a non-iterable", () => {
    // The caller does `new SvelteSet(loadHiddenItems())` during module init, so
    // returning an object or a number here used to throw before the popup could
    // render, with no way back short of clearing localStorage by hand.
    for (const stored of [{ 1: true }, 5, "nope", null]) {
      localStorage.setItem(HIDDEN_ITEMS_KEY, JSON.stringify(stored));
      expect(loadHiddenItems()).toEqual([]);
    }
  });

  it("drops entries of the wrong type rather than the whole list", () => {
    localStorage.setItem(HIDDEN_ITEMS_KEY, JSON.stringify([1, "two", null, 3, Number.NaN]));
    expect(loadHiddenItems()).toEqual([1, 3]);
  });
});

describe("loadPinnedItems / persistPinnedItems", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadPinnedItems()).toEqual([]);
  });

  it("round-trips pinned ids in order", () => {
    persistPinnedItems([10, 20, 30]);
    expect(loadPinnedItems()).toEqual([10, 20, 30]);
  });

  it("returns an empty list for unparseable JSON", () => {
    localStorage.setItem(PINNED_ITEMS_KEY, "not json");
    expect(loadPinnedItems()).toEqual([]);
  });

  it("uses its own key, independent of hidden items", () => {
    // Hiding and pinning are opposite actions; sharing a key would make one
    // overwrite the other.
    persistHiddenItems([1]);
    persistPinnedItems([2]);
    expect(loadHiddenItems()).toEqual([1]);
    expect(loadPinnedItems()).toEqual([2]);
  });
});

describe("loadWatchedOrgs / persistWatchedOrgs", () => {
  it("returns an empty list when nothing is stored", () => {
    expect(loadWatchedOrgs()).toEqual([]);
  });

  it("round-trips org logins", () => {
    persistWatchedOrgs(new Set(["yagince", "acme"]));
    expect(loadWatchedOrgs()).toEqual(["yagince", "acme"]);
  });

  it("returns an empty list for unparseable JSON", () => {
    localStorage.setItem(WATCHED_ORGS_KEY, '["yagince"');
    expect(loadWatchedOrgs()).toEqual([]);
  });

  it("rejects a parseable non-array", () => {
    localStorage.setItem(WATCHED_ORGS_KEY, JSON.stringify("yagince"));
    expect(loadWatchedOrgs()).toEqual([]);
  });

  it("drops blanks and non-strings rather than the whole list", () => {
    localStorage.setItem(WATCHED_ORGS_KEY, JSON.stringify(["yagince", "", 7, null, "acme"]));
    expect(loadWatchedOrgs()).toEqual(["yagince", "acme"]);
  });
});

describe("loadUnreadOnly / persistUnreadOnly", () => {
  it("defaults to off when nothing is stored", () => {
    expect(loadUnreadOnly()).toBe(false);
  });

  it("round-trips both states", () => {
    persistUnreadOnly(true);
    expect(loadUnreadOnly()).toBe(true);
    persistUnreadOnly(false);
    expect(loadUnreadOnly()).toBe(false);
  });

  it("requires the exact opt-in marker", () => {
    // Opposite default to notify/showLatestComment: an unreadable value must
    // not start hiding items the user can still see today.
    localStorage.setItem(UNREAD_ONLY_KEY, "true");
    expect(loadUnreadOnly()).toBe(false);
    localStorage.setItem(UNREAD_ONLY_KEY, "");
    expect(loadUnreadOnly()).toBe(false);
  });
});

describe("loadShowLatestComment / persistShowLatestComment", () => {
  it("defaults to on when nothing is stored", () => {
    expect(loadShowLatestComment()).toBe(true);
  });

  it("round-trips both states", () => {
    persistShowLatestComment(false);
    expect(loadShowLatestComment()).toBe(false);
    persistShowLatestComment(true);
    expect(loadShowLatestComment()).toBe(true);
  });

  it("treats anything other than the exact opt-out marker as on", () => {
    localStorage.setItem(SHOW_LATEST_COMMENT_KEY, "false");
    expect(loadShowLatestComment()).toBe(true);
  });
});

describe("loadViewMode / persistViewMode", () => {
  it("defaults to grouped when nothing is stored", () => {
    expect(loadViewMode()).toBe("grouped");
  });

  it("round-trips both modes", () => {
    persistViewMode("recent");
    expect(loadViewMode()).toBe("recent");
    persistViewMode("grouped");
    expect(loadViewMode()).toBe("grouped");
  });

  it("falls back to grouped for an unknown mode", () => {
    localStorage.setItem(VIEW_MODE_KEY, "kanban");
    expect(loadViewMode()).toBe("grouped");
  });
});

describe("loadIncludePRs / loadIncludeIssues", () => {
  it("default to on so a fresh install fetches both kinds", () => {
    expect(loadIncludePRs()).toBe(true);
    expect(loadIncludeIssues()).toBe(true);
  });

  it("round-trip independently of each other", () => {
    // These two drive the Rust-side query set; a shared key would make
    // disabling PRs also silently disable issues.
    persistIncludePRs(false);
    persistIncludeIssues(true);
    expect(loadIncludePRs()).toBe(false);
    expect(loadIncludeIssues()).toBe(true);

    persistIncludePRs(true);
    persistIncludeIssues(false);
    expect(loadIncludePRs()).toBe(true);
    expect(loadIncludeIssues()).toBe(false);
  });

  it("treat anything other than the exact opt-out marker as on", () => {
    localStorage.setItem(INCLUDE_PRS_KEY, "no");
    localStorage.setItem(INCLUDE_ISSUES_KEY, "");
    expect(loadIncludePRs()).toBe(true);
    expect(loadIncludeIssues()).toBe(true);
  });
});
