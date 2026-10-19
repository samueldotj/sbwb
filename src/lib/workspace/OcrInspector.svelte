<script lang="ts">
  // OCR tab: the engine settings in use, live progress, failed pages, and
  // the pages OCR was least sure of, so the weakest pages get checked first.
  import { api, type ProcessingSettings, type ProjectSummary } from "$lib/api";
  import { isTauri } from "$lib/ipc";
  import { formatEta, pipeline } from "$lib/stores/pipeline.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  /** Pages with fewer words than this are left out of the weakest list:
   *  on a title page or a plate the average says little. */
  const MIN_WORDS = 40;
  const WEAKEST = 10;

  let settings = $state<ProcessingSettings | null>(null);
  let confidences = $state<{ page: number; mean_confidence: number; words: number }[]>([]);

  $effect(() => {
    void ui.settingsOpen; // re-read after Settings closes
    if (!isTauri) return;
    api
      .settingsGet()
      .then((s) => (settings = s))
      .catch(() => {});
  });
  $effect(() => {
    void summary.counts.ocr_done;
    if (!isTauri) return;
    api
      .ocrConfidences()
      .then((c) => (confidences = c))
      .catch(() => (confidences = []));
  });

  const ocr = $derived(pipeline.ocr);
  const running = $derived(pipeline.active && ocr !== null);
  const failed = $derived(summary.pages.filter((p) => p.status === "failed"));
  const inScope = $derived(new Set(summary.pages.filter((p) => p.status !== "unprocessed" && p.status !== "excluded").map((p) => p.index)));
  const weakest = $derived(confidences.filter((c) => c.words >= MIN_WORDS && inScope.has(c.page)).slice(0, WEAKEST));
  const average = $derived.by(() => {
    const rows = confidences.filter((c) => c.words >= MIN_WORDS && inScope.has(c.page));
    if (!rows.length) return null;
    return rows.reduce((a, c) => a + c.mean_confidence, 0) / rows.length;
  });

  function openPage(i: number) {
    view.open(i);
  }
</script>

<div class="head">
  <div class="title">OCR</div>
  <p class="muted small">Recognises the words on each scanned page. Layout and the text pass follow automatically.</p>
</div>

<div class="card">
  <div class="row-between">
    <span class="label">Engine</span>
    <button type="button" class="link" onclick={() => (ui.settingsOpen = true)}>Change…</button>
  </div>
  {#if settings}
    <div class="facts">
      <span>Model</span><b>{settings.model === "eng_best" ? "English, best (slower)" : "English, fast"}</b>
      <span>Resolution</span><b>{settings.dpi} dpi</b>
      <span>Deskew</span><b>{settings.deskew ? "on" : "off"}</b>
      <span>Workers</span><b>{settings.workers}</b>
    </div>
    <p class="muted small">Changing these re-runs OCR on the pages they affect; Settings says how many before you save.</p>
  {:else}
    <p class="muted small">Settings are not available.</p>
  {/if}
</div>

<div class="card">
  <div class="label">Progress</div>
  <div class="row-between">
    <span>{summary.counts.ocr_done} of {summary.counts.in_scope} pages recognised</span>
    {#if average !== null}<span class="muted" title="Average word confidence over pages with at least {MIN_WORDS} words">avg {average.toFixed(0)}%</span>{/if}
  </div>
  {#if running && ocr}
    <div class="bar"><div class="fill" style="width:{ocr.total ? Math.round(((ocr.done + ocr.failed) / ocr.total) * 100) : 0}%"></div></div>
    <div class="muted small">
      This run: {ocr.done + ocr.failed} of {ocr.total}{ocr.secs_per_unit ? ` · ${ocr.secs_per_unit.toFixed(1)} s/page` : ""}{ocr.eta_ms !== null && pipeline.state === "running" ? ` · ${formatEta(ocr.eta_ms)}` : ""}
    </div>
  {/if}
  <div class="two">
    {#if pipeline.state === "running" || pipeline.state === "stopping"}
      <button type="button" class="ctl" onclick={() => pipeline.pause()} disabled={pipeline.state === "stopping"}>Pause</button>
    {:else if pipeline.state === "paused"}
      <button type="button" class="ctl primary" onclick={() => pipeline.resume()}>Resume</button>
    {:else if summary.counts.queued > 0 && !summary.read_only}
      <button type="button" class="ctl primary" onclick={() => pipeline.start()}>Process {summary.counts.queued} queued</button>
    {/if}
  </div>
</div>

{#if failed.length}
  <div class="card">
    <div class="row-between">
      <span class="label err">Failed · {failed.length}</span>
      {#if !pipeline.active}<button type="button" class="ctl small-btn primary" onclick={() => pipeline.retryFailed()}>Retry failed</button>{/if}
    </div>
    <ul class="pages">
      {#each failed as p (p.index)}
        <li>
          <button type="button" class="link" onclick={() => openPage(p.index)}>p. {p.index + 1}</button>
          <span class="muted small">{p.error ?? "no reason recorded"}</span>
        </li>
      {/each}
    </ul>
  </div>
{/if}

<div class="card">
  <div class="label">Weakest pages</div>
  {#if weakest.length}
    <p class="muted small">Lowest average word confidence first (pages with at least {MIN_WORDS} words). Check these first; Recognise region or a different model may read them better.</p>
    <ul class="pages">
      {#each weakest as c (c.page)}
        <li>
          <button type="button" class="link" onclick={() => openPage(c.page)}>p. {c.page + 1}</button>
          <span class="meter" aria-hidden="true"><span class="meter-fill" class:low={c.mean_confidence < 80} style="width:{Math.max(2, c.mean_confidence)}%"></span></span>
          <span class="mono">{c.mean_confidence.toFixed(0)}%</span>
          <span class="muted small">{c.words} words</span>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="muted small">No recognised pages yet.</p>
  {/if}
</div>

<style>
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
    margin: 4px 0 0;
    line-height: 1.4;
  }
  .card {
    display: grid;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--paper);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .label {
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .label.err {
    color: var(--danger);
  }
  .row-between {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: baseline;
    gap: 2px 10px;
    font-size: 12.5px;
  }
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 12px;
    font-size: 12.5px;
  }
  .facts span {
    color: var(--muted);
  }
  .facts b {
    font-weight: 500;
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
  .two {
    display: flex;
    gap: 8px;
  }
  .two:empty {
    display: none;
  }
  .ctl {
    flex: 1;
    padding: 6px 10px;
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
  .ctl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ctl:focus-visible,
  .link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .small-btn {
    flex: none;
    padding: 2px 8px;
    font-size: 11.5px;
  }
  .link {
    all: unset;
    color: var(--accent-text);
    cursor: pointer;
    text-decoration: underline;
    font-size: 12.5px;
  }
  .pages {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 5px;
  }
  .pages li {
    display: grid;
    grid-template-columns: 44px 1fr auto auto;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  .card:has(.err) .pages li {
    grid-template-columns: 44px 1fr;
  }
  .meter {
    height: 5px;
    background: var(--track);
    border-radius: 3px;
    overflow: hidden;
  }
  .meter-fill {
    display: block;
    height: 100%;
    background: var(--ok);
  }
  .meter-fill.low {
    background: var(--warn);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 12px;
    text-align: right;
    min-width: 34px;
  }
</style>
