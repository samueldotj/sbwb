<script lang="ts">
  // Welcome (design 4.1, UX-01): drop zone, Choose PDF, Open project, recent books.
  import { onMount } from "svelte";
  import { pickPdf, pickProject } from "$lib/dialogs";
  import { recent, type RecentEntry } from "$lib/recent";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import StackedBar from "$lib/workspace/StackedBar.svelte";

  type Props = { version?: string; dragging?: boolean; onsettings?: () => void };
  let { version = "0.1.0", dragging = false, onsettings }: Props = $props();

  let books = $state<RecentEntry[]>([]);

  onMount(async () => {
    books = await recent.list();
  });

  async function choosePdf() {
    const p = await pickPdf();
    if (p) await project.importPdf(p);
  }
  async function openProject() {
    const p = await pickProject();
    if (p) await project.open(p);
  }
  async function openRecent(e: RecentEntry) {
    try {
      await project.open(e.path);
    } catch {
      books = await recent.list();
    }
  }
  async function forget(e: RecentEntry, ev: MouseEvent) {
    ev.stopPropagation();
    await recent.forget(e.path);
    books = await recent.list();
    ui.toast("Removed from recent books. The project file was not deleted.", "info", 4000);
  }

  function stageLine(e: RecentEntry): string {
    const scope = e.inScope === e.sourcePages ? `${e.sourcePages} pages` : `${e.inScope} of ${e.sourcePages} pages`;
    let stage = "Imported";
    if (e.done > 0 && e.done < e.inScope) stage = `OCR ${e.done} of ${e.inScope}`;
    else if (e.done >= e.inScope && e.inScope > 0) stage = "OCR done";
    if (e.approved > 0) stage += ` · ${e.approved} pages approved`;
    if (e.failed > 0) stage += ` · ${e.failed} failed`;
    return `${scope} · ${stage}`;
  }
  function when(iso: string): string {
    const d = new Date(iso);
    const today = new Date();
    const sameDay = d.toDateString() === today.toDateString();
    return sameDay
      ? `today ${d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}`
      : d.toLocaleDateString([], { month: "short", day: "numeric" });
  }
</script>

<div class="welcome">
  <section class="left">
    <div>
      <h1 class="headline">Scanned books,<br />into Word.</h1>
      <p class="lede">Everything runs on this computer. AI proofreading is optional and asks first.</p>
    </div>
    <div class="drop" class:dragging>
      <div class="drop-title">Drop a PDF here</div>
      <div class="or">or</div>
      <button type="button" class="primary" onclick={choosePdf} disabled={project.busy !== null}>
        {project.busy ?? "Choose PDF…"}
      </button>
      <div class="hint">OCR starts right away. You pick the page range next.</div>
    </div>
    <button type="button" class="secondary" onclick={openProject} disabled={project.busy !== null}>Open a .sbwb project…</button>
    <div class="foot">
      <span>Core {version}</span>
      <button type="button" class="link" onclick={() => onsettings?.()}>Settings</button>
      <button type="button" class="link" onclick={() => ui.toast("Shortcuts: Ctrl+I import · Ctrl+O open · Ctrl+W close", "info", 5000)}>Shortcuts</button>
    </div>
  </section>
  <section class="right">
    <div class="recent-head">
      <span class="label">Recent books</span>
      <span class="muted small">Stored only on this computer</span>
    </div>
    {#if books.length === 0}
      <p class="muted empty">No books yet. Import a PDF to begin.</p>
    {:else}
      <div class="list">
        {#each books as b, i (b.path)}
          <div
            class="card"
            class:first={i === 0}
            role="button"
            tabindex="0"
            onclick={() => openRecent(b)}
            onkeydown={(e) => (e.key === "Enter" || e.key === " ") && openRecent(b)}
          >
            <div class="thumb" aria-hidden="true"></div>
            <div class="info">
              <div class="title">{b.title}{#if b.volume}<span class="vol"> {b.volume}</span>{/if}</div>
              <div class="muted">{stageLine(b)}</div>
              <StackedBar approved={b.approved} issues={Math.max(0, b.done - b.approved)} unseen={Math.max(0, b.inScope - b.done)} width="260px" />
            </div>
            <div class="actions">
              {#if i === 0}
                <button type="button" class="primary small-btn" onclick={(e) => { e.stopPropagation(); openRecent(b); }}>Continue</button>
              {/if}
              <span class="muted small">{when(b.openedAt)}</span>
              <button type="button" class="link small" onclick={(e) => forget(b, e)} aria-label="Remove {b.title} from recent books">Remove</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
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
    transition: background 120ms;
  }
  .drop.dragging {
    background: var(--accent-bg);
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
  .small {
    font-size: 11px;
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
  .primary:disabled,
  .secondary:disabled {
    opacity: 0.6;
  }
  .small-btn {
    padding: 7px 14px;
  }
  .secondary {
    padding: 9px 14px;
    border: 1px solid var(--border-input);
    background: var(--paper);
  }
  .primary:focus-visible,
  .secondary:focus-visible,
  .link:focus-visible,
  .card:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .link {
    all: unset;
    color: var(--muted);
    cursor: default;
  }
  .link:hover {
    color: var(--text);
    text-decoration: underline;
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
    overflow: auto;
  }
  .recent-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 18px;
  }
  .empty {
    margin: 0;
  }
  .list {
    display: grid;
    gap: 10px;
  }
  .card {
    display: grid;
    grid-template-columns: 56px 1fr auto;
    gap: 16px;
    align-items: center;
    padding: 14px 16px;
    background: var(--paper);
    border: 1px solid var(--border-soft);
    border-radius: 10px;
    cursor: default;
  }
  .card.first {
    border-color: var(--border);
  }
  .card:hover {
    border-color: var(--border-input);
  }
  .thumb {
    width: 56px;
    height: 74px;
    background: var(--scan-paper);
    border: 1px solid var(--border-input);
    box-shadow: 2px 2px 0 var(--strip);
  }
  .info {
    display: grid;
    gap: 4px;
  }
  .title {
    font-family: var(--font-serif);
    font-size: 19px;
  }
  .vol {
    color: var(--muted);
    font-size: 14px;
  }
  .actions {
    display: grid;
    gap: 6px;
    justify-items: end;
  }
</style>
