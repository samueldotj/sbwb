<script lang="ts">
  // Region overlay in page-point space (design 4.4). In review mode it only
  // outlines regions; in layout mode it supports select, move, resize,
  // draw, split, and merge with pointer and keyboard.
  import type { Region } from "$lib/api";
  import { layout, KIND_LABEL } from "$lib/stores/layout.svelte";

  type Props = { regions: Region[]; zoom: number; interactive?: boolean; pageW: number; pageH: number };
  let { regions, zoom, interactive = false, pageW, pageH }: Props = $props();

  let root = $state<HTMLElement | null>(null);
  type Drag =
    | { kind: "move"; id: string; start: { x: number; y: number }; box: Region["bbox"] }
    | { kind: "resize"; id: string; corner: "nw" | "ne" | "sw" | "se"; box: Region["bbox"] }
    | { kind: "draw"; start: { x: number; y: number }; cur: { x: number; y: number } };
  let drag = $state<Drag | null>(null);
  let mergeFirst = $state<string | null>(null);

  function toPage(e: PointerEvent): { x: number; y: number } {
    const r = root!.getBoundingClientRect();
    return { x: Math.max(0, Math.min(pageW, (e.clientX - r.left) / zoom)), y: Math.max(0, Math.min(pageH, (e.clientY - r.top) / zoom)) };
  }

  function onRegionDown(e: PointerEvent, r: Region) {
    if (!interactive) return;
    e.stopPropagation();
    const p = toPage(e);
    if (layout.tool === "select") {
      layout.selectedId = r.id;
      drag = { kind: "move", id: r.id, start: p, box: { ...r.bbox } };
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } else if (layout.tool === "split") {
      layout.splitAt(r.id, p.y);
    } else if (layout.tool === "merge") {
      if (mergeFirst && mergeFirst !== r.id) {
        layout.merge(mergeFirst, r.id);
        mergeFirst = null;
      } else {
        mergeFirst = r.id;
        layout.selectedId = r.id;
      }
    } else if (layout.tool === "draw") {
      drag = { kind: "draw", start: p, cur: p };
      root!.setPointerCapture(e.pointerId);
    }
  }
  function onHandleDown(e: PointerEvent, r: Region, corner: "nw" | "ne" | "sw" | "se") {
    if (!interactive || layout.tool !== "select") return;
    e.stopPropagation();
    layout.selectedId = r.id;
    drag = { kind: "resize", id: r.id, corner, box: { ...r.bbox } };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function onRootDown(e: PointerEvent) {
    if (!interactive) return;
    const p = toPage(e);
    if (layout.tool === "draw") {
      drag = { kind: "draw", start: p, cur: p };
      root!.setPointerCapture(e.pointerId);
    } else if (layout.tool === "select") {
      layout.selectedId = null;
    }
  }
  function onMove(e: PointerEvent) {
    if (!drag) return;
    const p = toPage(e);
    if (drag.kind === "move") {
      const dx = p.x - drag.start.x;
      const dy = p.y - drag.start.y;
      layout.setBox(drag.id, { ...drag.box, x: drag.box.x + dx, y: drag.box.y + dy }, false);
    } else if (drag.kind === "resize") {
      const b = drag.box;
      let x0 = b.x, y0 = b.y, x1 = b.x + b.w, y1 = b.y + b.h;
      if (drag.corner.includes("n")) y0 = Math.min(p.y, y1 - 4);
      if (drag.corner.includes("s")) y1 = Math.max(p.y, y0 + 4);
      if (drag.corner.includes("w")) x0 = Math.min(p.x, x1 - 4);
      if (drag.corner.includes("e")) x1 = Math.max(p.x, x0 + 4);
      layout.setBox(drag.id, { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }, false);
    } else {
      drag = { ...drag, cur: p };
    }
  }
  function onUp() {
    if (!drag) return;
    if (drag.kind === "draw") {
      const x0 = Math.min(drag.start.x, drag.cur.x);
      const y0 = Math.min(drag.start.y, drag.cur.y);
      const w = Math.abs(drag.cur.x - drag.start.x);
      const h = Math.abs(drag.cur.y - drag.start.y);
      if (w > 4 && h > 4) layout.add({ x: x0, y: y0, w, h });
    }
    drag = null;
  }
  function onkeydown(e: KeyboardEvent, r: Region) {
    if (!interactive) return;
    const step = e.shiftKey ? 10 : 1;
    const b = { ...r.bbox };
    if (e.key === "ArrowLeft") b.x -= step;
    else if (e.key === "ArrowRight") b.x += step;
    else if (e.key === "ArrowUp") b.y -= step;
    else if (e.key === "ArrowDown") b.y += step;
    else if (e.key === "Delete" || e.key === "Backspace") {
      layout.remove(r.id);
      e.preventDefault();
      return;
    } else return;
    layout.setBox(r.id, b);
    e.preventDefault();
  }
  const drawRect = $derived(
    drag && drag.kind === "draw"
      ? { x: Math.min(drag.start.x, drag.cur.x), y: Math.min(drag.start.y, drag.cur.y), w: Math.abs(drag.cur.x - drag.start.x), h: Math.abs(drag.cur.y - drag.start.y) }
      : null,
  );
</script>

<div
  class="root"
  class:interactive
  class:draw={interactive && layout.tool === "draw"}
  bind:this={root}
  style="width:{pageW}px;height:{pageH}px"
  onpointerdown={onRootDown}
  onpointermove={onMove}
  onpointerup={onUp}
  role="presentation"
>
  {#each regions as r (r.id)}
    <div
      class="region {r.kind}"
      class:selected={interactive && r.id === layout.selectedId}
      class:merge-first={r.id === mergeFirst}
      style="left:{r.bbox.x}px;top:{r.bbox.y}px;width:{r.bbox.w}px;height:{r.bbox.h}px"
      {...(interactive ? { role: "button", tabindex: 0 } : {})}
      aria-label="{KIND_LABEL[r.kind]} region {r.order + 1}"
      onpointerdown={(e) => onRegionDown(e, r)}
      onkeydown={(e) => onkeydown(e, r)}
    >
      <span class="tag" style="font-size:{Math.max(6, 8 / zoom)}px">{r.order + 1} {KIND_LABEL[r.kind]}{interactive && r.id === layout.selectedId ? " · selected" : ""}</span>
      {#if interactive && r.id === layout.selectedId && layout.tool === "select"}
        {#each ["nw", "ne", "sw", "se"] as c (c)}
          <span class="handle {c}" style="width:{7 / zoom}px;height:{7 / zoom}px" onpointerdown={(e) => onHandleDown(e, r, c as "nw" | "ne" | "sw" | "se")} role="presentation"></span>
        {/each}
      {/if}
    </div>
  {/each}
  {#if drawRect}
    <div class="region uncertain drawing" style="left:{drawRect.x}px;top:{drawRect.y}px;width:{drawRect.w}px;height:{drawRect.h}px"></div>
  {/if}
</div>

<style>
  .root {
    position: relative;
    pointer-events: none;
  }
  .root.interactive {
    pointer-events: auto;
  }
  .root.draw {
    cursor: crosshair;
  }
  .region {
    position: absolute;
    box-sizing: border-box;
    outline: 1.5px solid var(--ok);
    outline-offset: 2px;
    pointer-events: none;
  }
  .interactive .region {
    pointer-events: auto;
    cursor: default;
  }
  .region.body {
    outline-style: dashed;
    outline-color: var(--ok);
  }
  .region.marginalia,
  .region.footnote {
    outline-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .region.header,
  .region.page_number,
  .region.footer,
  .region.catchword {
    outline-color: var(--region-header);
  }
  .region.heading {
    outline-color: var(--text);
  }
  .region.uncertain {
    outline-color: var(--warn);
    outline-style: dotted;
    background: color-mix(in srgb, var(--warn) 18%, transparent);
  }
  .region.ignore,
  .region.illustration {
    outline-color: var(--disabled);
  }
  .region.selected {
    outline: 2px solid var(--text);
    outline-offset: 3px;
    background: color-mix(in srgb, var(--text) 5%, transparent);
    z-index: 2;
  }
  .region.merge-first {
    outline-style: double;
  }
  .region:focus-visible {
    outline: 2px solid var(--accent);
  }
  .tag {
    position: absolute;
    left: -2px;
    top: -1.6em;
    font-family: var(--font-ui);
    font-weight: 600;
    line-height: 1.4;
    padding: 0 0.4em;
    border-radius: 2px;
    background: var(--ok);
    color: #fff;
    white-space: nowrap;
  }
  .marginalia .tag,
  .footnote .tag {
    background: var(--accent);
  }
  .header .tag,
  .page_number .tag,
  .footer .tag,
  .catchword .tag {
    background: var(--region-header);
  }
  .uncertain .tag {
    background: var(--warn);
    color: #1a1a1a;
  }
  .selected .tag {
    background: var(--text);
    color: var(--bg);
  }
  .root:not(.interactive) .tag {
    display: none;
  }
  .handle {
    position: absolute;
    background: #fff;
    border: 1.5px solid var(--text);
    pointer-events: auto;
  }
  .handle.nw {
    left: -4px;
    top: -4px;
    cursor: nwse-resize;
  }
  .handle.ne {
    right: -4px;
    top: -4px;
    cursor: nesw-resize;
  }
  .handle.sw {
    left: -4px;
    bottom: -4px;
    cursor: nesw-resize;
  }
  .handle.se {
    right: -4px;
    bottom: -4px;
    cursor: nwse-resize;
  }
  .drawing {
    outline-style: dashed;
  }
</style>
