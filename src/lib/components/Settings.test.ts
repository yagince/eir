// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { tick } from "svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import Settings from "$lib/components/Settings.svelte";
import type { RepoSetting } from "$lib/storage";

/// Every callback the panel can reach, so a test can assert that pressing one
/// control leaves the others alone.
function handlers() {
  return {
    onBack: vi.fn(),
    onIntervalChange: vi.fn(),
    onNotifyChange: vi.fn(),
    onShowLatestCommentChange: vi.fn(),
    onIncludePRsChange: vi.fn(),
    onIncludeIssuesChange: vi.fn(),
    onThemeChange: vi.fn(),
    onToggleAutostart: vi.fn(),
    onDiagnosticsChange: vi.fn(),
    onSendTestNotification: vi.fn(),
    onRunUpdateCheck: vi.fn(),
    onInstallUpdate: vi.fn(),
    onStartCaptureShortcut: vi.fn(),
    onAddWatchedOrg: vi.fn(),
    onRemoveWatchedOrg: vi.fn(),
    onAddRepoOverride: vi.fn(),
    onRemoveRepoOverride: vi.fn(),
    onUpdateRepoOverride: vi.fn(),
    onExportSettings: vi.fn(),
    onImportSettings: vi.fn(),
  };
}

function mount(overrides: Record<string, unknown> = {}) {
  const on = handlers();
  render(Settings, {
    refreshMs: 60_000,
    refreshOptions: [
      { value: 30_000, label: "30 seconds" },
      { value: 60_000, label: "1 minute" },
      { value: 300_000, label: "5 minutes" },
    ],
    notifyEnabled: true,
    showLatestComment: true,
    includePRs: true,
    includeIssues: true,
    theme: "system",
    themeOptions: [
      { value: "system", label: "System" },
      { value: "light", label: "Light" },
      { value: "dark", label: "Dark" },
    ],
    autostartEnabled: true,
    diagnosticsEnabled: false,
    appVersion: "1.0.1",
    toggleShortcut: "Cmd+Shift+E",
    capturingShortcut: false,
    shortcutError: null,
    updateStatus: { kind: "idle" },
    watchedOrgs: new SvelteSet<string>(),
    repoSettings: new SvelteMap<string, RepoSetting>(),
    orgSuggestions: [],
    repoSuggestions: [],
    error: null,
    settingsIoNotice: null,
    settingsIoError: null,
    newWatchedOrg: "",
    newRepoOverride: "",
    ...on,
    ...overrides,
  });
  return on;
}

/// The watched-org and repo-override sections each end in a bare "Add" button,
/// so neither can be picked by name alone. DOM order is orgs, then overrides.
function addButton(section: "orgs" | "repos") {
  return screen.getAllByRole("button", { name: "Add" })[section === "orgs" ? 0 : 1];
}

describe("Settings", () => {
  it("goes back without touching any setting", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "← Back" }));
    expect(on.onBack).toHaveBeenCalledOnce();
    expect(on.onIntervalChange).not.toHaveBeenCalled();
    expect(on.onNotifyChange).not.toHaveBeenCalled();
  });

  it("reports the interval as a number, not the string the DOM gives it", async () => {
    const on = mount();
    await fireEvent.change(screen.getByRole("combobox", { name: /Refresh interval/i }), {
      target: { value: "300000" },
    });
    expect(on.onIntervalChange).toHaveBeenCalledWith(300_000);
  });

  it("reports the new checkbox state, not the old one", async () => {
    const on = mount({ notifyEnabled: true });
    await fireEvent.click(screen.getByRole("checkbox", { name: /Desktop notifications/i }));
    expect(on.onNotifyChange).toHaveBeenCalledWith(false);
  });

  it("shows the current version", () => {
    mount();
    expect(screen.getByText("— v1.0.1")).toBeInTheDocument();
  });
});

