<script lang="ts">
  // Expanded job monitor (design 4.2, artboard 1h): per-stage cards and a
  // timestamped log. Shown in the rail column in Processing mode.
  import { pipeline, formatEta } from "$lib/stores/pipeline.svelte";
  import type { ProjectSummary } from "$lib/api";

  type Props = { summary: ProjectSummary; settingsLine: string; oncollapse: () => void };
  let { summary, settingsLine, oncollapse }: Props = $props();

  const ocr = $derived(pipeline.ocr);
  const pct = $derived(ocr && ocr.total ? (ocr.done + ocr.failed) / ocr.total : 0);
  function t(ts: string): string {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour12: false });
  }
</script>

<div class="monitor">
  <div class="head">
    <span class="label">pipeline · {pipeline.state}</span>
    <span class="btns">
      {#if pipeline.state === "running"}
        <button type="button" onclick={() => pipeline.pause()}>Pause</button>
      {:else if pipeline.state === "paused"}
        <button type="button" onclick={() => pipeline.resume()}>Resume</button>
      {:else if summary.counts.queued > 0}
        <button type="button" onclick={() => pipeline.start()}>Start</button>
      {/if}
      <button type="button" class="muted" onclick={oncollapse}>Collapse</button>
    </span>
  </div>

  <div class="cards">
    <div class="card done">
      <div class="row"><span><i class="sq ok"></i>Import</span><span class="mono muted">{summary.meta.source.page_count} pp</span></div>
    </div>
    <div class="card" class:running={pipeline.state === "running" || pipeline.state === "paused"}>
      <div class="row">
        <span><i class="sq" class:ok={ocr && ocr.done + ocr.failed >= ocr.total && ocr.total > 0} class:live={pipeline.state === "running"}></i><b>OCR</b></span>
        <span class="mono accent">{ocr ? `${ocr.done}/${ocr.total}` : summary.counts.queued ? "queued" : "—"}{ocr?.secs_per_unit ? ` · ${ocr.secs_per_unit.toFixed(1)}s/p` : ""}</span>
      </div>
      {#if ocr}
        <div class="bar"><div class="fill" style="width:{Math.round(pct * 100)}%"></div></div>
        <div class="mono muted small">{settingsLine}{ocr.eta_ms !== null ? ` · ${formatEta(ocr.eta_ms)}` : ""}{ocr.failed ? ` · ${ocr.failed} failed` : ""}</div>
      {/if}
    </div>
    <div class="card off"><div class="row"><span><i class="sq"></i>Layout</span><span class="mono muted">queued</span></div></div>
    <div class="card off"><div class="row"><span><i class="sq"></i>Text pass</span><span class="mono muted">queued · ≥90%</span></div></div>
    <div class="card off"><div class="row"><span><i class="sq"></i>AI proof</span><span class="mono muted">off</span></div></div>
  </div>

  <div class="label">log</div>
  <div class="log" role="log" aria-live="polite">
    {#each pipeline.log as l, i (i)}
      <div class="line {l.level}"><span class="muted">{t(l.ts)}</span> {l.text}</div>
    {:else}
      <div class="muted">Nothing yet.</div>
    {/each}
  </div>
</div>

<style>
  .monitor {
    width: 360px;
    background: var(--panel);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 12px;
    gap: 8px;
    overflow: hidden;
    min-height: 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .btns {
    display: flex;
    gap: 6px;
  }
  .btns button {
    all: unset;
    padding: 3px 8px;
    border: 1px solid var(--border-input);
    border-radius: 4px;
    font-size: 11px;
    cursor: default;
  }
  .btns button.muted {
    color: var(--muted);
  }
  .btns button:focus-visible {
    outline: 2px solid var(--accent);
  }
  .cards {
    display: grid;
    gap: 4px;
  }
  .card {
    padding: 10px;
    border-radius: 5px;
    border: 1px solid var(--border);
    background: var(--raised);
  }
  .card.off {
    background: none;
    color: var(--muted);
  }
  .card.running {
    border-color: var(--accent);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .row > span:first-child {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .sq {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    border: 1px solid var(--disabled);
    display: inline-block;
  }
  .sq.ok {
    background: var(--ok);
    border-color: var(--ok);
  }
  .sq.live {
    background: var(--accent);
    border-color: var(--accent);
    box-shadow: var(--accent-glow);
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  .accent {
    color: var(--accent);
  }
  .muted {
    color: var(--muted);
  }
  .small {
    margin-top: 6px;
  }
  .bar {
    height: 3px;
    background: var(--track);
    margin: 8px 0 0;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 200ms;
  }
  .log {
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--chrome);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 10px;
    font-family: var(--font-mono);
    font-size: 10.5px;
    line-height: 1.7;
    color: var(--text-2);
    user-select: text;
  }
  .line.warn {
    color: var(--warn);
  }
  .line.error {
    color: var(--danger);
  }
</style>
