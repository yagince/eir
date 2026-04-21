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
