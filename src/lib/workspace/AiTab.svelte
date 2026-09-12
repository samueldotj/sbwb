<script lang="ts">
  // AI tab (design 4.6, AI-01..03): enable toggle, provider and model,
  // key status, page selection, the "what leaves this machine" consent
  // card, budgets, Send, cancel, and the last run with a link into the
  // review. Nothing is sent without the consent box ticked on this card.
  import { api, errorMessage, parseRanges, type AiRunRecord, type AiStatus, type Estimate, type ModelInfo, type ProjectSummary } from "$lib/api";
  import { isTauri, listen } from "$lib/ipc";
  import { review } from "$lib/stores/review.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  const PREF_KEY = "sbwb.ai.prefs";
  type Prefs = { enabled: boolean; provider: string; model: string; maxRequests: number; maxChars: number };
  function loadPrefs(): Prefs {
    try {
      const v = JSON.parse(localStorage.getItem(PREF_KEY) ?? "");
      if (v && typeof v === "object") return { enabled: !!v.enabled, provider: v.provider ?? "openai", model: v.model ?? "", maxRequests: v.maxRequests ?? 10, maxChars: v.maxChars ?? 200000 };
    } catch {
      /* defaults */
    }
    return { enabled: false, provider: "openai", model: "", maxRequests: 10, maxChars: 200000 };
  }
  let prefs = $state<Prefs>(loadPrefs());
  $effect(() => {
    try {
      localStorage.setItem(PREF_KEY, JSON.stringify(prefs));
    } catch {
      /* ignore */
    }
  });

  let status = $state<AiStatus | null>(null);
  let models = $state<ModelInfo[]>([]);
  let modelsError = $state("");
  let keyInput = $state("");
  let keyOpen = $state(false);
  let remember = $state(false);
  let scopeKind = $state<"page" | "scope" | "range">("page");
  let rangeText = $state("");
  let estimate = $state<Estimate | null>(null);
  let consent = $state(false);
  let running = $state<{ page: number; done: number; total: number } | null>(null);
  let lastError = $state("");
  let runs = $state<AiRunRecord[]>([]);
  let showRuns = $state(false);
  let unlisten: (() => void) | null = null;

  const providerStatus = $derived(status?.providers.find((p) => p.provider === prefs.provider) ?? null);
  const hasKey = $derived(!!providerStatus && (providerStatus.session_key || providerStatus.remembered));

  async function refresh() {
    if (!isTauri) return;
    try {
      status = await api.aiStatus();
      runs = await api.aiRuns();
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    }
  }
  $effect(() => {
    void refresh();
    void listen<{ type: string; page?: number; done?: number; total?: number; run?: AiRunRecord; error?: { message: string }; suggestions?: number }>("ai:event", (e) => {
      if (e.type === "progress") running = { page: e.page ?? 0, done: e.done ?? 0, total: e.total ?? 0 };
      else if (e.type === "page_done") ui.announce(`Page ${(e.page ?? 0) + 1}: ${e.suggestions ?? 0} AI suggestions`);
      else if (e.type === "finished") {
        running = null;
        consent = false;
        ui.toast(`AI check finished: ${e.run?.suggestions ?? 0} suggestions on ${e.run?.pages.length ?? 0} page(s)`, "ok", 6000);
        void refresh();
        void review.reload();
      } else if (e.type === "failed") {
        running = null;
        consent = false;
        lastError = e.error?.message ?? "failed";
        ui.toast(`AI check stopped: ${lastError}`, "error", 8000);
        void refresh();
        void review.reload();
      }
    }).then((u) => (unlisten = u));
    return () => unlisten?.();
  });

  const pages = $derived.by((): number[] => {
    if (scopeKind === "page") return [view.page];
    if (scopeKind === "scope") return summary.pages.filter((p) => p.text_done).map((p) => p.index);
    const ranges = parseRanges(rangeText) ?? [];
    const out: number[] = [];
    for (const [a, b] of ranges) for (let i = a - 1; i < b && i < summary.pages.length; i++) if (summary.pages[i]?.text_done) out.push(i);
    return out;
  });
  $effect(() => {
    const ps = pages;
    if (!isTauri || ps.length === 0) {
      estimate = null;
      return;
    }
    consent = false;
    let cancelled = false;
    api
      .aiEstimate(ps)
      .then((e) => {
        if (!cancelled) estimate = e;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  async function saveKey() {
    try {
      const stored = await api.aiKeySet(prefs.provider, keyInput, remember);
      keyInput = "";
      keyOpen = false;
      ui.toast(stored ? "Key saved in the Windows credential store" : "Key kept for this session only", "ok");
      await refresh();
      await discover();
    } catch (e) {
      ui.toast(errorMessage(e), "error", 7000);
    }
  }
  async function forgetKey() {
    await api.aiKeyForget(prefs.provider).catch(() => {});
    models = [];
    await refresh();
  }
  async function discover() {
    modelsError = "";
    try {
      models = await api.aiModels(prefs.provider);
      if (!prefs.model || !models.some((m) => m.id === prefs.model)) prefs.model = models[0]?.id ?? prefs.model;
    } catch (e) {
      models = [];
      modelsError = errorMessage(e);
    }
  }
  async function send() {
    if (!estimate || !consent) return;
    lastError = "";
    try {
      await api.aiRun({ provider: prefs.provider, model: prefs.model.trim(), pages: estimate.pages, max_requests: prefs.maxRequests, max_input_chars: prefs.maxChars, consent });
      running = { page: estimate.pages[0] ?? 0, done: 0, total: estimate.pages.length };
    } catch (e) {
      lastError = errorMessage(e);
      ui.toast(lastError, "error", 7000);
    }
  }
  async function reviewSuggestions() {
    review.excludeKinds = [];
    review.deferredView = false;
    await review.refreshCounts();
    // AI suggestions are unscored, so they match at every threshold
    await review.step(true);
  }
  function when(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  }
</script>

<div class="head">
  <div class="title">AI proofread</div>
  <label class="toggle"><input type="checkbox" bind:checked={prefs.enabled} /><span></span> <span class="small">{prefs.enabled ? "Enabled" : "Off"}</span></label>
</div>
{#if !prefs.enabled}
  <p class="muted small">Optional. Sends the saved words of pages you choose to a provider you connect, and brings back word-level suggestions into the review inbox. Nothing runs on its own; every run asks for consent. Local processing, review, and export work without it.</p>
{:else}
  <div class="grid">
    <label>Provider
      <select bind:value={prefs.provider} onchange={() => { models = []; modelsError = ""; }}>
        {#each status?.providers ?? [] as p (p.provider)}<option value={p.provider}>{p.label}</option>{/each}
      </select>
    </label>
    <label>Model
      <div class="row">
        {#if models.length}
          <select bind:value={prefs.model}>{#each models as m (m.id)}<option value={m.id}>{m.label}</option>{/each}</select>
        {:else}
          <input type="text" bind:value={prefs.model} placeholder="model id" aria-label="Model id" />
        {/if}
        <button type="button" class="ctl" onclick={discover} disabled={!hasKey} title="Refresh the model list">↻</button>
      </div>
    </label>
  </div>
  {#if modelsError}<p class="note warn">Model list: {modelsError}. The key is kept; enter a model id by hand or refresh.</p>{/if}

  <div class="keyrow">
    <span class="small">Key status</span>
    {#if hasKey}
      <span class="tag ok">Connected · {providerStatus?.remembered ? "kept on this computer" : "this session only"}</span>
      <button type="button" class="link" onclick={() => (keyOpen = !keyOpen)}>Change</button>
      <button type="button" class="link" onclick={forgetKey}>Forget</button>
    {:else}
      <span class="tag">No key</span>
      <button type="button" class="link" onclick={() => (keyOpen = !keyOpen)}>Add key</button>
    {/if}
  </div>
  {#if keyOpen}
    <div class="keybox">
      <p class="muted small">{providerStatus?.key_help}</p>
      <input type="password" bind:value={keyInput} placeholder="API key" aria-label="API key" autocomplete="off" />
      <label class="check small"><input type="checkbox" bind:checked={remember} /> Remember in the Windows credential store (otherwise session only)</label>
      <div class="two">
        <button type="button" class="ctl" onclick={() => { keyOpen = false; keyInput = ""; }}>Cancel</button>
        <button type="button" class="ctl primary" onclick={saveKey} disabled={!keyInput.trim()}>Save key</button>
      </div>
      <p class="muted small">Keys never go into the project file, exports, or logs. There is no account login.</p>
    </div>
  {/if}

  <div>
    <div class="label">Pages</div>
    <div class="seg" role="radiogroup" aria-label="Pages to send">
      <button type="button" role="radio" aria-checked={scopeKind === "page"} class:on={scopeKind === "page"} onclick={() => (scopeKind = "page")}>This page</button>
      <button type="button" role="radio" aria-checked={scopeKind === "scope"} class:on={scopeKind === "scope"} onclick={() => (scopeKind = "scope")}>1–{summary.counts.in_scope}</button>
      <button type="button" role="radio" aria-checked={scopeKind === "range"} class:on={scopeKind === "range"} onclick={() => (scopeKind = "range")}>Range…</button>
    </div>
    {#if scopeKind === "range"}<input type="text" class="range" bind:value={rangeText} placeholder="e.g. 10-20, 35" aria-label="Page range" />{/if}
  </div>

  <div class="card" aria-label="What leaves this machine">
    <div class="label">What leaves this machine</div>
    {#if estimate && estimate.pages.length}
      <ul class="small">
        {#each estimate.payload_kinds as k, i (i)}<li>{k}</li>{/each}
        <li>{estimate.pages.length} page{estimate.pages.length === 1 ? "" : "s"}, {estimate.words} words, about {estimate.approx_tokens} tokens in {estimate.requests} request{estimate.requests === 1 ? "" : "s"}</li>
        <li>Not sent: the PDF, scan images, file paths, project metadata, other pages</li>
        <li>Cost: {estimate.cost}</li>
      </ul>
      <div class="grid">
        <label class="small">Max requests <input type="number" min="1" max="500" bind:value={prefs.maxRequests} /></label>
        <label class="small">Max characters per page <input type="number" min="1000" step="1000" bind:value={prefs.maxChars} /></label>
      </div>
      <label class="check"><input type="checkbox" bind:checked={consent} disabled={running !== null} /> I understand what is sent to {providerStatus?.label ?? prefs.provider} ({prefs.model || "no model"})</label>
    {:else}
      <p class="muted small">No processed pages in the selection.</p>
    {/if}
  </div>

  {#if running}
    <div class="progress" role="status" aria-live="polite"><span class="spinner" aria-hidden="true"></span> Page {running.page + 1} · {running.done} of {running.total} sent</div>
    <button type="button" class="ctl" onclick={() => api.aiCancel()}>Cancel after this page</button>
  {:else}
    <button type="button" class="ctl primary wide" onclick={send} disabled={!consent || !hasKey || !prefs.model.trim() || !estimate || estimate.pages.length === 0 || estimate.pages.length > prefs.maxRequests}>
      Send {estimate?.pages.length ?? 0} page{(estimate?.pages.length ?? 0) === 1 ? "" : "s"}
    </button>
  {/if}
  {#if lastError}<p class="note err">{lastError}. No request was retried.</p>{/if}

  {#if status?.last_run}
    <p class="muted small">Last run {when(status.last_run.ts)} · {status.last_run.provider}/{status.last_run.model} · {status.last_run.status} · {status.last_run.suggestions} suggestions{status.last_run.input_tokens !== null ? ` · ${status.last_run.input_tokens} in / ${status.last_run.output_tokens ?? 0} out tokens` : " · tokens unknown"}</p>
  {/if}
  {#if status && status.open_suggestions > 0}
    <button type="button" class="ctl primary wide" onclick={reviewSuggestions}>Review {status.open_suggestions} AI suggestions →</button>
  {/if}
  <button type="button" class="link" onclick={() => (showRuns = !showRuns)}>{showRuns ? "Hide" : "Show"} run history ({runs.length})</button>
  {#if showRuns}
    <ul class="runs">
      {#each runs as r (r.id)}
        <li><span>{when(r.ts)} · {r.provider}/{r.model} · pages {r.pages.map((p) => p + 1).join(", ")}</span><span class="muted">{r.status} · {r.requests} req · {r.suggestions} ok · {r.rejected} rejected{r.error ? ` · ${r.error}` : ""}</span></li>
      {/each}
    </ul>
  {/if}
  <p class="muted small">Suggestions arrive as unscored candidates labelled with the provider and model. They are never applied automatically.</p>
{/if}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .grid label {
    display: grid;
    gap: 3px;
    font-size: 12px;
    color: var(--text-2);
  }
  .row {
    display: flex;
    gap: 4px;
  }
  .row select,
  .row input {
    flex: 1;
    min-width: 0;
  }
  select,
  input[type="text"],
  input[type="password"],
  input[type="number"] {
    padding: 5px 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
    font-size: 12.5px;
    width: 100%;
    box-sizing: border-box;
  }
  .keyrow {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .keybox {
    display: grid;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--raised);
  }
  .keybox p {
    margin: 0;
  }
  .tag {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 10px;
    background: var(--raised);
    border: 1px solid var(--border);
  }
  .tag.ok {
    background: var(--ok-bg);
    border-color: var(--ok);
  }
  .label {
    margin-bottom: 4px;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border-input);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    all: unset;
    padding: 5px 10px;
    font-size: 12px;
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
  .range {
    margin-top: 6px;
  }
  .card {
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
    display: grid;
    gap: 8px;
  }
  .card ul {
    margin: 0;
    padding-left: 18px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    font-size: 12.5px;
  }
  .toggle {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .two {
    display: flex;
    gap: 6px;
  }
  .two .ctl {
    flex: 1;
  }
  .ctl {
    padding: 6px 10px;
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
  .ctl.wide {
    width: 100%;
  }
  .ctl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ctl:focus-visible,
  .link:focus-visible,
  input:focus-visible,
  select:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .link {
    all: unset;
    color: var(--accent-text);
    text-decoration: underline;
    cursor: pointer;
    font-size: 12px;
  }
  .note {
    margin: 0;
    font-size: 12px;
  }
  .note.warn {
    color: var(--accent-text);
  }
  .note.err {
    color: var(--danger);
  }
  .progress {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12.5px;
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
  .runs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 11.5px;
  }
  .runs li {
    display: grid;
    gap: 2px;
    padding: 4px 6px;
    border-radius: 4px;
    background: var(--raised);
  }
</style>
