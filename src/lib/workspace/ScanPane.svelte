<script lang="ts">
  // Scan pane (design 4.3, UX-03, D-14). The previous page stays visible
  // until the requested render has loaded; stale loads never replace a newer
  // selection; zoom re-renders at a quantized backend scale and pan is
  // plain scrolling.
  import type { Snippet } from "svelte";
  import type { PageRow } from "$lib/api";
  import { fitZoom, nextZoom, pickScale, renderUrl } from "$lib/render";
  import { view } from "$lib/stores/view.svelte";

  type Props = {
    pages: PageRow[];
    /** Overlay rendered in page-point coordinates (highlights, regions). */
    overlay?: Snippet<[{ page: number; zoom: number }]>;
    footer?: Snippet;
    tools?: Snippet;
    overlayInteractive?: boolean;
  };
  let { pages, overlay, footer, tools, overlayInteractive = false }: Props = $props();

  let body = $state<HTMLElement | null>(null);
  let paneW = $state(800);
  let paneH = $state(600);
  let dpr = $state(1);

  // The page actually on screen (may lag `view.page` while loading).
  let shownPage = $state<number | null>(null);
  let shownSrc = $state<string | null>(null);
  let shownZoom = $state(1);
  let pending = $state<{ page: number; src: string; zoom: number; token: number } | null>(null);
  let token = 0;
  let gotoText = $state("");

  const current = $derived(pages[view.page] ?? null);
  const wPt = $derived(current?.width_pt ?? 612);
  const hPt = $derived(current?.height_pt ?? 792);
  const effectiveZoom = $derived(
    view.zoomMode === "fit"
      ? fitZoom(wPt, hPt, paneW - 36, paneH - 36, "page")
      : view.zoomMode === "width"
        ? fitZoom(wPt, hPt, paneW - 36, paneH - 36, "width")
        : view.zoom,
  );
  const scale = $derived(pickScale(effectiveZoom, dpr));
  const src = $derived(renderUrl(view.page, scale));
  $effect(() => {
    view.renderScale = scale;
  });
  const label = $derived(current?.printed_label ?? null);

  // Request a new render whenever page or scale changes.
  $effect(() => {
    const want = { page: view.page, src, zoom: effectiveZoom };
    if (shownSrc === want.src) {
      shownZoom = want.zoom;
      pending = null;
      view.loading = false;
      return;
    }
    const t = ++token;
    pending = { ...want, token: t };
    view.loading = true;
    const img = new Image();
    img.decoding = "async";
    img.onload = () => {
      if (t !== token) return; // stale
      shownPage = want.page;
      shownSrc = want.src;
      shownZoom = want.zoom;
      pending = null;
      view.loading = false;
    };
    img.onerror = () => {
      if (t !== token) return;
      pending = null;
      view.loading = false;
      loadError = `Could not render page ${want.page + 1}`;
    };
    loadError = "";
    img.src = want.src;
  });
  let loadError = $state("");

  $effect(() => {
    if (!body) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) {
        paneW = r.width;
        paneH = r.height;
      }
    });
    ro.observe(body);
    dpr = window.devicePixelRatio || 1;
    const mq = window.matchMedia(`(resolution: ${dpr}dppx)`);
    const onchange = () => (dpr = window.devicePixelRatio || 1);
    mq.addEventListener("change", onchange);
    return () => {
      ro.disconnect();
      mq.removeEventListener("change", onchange);
    };
  });

  function zoomBy(dir: 1 | -1) {
    view.zoom = nextZoom(effectiveZoom, dir);
    view.zoomMode = "custom";
  }
  function fit(mode: "fit" | "width") {
    view.zoomMode = mode;
  }
  function submitGoto() {
    const n = Number(gotoText);
    if (Number.isFinite(n) && n >= 1) view.goTo(n - 1);
    gotoText = "";
  }
  const pct = $derived(Math.round(effectiveZoom * 100));
</script>

