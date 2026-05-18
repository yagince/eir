<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import type { Update } from "@tauri-apps/plugin-updater";
  import type { RepoSetting, Theme } from "$lib/storage";
  import { isTextCaretTarget } from "$lib/shortcuts";

  type UpdateStatus =
    | { kind: "idle" }
    | { kind: "checking" }
    | { kind: "up-to-date" }
    | { kind: "available"; update: Update }
    | { kind: "downloading" }
    | { kind: "installed" }
    | { kind: "error"; message: string };

  type Props = {
    refreshMs: number;
    refreshOptions: { value: number; label: string }[];
    notifyEnabled: boolean;
    showLatestComment: boolean;
    includePRs: boolean;
    includeIssues: boolean;
    theme: Theme;
    themeOptions: { value: Theme; label: string }[];
    autostartEnabled: boolean | null;
    appVersion: string;
    toggleShortcut: string;
    capturingShortcut: boolean;
    shortcutError: string | null;
    updateStatus: UpdateStatus;
    watchedOrgs: SvelteSet<string>;
    repoSettings: Map<string, RepoSetting>;
    orgSuggestions: string[];
    repoSuggestions: string[];
    error: string | null;
    settingsIoNotice: string | null;
    settingsIoError: string | null;
    newWatchedOrg: string;
    newRepoOverride: string;
    onBack: () => void;
    onIntervalChange: (value: number) => void;
    onNotifyChange: (enabled: boolean) => void;
    onShowLatestCommentChange: (enabled: boolean) => void;
    onIncludePRsChange: (enabled: boolean) => void;
    onIncludeIssuesChange: (enabled: boolean) => void;
    onThemeChange: (value: Theme) => void;
    onToggleAutostart: (enabled: boolean) => void;
    onSendTestNotification: () => void;
    onRunUpdateCheck: () => void;
    onInstallUpdate: () => void;
    onStartCaptureShortcut: () => void;
    onAddWatchedOrg: () => void;
    onRemoveWatchedOrg: (org: string) => void;
    onAddRepoOverride: () => void;
    onRemoveRepoOverride: (repo: string) => void;
    onUpdateRepoOverride: (repo: string, next: RepoSetting) => void;
    onExportSettings: () => void;
    onImportSettings: () => void;
  };

  let {
    refreshMs,
    refreshOptions,
    notifyEnabled,
    showLatestComment,
    includePRs,
    includeIssues,
    theme,
    themeOptions,
    autostartEnabled,
    appVersion,
    toggleShortcut,
    capturingShortcut,
    shortcutError,
    updateStatus,
    watchedOrgs,
    repoSettings,
    orgSuggestions,
    repoSuggestions,
    error,
    settingsIoNotice,
    settingsIoError,
    newWatchedOrg = $bindable(),
    newRepoOverride = $bindable(),
    onBack,
    onIntervalChange,
    onNotifyChange,
    onShowLatestCommentChange,
    onIncludePRsChange,
    onIncludeIssuesChange,
    onThemeChange,
    onToggleAutostart,
    onSendTestNotification,
    onRunUpdateCheck,
    onInstallUpdate,
    onStartCaptureShortcut,
    onAddWatchedOrg,
    onRemoveWatchedOrg,
    onAddRepoOverride,
    onRemoveRepoOverride,
    onUpdateRepoOverride,
    onExportSettings,
    onImportSettings,
  }: Props = $props();

  // Render override rows in deterministic order. Sort by repo name so a
  // settings tweak doesn't reshuffle the list under the user's cursor.
  const sortedRepoOverrides = $derived(
    [...repoSettings.entries()].sort(([a], [b]) => a.localeCompare(b)),
  );

  let sectionEl: HTMLElement | undefined;
  let backEl: HTMLButtonElement | undefined;

  onMount(() => {
    backEl?.focus();
    window.addEventListener("keydown", handleArrowNav);
    return () => window.removeEventListener("keydown", handleArrowNav);
  });

  function focusableItems(): HTMLElement[] {
    if (!sectionEl) return [];
    const els = sectionEl.querySelectorAll<
      HTMLButtonElement | HTMLInputElement | HTMLSelectElement
    >("button, select, input");
    return Array.from(els).filter(
      (el) => !el.disabled && el.offsetParent !== null,
    );
  }

  function handleArrowNav(e: KeyboardEvent) {
    if (capturingShortcut) return;
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    // Text fields use arrows to move the caret natively — don't hijack.
    if (isTextCaretTarget(document.activeElement)) return;

    const items = focusableItems();
    if (items.length === 0) return;

    const active = document.activeElement as HTMLElement | null;
    const currentIdx = active ? items.indexOf(active) : -1;
    const delta = e.key === "ArrowDown" ? 1 : -1;

    let nextIdx: number;
    if (currentIdx === -1) {
      nextIdx = delta === 1 ? 0 : items.length - 1;
    } else {
      nextIdx = (currentIdx + delta + items.length) % items.length;
    }

    e.preventDefault();
    items[nextIdx].focus();
  }