describe("Settings update row", () => {
  it("offers a check when nothing is known yet", async () => {
    const on = mount({ updateStatus: { kind: "idle" } });
    await fireEvent.click(screen.getByRole("button", { name: "Check" }));
    expect(on.onRunUpdateCheck).toHaveBeenCalledOnce();
    expect(on.onInstallUpdate).not.toHaveBeenCalled();
  });

  it("switches the same button to installing once an update is on deck", async () => {
    const on = mount({
      updateStatus: { kind: "available", update: { version: "1.0.2" } },
    });
    expect(screen.getByText("· v1.0.2 available")).toBeInTheDocument();

    await fireEvent.click(screen.getByRole("button", { name: "Install v1.0.2" }));
    expect(on.onInstallUpdate).toHaveBeenCalledOnce();
    expect(on.onRunUpdateCheck).not.toHaveBeenCalled();
  });

  it("disables the button while a check or install is in flight", () => {
    mount({ updateStatus: { kind: "checking" } });
    expect(screen.getByRole("button", { name: "Checking…" })).toBeDisabled();
  });

  it("keeps the button dead while the download runs", () => {
    // A second click here would start a parallel download of the same bundle,
    // so "downloading" has to disable exactly like "checking" does.
    mount({ updateStatus: { kind: "downloading" } });
    expect(screen.getByRole("button", { name: "Installing…" })).toBeDisabled();
  });

  it("surfaces a failed check inline", () => {
    // This row is where v0.17.7's notification TypeError became visible, so the
    // error path is worth pinning: the message has to reach the user rather than
    // being swallowed into an unchanged "Check".
    mount({ updateStatus: { kind: "error", message: "network unreachable" } });
    expect(screen.getByText("· network unreachable")).toBeInTheDocument();
  });

  it("reports up-to-date without offering an install", () => {
    mount({ updateStatus: { kind: "up-to-date" } });
    expect(screen.getByText("· already latest")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check" })).toBeEnabled();
  });
});

describe("Settings toggles", () => {
  it("reports the new show-latest-comment state", async () => {
    const on = mount({ showLatestComment: true });
    await fireEvent.click(screen.getByRole("checkbox", { name: /Show latest comment/i }));
    expect(on.onShowLatestCommentChange).toHaveBeenCalledWith(false);
  });

  it("routes each kind toggle to its own callback", async () => {
    // Both kind toggles look identical and sit next to each other; a crossed
    // handler would silently flip the wrong query set.
    const on = mount({ includePRs: true, includeIssues: true });

    await fireEvent.click(screen.getByRole("checkbox", { name: "Include PRs" }));
    expect(on.onIncludePRsChange).toHaveBeenCalledWith(false);
    expect(on.onIncludeIssuesChange).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("checkbox", { name: "Include Issues" }));
    expect(on.onIncludeIssuesChange).toHaveBeenCalledWith(false);
    expect(on.onIncludePRsChange).toHaveBeenCalledOnce();
  });

  it("locks the last remaining kind on", () => {
    // Turning both off would leave nothing to search for, so whichever kind is
    // the only one left has to be un-untickable.
    mount({ includePRs: true, includeIssues: false });
    expect(screen.getByRole("checkbox", { name: "Include PRs" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Include Issues" })).toBeEnabled();
  });

  it("locks the last remaining kind on in the mirrored case", () => {
    mount({ includePRs: false, includeIssues: true });
    expect(screen.getByRole("checkbox", { name: "Include Issues" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Include PRs" })).toBeEnabled();
  });

  it("lets a switched-off kind be switched back on", async () => {
    const on = mount({ includePRs: false, includeIssues: true });
    const prs = screen.getByRole("checkbox", { name: "Include PRs" });
    expect(prs).not.toBeChecked();
    await fireEvent.click(prs);
    expect(on.onIncludePRsChange).toHaveBeenCalledWith(true);
  });

  it("reports the picked theme", async () => {
    const on = mount({ theme: "system" });
    await fireEvent.change(screen.getByRole("combobox", { name: /Theme/i }), {
      target: { value: "dark" },
    });
    expect(on.onThemeChange).toHaveBeenCalledWith("dark");
  });

  it("shows the theme that is already in effect", () => {
    mount({ theme: "light" });
    expect(screen.getByRole("combobox", { name: /Theme/i })).toHaveValue("light");
  });

  it("reports the new autostart state", async () => {
    const on = mount({ autostartEnabled: true });
    await fireEvent.click(screen.getByRole("checkbox", { name: /Start at login/i }));
    expect(on.onToggleAutostart).toHaveBeenCalledWith(false);
  });

  it("refuses to guess at autostart before the plugin answers", () => {
    // `null` means "we don't know yet / not supported here". Showing an
    // interactive unticked box would invite a click that flips a state we
    // never read, so the row is inert until a real boolean arrives.
    mount({ autostartEnabled: null });
    const box = screen.getByRole("checkbox", { name: /Start at login/i });
    expect(box).toBeDisabled();
    expect(box).not.toBeChecked();
  });

  it("reports the new diagnostics state", async () => {
    const on = mount({ diagnosticsEnabled: false });
    await fireEvent.click(screen.getByRole("checkbox", { name: /Auth diagnostics log/i }));
    expect(on.onDiagnosticsChange).toHaveBeenCalledWith(true);
  });

  it("sends a test notification without changing any setting", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(on.onSendTestNotification).toHaveBeenCalledOnce();
    expect(on.onNotifyChange).not.toHaveBeenCalled();
  });
});

describe("Settings shortcut row", () => {
  it("shows the bound combo and starts capture when it is clicked", async () => {
    const on = mount({ toggleShortcut: "Cmd+Shift+E" });
    await fireEvent.click(screen.getByRole("button", { name: "Cmd+Shift+E" }));
    expect(on.onStartCaptureShortcut).toHaveBeenCalledOnce();
  });

  it("announces that it is listening while capturing", () => {
    // The capture has no other affordance — if the label kept showing the old
    // combo the user would have no idea the next keypress is being recorded.
    mount({ toggleShortcut: "Cmd+Shift+E", capturingShortcut: true });
    expect(screen.getByRole("button", { name: "Press keys…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cmd+Shift+E" })).not.toBeInTheDocument();
  });

  it("surfaces a rejected shortcut", () => {
    mount({ shortcutError: "Cmd+Q is reserved by the system" });
    expect(screen.getByText("Cmd+Q is reserved by the system")).toBeInTheDocument();
  });

  it("splits the bound combo into one key cap per key", () => {
    // Keys the built-in reference list never mentions, so each cap found here
    // can only have come from splitting `toggleShortcut`.
    mount({ toggleShortcut: "Ctrl+Alt+K" });
    for (const cap of ["Ctrl", "Alt", "K"]) {
      expect(screen.getByText(cap)).toBeInTheDocument();
    }
  });
});

describe("Settings watched orgs", () => {
  it("explains what the list is for while it is empty", () => {
    mount({ watchedOrgs: new SvelteSet<string>() });
    expect(screen.getByText(/Add an org login/i)).toBeInTheDocument();
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });

  it("lists orgs alphabetically, not in the order they were added", () => {
    mount({ watchedOrgs: new SvelteSet(["zeta-inc", "acme"]) });
    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("acme");
    expect(rows[1]).toHaveTextContent("zeta-inc");
  });

  it("removes the org whose row was clicked", async () => {
    const on = mount({ watchedOrgs: new SvelteSet(["acme", "zeta-inc"]) });
    const rows = screen.getAllByRole("listitem");
    await fireEvent.click(within(rows[1]).getByRole("button", { name: "Remove" }));
    expect(on.onRemoveWatchedOrg).toHaveBeenCalledWith("zeta-inc");
  });

  it("adds from the button and from Enter in the field", async () => {
    const on = mount({ newWatchedOrg: "acme" });
    await fireEvent.click(addButton("orgs"));
    expect(on.onAddWatchedOrg).toHaveBeenCalledOnce();

    await fireEvent.keyDown(screen.getByPlaceholderText(/org login/i), { key: "Enter" });
    expect(on.onAddWatchedOrg).toHaveBeenCalledTimes(2);
  });

  it("ignores other keys in the org field", async () => {
    // Typing has to stay typing — only Enter commits.
    const on = mount({ newWatchedOrg: "acm" });
    await fireEvent.keyDown(screen.getByPlaceholderText(/org login/i), { key: "e" });
    expect(on.onAddWatchedOrg).not.toHaveBeenCalled();
  });

  it("hands an empty entry to the parent anyway", async () => {
    // The empty check lives in +page.svelte's addWatchedOrg. If the panel
    // swallowed the click the parent could never clear the field or report it.
    const on = mount({ newWatchedOrg: "" });
    await fireEvent.click(addButton("orgs"));
    expect(on.onAddWatchedOrg).toHaveBeenCalledOnce();
  });

  it("hands a malformed entry to the parent anyway", async () => {
    // Trimming and character-stripping are the parent's job too, so a login
    // made only of illegal characters still has to reach it.
    const on = mount({ newWatchedOrg: "  !!!  " });
    await fireEvent.click(addButton("orgs"));
    expect(on.onAddWatchedOrg).toHaveBeenCalledOnce();
  });

  it("picks up an org added while the panel is open", async () => {
    // The parent mutates this very set instead of handing over a new one, which
    // only shows up in the DOM because it is a SvelteSet. A plain Set would
    // leave the panel showing the stale list until it was reopened.
    const orgs = new SvelteSet(["mid-co"]);
    mount({ watchedOrgs: orgs });
    orgs.add("acme");
    await tick();

    const rows = screen.getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("acme");
  });

  it("offers the known orgs as autocomplete for the field", () => {
    mount({ orgSuggestions: ["Lecto-inc", "yagince"] });
    const field = screen.getByPlaceholderText(/org login/i);
    expect(field).toHaveAttribute("list", "org-suggestions");
    // The datalist is reachable only through the id the input names, so assert
    // both halves of that wiring — a renamed id breaks autocomplete silently.
    const list = document.getElementById("org-suggestions") as HTMLDataListElement;
    expect([...list.options].map((o) => o.value)).toEqual(["Lecto-inc", "yagince"]);
  });
});

describe("Settings repo overrides", () => {
  it("explains what overrides do while there are none", () => {
    mount({ repoSettings: new SvelteMap<string, RepoSetting>() });
    expect(screen.getByText(/All repos respect the global Include toggles/i)).toBeInTheDocument();
    expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  });

  it("sorts override rows by repo name, not by insertion order", () => {
    // The map is rebuilt on every settings tweak; an unsorted render would
    // reshuffle rows under the user's cursor mid-click.
    mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["zzz/last", { prs: true, issues: true }],
        ["aaa/first", { prs: true, issues: true }],
      ]),
    });
    const rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("aaa/first");
    expect(rows[1]).toHaveTextContent("zzz/last");
  });

  it("carries the untouched Issues flag through a PR toggle", async () => {
    // Each row reports the whole RepoSetting, so the flag that wasn't clicked
    // has to be copied from the current one — reading the checkbox for both
    // would quietly drag Issues along with PRs. The flags start out different
    // from the value the click produces so a crossed read can't pass.
    const on = mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["yagince/eir", { prs: true, issues: true }],
      ]),
    });
    await fireEvent.click(screen.getByRole("checkbox", { name: "PRs" }));
    expect(on.onUpdateRepoOverride).toHaveBeenCalledWith("yagince/eir", {
      prs: false,
      issues: true,
    });
  });

  it("carries the untouched PR flag through an Issues toggle", async () => {
    const on = mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["yagince/eir", { prs: true, issues: true }],
      ]),
    });
    await fireEvent.click(screen.getByRole("checkbox", { name: "Issues" }));
    expect(on.onUpdateRepoOverride).toHaveBeenCalledWith("yagince/eir", {
      prs: true,
      issues: false,
    });
  });

  it("brings a fully hidden repo back with one kind ticked", async () => {
    // Both flags off is how a repo gets hidden entirely; ticking PRs here has
    // to leave Issues off rather than restoring the whole repo.
    const on = mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["yagince/eir", { prs: false, issues: false }],
      ]),
    });
    await fireEvent.click(screen.getByRole("checkbox", { name: "PRs" }));
    expect(on.onUpdateRepoOverride).toHaveBeenCalledWith("yagince/eir", {
      prs: true,
      issues: false,
    });
  });

  it("reflects the stored flags in the row's boxes", () => {
    mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["yagince/eir", { prs: false, issues: true }],
      ]),
    });
    expect(screen.getByRole("checkbox", { name: "PRs" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Issues" })).toBeChecked();
  });

  it("toggles the row the click landed on when several repos are listed", async () => {
    const on = mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["aaa/first", { prs: true, issues: true }],
        ["zzz/last", { prs: true, issues: true }],
      ]),
    });
    const rows = screen.getAllByRole("listitem");
    await fireEvent.click(within(rows[1]).getByRole("checkbox", { name: "PRs" }));
    expect(on.onUpdateRepoOverride).toHaveBeenCalledWith("zzz/last", { prs: false, issues: true });
  });

  it("removes the override whose row was clicked", async () => {
    const on = mount({
      repoSettings: new SvelteMap<string, RepoSetting>([
        ["aaa/first", { prs: true, issues: true }],
        ["zzz/last", { prs: true, issues: true }],
      ]),
    });
    const rows = screen.getAllByRole("listitem");
    await fireEvent.click(within(rows[0]).getByRole("button", { name: "Remove override" }));
    expect(on.onRemoveRepoOverride).toHaveBeenCalledWith("aaa/first");
  });

  it("adds from the button and from Enter in the field", async () => {
    const on = mount({ newRepoOverride: "yagince/eir" });
    await fireEvent.click(addButton("repos"));
    expect(on.onAddRepoOverride).toHaveBeenCalledOnce();

    await fireEvent.keyDown(screen.getByPlaceholderText("owner/repo"), { key: "Enter" });
    expect(on.onAddRepoOverride).toHaveBeenCalledTimes(2);
    expect(on.onAddWatchedOrg).not.toHaveBeenCalled();
  });

  it("hands an entry with no slash to the parent anyway", async () => {
    // isValidRepoName lives in the parent; the panel must still fire so the
    // parent gets its chance to reject the entry and clear the field.
    const on = mount({ newRepoOverride: "eir" });
    await fireEvent.click(addButton("repos"));
    expect(on.onAddRepoOverride).toHaveBeenCalledOnce();
  });

  it("swaps the empty hint for a row when an override is added while open", async () => {
    // Same SvelteMap-mutation path as the orgs list, plus the hint/list switch.
    const repos = new SvelteMap<string, RepoSetting>();
    mount({ repoSettings: repos });
    repos.set("yagince/eir", { prs: true, issues: false });
    await tick();

    expect(
      screen.queryByText(/All repos respect the global Include toggles/i),
    ).not.toBeInTheDocument();
    expect(screen.getAllByRole("listitem")[0]).toHaveTextContent("yagince/eir");
  });

  it("offers the known repos as autocomplete for the field", () => {
    mount({ repoSuggestions: ["yagince/eir", "Lecto-inc/api"] });
    expect(screen.getByPlaceholderText("owner/repo")).toHaveAttribute("list", "repo-suggestions");
    const list = document.getElementById("repo-suggestions") as HTMLDataListElement;
    expect([...list.options].map((o) => o.value)).toEqual(["yagince/eir", "Lecto-inc/api"]);
  });
});

