<script lang="ts">
  // Word highlights on the scan (design 4.3, REV-03): amber for issues,
  // green for accepted readings, a double ring for the focused issue, and
  // the hovered transcript word. Rendered in page-point space inside the
  // scan overlay; marks mirror the transcript.
  import { review } from "$lib/stores/review.svelte";

  type Props = { zoom: number };
  let { zoom }: Props = $props();

  type Box = { id: string; x: number; y: number; w: number; h: number; kind: string; focused: boolean; hovered: boolean; title: string };
  const boxes = $derived.by((): Box[] => {
    const out: Box[] = [];
    const selected = review.selected;
    for (const s of review.spans) {
      const m = review.markFor(s);
      const focused = selected !== null && selected.span === s.id;
      const hovered = review.hoverSpan === s.id;
      if (m.kind === "none" && !focused && !hovered) continue;
      for (const a of s.anchors) {
        out.push({
          id: s.id,
          x: a.bbox.x,
          y: a.bbox.y,
          w: a.bbox.w,
          h: a.bbox.h,
          kind: m.kind,
          focused,
          hovered,
          title: m.issue ? `${m.issue.original}${m.issue.replacement ? ` → ${m.issue.replacement}` : ""}` : s.text,
        });
      }
    }
    // structural issues: dashed boxes on the scan
    for (const i of review.issues) {
      if (!i.bbox || i.candidates.length > 0) continue;
      const shown = review.matching.some((m) => m.id === i.id);
      const focused = selected?.id === i.id;
      if (!shown && !focused) continue;
      out.push({ id: i.id, x: i.bbox.x, y: i.bbox.y, w: i.bbox.w, h: i.bbox.h, kind: "structural", focused, hovered: false, title: i.reason });
    }
    return out;
  });
  const ring = $derived(Math.max(1, 1.5 / zoom));
</script>

{#each boxes as b (b.id + b.x + b.y)}
  <div
    class="hl {b.kind}"
    class:focused={b.focused}
    class:hovered={b.hovered}
    style="left:{b.x - 1.5}px;top:{b.y - 1.5}px;width:{b.w + 3}px;height:{b.h + 3}px;--ring:{ring}px"
    title={b.title}
  ></div>
{/each}

<style>
  .hl {
    position: absolute;
    border-radius: 2px;
    box-sizing: border-box;
    mix-blend-mode: multiply;
  }
  :global([data-theme="bench"]) .hl {
    mix-blend-mode: screen;
  }
  .hl.issue,
  .hl.deferred {
    background: var(--accent-bg-scan);
    outline: var(--ring) solid var(--accent);
  }
  .hl.deferred {
    outline-style: dotted;
  }
  .hl.structural {
    background: transparent;
    outline: var(--ring) dashed var(--danger);
  }
  .hl.accepted {
    background: var(--ok-bg-scan);
    outline: var(--ring) solid var(--ok);
  }
  .hl.auto {
    background: var(--ok-bg-scan);
  }
  .hl.hovered {
    outline: var(--ring) solid var(--text);
  }
  .hl.focused {
    outline: calc(var(--ring) * 2) double var(--accent);
    background: var(--accent-bg-scan);
  }
</style>
