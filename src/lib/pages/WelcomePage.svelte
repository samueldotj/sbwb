<script lang="ts">
  // Welcome (design 4.1). M0: static shell; M1 wires import/open/recent.
  import { ui } from "$lib/stores/ui.svelte";
  let version = "0.1.0";
</script>

<div class="welcome">
  <section class="left">
    <div>
      <h1 class="headline">Scanned books,<br />into Word.</h1>
      <p class="lede">Everything runs on this computer. AI proofreading is optional and asks first.</p>
    </div>
    <div class="drop">
      <div class="drop-title">Drop a PDF here</div>
      <div class="or">or</div>
      <button type="button" class="primary" onclick={() => ui.toast("Import arrives in M1", "info")}>Choose PDF…</button>
      <div class="hint">OCR starts right away. You pick the page range next.</div>
    </div>
    <button type="button" class="secondary" onclick={() => ui.toast("Open arrives in M1", "info")}>Open a .sbwb project…</button>
    <div class="foot">
      <span>Core {version}</span><span>Settings</span><span>Shortcuts</span>
    </div>
  </section>
  <section class="right">
    <div class="recent-head">
      <span class="label">Recent books</span>
      <span class="muted">Stored only on this computer</span>
    </div>
    <p class="muted empty">No books yet. Import a PDF to begin.</p>
  </section>
</div>

<style>
  .welcome {
    display: grid;
    grid-template-columns: 400px 1fr;
    min-height: 0;
  }
  .left {
    padding: 64px 48px;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 28px;
  }
  .headline {
    font-family: var(--font-serif);
    font-size: 40px;
    font-weight: 400;
    line-height: 1.05;
    letter-spacing: -0.01em;
    margin: 0 0 12px;
    text-wrap: balance;
  }
  .lede {
    color: var(--text-2);
    font-size: 14px;
    line-height: 1.5;
    margin: 0;
    text-wrap: pretty;
  }
  .drop {
    border: 1.5px dashed var(--accent);
    border-radius: 10px;
    padding: 28px 20px;
    text-align: center;
    background: var(--paper);
    display: grid;
    gap: 8px;
  }
  .drop-title {
    font-family: var(--font-serif);
    font-size: 20px;
  }
  .or,
  .hint,
  .muted {
    color: var(--muted);
  }
  .hint {
    font-size: 11.5px;
    margin-top: 4px;
  }
  .primary,
  .secondary {
    all: unset;
    padding: 9px 18px;
    border-radius: var(--radius-control);
    font-weight: 600;
    cursor: default;
    text-align: center;
  }
  .primary {
    justify-self: center;
    background: var(--primary-bg);
    color: var(--primary-fg);
  }
  .secondary {
    padding: 9px 14px;
    border: 1px solid var(--border-input);
    background: var(--paper);
  }
  .primary:focus-visible,
  .secondary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .foot {
    margin-top: auto;
    font-size: 11.5px;
    color: var(--muted);
    display: flex;
    gap: 14px;
  }
  .right {
    padding: 64px 56px;
    overflow: hidden;
  }
  .recent-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 18px;
  }
  .recent-head .muted {
    font-size: 12px;
  }
  .empty {
    margin: 0;
  }
</style>
