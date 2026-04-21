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

    <p class="setting-hint">
      <kbd>Cmd</kbd>+<kbd>,</kbd> opens settings ·
      <kbd>Backspace</kbd> goes back.
    </p>
    {#if error}
      <p class="error">{error}</p>
    {/if}
  </div>
</section>
