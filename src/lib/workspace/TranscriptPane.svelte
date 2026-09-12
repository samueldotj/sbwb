<script lang="ts">
  // Transcript pane (design 4.3, REV-02, REV-03). Header: title, issues on
  // this page, the review threshold slider. Body: running head in small
  // caps, logical paragraphs of the effective text, footnotes after a
  // divider. Marks: issue (amber), deferred (dotted), accepted (green),
  // auto-applied (light green); the focused issue carries a ring. Hover or
  // focus on a mark highlights the source word on the scan; click selects
  // the issue. Falls back to raw OCR lines before the text pass has run.
  import { api, type OcrWord, type PageRow, type Span } from "$lib/api";
  import { KIND_LABEL } from "$lib/stores/layout.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { view } from "$lib/stores/view.svelte";
  import type { Snippet } from "svelte";

  type Props = { page: PageRow | null; header?: Snippet };
  let { page, header }: Props = $props();

  // ----- effective text -----
  type Word = { span: Span; mark: ReturnType<typeof review.markFor> };
  type Para = { kind: string; label: string; words: Word[]; region: string | null };
  const paras = $derived.by((): Para[] => {
    if (review.page !== view.page) return [];
    const byRegion = new Map(review.regions.map((r) => [r.id, r]));
    const out: Para[] = [];
    let cur: Para | null = null;
    let lastRegion: string | null | undefined;
    for (const sp of review.spans) {
      const r = sp.region ? byRegion.get(sp.region) : undefined;
      if (sp.paragraph_start || sp.region !== lastRegion || !cur) {
        cur = { kind: r?.kind ?? "uncertain", label: r ? KIND_LABEL[r.kind] : "outside regions", words: [], region: sp.region };
        out.push(cur);
        lastRegion = sp.region;
      }
      cur.words.push({ span: sp, mark: review.markFor(sp) });
    }
    return out;
  });
  const head = $derived(paras.filter((p) => p.kind === "header" || p.kind === "page_number"));
  const body = $derived(paras.filter((p) => !["header", "page_number", "footer", "footnote"].includes(p.kind)));
  const foot = $derived(paras.filter((p) => p.kind === "footer" || p.kind === "footnote"));
  const headLine = $derived(head.map((p) => p.words.map((w) => w.span.text).join(" ")).join(" · "));
  const onPage = $derived(review.page === view.page ? review.matching.length : 0);

  // ----- raw fallback -----
  type Line = { text: string; conf: number };
  let lines = $state<Line[]>([]);
  let rawFor = $state<number | null>(null);
  function toLines(ws: OcrWord[]): Line[] {
    const byLine = new Map<string, { words: string[]; conf: number[]; y: number }>();
    for (const w of ws) {
      const k = `${w.block}/${w.paragraph}/${w.line}`;
      const e = byLine.get(k) ?? { words: [], conf: [], y: w.bbox.y };
      if (w.text) e.words.push(w.text);
      e.conf.push(w.confidence);
      byLine.set(k, e);
    }
    return [...byLine.values()]
      .sort((a, b) => a.y - b.y)
      .filter((l) => l.words.length)
      .map((l) => ({ text: l.words.join(" "), conf: l.conf.reduce((a, b) => a + b, 0) / l.conf.length }));
  }
  $effect(() => {
    const idx = view.page;
    if (!page?.ocr_done || page.text_done) {
      lines = [];
      rawFor = idx;
      return;
    }
    let cancelled = false;
    api
      .pageOcr(idx)
      .then((o) => {
        if (cancelled) return;
        lines = o ? toLines(o.words) : [];
        rawFor = idx;
      })
      .catch(() => {
        if (!cancelled) lines = [];
      });
    return () => {
      cancelled = true;
    };
  });

  // Keep the focused issue in view.
  let bodyEl = $state<HTMLElement | null>(null);
  $effect(() => {
    const id = review.selectedId;
    if (!id || !bodyEl) return;
    const issue = review.selected;
    if (!issue) return;
    const el = bodyEl.querySelector<HTMLElement>(`[data-span="${issue.span}"]`);
    el?.scrollIntoView({ block: "center" });
  });

  function onMarkKey(e: KeyboardEvent, w: Word) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (w.mark.issue) review.select(w.mark.issue.id);
    }
  }
  function title(w: Word): string {
    const i = w.mark.issue;
    if (i) return `${i.reason}${i.score !== null ? ` · ${i.score}% · ${i.score_source}` : ""}`;
    if (w.mark.kind === "accepted") return "accepted reading";
    if (w.mark.kind === "auto") return "changed by the text pass";
    return "";
  }
</script>

