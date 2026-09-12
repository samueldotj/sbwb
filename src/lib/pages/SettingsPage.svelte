<script lang="ts">
  // Settings page (design 4.7, UX-06): "this book" vs "app" groups; a
  // processing change states its consequence before applying.
  import { onMount } from "svelte";
  import { api, errorMessage, formatBytes, type ExportSettings, type ProcessingSettings, type PackReport, type StorageInfo } from "$lib/api";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { isTauri } from "$lib/ipc";

  type Props = { onclose: () => void };
  let { onclose }: Props = $props();

  type Section = "processing" | "review" | "export" | "ai" | "appearance" | "shortcuts" | "storage";
  let section = $state<Section>("processing");
  let settings = $state<ProcessingSettings | null>(null);
  let saved = $state<ProcessingSettings | null>(null);
  let preview = $state<{ rerun_pages: number; approved_untouched: number; effective_workers: number } | null>(null);
  let models = $state<PackReport[]>([]);
  let storage = $state<StorageInfo | null>(null);
  let version = $state("0.1.0");

  const dirty = $derived(JSON.stringify(settings) !== JSON.stringify(saved));

  onMount(async () => {
    if (!isTauri) {
      settings = defaults();
      saved = defaults();
      return;
    }
    try {
      settings = await api.settingsGet();
      saved = structuredClone($state.snapshot(settings));
      models = await api.modelsReport();
      storage = await api.storageInfo();
      version = (await api.appInfo()).version;
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    }
  });

  $effect(() => {
    if (!settings || !isTauri || !project.isOpen || !dirty) {
      preview = null;
      return;
    }
    const snap = $state.snapshot(settings);
    api.settingsPreview(snap).then((p) => (preview = p)).catch(() => (preview = null));
  });

  function defaults(): ProcessingSettings {
    return { model: "eng_best", dpi: 300, deskew: true, despeckle: false, auto_apply_threshold: 90, run_stages_automatically: true, workers: 4 };
  }

  async function apply() {
    if (!settings) return;
    try {
      const n = await api.settingsSet($state.snapshot(settings), (preview?.rerun_pages ?? 0) > 0);
      saved = structuredClone($state.snapshot(settings));
      ui.toast(n > 0 ? `Settings applied · re-running ${n} pages` : "Settings applied", "ok");
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    }
  }
  function reset() {
    settings = defaults();
  }
  async function clearCache() {
    try {
      const freed = await api.clearRenderCache();
      ui.toast(`Cleared ${formatBytes(freed)} of renders`, "ok");
      storage = await api.storageInfo();
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    }
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }

  let exportDefaults = $state<ExportSettings | null>(null);
  $effect(() => {
    if (!isTauri || !project.isOpen) {
      exportDefaults = null;
      return;
    }
    api.exportDefaultsGet().then((d) => (exportDefaults = d)).catch(() => (exportDefaults = null));
  });
  function setExport(patch: Partial<ExportSettings>) {
    if (!exportDefaults) return;
    exportDefaults = { ...exportDefaults, ...patch };
    void api.exportDefaultsSet(exportDefaults).catch((e) => ui.toast(errorMessage(e), "error"));
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="settings">
  <nav class="nav" aria-label="Settings sections">
    <div class="label">this book</div>
    <button type="button" class:on={section === "processing"} onclick={() => (section = "processing")} disabled={!project.isOpen}>Processing</button>
    <button type="button" class:on={section === "review"} onclick={() => (section = "review")} disabled={!project.isOpen}>Review</button>
    <button type="button" class:on={section === "export"} onclick={() => (section = "export")} disabled={!project.isOpen}>Export defaults</button>
    <div class="label app">app</div>
    <button type="button" class:on={section === "ai"} onclick={() => (section = "ai")}>AI providers</button>
    <button type="button" class:on={section === "appearance"} onclick={() => (section = "appearance")}>Appearance</button>
    <button type="button" class:on={section === "shortcuts"} onclick={() => (section = "shortcuts")}>Shortcuts</button>
    <button type="button" class:on={section === "storage"} onclick={() => (section = "storage")}>Storage &amp; privacy</button>
    <div class="foot mono">core {version} · tesseract {storage?.tesseract_version ?? "?"}</div>
    <button type="button" class="back" onclick={onclose}>← Back <kbd>Esc</kbd></button>
  </nav>

  <div class="content">
    {#if section === "processing" && settings}
      <div class="title">Processing</div>
      <div class="sub">Applies to {project.title || "this book"}. Changes re-run affected stages for unapproved pages only.</div>
      <div class="rows">
        <div class="row">
          <div><div class="name">OCR model</div><div class="hint">tessdata_best is slower and more accurate; the fast model suits quick trials.</div></div>
          <select bind:value={settings.model} aria-label="OCR model">
            {#each models.length ? models : [{ entry: { pack: "eng_best", label: "English (tessdata_best)" }, status: "verified" }, { entry: { pack: "eng_fast", label: "English (fast)" }, status: "verified" }] as m (m.entry.pack)}
              <option value={m.entry.pack} disabled={m.status !== "verified"}>{m.entry.label}{m.status !== "verified" ? " (missing)" : ""}</option>
            {/each}
          </select>
        </div>
        <div class="row">
          <div><div class="name">Render resolution</div><div class="hint">Higher is slower. 300 dpi suits most book scans.</div></div>
          <div class="seg" role="radiogroup" aria-label="Render resolution">
            {#each [200, 300, 400] as d (d)}
              <button type="button" role="radio" aria-checked={settings.dpi === d} class:on={settings.dpi === d} onclick={() => (settings!.dpi = d)}>{d}</button>
            {/each}
          </div>
        </div>
        <div class="row">
          <div><div class="name">Deskew</div><div class="hint">Before OCR. The original PDF is never modified.</div></div>
          <label class="toggle"><input type="checkbox" bind:checked={settings.deskew} /><span></span></label>
        </div>
        <div class="row">
          <div><div class="name">Despeckle</div><div class="hint">3×3 median filter for noisy scans. Can erase faint marks; off by default.</div></div>
          <label class="toggle"><input type="checkbox" bind:checked={settings.despeckle} /><span></span></label>
        </div>
        <div class="row">
          <div><div class="name">Text pass auto-apply threshold</div><div class="hint">Corrections scoring at or above this are applied silently.</div></div>
          <div class="slider"><input type="range" min="50" max="100" bind:value={settings.auto_apply_threshold} aria-label="Auto-apply threshold" /><span class="mono">{settings.auto_apply_threshold}%</span></div>
        </div>
        <div class="row">
          <div><div class="name">Run stages automatically</div><div class="hint">OCR → layout → text pass after import, without confirmation.</div></div>
          <label class="toggle"><input type="checkbox" bind:checked={settings.run_stages_automatically} /><span></span></label>
        </div>
        <div class="row">
          <div><div class="name">Parallel workers</div><div class="hint">This machine has {navigator.hardwareConcurrency ?? "?"} cores{preview ? ` · ${preview.effective_workers} will be used` : ""}.</div></div>
          <div class="seg" role="radiogroup" aria-label="Parallel workers">
            {#each [1, 2, 4, 6] as w (w)}
              <button type="button" role="radio" aria-checked={settings.workers === w} class:on={settings.workers === w} onclick={() => (settings!.workers = w)}>{w}</button>
            {/each}
          </div>
        </div>
      </div>
      <div class="actions">
        <button type="button" class="primary" disabled={!dirty} onclick={apply}>
          {preview && preview.rerun_pages > 0 ? `Apply · re-run ${preview.rerun_pages} pages` : "Apply"}
        </button>
        <button type="button" class="secondary" onclick={reset}>Reset to defaults</button>
        {#if preview && preview.approved_untouched > 0}<span class="hint">{preview.approved_untouched} approved pages are left untouched.</span>{/if}
      </div>
    {:else if section === "review"}
      <div class="title">Review</div>
      <div class="rows">
        <div class="row">
          <div><div class="name">Review threshold</div><div class="hint">Scored issues strictly below this are shown and visited. Equal or higher scores stay unmarked, not approved.</div></div>
          <div class="slider"><input type="range" min="0" max="100" bind:value={review.threshold} onchange={() => { review.savePrefs(); void review.refreshCounts(); }} aria-label="Review threshold" /><span class="mono">{review.threshold}%</span></div>
        </div>
        <div class="row">
          <div><div class="name">Auto-advance</div><div class="hint">Move to the next issue only after a decision is saved.</div></div>
          <label class="toggle"><input type="checkbox" bind:checked={review.autoAdvance} onchange={() => review.savePrefs()} /><span></span></label>
        </div>
        <div class="row">
          <div><div class="name">Visit by priority</div><div class="hint">J / K follow impact and evidence instead of reading order.</div></div>
          <label class="toggle"><input type="checkbox" bind:checked={review.byPriority} onchange={() => review.savePrefs()} /><span></span></label>
        </div>
      </div>
    {:else if section === "export"}
      <div class="title">Export defaults</div>
      <div class="sub">Defaults for the Export sheet. Each export records the choices it used in its snapshot.</div>
      {#if exportDefaults}
        <div class="rows">
          <div class="row">
            <div><div class="name">Page structure</div><div class="hint">Mirror the scan inserts a break after each source page.</div></div>
            <div class="seg" role="radiogroup" aria-label="Page structure">
              <button type="button" role="radio" aria-checked={exportDefaults.structure === "mirror"} class:on={exportDefaults.structure === "mirror"} onclick={() => setExport({ structure: "mirror" })}>Mirror</button>
              <button type="button" role="radio" aria-checked={exportDefaults.structure === "continuous"} class:on={exportDefaults.structure === "continuous"} onclick={() => setExport({ structure: "continuous" })}>Continuous</button>
            </div>
          </div>
          <div class="row">
            <div><div class="name">Page furniture</div><div class="hint">Styled paragraphs (default) or native Word headers and footers with a section per page.</div></div>
            <div class="seg" role="radiogroup" aria-label="Page furniture">
              <button type="button" role="radio" aria-checked={exportDefaults.furniture === "styled_paragraphs"} class:on={exportDefaults.furniture === "styled_paragraphs"} onclick={() => setExport({ furniture: "styled_paragraphs" })}>Styled</button>
              <button type="button" role="radio" aria-checked={exportDefaults.furniture === "native_headers_footers"} class:on={exportDefaults.furniture === "native_headers_footers"} onclick={() => setExport({ furniture: "native_headers_footers" })}>Native</button>
            </div>
          </div>
          <div class="row">
            <div><div class="name">Body font and size</div><div class="hint">Unavailable fonts produce a substitution warning at export time.</div></div>
            <div class="inline"><input type="text" class="text" value={exportDefaults.preset.body_font} onchange={(e) => setExport({ preset: { ...exportDefaults!.preset, body_font: (e.currentTarget as HTMLInputElement).value } })} aria-label="Body font" /><input type="number" class="num" min="8" max="18" step="0.5" value={exportDefaults.preset.body_size_pt} onchange={(e) => setExport({ preset: { ...exportDefaults!.preset, body_size_pt: Number((e.currentTarget as HTMLInputElement).value) } })} aria-label="Body size" /></div>
          </div>
          <div class="row">
            <div><div class="name">Paper</div><div class="hint">A4 or Letter; custom sizes are set in the Export sheet.</div></div>
            <div class="seg" role="radiogroup" aria-label="Paper">
              <button type="button" role="radio" aria-checked={exportDefaults.preset.paper === "a4"} class:on={exportDefaults.preset.paper === "a4"} onclick={() => setExport({ preset: { ...exportDefaults!.preset, paper: "a4" } })}>A4</button>
              <button type="button" role="radio" aria-checked={exportDefaults.preset.paper === "letter"} class:on={exportDefaults.preset.paper === "letter"} onclick={() => setExport({ preset: { ...exportDefaults!.preset, paper: "letter" } })}>Letter</button>
            </div>
          </div>
          <div class="row">
            <div><div class="name">Archive bundle</div><div class="hint">PAGE XML, JSON transcript with source map, snapshot, and validation report next to the document.</div></div>
            <label class="toggle"><input type="checkbox" checked={exportDefaults.archive} onchange={(e) => setExport({ archive: (e.currentTarget as HTMLInputElement).checked })} /><span></span></label>
          </div>
        </div>
      {:else}
        <div class="sub">Open a book to edit its export defaults.</div>
      {/if}
    {:else if section === "ai"}
      <div class="title">AI providers</div>
      <div class="sub">AI proofreading arrives in a later version. Nothing leaves this computer today.</div>
    {:else if section === "appearance"}
      <div class="title">Appearance</div>
      <div class="rows">
        <div class="row">
          <div><div class="name">Theme</div><div class="hint">Paper is the light theme; Bench is dark and denser.</div></div>
          <div class="seg" role="radiogroup" aria-label="Theme">
            <button type="button" role="radio" aria-checked={ui.theme === "paper"} class:on={ui.theme === "paper"} onclick={() => (ui.theme = "paper")}>Paper</button>
            <button type="button" role="radio" aria-checked={ui.theme === "bench"} class:on={ui.theme === "bench"} onclick={() => (ui.theme = "bench")}>Bench</button>
          </div>
        </div>
        <div class="row">
          <div><div class="name">Text size</div><div class="hint">Large enlarges the interface and the transcript.</div></div>
          <div class="seg" role="radiogroup" aria-label="Text size">
            <button type="button" role="radio" aria-checked={ui.textSize === "normal"} class:on={ui.textSize === "normal"} onclick={() => (ui.textSize = "normal")}>Normal</button>
            <button type="button" role="radio" aria-checked={ui.textSize === "large"} class:on={ui.textSize === "large"} onclick={() => (ui.textSize = "large")}>Large</button>
          </div>
        </div>
      </div>
    {:else if section === "shortcuts"}
      <div class="title">Shortcuts</div>
      <div class="rows keys">
        {#each [["Ctrl+I", "Import PDF"], ["Ctrl+O", "Open project"], ["Ctrl+Shift+S", "Save a copy"], ["Ctrl+W", "Close book"], ["PgUp / PgDn", "Previous / next page"], ["Ctrl+Home / Ctrl+End", "First / last page"], ["Ctrl+= / Ctrl+-", "Zoom in / out"], ["Ctrl+0", "Fit page"], ["Esc", "Back to the pages view"], ["Ctrl+,", "Settings"], ["J / K", "Next / previous issue"], ["A / E / S / L", "Accept / Edit / Skip / Later"], ["1–9", "Choose a candidate"], ["Ctrl+Enter", "Approve page"], ["Ctrl+L", "Layout mode"], ["V / R / X / M", "Layout tools"]] as [k, d] (k)}
          <div class="row"><span>{d}</span><kbd>{k}</kbd></div>
        {/each}
      </div>
    {:else if section === "storage"}
      <div class="title">Storage &amp; privacy</div>
      <div class="sub">Core processing runs offline. There is no document telemetry. Logs redact your home folder.</div>
      <div class="rows">
        <div class="row"><div><div class="name">Render cache</div><div class="hint mono">{storage?.cache_dir ?? "no book open"}</div></div><div class="inline"><span class="mono">{formatBytes(storage?.cache_bytes ?? 0)}</span><button type="button" class="secondary" onclick={clearCache} disabled={!project.isOpen}>Clear renders</button></div></div>
        <div class="row"><div><div class="name">Log folder</div><div class="hint mono">{storage?.log_dir ?? ""}</div></div><span></span></div>
        <div class="row"><div><div class="name">Model packs</div><div class="hint mono">{storage?.tessdata_dir ?? ""}</div></div><div class="packs">{#each models as m (m.entry.pack)}<div class="mono">{m.entry.label}: {m.status === "verified" ? "verified" : typeof m.status === "string" ? m.status : "corrupt"}</div>{/each}</div></div>
      </div>
    {/if}
  </div>
</div>

<style>
  .settings {
    display: grid;
    grid-template-columns: 220px 1fr;
    min-height: 0;
    height: 100%;
  }
  .nav {
    background: var(--panel);
    border-right: 1px solid var(--border);
    padding: 16px 10px;
    display: grid;
    gap: 2px;
    align-content: start;
  }
  .nav .label {
    padding: 0 8px 8px;
  }
  .nav .label.app {
    padding-top: 16px;
  }
  .nav button {
    all: unset;
    padding: 7px 8px;
    border-radius: 4px;
    color: var(--text-2);
    cursor: default;
  }
  .nav button.on {
    background: var(--raised);
    font-weight: 600;
    color: var(--text);
  }
  .nav button:disabled {
    color: var(--disabled);
  }
  .nav button:focus-visible {
    outline: 2px solid var(--accent);
  }
  .foot {
    margin-top: 24px;
    padding: 0 8px;
    color: var(--muted);
    font-size: 10.5px;
  }
  .back {
    margin-top: 8px;
  }
  .mono {
    font-family: var(--font-mono);
  }
  .content {
    padding: 32px 48px;
    overflow: auto;
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 760px;
  }
  .title {
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .sub {
    color: var(--muted);
    margin-top: -22px;
  }
  .rows {
    display: grid;
    gap: 1px;
    background: var(--border);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 220px;
    gap: 20px;
    align-items: center;
    padding: 14px 16px;
    background: var(--panel);
  }
  .keys .row {
    grid-template-columns: 1fr auto;
  }
  .name {
    font-weight: 500;
  }
  .hint {
    color: var(--muted);
    font-size: 11.5px;
    word-break: break-all;
  }
  select {
    padding: 6px 8px;
    border: 1px solid var(--border-input);
    border-radius: 4px;
    background: var(--raised);
  }
  .seg {
    display: flex;
    gap: 4px;
  }
  .seg button {
    all: unset;
    flex: 1;
    text-align: center;
    padding: 6px;
    border-radius: 3px;
    border: 1px solid var(--border-input);
    cursor: default;
  }
  .seg button.on {
    background: var(--raised);
    border-color: var(--accent);
    font-weight: 600;
  }
  .seg button:focus-visible,
  .toggle input:focus-visible + span,
  select:focus-visible,
  input[type="range"]:focus-visible {
    outline: 2px solid var(--accent);
  }
  .toggle {
    justify-self: end;
    position: relative;
    width: 30px;
    height: 16px;
  }
  .toggle input {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
  }
  .toggle span {
    position: absolute;
    inset: 0;
    border-radius: 8px;
    background: var(--track);
  }
  .toggle span::after {
    content: "";
    position: absolute;
    left: 2px;
    top: 2px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--paper);
    transition: left 120ms;
  }
  .toggle input:checked + span {
    background: var(--accent);
  }
  .toggle input:checked + span::after {
    left: 16px;
    background: var(--primary-fg);
  }
  .text {
    width: 140px;
    padding: 4px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
  }
  .num {
    width: 60px;
    padding: 4px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
  }
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .slider input {
    flex: 1;
    accent-color: var(--accent);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .primary,
  .secondary {
    all: unset;
    padding: 8px 14px;
    border-radius: 4px;
    cursor: default;
    font-weight: 600;
  }
  .primary {
    background: var(--primary-bg);
    color: var(--primary-fg);
  }
  .primary:disabled {
    opacity: 0.5;
  }
  .secondary {
    border: 1px solid var(--border-input);
    font-weight: 400;
  }
  .primary:focus-visible,
  .secondary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .inline {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }
  .packs {
    font-size: 11px;
    display: grid;
    gap: 2px;
  }
</style>
