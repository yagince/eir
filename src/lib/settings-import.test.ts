import { describe, expect, it } from "vitest";
import {
  parseImportedSettings,
  SETTINGS_EXPORT_VERSION,
  type SettingsExport,
} from "$lib/settings-import";

function exported(overrides: Partial<SettingsExport> = {}): unknown {
  return { version: SETTINGS_EXPORT_VERSION, ...overrides };
}

describe("parseImportedSettings", () => {
  it("rejects anything that isn't an object", () => {
    expect(() => parseImportedSettings(null)).toThrow("not a JSON object");
    expect(() => parseImportedSettings("{}")).toThrow("not a JSON object");
    expect(() => parseImportedSettings(42)).toThrow("not a JSON object");
  });

  it("rejects a missing or mismatched version", () => {
    expect(() => parseImportedSettings({})).toThrow("unsupported version: missing");
    expect(() => parseImportedSettings({ version: 99 })).toThrow("unsupported version: 99");
  });

  it("reports every field as absent for a bare export", () => {
    expect(parseImportedSettings(exported())).toEqual({
      refreshMs: undefined,
      notifyEnabled: undefined,
      showLatestComment: undefined,
      includePRs: undefined,
      includeIssues: undefined,
      theme: undefined,
      repoSettings: undefined,
      watchedOrgs: undefined,
      hiddenItems: undefined,
      pinnedItems: undefined,
      toggleShortcut: undefined,
    });
  });

  it("keeps booleans and rejects non-booleans", () => {
    const ok = parseImportedSettings(
      exported({ notifyEnabled: false, showLatestComment: true, includePRs: true }),
    );
    expect(ok.notifyEnabled).toBe(false);
    expect(ok.showLatestComment).toBe(true);
    expect(ok.includePRs).toBe(true);

    const bad = parseImportedSettings(
      exported({ notifyEnabled: "yes", includeIssues: 1 } as unknown as Partial<SettingsExport>),
    );
    expect(bad.notifyEnabled).toBeUndefined();
    expect(bad.includeIssues).toBeUndefined();
  });

  it("drops a refresh interval below the floor instead of clamping it", () => {
    expect(parseImportedSettings(exported({ refreshMs: 30_000 })).refreshMs).toBe(30_000);
    expect(parseImportedSettings(exported({ refreshMs: 5_000 })).refreshMs).toBe(5_000);
    expect(parseImportedSettings(exported({ refreshMs: 1_000 })).refreshMs).toBeUndefined();
    expect(
      parseImportedSettings(exported({ refreshMs: "60000" } as unknown as Partial<SettingsExport>))
        .refreshMs,
    ).toBeUndefined();
  });

  it("accepts only the three known themes", () => {
    expect(parseImportedSettings(exported({ theme: "dark" })).theme).toBe("dark");
    expect(parseImportedSettings(exported({ theme: "system" })).theme).toBe("system");
    expect(
      parseImportedSettings(exported({ theme: "solarized" } as unknown as Partial<SettingsExport>))
        .theme,
    ).toBeUndefined();
  });

  it("filters unusable entries out of string and number lists", () => {
    const parsed = parseImportedSettings(
      exported({
        watchedOrgs: ["yagince", "", 7, null, "acme"],
        hiddenItems: [1, "2", Number.NaN, 3, Infinity],
        pinnedItems: [],
      } as unknown as Partial<SettingsExport>),
    );
    expect(parsed.watchedOrgs).toEqual(["yagince", "acme"]);
    expect(parsed.hiddenItems).toEqual([1, 3]);
    // An empty array is a deliberate "clear the list", so it must survive as
    // present rather than collapsing to undefined.
    expect(parsed.pinnedItems).toEqual([]);
  });

  it("treats a non-array list as absent", () => {
    const parsed = parseImportedSettings(
      exported({ watchedOrgs: "yagince", hiddenItems: 3 } as unknown as Partial<SettingsExport>),
    );
    expect(parsed.watchedOrgs).toBeUndefined();
    expect(parsed.hiddenItems).toBeUndefined();
  });

  it("normalizes repo overrides and keeps the legacy excludedRepos shape working", () => {
    const modern = parseImportedSettings(
      exported({
        repoSettings: {
          "yagince/eir": { prs: true, issues: false },
          "not a repo name": { prs: true, issues: true },
        },
      }),
    );
    expect(modern.repoSettings).toEqual({ "yagince/eir": { prs: true, issues: false } });

    const legacy = parseImportedSettings(exported({ excludedRepos: ["yagince/eir"] }));
    expect(legacy.repoSettings).toEqual({
      "yagince/eir": { prs: false, issues: false },
    });
  });

  it("distinguishes clearing every repo override from not mentioning them", () => {
    expect(parseImportedSettings(exported({ repoSettings: {} })).repoSettings).toEqual({});
    expect(parseImportedSettings(exported()).repoSettings).toBeUndefined();
  });

  it("ignores a blank toggle shortcut", () => {
    expect(parseImportedSettings(exported({ toggleShortcut: "Ctrl+Shift+E" })).toggleShortcut).toBe(
      "Ctrl+Shift+E",
    );
    expect(parseImportedSettings(exported({ toggleShortcut: "" })).toggleShortcut).toBeUndefined();
  });
});
