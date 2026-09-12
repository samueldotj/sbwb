<script lang="ts">
  // Transcript pane (design 4.3). M2: shows raw OCR lines when a page has
  // evidence; the effective text with marks arrives with M5/M6.
  import { api, type OcrWord, type PageRow, type Region } from "$lib/api";
  import { KIND_LABEL } from "$lib/stores/layout.svelte";
  import { view } from "$lib/stores/view.svelte";
  import type { Snippet } from "svelte";

  type Props = { page: PageRow | null; header?: Snippet };
  let { page, header }: Props = $props();

  type Line = { text: string; conf: number };
  type Group = { label: string; kind: string; lines: Line[] };
  let lines = $state<Line[]>([]);
  let groups = $state<Group[]>([]);

  function inside(w: OcrWord, r: Region): boolean {
    const cx = w.bbox.x + w.bbox.w / 2;
    const cy = w.bbox.y + w.bbox.h / 2;
    return cx >= r.bbox.x && cx <= r.bbox.x + r.bbox.w && cy >= r.bbox.y && cy <= r.bbox.y + r.bbox.h;
  }
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
  let meanConf = $state<number | null>(null);
  let status = $state<"idle" | "loading" | "none" | "ready">("idle");
  let shownFor = $state<number | null>(null);

  $effect(() => {
    const idx = view.page;
    if (page?.status !== "done") {
      lines = [];
      meanConf = null;
      status = "none";
      shownFor = idx;
      return;
    }
    status = "loading";
    let cancelled = false;
    api
      .pageOcr(idx)
      .then((o) => {
        if (cancelled) return;
        if (!o) {
          lines = [];
          status = "none";
        } else {
          lines = toLines(o.words);
          meanConf = o.mean_confidence;
          status = "ready";
          // group by layout regions in reading order when a layout exists
          api
            .pageLayout(idx)
            .then((l) => {
              if (cancelled || !l || l.regions.length === 0) {
                groups = [];
                return;
              }
              const used = new Set<number>();
              const out: Group[] = [];
              for (const r of [...l.regions].sort((a, b) => a.order - b.order)) {
                if (r.kind === "ignore" || r.kind === "illustration") continue;
                const ws = o.words.filter((w, i) => !used.has(i) && inside(w, r));
                ws.forEach((w) => used.add(o.words.indexOf(w)));
                if (ws.length) out.push({ label: KIND_LABEL[r.kind], kind: r.kind, lines: toLines(ws) });
              }
              const rest = o.words.filter((_, i) => !used.has(i) && o.words[i]!.text);
              if (rest.length) out.push({ label: "outside regions", kind: "uncertain", lines: toLines(rest) });
              groups = out;
            })
            .catch(() => (groups = []));
        }
        shownFor = idx;
      })
      .catch(() => {
        if (!cancelled) status = "none";
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<section class="transcript" aria-label="Transcript">
  <div class="header">
    <span class="strong">Transcript</span>
    {#if status === "ready" && meanConf !== null}
      <span class="muted">mean OCR score {Math.round(meanConf)}</span>
    {/if}
    <span class="grow"></span>
    {#if header}{@render header()}{/if}
  </div>
  <div class="body">
    {#if status === "loading" && shownFor !== view.page}
      <p class="muted">Loading…</p>
    {:else if status === "none"}
      <p class="muted">No recognized text for page {view.page + 1} yet.
        {#if page?.status === "queued"}It is queued for OCR.{:else if page?.status === "unprocessed"}It is outside the processing scope.{:else if page?.status === "failed"}OCR failed: {page.error}{/if}
      </p>
    {:else if groups.length}
      {#each groups as g, gi (gi)}
        <div class="group {g.kind}">
          <div class="glabel">{g.label}</div>
          {#each g.lines as l, i (i)}
            <p class="line" class:low={l.conf < 70}>{l.text}</p>
          {/each}
        </div>
      {/each}
    {:else}
      {#each lines as l, i (i)}
        <p class="line" class:low={l.conf < 70}>{l.text}</p>
      {/each}
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
  .body {
    overflow: auto;
    padding: 18px 26px;
    font-family: var(--font-serif);
    font-size: var(--font-size-transcript);
    line-height: var(--line-height-transcript);
    color: var(--text);
    user-select: text;
  }
  .line {
    margin: 0;
    text-wrap: pretty;
  }
  .group {
    margin-bottom: 14px;
  }
  .glabel {
    font: 500 10px var(--font-ui);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 2px;
  }
  .group.header .line,
  .group.page_number .line,
  .group.footer .line {
    font-variant: small-caps;
    letter-spacing: 0.06em;
  }
  .group.marginalia .line,
  .group.footnote .line {
    font-size: 13px;
    color: var(--text-2);
  }
  .group.uncertain .glabel {
    color: var(--accent-text);
  }
  .line.low {
    color: var(--accent-text);
  }
  .body p.muted {
    font-family: var(--font-ui);
    font-size: 13px;
  }
</style>
