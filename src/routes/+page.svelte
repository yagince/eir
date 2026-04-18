<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";

  type WatchedItem = {
    id: number;
    kind: "pr" | "issue";
    title: string;
    number: number;
    repo: string;
    url: string;
    author: string;
    updated_at: string;
    state: string;
  };

  type DeviceCode = {
    user_code: string;
    verification_uri: string;
    device_code: string;
    interval: number;
    expires_in: number;
  };

  type Phase = "idle" | "pending" | "loaded";

  let phase = $state<Phase>("idle");
  let deviceCode = $state<DeviceCode | null>(null);
  let items = $state<WatchedItem[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let copied = $state(false);

  onMount(async () => {
    if (await invoke<boolean>("is_authenticated")) {
      await loadItems();
    }
  });

  async function signIn() {
    error = null;
    await invoke("set_auto_hide", { enabled: false });
    try {
      const code = await invoke<DeviceCode>("start_device_flow");
      deviceCode = code;
      phase = "pending";
      try {
        await navigator.clipboard.writeText(code.user_code);
        copied = true;
        await invoke("set_auto_hide", { enabled: true });
      } catch {
        copied = false;
      }
      await openUrl(code.verification_uri);
      await invoke("poll_device_flow", {
        deviceCode: code.device_code,
        interval: code.interval,
      });
      deviceCode = null;
      copied = false;
      await loadItems();
    } catch (e) {
      error = String(e);
      phase = "idle";
      deviceCode = null;
      await invoke("set_auto_hide", { enabled: true });
    }
  }

  async function loadItems() {
    loading = true;
    error = null;
    try {
      items = await invoke<WatchedItem[]>("fetch_watched");
      phase = "loaded";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function signOut() {
    await invoke("sign_out");
    items = [];
    phase = "idle";
  }

  async function copyCode() {
    if (!deviceCode) return;
    try {
      await navigator.clipboard.writeText(deviceCode.user_code);
      copied = true;
      await invoke("set_auto_hide", { enabled: true });
    } catch (e) {
      error = `copy failed: ${e}`;
    }
  }

  function relativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    const m = Math.floor(diff / 60000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h`;
    const d = Math.floor(h / 24);
    return `${d}d`;
  }
</script>

<main class="container">
  <header>
    <h1>eir</h1>
    <p class="subtitle">GitHub PR / Issue watcher</p>
  </header>

  {#if phase === "idle"}
    <section class="auth">
      <p class="hint">Sign in to start tracking your PRs and Issues.</p>
      <button class="primary" onclick={signIn}>Sign in with GitHub</button>
      {#if error}
        <p class="error">{error}</p>
      {/if}
    </section>
  {:else if phase === "pending" && deviceCode}
    <section class="device">
      <p class="hint">Enter this code on GitHub:</p>
      <button class="code" onclick={copyCode} title="Click to copy">
        {deviceCode.user_code}
      </button>
      <p class="copy-status" class:ok={copied}>
        {copied ? "✓ Copied to clipboard" : "Tap to copy"}
      </p>
      <button class="secondary" onclick={() => openUrl(deviceCode!.verification_uri)}>
        Open GitHub again
      </button>
      <p class="waiting">Waiting for authorization…</p>
    </section>
  {:else}
    {#if items.length === 0 && !loading}
      <section class="empty">
        <p>No open PRs or Issues involving you.</p>
      </section>
    {:else}
      <ul class="list">
        {#each items as item (item.id)}
          <li>
            <button class="item" onclick={() => openUrl(item.url)}>
              <span class="badge" class:pr={item.kind === "pr"}>
                {item.kind === "pr" ? "PR" : "IS"}
              </span>
              <span class="body">
                <span class="title">{item.title}</span>
                <span class="meta">
                  {item.repo}#{item.number} · {item.author} · {relativeTime(
                    item.updated_at,
                  )}
                </span>
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}
      <p class="error">{error}</p>
    {/if}
    <footer>
      <button class="refresh" onclick={loadItems} disabled={loading}>
        {loading ? "Refreshing…" : "Refresh"}
      </button>
      <button class="signout" onclick={signOut}>Sign out</button>
    </footer>
  {/if}
</main>

<style>
  :global(:root) {
    font-family: -apple-system, BlinkMacSystemFont, "Inter", system-ui, sans-serif;
    color: #1b1b1f;
    background: rgba(246, 246, 248, 0.98);
  }

  :global(body) {
    margin: 0;
    padding: 0;
  }

  .container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: 16px;
    box-sizing: border-box;
  }

  header {
    border-bottom: 1px solid rgba(0, 0, 0, 0.08);
    padding-bottom: 12px;
    margin-bottom: 12px;
  }

  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .subtitle {
    margin: 2px 0 0;
    font-size: 12px;
    color: rgba(27, 27, 31, 0.6);
  }

  .auth,
  .device,
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    text-align: center;
    gap: 12px;
  }

  .hint {
    margin: 0;
    font-size: 13px;
    color: rgba(27, 27, 31, 0.7);
  }

  .waiting {
    margin: 0;
    font-size: 12px;
    color: rgba(27, 27, 31, 0.5);
  }

  .code {
    font-family: "SF Mono", Menlo, monospace;
    font-size: 26px;
    letter-spacing: 4px;
    font-weight: 600;
    background: rgba(0, 0, 0, 0.05);
    border: 1px dashed rgba(0, 0, 0, 0.2);
    border-radius: 8px;
    padding: 12px 18px;
    color: inherit;
    cursor: pointer;
  }

  .code:hover {
    background: rgba(0, 0, 0, 0.08);
  }

  .copy-status {
    margin: -6px 0 0;
    font-size: 11px;
    color: rgba(27, 27, 31, 0.5);
  }

  .copy-status.ok {
    color: #1a7f37;
    font-weight: 500;
  }

  button.primary,
  button.secondary,
  button.refresh,
  button.signout {
    padding: 8px 14px;
    font-size: 13px;
    border: none;
    border-radius: 6px;
    cursor: pointer;
  }

  .primary,
  .refresh {
    background: #0969da;
    color: white;
  }

  .secondary {
    background: rgba(0, 0, 0, 0.06);
    color: inherit;
  }

  .signout {
    background: none;
    color: rgba(27, 27, 31, 0.6);
  }

  .signout:hover {
    color: #d1242f;
  }

  .refresh:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    margin: 0;
    font-size: 12px;
    color: #d1242f;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .item {
    width: 100%;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px;
    background: none;
    border: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }

  .item:hover {
    background: rgba(0, 0, 0, 0.05);
  }

  .badge {
    flex: 0 0 auto;
    font-size: 10px;
    font-weight: 600;
    padding: 2px 5px;
    border-radius: 3px;
    background: rgba(154, 103, 0, 0.15);
    color: #9a6700;
    margin-top: 1px;
  }

  .badge.pr {
    background: rgba(26, 127, 55, 0.15);
    color: #1a7f37;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .title {
    font-size: 13px;
    line-height: 1.3;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .meta {
    font-size: 11px;
    color: rgba(27, 27, 31, 0.55);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  footer {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px solid rgba(0, 0, 0, 0.08);
    display: flex;
    gap: 8px;
  }

  .refresh {
    flex: 1;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      color: #ececef;
      background: rgba(30, 30, 32, 0.98);
    }
    header,
    footer {
      border-color: rgba(255, 255, 255, 0.08);
    }
    .subtitle,
    .meta,
    .hint,
    .waiting,
    .signout {
      color: rgba(236, 236, 239, 0.6);
    }
    .code {
      background: rgba(255, 255, 255, 0.05);
      border-color: rgba(255, 255, 255, 0.15);
    }
    .code:hover {
      background: rgba(255, 255, 255, 0.08);
    }
    .secondary {
      background: rgba(255, 255, 255, 0.08);
    }
    .item:hover {
      background: rgba(255, 255, 255, 0.06);
    }
  }
</style>
