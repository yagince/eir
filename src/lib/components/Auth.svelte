<script lang="ts">
  type DeviceCode = {
    user_code: string;
    verification_uri: string;
    device_code: string;
    interval: number;
    expires_in: number;
  };

  type Props =
    | {
        phase: "idle";
        error: string | null;
        onSignIn: () => void;
      }
    | {
        phase: "pending";
        deviceCode: DeviceCode;
        copied: boolean;
        error: string | null;
        onCopyCode: () => void;
        onReopenVerification: (url: string) => void;
      };

  const props: Props = $props();
</script>

{#if props.phase === "idle"}
  <section class="auth">
    <p class="hint">Sign in to start tracking your PRs and Issues.</p>
    <button class="primary" onclick={props.onSignIn}>Sign in with GitHub</button>
    {#if props.error}
      <p class="error">{props.error}</p>
    {/if}
  </section>
{:else}
  <section class="device">
    <p class="hint">Enter this code on GitHub:</p>
    <button class="code" onclick={props.onCopyCode} title="Click to copy">
      {props.deviceCode.user_code}
    </button>
    <p class="copy-status" class:ok={props.copied}>
      {props.copied ? "✓ Copied to clipboard" : "Tap to copy"}
    </p>
    <button
      class="secondary"
      onclick={() => props.onReopenVerification(props.deviceCode.verification_uri)}
    >
      Open GitHub again
    </button>
    <p class="waiting">Waiting for authorization…</p>
  </section>
{/if}

<style>
  .hint {
    margin: 0;
    font-size: 13px;
    color: var(--fg-muted-strong);
  }

  .waiting {
    margin: 0;
    font-size: 12px;
    color: var(--fg-subtle);
  }

  .code {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 26px;
    letter-spacing: 4px;
    font-weight: 600;
    background: var(--surface-2);
    border: 1px dashed var(--border);
    border-radius: 8px;
    padding: 12px 18px;
    color: inherit;
    cursor: pointer;
  }

  .code:hover {
    background: var(--surface-2-hover);
  }

  .copy-status {
    margin: -6px 0 0;
    font-size: 11px;
    color: var(--fg-subtle);
  }

  .copy-status.ok {
    color: var(--success);
    font-weight: 500;
  }
</style>
