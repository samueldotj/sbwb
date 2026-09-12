<script lang="ts">
  // Reading-order list (design 4.4): drag to reorder, kind chip, first
  // words, and the export placement each region will get.
  import { dndzone, type DndEvent } from "svelte-dnd-action";
  import type { OcrWord, Region } from "$lib/api";
  import { layout, exportPolicy, KIND_LABEL, KIND_PLACEMENT, KIND_PLACEMENT_NATIVE } from "$lib/stores/layout.svelte";

  type Props = { words: OcrWord[] };
  let { words }: Props = $props();

  type Item = { id: string; region: Region; preview: string };
  const items = $derived<Item[]>(
    layout.regions.map((region) => {
      const inside = words.filter((w) => w.text && w.bbox.x + w.bbox.w / 2 >= region.bbox.x && w.bbox.x + w.bbox.w / 2 <= region.bbox.x + region.bbox.w && w.bbox.y + w.bbox.h / 2 >= region.bbox.y && w.bbox.y + w.bbox.h / 2 <= region.bbox.y + region.bbox.h);
      const preview = inside.slice(0, 8).map((w) => w.text).join(" ") + (inside.length > 8 ? "…" : "");
      return { id: region.id, region, preview };
    }),
  );
  let dragItems = $state<Item[]>([]);
  $effect(() => {
    dragItems = items;
  });
  function onconsider(e: CustomEvent<DndEvent<Item>>) {
    dragItems = e.detail.items;
  }
  function onfinalize(e: CustomEvent<DndEvent<Item>>) {
    dragItems = e.detail.items;
    layout.reorder(e.detail.items.map((i) => i.id));
  }
  function moveBy(id: string, delta: number) {
    const ids = layout.regions.map((r) => r.id);
    const i = ids.indexOf(id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j]!, ids[i]!];
    layout.reorder(ids);
  }
</script>

<section class="order" aria-label="Reading order">
  <div class="header">
    <span class="strong">Reading order</span>
    <span class="muted">{layout.regions.length} regions</span>
    <span class="grow"></span>
    <span class="muted small">drag or Alt+↑/↓ to reorder</span>
  </div>
  <div class="list" use:dndzone={{ items: dragItems, flipDurationMs: 120, dropTargetStyle: {} }} onconsider={onconsider} onfinalize={onfinalize}>
    {#each dragItems as it (it.id)}
      <div
        class="row {it.region.kind}"
        class:selected={it.id === layout.selectedId}
        role="button"
        tabindex="0"
        aria-label="{KIND_LABEL[it.region.kind]}: {it.preview || 'no text'}"
        onclick={() => (layout.selectedId = it.id)}
        onkeydown={(e) => {
          if (e.key === "Enter" || e.key === " ") layout.selectedId = it.id;
          else if (e.altKey && e.key === "ArrowUp") moveBy(it.id, -1);
          else if (e.altKey && e.key === "ArrowDown") moveBy(it.id, 1);
          else return;
          e.preventDefault();
        }}
      >
        <span class="grip" aria-hidden="true">⋮⋮</span>
        <span class="kind">{KIND_LABEL[it.region.kind]}</span>
        <span class="text">{it.preview || (it.region.kind === "illustration" ? "(image)" : "(no words)")}</span>
        <span class="place muted">{it.region.kind === "body" ? `${it.region.line_count || "?"} lines` : (exportPolicy.native ? KIND_PLACEMENT_NATIVE : KIND_PLACEMENT)[it.region.kind]}</span>
      </div>
    {/each}
  </div>
  <div class="hint">Body lines are grouped into one region. Split only where paragraphs need separate Word structure (headings, table cells).</div>
</section>

<style>
  .order {
    background: var(--paper);
    display: grid;
    grid-template-rows: var(--pane-header-h) 1fr auto;
    min-height: 0;
    min-width: 0;
    border-right: 1px solid var(--border);
  }
  .header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    white-space: nowrap;
    overflow: hidden;
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
  .list {
    padding: 12px;
    display: grid;
    gap: 6px;
    align-content: start;
    overflow: auto;
    min-height: 0;
  }
  .row {
    display: grid;
    grid-template-columns: 22px 64px 1fr auto;
    gap: 10px;
    align-items: center;
    padding: 9px 10px;
    border-radius: var(--radius-control);
    border: 1px solid var(--track);
    cursor: default;
    background: var(--paper);
  }
  .row.marginalia,
  .row.footnote {
    border-color: var(--accent);
    background: var(--accent-bg);
  }
  .row.header,
  .row.page_number,
  .row.footer {
    border-color: var(--border);
  }
  .row.uncertain {
    border-color: var(--warn);
  }
  .row.selected {
    border: 2px solid var(--text);
    background: var(--bg);
  }
  .row:focus-visible {
    outline: 2px solid var(--accent);
  }
  .grip {
    color: var(--disabled);
    cursor: grab;
  }
  .kind {
    font-size: 11px;
    font-weight: 600;
  }
  .marginalia .kind,
  .footnote .kind {
    color: var(--accent-text);
  }
  .header .kind,
  .page_number .kind {
    color: var(--region-header);
  }
  .text {
    font-family: var(--font-serif);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .place {
    font-size: 11px;
    white-space: nowrap;
  }
  .hint {
    margin: 0 12px 12px;
    padding: 10px;
    border-radius: var(--radius-control);
    background: var(--bg);
    font-size: 12px;
    color: var(--text-2);
    line-height: 1.5;
  }
</style>
