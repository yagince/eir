<script lang="ts">
  import type { UpdateBanner } from "$lib/update-banner";

  /// Rendered above the list, outside its scroll container, so a pending update
  /// stays visible however far down the list the user is.
  let {
    banner,
    installError = null,
    onUpdate,
    onRetry,
    onDismiss,
  }: {
    banner: UpdateBanner;
    installError?: string | null;
    onUpdate: () => void;
    onRetry: () => void;
    onDismiss: () => void;
  } = $props();
</script>

<div class="update-banner" class:failed={banner.state === "failed"} role="status">
  {#if banner.state === "downloading"}
    <span class="update-banner-text">Installing update…</span>
  {:else if banner.state === "failed"}
    <!-- The message can be long and this bar is 440px wide, so it lives in the
         tooltip rather than being truncated into uselessness. -->
    <span class="update-banner-text" title={installError}>Update failed</span>
    <button class="update-banner-action" onclick={onRetry}>Retry</button>
  {:else}
    <span class="update-banner-text">Version {banner.version} is available</span>
    <button class="update-banner-action" onclick={onUpdate}>Update</button>
  {/if}
  {#if banner.state !== "downloading"}
    <button
      class="update-banner-dismiss"
      onclick={onDismiss}
      title="Dismiss"
      aria-label="Dismiss update notice"
    >
      ×
    </button>
  {/if}
</div>

<style>
  .update-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    margin-bottom: 8px;
    padding: 6px 6px 6px 10px;
    border: 1px solid var(--accent-bg-strong);
    border-radius: 6px;
    background: var(--accent-bg);
    font-size: 12px;
    color: var(--fg);
  }

  .update-banner.failed {
    border-color: var(--danger);
    background: var(--danger-bg-faint);
  }

  .update-banner-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .update-banner-action {
    flex: none;
    padding: 4px 10px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .update-banner-action:hover {
    background: var(--accent-bg-hover);
  }

  .update-banner-dismiss {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--fg-muted);
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
  }

  .update-banner-dismiss:hover {
    background: var(--hover-bg);
    color: var(--fg);
  }
</style>
