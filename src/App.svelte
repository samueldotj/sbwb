<script lang="ts">
  import { onMount } from "svelte";
  import TitleBar from "$lib/shell/TitleBar.svelte";
  import StatusBar from "$lib/shell/StatusBar.svelte";
  import Toast from "$lib/shell/Toast.svelte";
  import MenuButton, { type MenuItem } from "$lib/shell/MenuButton.svelte";
  import WelcomePage from "$lib/pages/WelcomePage.svelte";
  import WorkspacePage from "$lib/pages/WorkspacePage.svelte";
  import SettingsPage from "$lib/pages/SettingsPage.svelte";
  import { pipeline, formatEta } from "$lib/stores/pipeline.svelte";
  import { textPass } from "$lib/stores/text.svelte";
  import { review } from "$lib/stores/review.svelte";
  import ChoiceDialog from "$lib/shell/ChoiceDialog.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { project } from "$lib/stores/project.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { isTauri } from "$lib/ipc";
  import { pickPdf, pickProject, pickSaveCopy } from "$lib/dialogs";
  import { api } from "$lib/api";
  import { view } from "$lib/stores/view.svelte";
  import { nextZoom, pickScale } from "$lib/render";

  let version = $state("0.1.0");
  let dragging = $state(false);

  const menu = $derived<MenuItem[]>([
    { id: "import", label: "Import PDF…", accel: "Ctrl+I", disabled: project.isOpen },
    { id: "open", label: "Open project…", accel: "Ctrl+O", disabled: project.isOpen, separatorAfter: true },
    { id: "save_copy", label: "Save a copy…", accel: "Ctrl+Shift+S", disabled: !project.isOpen },
    { id: "export", label: "Export to Word…", accel: "Ctrl+E", disabled: !project.isOpen, separatorAfter: true },
    { id: "close", label: "Close book", accel: "Ctrl+W", disabled: !project.isOpen, separatorAfter: true },
    { id: "settings", label: "Settings…", accel: "Ctrl+," },
  ]);

  async function onmenu(id: string) {
    switch (id) {
      case "import": {
        const p = await pickPdf();
        if (p) await project.importPdf(p);
        break;
      }
      case "open": {
        const p = await pickProject();
        if (p) await project.open(p);
        break;
      }
      case "save_copy": {
        const stem = project.summary?.path.replace(/\.sbwb$/i, "") ?? "book";
        const dest = await pickSaveCopy(`${stem} copy.sbwb`);
        if (dest) await project.saveCopy(dest);
        break;
      }
      case "close":
        await closeBook();
        break;
      case "export":
        ui.exportOpen = true;
        break;
      case "settings":
        ui.settingsOpen = true;
        break;
    }
  }

  // PRJ-04: a draft edit is never discarded silently.
  let closeDialog = $state(false);
  let closeResolve: ((id: string) => void) | null = null;
  async function closeBook() {
    if (review.editing && review.editing.text.trim() && review.editing.text !== review.selected?.original) {
      const choice = await new Promise<string>((resolve) => {
        closeResolve = resolve;
        closeDialog = true;
      });
      closeDialog = false;
      if (choice === "cancel") return;
      if (choice === "save") {
        const out = await review.decide({ kind: "edit", text: review.editing.text });
        if (!out) return; // save failed: stay open with the draft
      } else if (choice === "discard") {
        const spanId = review.editing.span;
        review.editing = null;
        await api.draftDelete(spanId).catch(() => {});
      }
    }
    review.reset();
    textPass.reset();
    await project.close();
  }

  function onkeydown(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement | null)?.closest("input, textarea, [contenteditable]");
    if (project.isOpen && !typing) {
      if (e.key === "PageDown") {
        view.next();
        e.preventDefault();
        return;
      }
      if (e.key === "PageUp") {
        view.prev();
        e.preventDefault();
        return;
      }
      if (e.key === "Home" && e.ctrlKey) {
        view.goTo(0);
        e.preventDefault();
        return;
      }
      if (e.key === "End" && e.ctrlKey) {
        view.goTo(view.pageCount - 1);
        e.preventDefault();
        return;
      }
      if (e.key === "," && (e.ctrlKey || e.metaKey)) {
        ui.settingsOpen = !ui.settingsOpen;
        e.preventDefault();
        return;
      }
      if (e.key === "Escape" && ui.settingsOpen) {
        ui.settingsOpen = false;
        return;
      }
      if (e.key === "Escape" && view.mode === "review") {
        view.mode = "processing";
        return;
      }
      if ((e.ctrlKey || e.metaKey) && (e.key === "=" || e.key === "+" || e.key === "-" || e.key === "0")) {
        if (e.key === "0") view.zoomMode = "fit";
        else {
          view.zoom = nextZoom(view.zoom, e.key === "-" ? -1 : 1);
          view.zoomMode = "custom";
        }
        e.preventDefault();
        return;
      }
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    const k = e.key.toLowerCase();
    if (k === "i" && !project.isOpen) void onmenu("import");
    else if (k === "o" && !project.isOpen) void onmenu("open");
    else if (k === "w" && project.isOpen) void onmenu("close");
    else if (k === "e" && project.isOpen) void onmenu("export");
    else if (k === "s" && e.shiftKey && project.isOpen) void onmenu("save_copy");
    else return;
    e.preventDefault();
  }

  // Warm the neighbours of the current page at the current preview scale.
  $effect(() => {
    if (!isTauri || view.mode !== "review") return;
    const scale = pickScale(view.zoomMode === "custom" ? view.zoom : 1, window.devicePixelRatio || 1);
    for (const i of [view.page + 1, view.page - 1]) {
      if (i >= 0 && i < view.pageCount) void api.prefetchRender(i, scale).catch(() => {});
    }
  });

  onMount(() => {
    void project.init();
    void pipeline.init();
    void textPass.init();
    void review.init();
    if (import.meta.env.DEV) {
      (window as unknown as { __sbwb: unknown }).__sbwb = {
        importPdf: (p: string) => project.importPdf(p),
        open: (p: string) => project.open(p),
        close: () => project.close(),
        review: (i: number) => view.open(i),
        tab: (id: string) => (ui.inspectorTab = id),
        step: (fwd = true) => review.step(fwd),
        decide: (d: { kind: string; text?: string }) => review.decide(d as never),
        approve: (i: number, ack = 0) => review.approve(i, ack),
        review_store: review,
        export_open: () => (ui.exportOpen = true),
        layout: (i: number) => {
          view.open(i);
          view.editLayout();
        },
      };
    }
    if (!isTauri) {
      if (import.meta.env.DEV && new URLSearchParams(location.search).get("mock") === "review") setTimeout(() => view.open(47), 50);
      return;
    }
    void api.appInfo().then((i) => (version = i.version)).catch(() => {});
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(async ({ getCurrentWebview }) => {
      unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const t = event.payload.type;
        if (t === "enter" || t === "over") dragging = true;
        else if (t === "leave") dragging = false;
        else if (t === "drop") {
          dragging = false;
          const pdf = event.payload.paths.find((p) => /\.pdf$/i.test(p));
          const proj = event.payload.paths.find((p) => /\.sbwb$/i.test(p));
          if (project.isOpen) ui.toast("Close the current book before opening another.", "warn");
          else if (pdf) void project.importPdf(pdf);
          else if (proj) void project.open(proj);
          else ui.toast("Drop a PDF or a .sbwb project.", "warn");
        }
      });
    });
    return () => unlisten?.();
  });

  const STAGE_NAME: Record<string, string> = { ocr: "OCR", layout: "Layout", text_pass: "Text pass" };
  const stageLine = $derived.by(() => {
    if (!pipeline.active) return "";
    const c = pipeline.current;
    if (!c || !c.total) return "";
    const finished = c.done + c.failed;
    const pct = Math.round((finished / c.total) * 100);
    const eta = c.stage === "ocr" && c.eta_ms !== null ? ` · ${formatEta(c.eta_ms)}` : "";
    return `${STAGE_NAME[c.stage] ?? c.stage} · page ${Math.min(finished + 1, c.total)} of ${c.total} · ${pct}%${eta}`;
  });
  const counters = $derived.by(() => {
    const parts: string[] = [];
    const failed = pipeline.stages.reduce((n, s) => n + s.failed, 0);
    if (failed) parts.push(`${failed} failed`);
    const t = textPass.summary;
    if (t && (t.applied || t.suggested)) parts.push(`${t.applied} applied · ${t.suggested} suggested`);
    return parts.join(" · ");
  });

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(t);
  });
  const savedAgo = $derived.by(() => {
    if (!ui.savedAt) return "";
    const s = Math.max(0, Math.round((now - ui.savedAt) / 1000));
    return s < 5 ? "just now" : s < 60 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
  });
