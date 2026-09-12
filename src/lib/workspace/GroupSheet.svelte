<script lang="ts">
  // Grouped correction (REV-05): after an accepted correction, preview every
  // whole-token match in scope with context, conflicts, and exclusions;
  // apply the selected ones atomically. A stale match blocks the whole
  // apply until the selection is refreshed.
  import { api, errorMessage, type GroupMatch } from "$lib/api";
  import Sheet from "$lib/shell/Sheet.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { ui } from "$lib/stores/ui.svelte";

  type Props = { open: boolean; original: string; replacement: string; onclose: () => void };
  let { open, original, replacement, onclose }: Props = $props();

  let matches = $state<GroupMatch[]>([]);
  let excluded = $state<Set<string>>(new Set());
  let loading = $state(false);
  let stale = $state<Set<string>>(new Set());
  let busy = $state(false);

  async function load() {
    loading = true;
    stale = new Set();
    try {
      matches = await api.groupPreview(original, replacement);
      excluded = new Set(matches.filter((m) => m.conflict !== null).map((m) => m.span));
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
      matches = [];
    } finally {
      loading = false;
    }
  }
  $effect(() => {
    if (open && original) void load();
  });

  const eligible = $derived(matches.filter((m) => m.conflict === null));
  const selected = $derived(matches.filter((m) => !excluded.has(m.span) && m.conflict === null));
  const pages = $derived(new Set(selected.map((m) => m.page)).size);

  function toggle(m: GroupMatch) {
    const next = new Set(excluded);
    if (next.has(m.span)) next.delete(m.span);
    else next.add(m.span);
    excluded = next;
  }
  async function apply() {
    if (selected.length === 0) return;
    busy = true;
    try {
      const out = await ui.save(() => api.groupApply(original, replacement, selected.map((m) => ({ span: m.span, revision: m.revision }))));
      if (out.stale.length) {
        stale = new Set(out.stale);
        ui.toast(`${out.stale.length} match${out.stale.length === 1 ? "" : "es"} changed since the preview. Nothing was applied; refresh the selection.`, "warn", 7000);
        return;
      }
      ui.toast(`Replaced “${original}” with “${replacement}” in ${out.applied} place${out.applied === 1 ? "" : "s"}`, "ok", 5000);
      ui.announce(`Replaced in ${out.applied} places`);
      await review.reload();
      onclose();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 6000);
    } finally {
      busy = false;
    }
  }
</script>

<Sheet {open} title="Also replace elsewhere?" subtitle="“{original}” → “{replacement}” · whole words only, case and punctuation must match" width="640px" {onclose}>
  {#if loading}
    <p class="muted">Searching the book…</p>
  {:else if matches.length === 0}
    <p class="muted">No other occurrences of “{original}” in the selected pages.</p>
  {:else}
    <p class="lead">{matches.length} match{matches.length === 1 ? "" : "es"} · {eligible.length} eligible · {matches.length - eligible.length} excluded ({[...new Set(matches.filter((m) => m.conflict).map((m) => m.conflict))].join(", ") || "none"})</p>
    <ul class="list" aria-label="Matches">
      {#each matches as m (m.span)}
        <li class:conflict={m.conflict !== null} class:stale={stale.has(m.span)}>
          <label>
            <input type="checkbox" checked={!excluded.has(m.span) && m.conflict === null} disabled={m.conflict !== null} onchange={() => toggle(m)} aria-label="Replace on page {m.page + 1}" />
            <span class="page">p. {m.page + 1}</span>
            <span class="ctx serif">{m.context}</span>
            <span class="muted small">{stale.has(m.span) ? "changed since preview" : (m.conflict ?? `→ ${m.replacement}`)}</span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet footer()}
    <span class="muted small">{selected.length} of {eligible.length} selected on {pages} page{pages === 1 ? "" : "s"} · one undoable history entry · no page is approved by this</span>
    <span class="grow"></span>
    {#if stale.size}<button type="button" class="ctl" onclick={load}>Refresh selection</button>{/if}
    <button type="button" class="ctl" onclick={onclose}>Skip</button>
    <button type="button" class="ctl primary" onclick={apply} disabled={busy || selected.length === 0 || stale.size > 0}>Replace {selected.length}</button>
  {/snippet}
</Sheet>

<style>
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .lead {
    margin: 0 0 10px;
    font-size: 13px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    max-height: 50vh;
    overflow: auto;
  }
  .list li label {
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    gap: 10px;
    align-items: baseline;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--raised);
    font-size: 13px;
  }
  .list li.conflict label {
    opacity: 0.7;
  }
  .list li.stale label {
    outline: 1px solid var(--warn);
  }
  .page {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--muted);
  }
  .ctx {
    overflow-wrap: anywhere;
  }
  .serif {
    font-family: var(--font-serif);
  }
  .grow {
    flex: 1;
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
