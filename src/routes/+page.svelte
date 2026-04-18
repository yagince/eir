<main class="container">
  <header>
    <h1>eir</h1>
    <p class="subtitle">GitHub PR / Issue watcher</p>
  </header>

  <section class="empty">
    <p>No items yet.</p>
    <p class="hint">Connect your GitHub account to start tracking.</p>
  </section>
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

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 4px;
  }

  .empty p {
    margin: 0;
    font-size: 13px;
  }

  .hint {
    color: rgba(27, 27, 31, 0.55);
    font-size: 12px !important;
  }

  @media (prefers-color-scheme: dark) {
    :global(:root) {
      color: #ececef;
      background: rgba(30, 30, 32, 0.98);
    }
    header {
      border-bottom-color: rgba(255, 255, 255, 0.08);
    }
    .subtitle,
    .hint {
      color: rgba(236, 236, 239, 0.6);
    }
  }
</style>