</script>

<svelte:window onkeydown={onkeydown} />

<div class="app">
  <TitleBar
    context={ui.settingsOpen ? "/ Settings" : project.isOpen ? (project.lockedBy ? "read-only" : "") : "Local · nothing uploaded"}
    bookTitle={project.title}
    bookMeta={project.isOpen ? project.scopeLabel : ""}
  >
    {#snippet menu_()}
      <MenuButton items={menu} onselect={onmenu} />
    {/snippet}
  </TitleBar>
  <main class="main">
    {#if ui.settingsOpen}
      <SettingsPage onclose={() => (ui.settingsOpen = false)} />
    {:else if project.summary}
      <WorkspacePage summary={project.summary} />
    {:else}
      <WelcomePage {version} {dragging} onsettings={() => (ui.settingsOpen = true)} />
    {/if}
  </main>
  <StatusBar
    saveState={project.isOpen ? (ui.saveState === "idle" ? "saved" : ui.saveState) : "idle"}
    savedAgo={savedAgo}
    saveError={ui.saveError}
    projectFile={view.mode === "layout" ? `Layout · page ${view.page + 1}${layout.dirty ? " · unsaved changes" : ""} · Esc returns to Review` : (project.summary?.path.split(/[\\/]/).pop() ?? "")}
    stage={project.busy ?? stageLine}
    running={project.busy !== null || pipeline.state === "running"}
    counters={counters}
  />
  <Toast />
  <div class="sr-only" aria-live="polite" role="status">{ui.announcement}</div>
  <ChoiceDialog
    open={closeDialog}
    title="Unsaved edit"
    message="You have an edit that has not been saved. Save it and close, discard it and close, or keep working?"
    choices={[{ id: "save", label: "Save and close", kind: "primary" }, { id: "discard", label: "Discard unsaved changes and close", kind: "danger" }, { id: "cancel", label: "Cancel" }]}
    onchoose={(id) => closeResolve?.(id)}
  />
</div>

<style>
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  .app {
    height: 100%;
    display: grid;
    grid-template-rows: var(--titlebar-h) 1fr var(--statusbar-h);
    position: relative;
    background: var(--bg);
  }
  .main {
    min-height: 0;
    display: grid;
  }
</style>