<section class="scan" aria-label="Scan">
  <div class="header">
    <button type="button" class="nav" onclick={() => view.prev()} disabled={view.page === 0} aria-label="Previous page">‹</button>
    <span class="pageno"><b>Page {view.page + 1}</b> <span class="muted">of {view.pageCount}</span></span>
    <button type="button" class="nav" onclick={() => view.next()} disabled={view.page >= view.pageCount - 1} aria-label="Next page">›</button>
    <form class="goto" onsubmit={(e) => { e.preventDefault(); submitGoto(); }}>
      <input type="text" inputmode="numeric" placeholder="Go to" aria-label="Go to page" bind:value={gotoText} size="4" />
    </form>
    {#if tools}<span class="sep"></span>{@render tools()}{/if}
    <span class="grow"></span>
    <button type="button" class="nav" onclick={() => zoomBy(-1)} aria-label="Zoom out">−</button>
    <button type="button" class="text" class:on={view.zoomMode === "fit"} onclick={() => fit("fit")}>Fit</button>
    <span class="muted">·</span>
    <span class="pct" aria-live="polite">{pct}%</span>
    <button type="button" class="nav" onclick={() => zoomBy(1)} aria-label="Zoom in">+</button>
    {#if label}<span class="chip">{label}</span>{/if}
    {#if current}<span class="chip muted">{current.status}</span>{/if}
  </div>
  <div class="body" bind:this={body}>
    {#if shownSrc && shownPage !== null}
      <div class="page-wrap" style="width:{wPt * shownZoom}px;height:{hPt * shownZoom}px">
        <img src={shownSrc} alt="Scan of page {shownPage + 1}" draggable="false" style="width:{wPt * shownZoom}px;height:{hPt * shownZoom}px" />
        {#if overlay}
          <div class="overlay" class:live={overlayInteractive} style="width:{wPt}px;height:{hPt}px;transform:scale({shownZoom})">
            {@render overlay({ page: shownPage, zoom: shownZoom })}
          </div>
        {/if}
        {#if pending && pending.page !== shownPage}
          <div class="veil" aria-hidden="true"></div>
        {/if}
      </div>
    {:else if !loadError}
      <div class="placeholder muted">Rendering page {view.page + 1}…</div>
    {/if}
    {#if loadError}
      <div class="placeholder error" role="alert">{loadError}</div>
    {/if}
    {#if pending}
      <div class="loading" role="status" aria-live="polite">Loading page {pending.page + 1}…</div>
    {/if}
    {#if footer}<div class="footer">{@render footer()}</div>{/if}
  </div>
</section>

<style>
  .scan {
    background: var(--scan-bg);
    display: grid;
    grid-template-rows: var(--pane-header-h) 1fr;
    grid-template-columns: minmax(0, 1fr);
    min-height: 0;
    min-width: 0;
    border-right: 1px solid var(--scan-border);
    position: relative;
  }
  .header {
    display: flex;
    min-width: 0;
    overflow: hidden;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: var(--panel-header);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .nav,
  .text {
    all: unset;
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid var(--border-input);
    background: var(--paper);
    cursor: default;
    line-height: 1.2;
  }
  .text {
    border-color: transparent;
    background: none;
    color: var(--muted);
  }
  .text.on {
    color: var(--text);
    font-weight: 600;
  }
  .nav:disabled {
    opacity: 0.4;
  }
  .nav:focus-visible,
  .text:focus-visible,
  .goto input:focus-visible {
    outline: 2px solid var(--accent);
  }
  .goto input {
    width: 56px;
    padding: 3px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    font-size: 12px;
  }
  .muted {
    color: var(--muted);
  }
  .grow {
    flex: 1;
  }
  .pct {
    min-width: 40px;
    text-align: center;
  }
  .chip {
    padding: 2px 8px;
    border-radius: 10px;
    background: var(--paper);
    border: 1px solid var(--border-input);
    font-size: 11px;
  }
  .body {
    overflow: auto;
    display: grid;
    place-items: start center;
    padding: 18px;
    position: relative;
    min-height: 0;
  }
  .page-wrap {
    position: relative;
    background: var(--scan-paper);
    box-shadow: var(--shadow-page);
    flex: none;
  }
  .page-wrap img {
    display: block;
    user-select: none;
    image-rendering: auto;
  }
  .overlay {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
    pointer-events: none;
  }
  .overlay.live {
    pointer-events: auto;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--border);
    margin: 0 4px;
  }
  .veil {
    position: absolute;
    inset: 0;
    background: rgba(255, 255, 255, 0.25);
  }
  .placeholder {
    color: var(--text);
    align-self: center;
    padding: 40px;
  }
  .error {
    color: var(--danger);
  }
  .loading {
    position: absolute;
    top: 10px;
    right: 14px;
    padding: 3px 8px;
    border-radius: 10px;
    background: var(--paper);
    border: 1px solid var(--border-input);
    font-size: 11px;
    color: var(--muted);
  }
  .footer {
    position: absolute;
    left: 12px;
    bottom: 12px;
    display: flex;
    gap: 6px;
  }
</style>
