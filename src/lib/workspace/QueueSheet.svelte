<script lang="ts">
  // Book queue (PRJ-06): several PDFs with a chosen profile and destination,
  // one project each, processed in order. Pending books can be reordered or
  // removed without touching their projects; a failed book does not stop
  // the rest; after a restart the queue resumes only on request.
  import { api, errorMessage, type ProcessingSettings, type QueueItem, type QueueView } from "$lib/api";
  import { pickFolder, pickPdfs } from "$lib/dialogs";
  import { isTauri, listen } from "$lib/ipc";
  import Sheet from "$lib/shell/Sheet.svelte";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";

  type Props = { open: boolean; onclose: () => void };
  let { open, onclose }: Props = $props();

  let q = $state<QueueView>({ items: [], running: false });
  let destDir = $state<string | null>(null);
  let firstPages = $state<number | null>(50);
  let settings = $state<ProcessingSettings>({ model: "eng_best", dpi: 300, deskew: true, despeckle: false, auto_apply_threshold: 90, run_stages_automatically: true, workers: 4, language: "modern" });
  let unlisten: (() => void) | null = null;

  async function refresh() {
    if (!isTauri) return;
    try {
      q = await api.queueList();
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    }
  }
  $effect(() => {
    if (!open) return;
    void refresh();
    if (isTauri) api.settingsGet().then((s) => (settings = s)).catch(() => {});
    void listen<QueueView>("queue:event", (v) => (q = v)).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  async function add() {
    const files = await pickPdfs();
    if (!files.length) return;
    try {
      q = await api.queueAdd({ sources: files, dest_dir: destDir, first_pages: firstPages, settings });
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  async function chooseDest() {
    const d = await pickFolder();
    if (d) destDir = d;
  }
  async function start() {
    try {
      await api.queueStart();
      ui.announce("Queue started");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 7000);
    }
  }
  async function openBook(it: QueueItem) {
    if (project.isOpen) {
      ui.toast("Close the current book first (Ctrl+W)", "info");
      return;
    }
    onclose();
    await project.open(it.project_path);
  }
  const pending = $derived(q.items.filter((i) => i.status === "pending").length);
  const interrupted = $derived(q.items.filter((i) => i.status === "interrupted").length);
  function name(p: string): string {
    return p.split(/[\\/]/).pop() ?? p;
  }
</script>

<Sheet {open} title="Book queue" subtitle="{q.items.length} book{q.items.length === 1 ? '' : 's'} · {pending} pending{q.running ? ' · running' : ''}" width="760px" {onclose}>
  <div class="profile">
    <div class="label">Profile for books added now</div>
    <div class="row">
      <label>Model
        <select bind:value={settings.model}><option value="eng_best">eng best</option><option value="eng_fast">eng fast</option></select>
      </label>
      <label>Resolution
        <select bind:value={settings.dpi}><option value={200}>200 dpi</option><option value={300}>300 dpi</option><option value={400}>400 dpi</option></select>
      </label>
      <label>Pages
        <select bind:value={firstPages}><option value={null}>all pages</option><option value={50}>first 50</option><option value={110}>first 110</option></select>
      </label>
      <label>Language
        <select bind:value={settings.language}><option value="modern">modern English</option><option value="early_modern">Early Modern English</option></select>
      </label>
      <label>Workers <input type="number" min="1" max="16" bind:value={settings.workers} /></label>
    </div>
    <div class="row">
      <button type="button" class="ctl" onclick={chooseDest}>{destDir ? `Projects in ${name(destDir)}` : "Projects next to each PDF"}</button>
      {#if destDir}<button type="button" class="link" onclick={() => (destDir = null)}>next to each PDF instead</button>{/if}
      <span class="grow"></span>
      <button type="button" class="ctl primary" onclick={add} disabled={q.running}>Add PDFs…</button>
    </div>
  </div>

  {#if interrupted}
    <p class="note warn">{interrupted} book{interrupted === 1 ? " was" : "s were"} interrupted when SBWB last closed. They are not resumed automatically; use Retry to queue them again.</p>
  {/if}

  {#if q.items.length === 0}
    <p class="muted">The queue is empty. Add PDFs; each becomes its own project and is processed in order with the profile above.</p>
  {:else}
    <ol class="list" aria-label="Queued books">
      {#each q.items as it, i (it.id)}
        <li class={it.status}>
          <div class="main">
            <div class="name">{name(it.source.toString())}</div>
            <div class="muted small">→ {name(it.project_path.toString())} · {it.settings.model === "eng_best" ? "eng best" : "eng fast"} · {it.settings.dpi} dpi · {it.first_pages ? `first ${it.first_pages}` : "all pages"}{it.settings.language === "early_modern" ? " · Early Modern" : ""}</div>
            {#if it.error}<div class="err small">{it.error}</div>{/if}
          </div>
          <div class="status small">
            {#if it.status === "running"}<span class="spinner" aria-hidden="true"></span> {it.done} / {it.total || "?"} pages
            {:else if it.status === "done"}done · {it.done} pages
            {:else}{it.status}{/if}
          </div>
          <div class="actions">
            {#if it.status === "pending"}
              <button type="button" class="link" onclick={() => api.queueMove(it.id, -1).then((v) => (q = v))} disabled={i === 0} aria-label="Move up">↑</button>
              <button type="button" class="link" onclick={() => api.queueMove(it.id, 1).then((v) => (q = v))} disabled={i === q.items.length - 1} aria-label="Move down">↓</button>
            {/if}
            {#if it.status === "failed" || it.status === "interrupted"}
              <button type="button" class="link" onclick={() => api.queueRetry(it.id).then((v) => (q = v))}>Retry</button>
            {/if}
            {#if it.status === "done" || it.status === "interrupted" || it.status === "failed"}
              <button type="button" class="link" onclick={() => openBook(it)}>Open</button>
            {/if}
            {#if it.status !== "running"}
              <button type="button" class="link" onclick={() => api.queueRemove(it.id).then((v) => (q = v)).catch((e) => ui.toast(errorMessage(e), "error"))}>Remove</button>
            {/if}
          </div>
        </li>
      {/each}
    </ol>
  {/if}
  {#snippet footer()}
    <span class="muted small">Removing a book never deletes its project. One book is processed at a time with the worker limit of the app.</span>
    <span class="grow"></span>
    <button type="button" class="ctl" onclick={() => api.queueClearFinished().then((v) => (q = v))} disabled={!q.items.some((i) => i.status === "done")}>Clear finished</button>
    {#if q.running}
      <button type="button" class="ctl" onclick={() => api.queueStop()}>Stop after this book</button>
    {:else}
      <button type="button" class="ctl primary" onclick={start} disabled={pending === 0}>Start {pending} pending</button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .label {
    margin-bottom: 6px;
  }
  .profile {
    display: grid;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
    margin-bottom: 12px;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: end;
    flex-wrap: wrap;
  }
  .row label {
    display: grid;
    gap: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  select,
  input[type="number"] {
    padding: 4px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
    font-size: 12.5px;
  }
  input[type="number"] {
    width: 60px;
  }
  .grow {
    flex: 1;
  }
  .note {
    margin: 0 0 10px;
    font-size: 12.5px;
  }
  .note.warn {
    color: var(--accent-text);
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    max-height: 46vh;
    overflow: auto;
  }
  .list li {
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
    align-items: center;
    padding: 8px 10px;
    border-radius: 6px;
    background: var(--raised);
    border: 1px solid var(--border);
  }
  .list li.running {
    border-color: var(--accent);
  }
  .list li.failed {
    border-color: var(--danger);
  }
  .name {
    font-weight: 600;
    font-size: 13px;
  }
  .err {
    color: var(--danger);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
    vertical-align: middle;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ctl {
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    font: 500 12.5px var(--font-ui);
    cursor: pointer;
  }
  .ctl.primary {
    background: var(--primary-bg);
    border-color: var(--primary-bg);
    color: var(--primary-fg);
    font-weight: 600;
  }
  .ctl:disabled,
  .link:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .link {
    all: unset;
    color: var(--accent-text);
    text-decoration: underline;
    cursor: pointer;
    font-size: 12px;
  }
  .ctl:focus-visible,
  .link:focus-visible,
  select:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
