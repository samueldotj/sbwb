<script lang="ts">
  // Region inspector (design 4.4): type, Word structure, bounds, split and
  // merge, with the consequence of a body change stated before saving.
  import type { OcrWord, RegionKind, WordStructure } from "$lib/api";
  import { layout, KIND_LABEL } from "$lib/stores/layout.svelte";

  type Props = { pageW: number; pageH: number; words: OcrWord[] };
  let { pageW, pageH, words }: Props = $props();

  const KINDS: RegionKind[] = ["body", "heading", "header", "marginalia", "footnote", "page_number", "footer", "catchword", "illustration", "table", "uncertain", "ignore"];
  const STRUCTURES: [WordStructure, string][] = [
    ["text", "Ordinary text"],
    ["heading1", "Heading 1"],
    ["heading2", "Heading 2"],
    ["heading3", "Heading 3"],
    ["table_cell", "Table cell"],
  ];
  const r = $derived(layout.selected);
  const pct = (v: number, total: number) => `${((v / total) * 100).toFixed(1)}%`;

  // Line bottoms inside the region, for "split after line N".
  const lineBottoms = $derived.by(() => {
    if (!r) return [] as number[];
    const inside = words.filter((w) => w.text && w.bbox.y + w.bbox.h / 2 >= r.bbox.y && w.bbox.y + w.bbox.h / 2 <= r.bbox.y + r.bbox.h && w.bbox.x + w.bbox.w / 2 >= r.bbox.x && w.bbox.x + w.bbox.w / 2 <= r.bbox.x + r.bbox.w);
    const byLine = new Map<string, number>();
    for (const w of inside) {
      const k = `${w.block}/${w.paragraph}/${w.line}`;
      byLine.set(k, Math.max(byLine.get(k) ?? 0, w.bbox.y + w.bbox.h));
    }
    return [...byLine.values()].sort((a, b) => a - b);
  });
  let splitAfter = $state(1);
  function doSplit() {
    if (!r) return;
    const bottom = lineBottoms[splitAfter - 1];
    const next = lineBottoms[splitAfter];
    if (bottom === undefined) return;
    const y = next !== undefined ? (bottom + next) / 2 : bottom + 1;
    layout.splitAt(r.id, y);
  }
</script>

{#if r}
  <div>
    <div class="label">Type</div>
    <div class="chips" role="radiogroup" aria-label="Region type">
      {#each KINDS as k (k)}
        <button type="button" role="radio" aria-checked={r.kind === k} class="chip" class:on={r.kind === k} onclick={() => layout.setKind(r.id, k)}>{KIND_LABEL[k]}</button>
      {/each}
    </div>
  </div>
  <div>
    <div class="label">In Word</div>
    <div class="radios" role="radiogroup" aria-label="Word structure">
      {#each STRUCTURES as [s, label] (s)}
        <label class="radio" class:muted={r.structure !== s}>
          <input type="radio" name="structure" value={s} checked={r.structure === s} onchange={() => layout.setStructure(r.id, s)} />
          <span class="dot" class:on={r.structure === s}></span>{label}
        </label>
      {/each}
    </div>
  </div>
  <div>
    <div class="label">Bounds</div>
    <div class="bounds">
      <div><span class="muted">x</span>{pct(r.bbox.x, pageW)}</div>
      <div><span class="muted">y</span>{pct(r.bbox.y, pageH)}</div>
      <div><span class="muted">w</span>{pct(r.bbox.w, pageW)}</div>
      <div><span class="muted">h</span>{pct(r.bbox.h, pageH)}</div>
    </div>
  </div>
  <div class="two">
    <div class="split">
      <label class="muted small" for="split-after">Split after line</label>
      <div class="row">
        <input id="split-after" type="number" min="1" max={Math.max(1, lineBottoms.length - 1)} bind:value={splitAfter} disabled={lineBottoms.length < 2} />
        <button type="button" class="btn" onclick={doSplit} disabled={lineBottoms.length < 2}>Split</button>
      </div>
    </div>
    <button type="button" class="btn" onclick={() => layout.mergeWithNext(r.id)} disabled={layout.regions[layout.regions.length - 1]?.id === r.id}>Merge with next</button>
  </div>
  <button type="button" class="btn danger" onclick={() => layout.remove(r.id)}>Delete region</button>
  {#if r.kind === "body"}
    <div class="hint">Changing a body region re-runs the text pass for this page and clears its approval.</div>
  {/if}
{:else}
  <p class="muted">Select a region on the scan or in the reading order. Draw (R) adds one; Split (X) and Merge (M) act on click.</p>
  {#if layout.report}
    <div class="label">Coverage</div>
    <div class="facts">
      <div><span class="muted">Words in regions</span><span>{Number(layout.report.words_total ?? 0) - Number(layout.report.words_uncovered ?? 0)} / {String(layout.report.words_total ?? 0)}</span></div>
      <div><span class="muted">Unrecognized areas</span><span>{Array.isArray(layout.report.uncovered_text) ? layout.report.uncovered_text.length : 0}</span></div>
      <div><span class="muted">Columns</span><span>{String(layout.report.columns ?? 1)}</span></div>
      <div><span class="muted">Segmentation disagreements</span><span>{String(layout.report.segmentation_disagreements ?? 0)}</span></div>
      <div><span class="muted">Source</span><span>{layout.manual ? "manual" : "automatic"} · rev {layout.revision}</span></div>
    </div>
  {/if}
{/if}

<style>
  .label {
    margin-bottom: 6px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip {
    all: unset;
    padding: 4px 10px;
    border-radius: 12px;
    border: 1px solid var(--border-input);
    font-size: 12px;
    cursor: default;
  }
  .chip.on {
    background: var(--primary-bg);
    color: var(--primary-fg);
    border-color: var(--primary-bg);
  }
  .chip:focus-visible,
  .btn:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .radios {
    display: grid;
    gap: 5px;
  }
  .radio {
    display: flex;
    gap: 8px;
    align-items: center;
    cursor: default;
  }
  .radio input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }
  .radio .dot {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    border: 1px solid var(--disabled);
    background: var(--paper);
  }
  .radio .dot.on {
    border: 4px solid var(--text);
  }
  .radio input:focus-visible + .dot {
    outline: 2px solid var(--accent);
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .bounds {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    font-size: 12px;
  }
  .bounds > div {
    padding: 6px 8px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    display: flex;
    justify-content: space-between;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    align-items: end;
  }
  .split {
    display: grid;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 4px;
  }
  input[type="number"] {
    width: 52px;
    padding: 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
  }
  .btn {
    all: unset;
    text-align: center;
    padding: 7px;
    border-radius: var(--radius-control);
    border: 1px solid var(--border-input);
    background: var(--paper);
    font-size: 12px;
    cursor: default;
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .btn.danger {
    color: var(--danger);
  }
  .hint {
    font-size: 11.5px;
    color: var(--muted);
    line-height: 1.5;
  }
  .facts {
    display: grid;
    gap: 6px;
    font-size: 12.5px;
  }
  .facts > div {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
</style>
