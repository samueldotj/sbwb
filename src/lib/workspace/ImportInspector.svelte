<script lang="ts">
  // Import inspector (design 4.2): source card, pages to process, project location.
  import { formatBytes, parseRanges, scopeLabel, type ProjectSummary } from "$lib/api";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";

  import { pipeline, formatEta } from "$lib/stores/pipeline.svelte";

  type Props = { summary: ProjectSummary; onstartreview?: () => void; eta?: number | null; secsPerPage?: number | null; pipelineState?: string };
  let { summary, onstartreview, eta = null, secsPerPage = null, pipelineState = "idle" }: Props = $props();
  // Before the first pages finish, 3 s/page is typical for tessdata_best at 300 dpi.
  const estimate = $derived.by(() => {
    const remaining = summary.counts.queued + summary.counts.running;
    if (remaining === 0) return "done";
    if (eta !== null) return formatEta(eta).replace(" left", "");
    const s = (secsPerPage ?? 3) * remaining;
    return s < 90 ? `~${Math.round(s)} s (estimate)` : `~${Math.round(s / 60)} min (estimate)`;
  });

  const total = $derived(summary.meta.source.page_count);
  const ranges = $derived(summary.meta.scope.ranges);
  const isFirst50 = $derived(ranges.length === 1 && ranges[0]?.[0] === 1 && ranges[0]?.[1] === Math.min(50, total));
  const isAll = $derived(ranges.length === 1 && ranges[0]?.[0] === 1 && ranges[0]?.[1] === total);
  let customOpen = $state(false);
  let customText = $state("");
  let customError = $state("");

  async function choose(kind: "first50" | "all") {
    if (summary.read_only) return;
    await project.setScope(kind === "first50" ? [[1, Math.min(50, total)]] : [[1, total]]);
    customOpen = false;
  }
  function openCustom() {
    customText = scopeLabel(ranges);
    customError = "";
    customOpen = true;
  }
  async function applyCustom() {
    const r = parseRanges(customText);
    if (!r) {
      customError = "Use page numbers like 1-50, 60-70";
      return;
    }
    if (r.some(([, b]) => b > total)) {
      customError = `The book has ${total} pages`;
      return;
    }
    await project.setScope(r);
    customOpen = false;
  }
  const fileName = $derived(summary.path.split(/[\\/]/).pop() ?? summary.path);
  const folder = $derived(summary.path.slice(0, summary.path.length - fileName.length));
</script>

<div class="card-plain">
  <div class="label">Source</div>
  <div class="src-name">{summary.meta.source.name}</div>
  <div class="muted">{total} pages · {formatBytes(summary.meta.source.size)} · integrity verified</div>
</div>

<div class="card">
  <div class="strong">Pages to process</div>
  <div class="seg" role="radiogroup" aria-label="Pages to process">
    <button type="button" role="radio" aria-checked={isFirst50} class:on={isFirst50} onclick={() => choose("first50")} disabled={summary.read_only}>First {Math.min(50, total)}</button>
    <button type="button" role="radio" aria-checked={!isFirst50 && !isAll} class:on={!isFirst50 && !isAll} onclick={openCustom} disabled={summary.read_only}>Range…</button>
    <button type="button" role="radio" aria-checked={isAll} class:on={isAll} onclick={() => choose("all")} disabled={summary.read_only}>All {total}</button>
  </div>
  {#if customOpen}
    <div class="custom">
      <label class="muted" for="scope-input">Pages, e.g. 1-50, 60-70</label>
      <div class="row">
        <input id="scope-input" type="text" bind:value={customText} onkeydown={(e) => e.key === "Enter" && applyCustom()} />
        <button type="button" class="primary" onclick={applyCustom}>Apply</button>
      </div>
      {#if customError}<div class="err" role="alert">{customError}</div>{/if}
    </div>
  {:else}
    <div class="muted">Processing {scopeLabel(ranges)} · {total - summary.counts.in_scope} untouched in the source</div>
  {/if}
  <div class="hint">Start small to check quality. You can extend the range later without re-importing.</div>
  <div class="row-between small-row"><span class="muted">Estimated time</span><span>{estimate}</span></div>
</div>

<div class="card">
  <div class="strong">{pipelineState === "running" ? "Now running" : pipelineState === "paused" ? "Paused" : "Status"}</div>
  {#if pipeline.ocr && pipeline.active}
    <div class="row-between"><span>OCR · {pipeline.ocr.done + pipeline.ocr.failed} of {pipeline.ocr.total}</span><span class="muted">{pipeline.ocr.secs_per_unit ? `${pipeline.ocr.secs_per_unit.toFixed(1)} s / page` : "…"}</span></div>
    <div class="bar"><div class="fill" style="width:{pipeline.ocr.total ? Math.round(((pipeline.ocr.done + pipeline.ocr.failed) / pipeline.ocr.total) * 100) : 0}%"></div></div>
  {:else}
    <div class="row-between"><span>Queued for OCR</span><span class="muted">{summary.counts.queued}</span></div>
    <div class="row-between"><span>Done</span><span class="muted">{summary.counts.done}</span></div>
  {/if}
  {#if summary.counts.failed}<div class="row-between"><span>Failed</span><span class="err">{summary.counts.failed}</span></div>{/if}
  <div class="muted">Then layout → text pass, automatically. Review opens on page 1 when you are ready. No need to wait.</div>
  <button type="button" class="primary" onclick={() => onstartreview?.()} disabled={summary.counts.done === 0}>Start reviewing page 1</button>
</div>

<div class="muted small">
  Project saved to <span class="mono">{folder}{fileName}</span>
  {#if summary.read_only}<span class="ro"> · read-only</span>{/if}
  <button type="button" class="link" onclick={() => ui.toast("Save a copy from the File menu to move the project.", "info", 4000)}>Change</button>
</div>

<style>
  .label {
    margin-bottom: 6px;
  }
  .card-plain {
    display: grid;
    gap: 2px;
  }
  .src-name {
    font-family: var(--font-serif);
    font-size: 17px;
  }
  .card {
    background: var(--paper);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: 14px;
    display: grid;
    gap: 10px;
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 11.5px;
    line-height: 1.5;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .hint {
    font-size: 11.5px;
    color: var(--muted);
  }
  .seg {
    display: flex;
    gap: 6px;
  }
  .seg button {
    all: unset;
    padding: 5px 10px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-input);
    cursor: default;
  }
  .seg button.on {
    background: var(--primary-bg);
    color: var(--primary-fg);
    border-color: var(--primary-bg);
    font-weight: 600;
  }
  .seg button:focus-visible,
  .primary:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .seg button:disabled {
    opacity: 0.6;
  }
  .custom {
    display: grid;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  input {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--bg);
  }
  .primary {
    all: unset;
    text-align: center;
    padding: 8px 12px;
    border-radius: var(--radius-control);
    background: var(--primary-bg);
    color: var(--primary-fg);
    font-weight: 600;
    cursor: default;
  }
  .primary:disabled {
    opacity: 0.5;
  }
  .row-between {
    display: flex;
    justify-content: space-between;
  }
  .small-row {
    font-size: 12px;
    border-top: 1px solid var(--border-soft);
    padding-top: 10px;
  }
  .bar {
    height: 6px;
    background: var(--track);
    border-radius: 3px;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 3px;
    transition: width 200ms;
  }
  .err {
    color: var(--danger);
    font-size: 12px;
  }
  .ro {
    color: var(--accent-text);
    font-weight: 600;
  }
  .link {
    all: unset;
    text-decoration: underline;
    margin-left: 4px;
    cursor: default;
  }
</style>
