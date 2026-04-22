<script lang="ts">
  import { SvelteSet } from "svelte/reactivity";
  import type { Update } from "@tauri-apps/plugin-updater";
  import type { Theme } from "$lib/storage";

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
    theme: Theme;
    themeOptions: { value: Theme; label: string }[];
    autostartEnabled: boolean | null;
    appVersion: string;
    toggleShortcut: string;
    capturingShortcut: boolean;
    shortcutError: string | null;
    updateStatus: UpdateStatus;
    watchedOrgs: SvelteSet<string>;
    excludedRepos: SvelteSet<string>;
    orgSuggestions: string[];
    repoSuggestions: string[];
    error: string | null;
    settingsIoNotice: string | null;
    settingsIoError: string | null;
    newWatchedOrg: string;
    newExcludedRepo: string;
    onBack: () => void;
    onIntervalChange: (value: number) => void;
    onNotifyChange: (enabled: boolean) => void;
    onThemeChange: (value: Theme) => void;
    onToggleAutostart: (enabled: boolean) => void;
    onSendTestNotification: () => void;
    onRunUpdateCheck: () => void;
    onInstallUpdate: () => void;
    onStartCaptureShortcut: () => void;
    onAddWatchedOrg: () => void;
    onRemoveWatchedOrg: (org: string) => void;
    onAddExcludedRepo: () => void;
    onRemoveExcludedRepo: (repo: string) => void;
    onExportSettings: () => void;
    onImportSettings: () => void;
  };

  let {
    refreshMs,
    refreshOptions,
    notifyEnabled,
    theme,
    themeOptions,
    autostartEnabled,
    appVersion,
    toggleShortcut,
    capturingShortcut,
    shortcutError,
    updateStatus,
    watchedOrgs,
    excludedRepos,
    orgSuggestions,
    repoSuggestions,
    error,
    settingsIoNotice,
    settingsIoError,
    newWatchedOrg = $bindable(),
    newExcludedRepo = $bindable(),
    onBack,
    onIntervalChange,
    onNotifyChange,
    onThemeChange,
    onToggleAutostart,
    onSendTestNotification,
    onRunUpdateCheck,
    onInstallUpdate,
    onStartCaptureShortcut,
    onAddWatchedOrg,
    onRemoveWatchedOrg,
    onAddExcludedRepo,
    onRemoveExcludedRepo,
    onExportSettings,
    onImportSettings,
  }: Props = $props();
</script>

<section class="settings">
  <div class="settings-header">
    <button class="back" onclick={onBack}>← Back</button>
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
          open PRs from that org — catches dependabot PRs in org repos.
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
      <span class="setting-label">Excluded repositories</span>
      {#if excludedRepos.size === 0}
        <p class="setting-hint">None. Items from all repos are shown.</p>
      {:else}
        <ul class="excluded-list">
          {#each [...excludedRepos].sort() as repo (repo)}
            <li>
              <span class="excluded-repo">{repo}</span>
              <button
                class="row-action"
                onclick={() => onRemoveExcludedRepo(repo)}
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
          list="repo-suggestions"
          placeholder="owner/repo"
          bind:value={newExcludedRepo}
          onkeydown={(e) => e.key === "Enter" && onAddExcludedRepo()}
        />
        <datalist id="repo-suggestions">
          {#each repoSuggestions as repo (repo)}
            <option value={repo}></option>
          {/each}
        </datalist>
        <button class="secondary" onclick={onAddExcludedRepo}>Add</button>
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
