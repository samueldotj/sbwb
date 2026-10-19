<script lang="ts">
  // Export sheet (design 4.5, EXP-01..06): readiness counters, working or
  // clean copy, page structure and furniture policy, inclusion choices,
  // archive and flag threshold, formatting and metadata, previous exports,
  // destination, progress with real phases, and the result.
  import { api, errorMessage, type ExportPreview, type ExportReport, type ExportSettings, type Readiness } from "$lib/api";
  import { pickSaveDocx } from "$lib/dialogs";
  import { isTauri, listen } from "$lib/ipc";
  import Sheet from "$lib/shell/Sheet.svelte";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";

  type Props = { open: boolean; onclose: () => void };
  let { open, onclose }: Props = $props();

  let ready = $state<Readiness | null>(null);
  let settings = $state<ExportSettings | null>(null);
  let preview = $state<ExportPreview | null>(null);
  let dest = $state("");
  let existing = $state<Set<string>>(new Set());
  const willReplace = $derived(existing.has(dest.replace(/\\/g, "/").toLowerCase()));
  let phase = $state<{ phase: string; done: number; total: number } | null>(null);
  let result = $state<ExportReport | null>(null);
  let failure = $state("");
  let busy = $state(false);
  let showFormat = $state(false);
  let unlisten: (() => void) | null = null;

  // ----- page size and margins -----
  const PT_PER_IN = 72;
  const PT_PER_MM = 72 / 25.4;
  type SizeOption = { id: string; label: string; paper: "a4" | "letter" | "custom"; w: number; h: number };
  const inches = (w: number, h: number): [number, number] => [w * PT_PER_IN, h * PT_PER_IN];
  const BOOK_SIZES: [string, string, [number, number]][] = [
    ["a5", "A5 · 148 × 210 mm", [148 * PT_PER_MM, 210 * PT_PER_MM]],
    ["5x8", "5 × 8 in · small trade", inches(5, 8)],
    ["5.25x8", "5.25 × 8 in", inches(5.25, 8)],
    ["5.5x8.5", "5.5 × 8.5 in · digest", inches(5.5, 8.5)],
    ["6x9", "6 × 9 in · US trade", inches(6, 9)],
    ["6.14x9.21", "6.14 × 9.21 in · royal", inches(6.14, 9.21)],
    ["7x10", "7 × 10 in", inches(7, 10)],
  ];
  const fmtIn = (pt: number) => (pt / PT_PER_IN).toFixed(2).replace(/\.?0+$/, "");
  /** The scan's most common page size, rounded to a tenth of an inch. */
  const scanSize = $derived.by((): [number, number] | null => {
    const counts = new Map<string, { n: number; w: number; h: number }>();
    for (const pg of project.summary?.pages ?? []) {
      if (!pg.width_pt || !pg.height_pt) continue;
      const w = Math.round((pg.width_pt / PT_PER_IN) * 10) / 10;
      const h = Math.round((pg.height_pt / PT_PER_IN) * 10) / 10;
      const k = `${w}x${h}`;
      const c = counts.get(k) ?? { n: 0, w, h };
      c.n++;
      counts.set(k, c);
    }
    const best = [...counts.values()].sort((a, b) => b.n - a.n)[0];
    return best ? inches(best.w, best.h) : null;
  });
  const sizes = $derived.by((): SizeOption[] => [
    { id: "a4", label: "A4 · 210 × 297 mm", paper: "a4", w: 595.276, h: 841.89 },
    { id: "letter", label: "Letter · 8.5 × 11 in", paper: "letter", w: 612, h: 792 },
    ...BOOK_SIZES.map(([id, label, [w, h]]) => ({ id, label, paper: "custom" as const, w, h })),
    ...(scanSize ? [{ id: "scan", label: `Same as the scan · ${fmtIn(scanSize[0])} × ${fmtIn(scanSize[1])} in`, paper: "custom" as const, w: scanSize[0], h: scanSize[1] }] : []),
  ]);
  /** The listed size the settings match, or "custom". */
  const sizeId = $derived.by(() => {
    const pr = settings?.preset;
    if (!pr) return "custom";
    if (pr.paper !== "custom") return pr.paper;
    const hit = sizes.find((o) => o.paper === "custom" && Math.abs(o.w - pr.custom_width_pt) < 0.6 && Math.abs(o.h - pr.custom_height_pt) < 0.6);
    return hit?.id ?? "custom";
  });
  let customUnit = $state<"in" | "mm">("in");
  const unitPt = $derived(customUnit === "in" ? PT_PER_IN : PT_PER_MM);
  function chooseSize(id: string) {
    const pr = settings?.preset;
    if (!pr) return;
    const o = sizes.find((x) => x.id === id);
    if (!o) {
      pr.paper = "custom"; // keep the current numbers to edit
      return;
    }
    pr.paper = o.paper;
    if (o.paper === "custom") {
      pr.custom_width_pt = Math.round(o.w * 100) / 100;
      pr.custom_height_pt = Math.round(o.h * 100) / 100;
    }
  }
  function setCustom(which: "w" | "h", value: number) {
    const pr = settings?.preset;
    if (!pr || !(value > 0)) return;
    const pt = Math.round(value * unitPt * 100) / 100;
    pr.paper = "custom";
    if (which === "w") pr.custom_width_pt = pt;
    else pr.custom_height_pt = pt;
  }
  type Margins = { top: number; bottom: number; left: number; right: number };
  const MARGINS: [string, string, Margins][] = [
    ["narrow", "Narrow · 12.7 mm", { top: 12.7, bottom: 12.7, left: 12.7, right: 12.7 }],
    ["book", "Book · 15 / 18 / 18 / 15 mm", { top: 15, bottom: 18, left: 18, right: 15 }],
    ["moderate", "Moderate · 25.4 / 19 mm", { top: 25.4, bottom: 25.4, left: 19.05, right: 19.05 }],
    ["normal", "Normal · 25.4 mm", { top: 25.4, bottom: 25.4, left: 25.4, right: 25.4 }],
    ["wide", "Wide · 25.4 / 50.8 mm", { top: 25.4, bottom: 25.4, left: 50.8, right: 50.8 }],
  ];
  const marginId = $derived.by(() => {
    const pr = settings?.preset;
    if (!pr) return "custom";
    const hit = MARGINS.find(([, , m]) => Math.abs(m.top - pr.margin_top_mm) < 0.05 && Math.abs(m.bottom - pr.margin_bottom_mm) < 0.05 && Math.abs(m.left - pr.margin_left_mm) < 0.05 && Math.abs(m.right - pr.margin_right_mm) < 0.05);
    return hit?.[0] ?? "custom";
  });
  function chooseMargins(id: string) {
    const pr = settings?.preset;
    const m = MARGINS.find(([k]) => k === id)?.[2];
    if (!pr || !m) return;
    pr.margin_top_mm = m.top;
    pr.margin_bottom_mm = m.bottom;
    pr.margin_left_mm = m.left;
    pr.margin_right_mm = m.right;
  }
  /** Page and text area in points, for the summary line and the warning. */
  const pageBox = $derived.by(() => {
    const pr = settings?.preset;
    if (!pr) return null;
    const [w, h] = pr.paper === "a4" ? [595.276, 841.89] : pr.paper === "letter" ? [612, 792] : [Math.max(200, pr.custom_width_pt), Math.max(200, pr.custom_height_pt)];
    const tw = w - (pr.margin_left_mm + pr.margin_right_mm) * PT_PER_MM;
    const th = h - (pr.margin_top_mm + pr.margin_bottom_mm) * PT_PER_MM;
    return { w, h, tw, th };
  });

  const PHASE_LABEL: Record<string, string> = { compose: "Composing paragraphs", package: "Packaging the document", validate: "Validating the package", publish: "Publishing", archive: "Writing the archive bundle", done: "Done" };

  async function load() {
    if (!isTauri) return;
    try {
      const [r, s] = await Promise.all([api.exportReadiness(), api.exportDefaultsGet()]);
      ready = r;
      settings = s;
      if (!settings.metadata.title) settings.metadata.title = r.book_title;
      if (!settings.metadata.author) settings.metadata.author = r.book_author;
      // Default: `<book>.docx` beside the project file; an existing file there is replaced.
      const stem = project.summary?.path.replace(/\.sbwb$/i, "") ?? "";
      dest = stem ? `${stem}.docx` : r.default_name;
      existing = new Set(r.previous.map((e) => e.path.replace(/\\/g, "/").toLowerCase()));
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  $effect(() => {
    if (open) {
      result = null;
      failure = "";
      phase = null;
      void load();
      void listen<{ type: string; phase?: string; done?: number; total?: number; report?: ExportReport; message?: string }>("export:event", (e) => {
        if (e.type === "progress") phase = { phase: e.phase ?? "", done: e.done ?? 0, total: e.total ?? 0 };
        else if (e.type === "finished" && e.report) {
          result = e.report;
          busy = false;
          phase = null;
          ui.toast(`Exported ${e.report.stats.pages} pages to ${e.report.docx_path.split(/[\\/]/).pop()}`, "ok", 6000);
          ui.announce("Export finished");
          void load();
        } else if (e.type === "failed") {
          failure = e.message ?? "export failed";
          busy = false;
          phase = null;
          ui.announce("Export failed");
        }
      }).then((u) => (unlisten = u));
      return () => {
        unlisten?.();
        unlisten = null;
      };
    }
  });

  // Preview exclusions and warnings whenever the choices change.
  $effect(() => {
    if (!open || !settings || !isTauri) return;
    const snapshot = JSON.stringify(settings);
    let cancelled = false;
    api
      .exportPreview(JSON.parse(snapshot))
      .then((p) => {
        if (!cancelled) preview = p;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  const cleanBlocked = $derived(settings?.copy === "clean" && !(ready?.clean_ready ?? false));
  const canExport = $derived(!!settings && !!dest && !busy && !cleanBlocked && (ready?.pages_indexed ?? 0) > 0);

  async function chooseDest() {
    const r = await pickSaveDocx(dest || ready?.default_name);
    if (r) dest = r;
  }
  async function start() {
    if (!settings || !dest) return;
    busy = true;
    failure = "";
    result = null;
    try {
      await api.exportDefaultsSet(settings);
      await api.exportRun(settings, dest);
    } catch (e) {
      busy = false;
      failure = errorMessage(e);
    }
  }
  /** Open an exported document in the system's Word processor. */
  async function openDoc(path: string) {
    try {
      await api.openFile(path);
    } catch (e) {
      ui.toast(`Could not open ${path.split(/[\\/]/).pop()}: ${errorMessage(e)}`, "error", 6000);
    }
  }
  async function cancel() {
    await api.exportCancel().catch(() => {});
  }
  function kb(n: number): string {
    return n < 1024 * 1024 ? `${Math.round(n / 1024)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`;
  }
  const _kb = kb;
</script>

<Sheet {open} title="Export to Word" subtitle="{project.title} · pages {project.scopeLabel}" {onclose}>
  {#if !settings || !ready}
    <p class="muted">Loading…</p>
  {:else}
    <div class="counters" aria-label="Readiness">
      <div class="counter ok"><b>{ready.pages_approved} / {ready.pages_in_scope}</b><span>pages approved</span></div>
      <div class="counter warn"><b>{ready.unresolved_below_threshold}</b><span>unresolved below threshold</span></div>
      <div class="counter"><b>{ready.deferred}</b><span>deferred</span></div>
      <div class="counter"><b>{ready.ai_pending}</b><span>AI suggestions pending</span></div>
    </div>
    {#if ready.pages_indexed < ready.pages_in_scope}
      <p class="note warn">{ready.pages_in_scope - ready.pages_indexed} of {ready.pages_in_scope} pages have not finished processing and will be left out.</p>
    {/if}

    <fieldset class="group">
      <legend>What to export</legend>
      <label class="radio" class:on={settings.copy === "working"}>
        <input type="radio" name="copy" value="working" bind:group={settings.copy} />
        <span><b>Working copy</b><br /><span class="muted small">Current text as-is. Unresolved words highlighted{settings.comments ? ", one comment per flag" : ", no comments"}.</span></span>
        <span class="tag ok">ready now</span>
      </label>
      {#if settings.copy === "working"}
        <label class="sub"><input type="checkbox" bind:checked={settings.comments} /> Add a Word comment to each flagged word</label>
      {/if}
      <label class="radio" class:on={settings.copy === "clean"}>
        <input type="radio" name="copy" value="clean" bind:group={settings.copy} />
        <span><b>Clean copy</b><br /><span class="muted small">No annotations. Requires every page in scope approved.</span></span>
        <span class="tag" class:warn={!ready.clean_ready}>{ready.clean_ready ? "ready" : `${ready.pages_left} page${ready.pages_left === 1 ? "" : "s"} left`}</span>
      </label>
    </fieldset>

    <fieldset class="group">
      <legend>Page structure</legend>
      <div class="seg" role="radiogroup" aria-label="Page structure">
        <button type="button" role="radio" aria-checked={settings.structure === "mirror"} class:on={settings.structure === "mirror"} onclick={() => (settings!.structure = "mirror")}>Mirror the scan</button>
        <button type="button" role="radio" aria-checked={settings.structure === "continuous"} class:on={settings.structure === "continuous"} onclick={() => (settings!.structure = "continuous")}>Continuous text</button>
      </div>
      <div class="seg" role="radiogroup" aria-label="Page furniture">
        <button type="button" role="radio" aria-checked={settings.furniture === "styled_paragraphs"} class:on={settings.furniture === "styled_paragraphs"} onclick={() => (settings!.furniture = "styled_paragraphs")}>Styled paragraphs</button>
        <button type="button" role="radio" aria-checked={settings.furniture === "native_headers_footers"} class:on={settings.furniture === "native_headers_footers"} onclick={() => (settings!.furniture = "native_headers_footers")}>Native headers &amp; footers</button>
      </div>
      <p class="muted small">{preview?.explanation ?? ""}</p>
      <div class="checks">
        <label><input type="checkbox" bind:checked={settings.include.marginalia} /> Marginal notes</label>
        <label><input type="checkbox" bind:checked={settings.include.footnotes} /> Footnotes</label>
        <label><input type="checkbox" bind:checked={settings.include.page_numbers} /> Source page numbers</label>
        <label><input type="checkbox" bind:checked={settings.include.illustrations} /> Illustrations as images</label>
        <label><input type="checkbox" bind:checked={settings.include.catchwords} /> Catchwords</label>
        <label><input type="checkbox" bind:checked={settings.include.uncertain} /> Uncertain regions as text</label>
        <label>Tables
          <select bind:value={settings.include.tables} aria-label="Tables">
            <option value="text">as text (warned)</option>
            <option value="image">as image</option>
            <option value="skip">skip</option>
          </select>
        </label>
      </div>
      {#if preview}
        <p class="muted small">
          {preview.stats.words} words · {preview.stats.paragraphs} paragraphs · {preview.stats.running_heads} running heads · {preview.stats.side_notes} side notes · {preview.stats.footnotes} footnotes · {preview.stats.images} images
          {#if preview.exclusions.length}· <b>{preview.exclusions.length} regions excluded ({preview.excluded_words} words)</b>{/if}
        </p>
      {/if}
    </fieldset>

    <fieldset class="group">
      <legend>Page size and margins</legend>
      <div class="grid two-col">
        <label>Page size
          <select value={sizeId} onchange={(e) => chooseSize(e.currentTarget.value)}>
            {#each sizes as o (o.id)}<option value={o.id}>{o.label}</option>{/each}
            <option value="custom">Custom…</option>
          </select>
        </label>
        <label>Margins
          <select value={marginId} onchange={(e) => chooseMargins(e.currentTarget.value)}>
            {#each MARGINS as [id, label] (id)}<option value={id}>{label}</option>{/each}
            <option value="custom" disabled={marginId !== "custom"}>Custom</option>
          </select>
        </label>
      </div>
      {#if sizeId === "custom"}
        <div class="grid four-col">
          <label>Width ({customUnit})
            <input type="number" min="1" step={customUnit === "in" ? 0.01 : 1} value={Math.round((settings.preset.custom_width_pt / unitPt) * 100) / 100} onchange={(e) => setCustom("w", Number(e.currentTarget.value))} />
          </label>
          <label>Height ({customUnit})
            <input type="number" min="1" step={customUnit === "in" ? 0.01 : 1} value={Math.round((settings.preset.custom_height_pt / unitPt) * 100) / 100} onchange={(e) => setCustom("h", Number(e.currentTarget.value))} />
          </label>
          <label>Units
            <select bind:value={customUnit}>
              <option value="in">inches</option>
              <option value="mm">mm</option>
            </select>
          </label>
        </div>
      {/if}
      <div class="grid four-col">
        <label>Top (mm) <input type="number" min="5" max="60" step="0.5" bind:value={settings.preset.margin_top_mm} /></label>
        <label>Bottom (mm) <input type="number" min="5" max="60" step="0.5" bind:value={settings.preset.margin_bottom_mm} /></label>
        <label>Left (mm) <input type="number" min="5" max="60" step="0.5" bind:value={settings.preset.margin_left_mm} /></label>
        <label>Right (mm) <input type="number" min="5" max="60" step="0.5" bind:value={settings.preset.margin_right_mm} /></label>
      </div>
      {#if pageBox}
        <p class="note muted">Page {fmtIn(pageBox.w)} × {fmtIn(pageBox.h)} in · text area {fmtIn(pageBox.tw)} × {fmtIn(pageBox.th)} in</p>
        {#if pageBox.tw < 2.5 * PT_PER_IN || pageBox.th < 3.5 * PT_PER_IN}
          <p class="note warn">The text area is small for this page size; side notes and long lines will crowd. Try narrower margins.</p>
        {/if}
      {/if}
    </fieldset>

    <details class="details" bind:open={showFormat}>
      <summary>Formatting, metadata &amp; archive</summary>
      <div class="grid">
        <label>Preset <input type="text" bind:value={settings.preset.name} /></label>
        <label>Body font <input type="text" bind:value={settings.preset.body_font} /></label>
        <label>Body size (pt) <input type="number" min="8" max="18" step="0.5" bind:value={settings.preset.body_size_pt} /></label>
        <label>Paragraph spacing (pt) <input type="number" min="0" max="24" bind:value={settings.preset.paragraph_spacing_pt} /></label>
        <label>Line spacing <input type="number" min="1" max="2" step="0.05" bind:value={settings.preset.line_spacing} /></label>
        <label>Heading font <input type="text" bind:value={settings.preset.heading_font} /></label>
        <label>Note size (pt) <input type="number" min="6" max="14" step="0.5" bind:value={settings.preset.note_size_pt} /></label>
        <div class="dropcap">
          <label><input type="checkbox" bind:checked={settings.preset.drop_cap.enabled} /> Drop cap on the first paragraph of each page</label>
          <label>spanning <input type="number" min="2" max="5" bind:value={settings.preset.drop_cap.lines} disabled={!settings.preset.drop_cap.enabled} /> lines</label>
        </div>
      </div>
      <div class="grid">
        <label>Document title <input type="text" bind:value={settings.metadata.title} /></label>
        <label>Author <input type="text" bind:value={settings.metadata.author} /></label>
        <label>Subject <input type="text" bind:value={settings.metadata.subject} /></label>
      </div>
      <p class="muted small">PDF metadata (read-only): title “{ready.pdf_title ?? "—"}”, author “{ready.pdf_author ?? "—"}”. Editing the fields above changes only the Word document properties.</p>
      <div class="grid">
        <label><input type="checkbox" bind:checked={settings.archive} /> Archive bundle (PAGE XML, JSON transcript with source map, snapshot, validation report)</label>
        <label>Flag unresolved below <input type="number" min="0" max="100" bind:value={settings.flag_threshold} /> % in the working copy</label>
      </div>
    </details>

    {#if ready.previous.length}
      <p class="muted small">Previous exports: {ready.previous.slice(0, 3).map((e) => `${e.kind} · ${new Date(e.ts).toLocaleString()} · ${e.path.split(/[\\/]/).pop()}`).join(" · ")} <button type="button" class="link" onclick={() => openDoc(ready!.previous[0]!.path)} title={ready.previous[0]!.path}>open latest</button></p>
    {/if}

    {#if preview && (preview.warnings.length || preview.exclusions.length)}
      <fieldset class="group">
        <legend>Warnings · {preview.warnings.length}{preview.exclusions.length ? ` · ${preview.exclusions.length} excluded` : ""}</legend>
        {#if preview.exclusions.length}
          <details class="details">
            <summary>Excluded before export</summary>
            <ul class="list">
              {#each preview.exclusions.slice(0, 60) as e, i (i)}
                <li><span class="mono">p. {e.page + 1}</span><span>{e.kind} · {e.words} words</span><span class="muted">{e.why}</span></li>
              {/each}
              {#if preview.exclusions.length > 60}<li class="muted">… and {preview.exclusions.length - 60} more (all listed in the report)</li>{/if}
            </ul>
          </details>
        {/if}
        {#if preview.warnings.length}
          <div class="warn-list">
            {#each preview.warnings as w, i (i)}
              <p class="note warn">{w}</p>
            {/each}
          </div>
        {/if}
      </fieldset>
    {/if}

    {#if phase}
      <div class="progress" role="status" aria-live="polite">
        <span class="spinner" aria-hidden="true"></span>
        <span>{PHASE_LABEL[phase.phase] ?? phase.phase}{phase.total > 1 ? ` · ${phase.done} / ${phase.total}` : ""}</span>
      </div>
    {/if}
    {#if failure}
      <p class="note err" role="alert">Export failed: {failure}. Nothing was published; earlier exports are untouched.</p>
    {/if}
    {#if result}
      <div class="result">
        <div><b>Exported {result.copy} copy{result.replaced ? " (replaced the earlier file)" : ""}</b> · {result.stats.pages} pages · {result.stats.words} words · {result.stats.paragraphs} paragraphs · {(result.elapsed_ms / 1000).toFixed(1)} s</div>
        <div class="muted small">Validation: {result.validation.checks.filter((c) => c.ok).length} / {result.validation.checks.length} checks passed · checksum {result.checksum_blake3.slice(0, 12)}… · {result.comments} comments · {result.exclusions.length} regions excluded{result.archive_path ? " · archive written" : ""}</div>
        {#each result.warnings as w, i (i)}<div class="note warn small">{w}</div>{/each}
        <div class="two">
          <button type="button" class="ctl primary" onclick={() => openDoc(result!.docx_path)}>Open document</button>
          <button type="button" class="ctl" onclick={() => api.openPath(result!.docx_path)}>Show in folder</button>
        </div>
      </div>
    {/if}
  {/if}
  {#snippet footer()}
    <span class="destwrap">
      <button type="button" class="dest" onclick={chooseDest} title={dest} disabled={busy}>{dest ? dest.split(/[\\/]/).pop() : "Choose destination…"}</button>
      <span class="muted small">{willReplace ? "replaces the existing file" : "in the project folder"}</span>
    </span>
    <span class="grow"></span>
    {#if busy}
      <button type="button" class="ctl" onclick={cancel}>Cancel export</button>
    {:else}
      <button type="button" class="ctl" onclick={onclose}>Close</button>
      {#if result}
        <button type="button" class="ctl" onclick={() => openDoc(result!.docx_path)} title={result.docx_path}>Open document</button>
      {/if}
      <button type="button" class="ctl primary" onclick={start} disabled={!canExport}>Export {settings?.copy === "clean" ? "clean" : "working"} copy</button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .counters {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    margin-bottom: 14px;
  }
  .counter {
    display: grid;
    justify-items: center;
    padding: 8px 4px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
    text-align: center;
  }
  .counter b {
    font-size: 18px;
  }
  .counter span {
    font-size: 11px;
    color: var(--muted);
  }
  .counter.ok b {
    color: var(--ok-text, var(--ok));
  }
  .counter.warn b {
    color: var(--accent-text);
  }
  .group {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 12px 12px;
    margin: 0 0 12px;
    display: grid;
    gap: 8px;
  }
  legend {
    font: 500 10px var(--font-ui);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 0 4px;
  }
  .radio {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 10px;
    align-items: center;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 13px;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: -2px 0 2px 34px;
    font-size: 12px;
  }
  .radio.on {
    border-color: var(--accent);
    background: var(--accent-bg);
  }
  .tag {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 10px;
    background: var(--paper);
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .tag.ok {
    background: var(--ok-bg);
    border-color: var(--ok);
  }
  .tag.warn {
    background: var(--accent-bg);
    border-color: var(--warn);
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border-input);
    border-radius: 6px;
    overflow: hidden;
    width: fit-content;
  }
  .seg button {
    all: unset;
    padding: 5px 12px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--primary-bg);
    color: var(--primary-fg);
  }
  .seg button:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .checks {
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: 6px 16px;
    font-size: 12.5px;
  }
  .checks label {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .note {
    margin: 0;
    font-size: 12.5px;
  }
  .note.warn {
    color: var(--accent-text);
  }
  .link {
    all: unset;
    color: var(--accent-text);
    text-decoration: underline;
    cursor: pointer;
  }
  .link:focus-visible {
    outline: 2px solid var(--accent);
  }
  .warn-list {
    display: grid;
    gap: 4px;
    max-height: 180px;
    overflow: auto;
  }
  .note.err {
    color: var(--danger);
  }
  .details {
    margin: 0 0 12px;
    font-size: 12.5px;
  }
  .details summary {
    cursor: pointer;
    color: var(--text-2);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px 14px;
    margin: 8px 0;
  }
  .grid.two-col {
    grid-template-columns: repeat(2, 1fr);
    margin: 0;
  }
  .grid.four-col {
    grid-template-columns: repeat(4, 1fr);
    margin: 0;
  }
  .grid label {
    display: grid;
    gap: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  .grid input[type="text"],
  .grid input[type="number"],
  .checks select,
  .grid select {
    padding: 4px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
    font-size: 12.5px;
  }
  .dropcap {
    grid-column: 1 / -1;
    display: flex;
    gap: 14px;
    align-items: center;
  }
  .dropcap label {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .dropcap input[type="number"] {
    width: 50px;
  }
  .list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: grid;
    gap: 3px;
    max-height: 160px;
    overflow: auto;
  }
  .list li {
    display: grid;
    grid-template-columns: 50px 1fr 1fr;
    gap: 8px;
  }
  .progress {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--raised);
    font-size: 13px;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .result {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--ok-bg);
    font-size: 13px;
  }
  .two {
    display: flex;
    gap: 8px;
  }
  .grow {
    flex: 1;
  }
  .destwrap {
    display: grid;
    gap: 2px;
  }
  .dest {
    all: unset;
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-2);
    padding: 5px 8px;
    border: 1px dashed var(--border-input);
    border-radius: 6px;
    cursor: pointer;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dest:focus-visible {
    outline: 2px solid var(--accent);
  }
  .ctl {
    padding: 6px 12px;
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
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