describe("Settings backup", () => {
  it("exports without importing", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Export" }));
    expect(on.onExportSettings).toHaveBeenCalledOnce();
    expect(on.onImportSettings).not.toHaveBeenCalled();
  });

  it("imports without exporting", async () => {
    const on = mount();
    await fireEvent.click(screen.getByRole("button", { name: "Import" }));
    expect(on.onImportSettings).toHaveBeenCalledOnce();
    expect(on.onExportSettings).not.toHaveBeenCalled();
  });

  it("shows where the file went", () => {
    // The native save dialog can put the file anywhere; the notice is the only
    // record of the path the user actually picked.
    mount({ settingsIoNotice: "Saved to /Users/me/Desktop/eir-settings.json" });
    expect(screen.getByText("Saved to /Users/me/Desktop/eir-settings.json")).toBeInTheDocument();
  });

  it("shows a failed import", () => {
    mount({ settingsIoError: "not valid JSON" });
    expect(screen.getByText("not valid JSON")).toBeInTheDocument();
  });

  it("stays quiet when there is nothing to report", () => {
    mount({ settingsIoNotice: null, settingsIoError: null });
    expect(screen.queryByText(/Saved to/i)).not.toBeInTheDocument();
  });
});

describe("Settings error", () => {
  it("shows the panel-level error", () => {
    mount({ error: "GitHub API rate limit exceeded" });
    expect(screen.getByText("GitHub API rate limit exceeded")).toBeInTheDocument();
  });

  it("shows the shortcut error and the panel error side by side", () => {
    // Two independent failures with the same styling — one must not replace
    // the other, since they come from unrelated subsystems.
    mount({ error: "GitHub API rate limit exceeded", shortcutError: "combo already taken" });
    expect(screen.getByText("GitHub API rate limit exceeded")).toBeInTheDocument();
    expect(screen.getByText("combo already taken")).toBeInTheDocument();
  });
});