</script>

<section class="settings" bind:this={sectionEl}>
  <div class="settings-header">
    <button class="back" bind:this={backEl} onclick={onBack}>← Back</button>
  </div>
  <div class="settings-body">
    <label class="setting-row">
      <span class="setting-label">Refresh interval</span>
      <select
        value={refreshMs}
        onchange={(e) => onIntervalChange(Number(e.currentTarget.value))}
      >
        {#each refreshOptions as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </label>
    <label class="setting-row">
      <span class="setting-label">Desktop notifications</span>
      <input
        type="checkbox"
        checked={notifyEnabled}
        onchange={(e) => onNotifyChange(e.currentTarget.checked)}
      />
    </label>
    <label class="setting-row">
      <span class="setting-label">Show latest comment under unread items</span>
      <input
        type="checkbox"
        checked={showLatestComment}
        onchange={(e) => onShowLatestCommentChange(e.currentTarget.checked)}
      />
    </label>
    <label
      class="setting-row"
      title="Turn off to hide PRs across every tab and watched org."
    >
      <span class="setting-label">Include PRs</span>
      <input
        type="checkbox"
        checked={includePRs}
        disabled={includePRs && !includeIssues}
        onchange={(e) => onIncludePRsChange(e.currentTarget.checked)}
      />
    </label>
    <label
      class="setting-row"
      title="Turn off to hide Issues across every tab and watched org."
    >
      <span class="setting-label">Include Issues</span>
      <input
        type="checkbox"
        checked={includeIssues}
        disabled={includeIssues && !includePRs}
        onchange={(e) => onIncludeIssuesChange(e.currentTarget.checked)}
      />
    </label>
    <label class="setting-row">
      <span class="setting-label">Theme</span>
      <select
        value={theme}
        onchange={(e) => onThemeChange(e.currentTarget.value as Theme)}
      >
        {#each themeOptions as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </label>
    <label class="setting-row">
      <span class="setting-label">Start at login</span>
      <input
        type="checkbox"
        checked={autostartEnabled === true}
        disabled={autostartEnabled === null}
        onchange={(e) => onToggleAutostart(e.currentTarget.checked)}
      />
    </label>
    <div class="setting-row">
      <span class="setting-label">Test notification</span>
      <button class="secondary" onclick={onSendTestNotification}>Send</button>
    </div>

    <div class="setting-row">
      <span class="setting-label">
        Check for updates
        {#if appVersion}
          <span class="setting-hint-inline">— v{appVersion}</span>
        {/if}
        {#if updateStatus.kind === "up-to-date"}
          <span class="setting-hint-inline">· already latest</span>
        {:else if updateStatus.kind === "available"}
          <span class="setting-hint-inline update-available"
            >· v{updateStatus.update.version} available</span
          >
        {:else if updateStatus.kind === "installed"}
          <span class="setting-hint-inline">· installed, relaunching…</span>
        {:else if updateStatus.kind === "error"}
          <span class="setting-hint-inline error-inline"
            >· {updateStatus.message}</span
          >
        {/if}
      </span>
      <button
        class="secondary"
        disabled={updateStatus.kind === "checking" ||
          updateStatus.kind === "downloading"}
        onclick={() =>
          updateStatus.kind === "available"
            ? onInstallUpdate()
            : onRunUpdateCheck()}
      >
        {#if updateStatus.kind === "checking"}
          Checking…
        {:else if updateStatus.kind === "downloading"}
          Installing…
        {:else if updateStatus.kind === "available"}
          Install v{updateStatus.update.version}
        {:else}
          Check
        {/if}
      </button>
    </div>
    <div class="setting-row">
      <span class="setting-label">Toggle popup shortcut</span>
      <button
        class="shortcut-capture"
        class:capturing={capturingShortcut}
        onclick={onStartCaptureShortcut}
      >
        {#if capturingShortcut}
          Press keys…
        {:else}
          {toggleShortcut}
        {/if}
      </button>
    </div>
    {#if shortcutError}
      <p class="error">{shortcutError}</p>
    {/if}

    <div class="setting-section">
      <span class="setting-label">Watched orgs / users</span>
      {#if watchedOrgs.size === 0}
        <p class="setting-hint">
          Only your personal repos and items you're involved with show up in
          All. Add an org login (e.g. <code>Lecto-inc</code>) to pull in all
          open items from that org — respects the PR / Issue toggles above.
        </p>
      {:else}
        <ul class="excluded-list">
          {#each [...watchedOrgs].sort() as org (org)}
            <li>
              <span class="excluded-repo">{org}</span>
              <button
                class="row-action"
                onclick={() => onRemoveWatchedOrg(org)}
                aria-label="Remove"
                title="Remove"
              >
                ×
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="excluded-add">
        <input
          type="text"
          list="org-suggestions"
          placeholder="org login (e.g. Lecto-inc)"
          bind:value={newWatchedOrg}
          onkeydown={(e) => e.key === "Enter" && onAddWatchedOrg()}
        />
        <datalist id="org-suggestions">
          {#each orgSuggestions as org (org)}
            <option value={org}></option>
          {/each}
        </datalist>
        <button class="secondary" onclick={onAddWatchedOrg}>Add</button>
      </div>
    </div>

    <div class="setting-section">
      <span class="setting-label">Repository overrides</span>
      {#if sortedRepoOverrides.length === 0}
        <p class="setting-hint">
          None. All repos respect the global Include toggles above. Add a
          repo to flip PRs or Issues on/off just for that repo. Unchecking
          both hides the repo entirely (replaces the old Excluded list).
        </p>
      {:else}
        <p class="setting-hint">
          Per-repo PR / Issue toggles. With the matching global toggle on
          this just narrows; with the global toggle off, ticking the box
          here brings that kind back in for this repo via an extra search.
        </p>
        <ul class="repo-overrides">
          {#each sortedRepoOverrides as [repo, s] (repo)}
            <li class="repo-override-row">
              <span class="repo-override-name" title={repo}>{repo}</span>
              <label class="repo-override-toggle" title="Show PRs">
                <input
                  type="checkbox"
                  checked={s.prs}
                  onchange={(e) =>
                    onUpdateRepoOverride(repo, {
                      prs: e.currentTarget.checked,
                      issues: s.issues,
                    })}
                />
                <span>PRs</span>
              </label>
              <label class="repo-override-toggle" title="Show Issues">
                <input
                  type="checkbox"
                  checked={s.issues}
                  onchange={(e) =>
                    onUpdateRepoOverride(repo, {
                      prs: s.prs,
                      issues: e.currentTarget.checked,
                    })}
                />
                <span>Issues</span>
              </label>
              <button
                class="row-action"
                onclick={() => onRemoveRepoOverride(repo)}
                aria-label="Remove override"
                title="Remove override (back to global defaults)"
              >
                ×
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="excluded-add">
        <input
          type="text"
          list="repo-suggestions"
          placeholder="owner/repo"
          bind:value={newRepoOverride}
          onkeydown={(e) => e.key === "Enter" && onAddRepoOverride()}
        />
        <datalist id="repo-suggestions">
          {#each repoSuggestions as repo (repo)}
            <option value={repo}></option>
          {/each}
        </datalist>
        <button class="secondary" onclick={onAddRepoOverride}>Add</button>
      </div>
    </div>

    <div class="setting-section">
      <span class="setting-label">Backup settings</span>
      <p class="setting-hint">
        Export your configuration (refresh interval, notifications, watched
        orgs, excluded repos, hidden items, shortcut) as a JSON file, or
        import a previously saved file to restore it.
      </p>
      <div class="setting-buttons">
        <button class="secondary" onclick={onExportSettings}>Export</button>
        <button class="secondary" onclick={onImportSettings}>Import</button>
      </div>
      {#if settingsIoNotice}
        <p class="setting-hint io-path" title={settingsIoNotice}>
          {settingsIoNotice}
        </p>
      {/if}
      {#if settingsIoError}
        <p class="error">{settingsIoError}</p>
      {/if}
    </div>

    <div class="setting-section">
      <span class="setting-label">Keyboard shortcuts</span>
      <dl class="shortcut-list">
        <div class="shortcut-item">
          <dt>
            Toggle popup
            <span class="shortcut-scope" title="Works even when the popup is hidden">global</span>
          </dt>
          <dd>
            {#each toggleShortcut.split("+") as part, i (i)}
              {#if i > 0}<span class="shortcut-plus">+</span>{/if}<kbd>{part}</kbd>
            {/each}
          </dd>
        </div>
        <div class="shortcut-item">
          <dt>Reload</dt>
          <dd><kbd>Cmd</kbd><span class="shortcut-plus">+</span><kbd>R</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Mark all as read</dt>
          <dd>
            <kbd>Cmd</kbd><span class="shortcut-plus">+</span><kbd>Shift</kbd
            ><span class="shortcut-plus">+</span><kbd>A</kbd>
          </dd>
        </div>
        <div class="shortcut-item">
          <dt>Open settings</dt>
          <dd><kbd>Cmd</kbd><span class="shortcut-plus">+</span><kbd>,</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Back to list</dt>
          <dd><kbd>Backspace</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Search</dt>
          <dd>
            <kbd>Cmd</kbd><span class="shortcut-plus">+</span><kbd>F</kbd>
            <span class="shortcut-or">or</span> <kbd>/</kbd>
          </dd>
        </div>
        <div class="shortcut-item">
          <dt>Select item</dt>
          <dd><kbd>↑</kbd> <kbd>↓</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Switch tab</dt>
          <dd><kbd>←</kbd> <kbd>→</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Open selected</dt>
          <dd><kbd>Enter</kbd></dd>
        </div>
        <div class="shortcut-item">
          <dt>Scroll</dt>
          <dd>
            <kbd>PageUp</kbd> <kbd>PageDown</kbd> <kbd>Home</kbd> <kbd>End</kbd>
          </dd>
        </div>
        <div class="shortcut-item">
          <dt>Navigate settings</dt>
          <dd><kbd>↑</kbd> <kbd>↓</kbd></dd>
        </div>
      </dl>
      <p class="setting-hint">
        <span class="shortcut-scope">global</span> works when the popup is hidden.
        The rest require the popup to be focused.
      </p>
    </div>
    {#if error}
      <p class="error">{error}</p>
    {/if}
  </div>
</section>

<style>
  .settings {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .settings-header {
    margin-bottom: 12px;
  }

  .back {
    background: none;
    border: none;
    padding: 4px 0;
    font-size: 12px;
    color: var(--fg-muted);
    cursor: pointer;
  }

  .back:hover {
    color: var(--accent);
  }

  .settings-body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    gap: 12px;
  }

  .setting-label {
    color: inherit;
  }

  .setting-row select {
    font-size: 13px;
    padding: 4px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
  }

  .setting-row input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }

  .setting-hint {
    margin: 8px 0 0;
    font-size: 11px;
    color: var(--fg-muted);
  }

  .shortcut-capture {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 12px;
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
    cursor: pointer;
  }

  .shortcut-capture.capturing {
    background: var(--accent-bg-strong);
    border-color: var(--accent);
    color: var(--accent);
  }

  .setting-hint-inline {
    font-size: 11px;
    color: var(--fg-muted);
    margin-left: 4px;
  }

  .setting-hint-inline.update-available {
    color: var(--success);
    font-weight: 500;
  }

  .setting-hint-inline.error-inline {
    color: var(--danger);
  }

  .setting-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--border-faint);
  }

  .setting-buttons {
    display: flex;
    gap: 8px;
  }

  .setting-hint.io-path {
    font-family: "SF Mono", Menlo, monospace;
    word-break: break-all;
  }

  .excluded-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .excluded-list li {
    display: flex;
    align-items: center;
    padding: 4px 6px;
    font-size: 12px;
    border-radius: 5px;
    background: var(--surface-1);
  }

  .excluded-list .row-action {
    opacity: 0.6;
  }

  .excluded-list li:hover .row-action {
    opacity: 1;
  }

  .excluded-repo {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .excluded-add {
    display: flex;
    gap: 6px;
  }

  .excluded-add input {
    flex: 1;
    padding: 4px 6px;
    font-size: 12px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--surface-1);
    color: inherit;
    min-width: 0;
  }

  .repo-overrides {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .repo-override-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    font-size: 12px;
    border-radius: 5px;
    background: var(--surface-1);
  }

  .repo-override-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .repo-override-toggle {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    cursor: pointer;
    color: var(--fg-muted);
  }

  .repo-override-toggle input {
    cursor: pointer;
  }

  .repo-override-row .row-action {
    opacity: 0.6;
  }

  .repo-override-row:hover .row-action {
    opacity: 1;
  }

  .shortcut-list {
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .shortcut-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    font-size: 12px;
  }

  .shortcut-item dt {
    color: var(--fg-muted);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .shortcut-item dd {
    margin: 0;
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .shortcut-list kbd {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 10px;
    padding: 1px 5px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: var(--surface-1);
    color: inherit;
  }

  .shortcut-plus {
    opacity: 0.5;
    font-size: 10px;
  }

  .shortcut-or {
    opacity: 0.55;
    font-size: 11px;
    margin: 0 2px;
  }

  .shortcut-scope {
    padding: 1px 6px;
    font-size: 9px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-bg);
    border-radius: 8px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
</style>
