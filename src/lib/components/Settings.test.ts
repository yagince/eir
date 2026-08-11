// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { describe, expect, it, vi } from "vitest";
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