<section class="transcript" aria-label="Transcript">
  <div class="header">
    <span class="strong">Transcript</span>
    {#if review.page === view.page && review.spans.length}
      <span class="muted count" aria-live="polite">{onPage} {review.deferredView ? "deferred" : onPage === 1 ? "issue" : "issues"}<span class="lbl"> on this page</span></span>
    {/if}
    <span class="grow"></span>
    <label class="slider">
      <span class="muted"><span class="lbl">Show issues </span>below</span>
      <input
        type="range"
        min="0"
        max="100"
        step="1"
        bind:value={review.threshold}
        onchange={() => {
          review.savePrefs();
          void review.refreshCounts();
        }}
        aria-label="Review threshold: show issues below this score"
        aria-valuetext="{review.threshold} percent"
      />
      <span class="mono">{review.threshold}%</span>
    </label>
    {#if header}{@render header()}{/if}
  </div>
  <div class="body" bind:this={bodyEl}>
    {#if paras.length}
      {#if headLine}
        <p class="runhead" aria-label="Running head">{headLine}</p>
      {/if}
      {#each body as p, pi (pi)}
        <p class="para {p.kind}" data-label={p.label}>
          {#each p.words as w (w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button
              type="button"
              class="mark {w.mark.kind}"
              class:focused={review.selected?.span === w.span.id}
              class:hover={review.hoverSpan === w.span.id}
              data-span={w.span.id}
              aria-label="{w.span.text}: {w.mark.issue.kind.replace('_', ' ')}"
              title={title(w)}
              onmouseenter={() => (review.hoverSpan = w.span.id)}
              onmouseleave={() => (review.hoverSpan = null)}
              onfocus={() => (review.hoverSpan = w.span.id)}
              onblur={() => (review.hoverSpan = null)}
              onclick={() => review.select(w.mark.issue!.id)}
              onkeydown={(e) => onMarkKey(e, w)}>{w.span.text}</button>{:else if w.mark.kind !== "none"}<mark
              class="mark {w.mark.kind}"
              class:focused={review.selected?.span === w.span.id}
              data-span={w.span.id}
              title={title(w)}>{w.span.text}</mark>{:else}<span
              data-span={w.span.id}
              class:focused={review.selected?.span === w.span.id}>{w.span.text}</span>{/if}{w.span.trailing}{/each}
        </p>
      {/each}
      {#if foot.length}
        <hr class="divider" />
        {#each foot as p, pi (pi)}
          <p class="para {p.kind}" data-label={p.label}>
            {#each p.words as w (w.span.id)}{#if w.mark.issue && w.mark.kind !== "none"}<button type="button" class="mark {w.mark.kind}" class:focused={review.selected?.span === w.span.id} data-span={w.span.id} aria-label="{w.span.text}: {w.mark.issue.kind.replace('_', ' ')}" title={title(w)} onmouseenter={() => (review.hoverSpan = w.span.id)} onmouseleave={() => (review.hoverSpan = null)} onfocus={() => (review.hoverSpan = w.span.id)} onblur={() => (review.hoverSpan = null)} onclick={() => review.select(w.mark.issue!.id)} onkeydown={(e) => onMarkKey(e, w)}>{w.span.text}</button>{:else if w.mark.kind !== "none"}<mark class="mark {w.mark.kind}" data-span={w.span.id} title={title(w)}>{w.span.text}</mark>{:else}<span data-span={w.span.id}>{w.span.text}</span>{/if}{w.span.trailing}{/each}
          </p>
        {/each}
      {/if}
    {:else if review.loading && review.page !== view.page}
      <p class="muted note">Loading…</p>
    {:else if lines.length && rawFor === view.page}
      <p class="muted note">Raw OCR lines · the text pass has not run on this page yet.</p>
      {#each lines as l, i (i)}
        <p class="line" class:low={l.conf < 70}>{l.text}</p>
      {/each}
    {:else}
      <p class="muted note">
        No text for page {view.page + 1} yet.
        {#if page?.status === "queued"}It is queued for processing.{:else if page?.status === "unprocessed"}It is outside the processing scope.{:else if page?.status === "failed"}Processing failed: {page.error}{/if}
      </p>
    {/if}
  </div>
</section>

<style>
  .transcript {
    background: var(--paper);
    display: grid;
    grid-template-rows: var(--pane-header-h) 1fr;
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
  }
  .header {
    display: flex;
    min-width: 0;
    overflow: hidden;
    container-type: inline-size;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-soft);
    white-space: nowrap;
  }
  .strong {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
  .grow {
    flex: 1;
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .slider input {
    width: 80px;
    min-width: 50px;
    accent-color: var(--accent);
  }
  @container (max-width: 560px) {
    .lbl {
      display: none;
    }
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
    min-width: 32px;
  }
  .body {
    overflow: auto;
    padding: 18px 26px;
    font-family: var(--font-serif);
    font-size: var(--font-size-transcript);
    line-height: var(--line-height-transcript);
    color: var(--text);
    user-select: text;
  }
  .runhead {
    margin: 0 0 14px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border-soft);
    font-variant: small-caps;
    letter-spacing: 0.08em;
    font-size: 0.85em;
    color: var(--text-2);
  }
  .para {
    margin: 0 0 0.9em;
    text-wrap: pretty;
    white-space: pre-wrap;
  }
  .para.heading {
    font-weight: 600;
    text-align: center;
  }
  .para.marginalia,
  .para.footnote,
  .para.footer {
    font-size: 0.86em;
    color: var(--text-2);
  }
  .para.marginalia::before,
  .para.footnote::before,
  .para.footer::before,
  .para.uncertain::before {
    content: attr(data-label);
    display: block;
    font: 500 10px var(--font-ui);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .divider {
    border: 0;
    border-top: 1px solid var(--border-soft);
    margin: 12px 0 14px;
  }
  .mark {
    background: transparent;
    color: inherit;
    font: inherit;
    padding: 0;
    border: 0;
    border-radius: 2px;
    text-decoration-thickness: 2px;
    text-underline-offset: 3px;
    cursor: pointer;
  }
  .mark.accepted {
    background: var(--ok-bg);
    text-decoration: underline var(--ok);
  }
  .mark.auto {
    text-decoration: underline var(--ok);
    text-decoration-style: dotted;
  }
  .mark.issue {
    background: var(--accent-bg);
    text-decoration: underline var(--accent);
  }
  .mark.deferred {
    text-decoration: underline dotted var(--accent);
  }
  .mark.focused,
  span.focused {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-radius: 2px;
  }
  .mark.hover {
    outline: 1px solid var(--text);
    outline-offset: 1px;
  }
  .mark:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .line {
    margin: 0;
    text-wrap: pretty;
  }
  .line.low {
    color: var(--accent-text);
  }
  .note {
    font-family: var(--font-ui);
    font-size: 13px;
  }
</style>