describe("Settings keyboard navigation", () => {
  // jsdom does no layout, so `offsetParent` is null on every element and the
  // component's own visibility filter would discard the entire control list.
  // Standing in the parent element matches what a browser reports for anything
  // that isn't display:none, which is what that filter is really asking.
  const originalOffsetParent = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    "offsetParent",
  );

  beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetParent", {
      configurable: true,
      get(this: HTMLElement) {
        return this.parentElement;
      },
    });
  });

  afterAll(() => {
    if (originalOffsetParent) {
      Object.defineProperty(HTMLElement.prototype, "offsetParent", originalOffsetParent);
    }
  });

  it("puts focus on Back as soon as the panel opens", () => {
    // The popup is opened by a shortcut, so there is no click to seed focus —
    // without this the arrow keys would have nothing to move from.
    mount();
    expect(screen.getByRole("button", { name: "← Back" })).toHaveFocus();
  });

  it("walks focus down the panel with ArrowDown", async () => {
    mount();
    await fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(screen.getByRole("combobox", { name: /Refresh interval/i })).toHaveFocus();
  });

  it("wraps around to the last control with ArrowUp", async () => {
    mount();
    await fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(screen.getByRole("checkbox", { name: /Auth diagnostics log/i })).toHaveFocus();
  });

  it("re-enters from the top when focus has left the panel", async () => {
    // Clicking the panel background drops focus onto <body>, which is in no
    // position in the control list — the arrows have to fall back to an end.
    mount();
    (document.activeElement as HTMLElement).blur();
    await fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(screen.getByRole("button", { name: "← Back" })).toHaveFocus();
  });

  it("re-enters from the bottom when focus has left the panel", async () => {
    mount();
    (document.activeElement as HTMLElement).blur();
    await fireEvent.keyDown(window, { key: "ArrowUp" });
    expect(screen.getByRole("checkbox", { name: /Auth diagnostics log/i })).toHaveFocus();
  });

  it("steps over controls that cannot be focused", async () => {
    // Include PRs is disabled here because it is the last kind left; landing
    // focus on it would strand the user on a dead control.
    mount({ includePRs: true, includeIssues: false });
    screen.getByRole("checkbox", { name: /Show latest comment/i }).focus();
    await fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(screen.getByRole("checkbox", { name: "Include Issues" })).toHaveFocus();
  });

  it("leaves the arrow keys alone while a shortcut is being captured", async () => {
    // Arrow keys are legal parts of a shortcut, so the capture gets them.
    mount({ capturingShortcut: true });
    const back = screen.getByRole("button", { name: "← Back" });
    back.focus();
    await fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(back).toHaveFocus();
  });

  it("leaves the arrow keys alone inside a text field", async () => {
    // Arrows move the caret there; hijacking them would make the org field
    // impossible to edit.
    mount({ newWatchedOrg: "acme" });
    const field = screen.getByPlaceholderText(/org login/i);
    field.focus();
    await fireEvent.keyDown(window, { key: "ArrowDown" });
    expect(field).toHaveFocus();
  });

  it("ignores keys it has no business with", async () => {
    mount();
    const back = screen.getByRole("button", { name: "← Back" });
    await fireEvent.keyDown(window, { key: "ArrowLeft" });
    expect(back).toHaveFocus();
  });
});
