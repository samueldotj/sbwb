<script lang="ts">
  // AI tab (design 4.6, AI-01..03): enable toggle, provider and model,
  // key status, page selection, the prompt (view and edit the
  // instructions; the data and reply-format rules stay fixed), the "what
  // leaves this machine" consent card, budgets, Send, cancel, and the last
  // run with a link into the review. Nothing is sent without the consent box ticked on this card.
  import { api, errorMessage, parseRanges, type AiRunRecord, type AiStatus, type CostEstimate, type Estimate, type ModelInfo, type ProjectSummary, type PromptPreview } from "$lib/api";
  import { isTauri } from "$lib/ipc";
  import { ai, formatSpend } from "$lib/stores/ai.svelte";
  import { review } from "$lib/stores/review.svelte";
  import { ui } from "$lib/stores/ui.svelte";
  import { view } from "$lib/stores/view.svelte";

  type Props = { summary: ProjectSummary };
  let { summary }: Props = $props();

  const PREF_KEY = "sbwb.ai.prefs";
  // instructions: the user's edited prompt instructions; "" means the default
  type Prefs = { enabled: boolean; provider: string; model: string; maxRequests: number; maxChars: number; instructions: string };
  function loadPrefs(): Prefs {
    try {
      const v = JSON.parse(localStorage.getItem(PREF_KEY) ?? "");
      if (v && typeof v === "object") return { enabled: !!v.enabled, provider: v.provider ?? "openai", model: v.model ?? "", maxRequests: v.maxRequests ?? 10, maxChars: v.maxChars ?? 200000, instructions: typeof v.instructions === "string" ? v.instructions : "" };
    } catch {
      /* defaults */
    }
    return { enabled: false, provider: "openai", model: "", maxRequests: 10, maxChars: 200000, instructions: "" };
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
  let cost = $state<CostEstimate | null>(null);
  let consent = $state(false);
  let runs = $state<AiRunRecord[]>([]);
  let showRuns = $state(false);
  let promptOpen = $state(false);
  let preview = $state<PromptPreview | null>(null);
  let draft = $state("");
  let draftReady = false;

  const runCost = $derived(new Map((ai.spend?.runs ?? []).map((r) => [r.id, r.usd])));
  function costOf(id: string): string {
    const usd = runCost.get(id);
    return usd === undefined ? "" : usd === null ? "cost unknown" : formatSpend(usd);
  }
  const providerStatus = $derived(status?.providers.find((p) => p.provider === prefs.provider) ?? null);
  const hasKey = $derived(!!providerStatus && (providerStatus.session_key || providerStatus.remembered));

  async function refresh() {
    if (!isTauri) return;
    try {
      status = await api.aiStatus();
      runs = await api.aiRuns();
      void ai.loadSpend();
    } catch (e) {
      ui.toast(errorMessage(e), "error");
    }
  }
  // the run's progress and its finish notice live in the ai store, so they
  // carry on while this tab is closed; reload status and history when a run ends
  $effect(() => {
    if (ai.ended) consent = false;
    void refresh();
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
    const instructions = prefs.instructions;
    consent = false;
    let cancelled = false;
    api
      .aiEstimate(ps, instructions)
      .then((e) => {
        if (!cancelled) estimate = e;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // why Send is off, shown under it; the request budget has its own note with a fix
  const overBudget = $derived(!!estimate && estimate.pages.length > prefs.maxRequests);
  const sendBlocker = $derived.by((): string => {
    if (!estimate || estimate.pages.length === 0) return "Choose pages that have been through the text pass.";
    if (!hasKey) return `Add an API key for ${providerStatus?.label ?? prefs.provider} to send.`;
    if (!prefs.model.trim()) return "Choose a model to send.";
    if (overBudget) return `Raise Max requests to ${estimate.pages.length} to send these pages.`;
    if (!consent) return "Tick the consent box to send.";
    return "";
  });

  // priced again whenever the estimate, provider, or model changes; cleared
  // first so a stale price never shows beside another model
  $effect(() => {
    const e = estimate;
    const provider = prefs.provider;
    const model = prefs.model.trim();
    cost = null;
    if (!isTauri || !e || !model) return;
    let cancelled = false;
    api
      .aiCost(provider, model, e.approx_tokens, e.approx_output_tokens)
      .then((c) => {
        if (!cancelled) cost = c;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // the prompt as it would be sent with the current page, while the panel is open
  $effect(() => {
    const page = view.page;
    const instructions = prefs.instructions;
    if (!isTauri || !promptOpen) return;
    let cancelled = false;
    api
      .aiPrompt(page, instructions)
      .then((p) => {
        if (cancelled) return;
        preview = p;
        if (!draftReady) {
          draft = instructions || p.default_instructions;
          draftReady = true;
        }
      })
      .catch((e) => ui.toast(errorMessage(e), "error"));
    return () => {
      cancelled = true;
    };
  });
  // a draft equal to the default is saved as "" so later default changes reach it
  const normalized = $derived(preview && draft.trim() !== preview.default_instructions ? draft.trim() : "");
  const promptDirty = $derived(!!preview && normalized !== prefs.instructions);
  const dataLines = $derived.by(() => {
    const lines = (preview?.user ?? "").trimEnd().split("\n");
    return { shown: lines.slice(0, 14).join("\n"), more: Math.max(0, lines.length - 14) };
  });
  function togglePrompt() {
    promptOpen = !promptOpen;
    draftReady = false;
  }
  function savePrompt() {
    if (!preview || !draft.trim()) return;
    prefs.instructions = normalized;
    draft = prefs.instructions || preview.default_instructions;
    ui.toast(prefs.instructions ? "Prompt instructions saved" : "Using the default prompt instructions", "ok");
  }
  function resetPrompt() {
    if (preview) draft = preview.default_instructions;
  }

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
    ai.lastError = "";
    try {
      await api.aiRun({ provider: prefs.provider, model: prefs.model.trim(), pages: estimate.pages, max_requests: prefs.maxRequests, max_input_chars: prefs.maxChars, instructions: prefs.instructions, consent });
      ai.started(estimate.pages[0] ?? 0, estimate.pages.length);
    } catch (e) {
      ai.lastError = errorMessage(e);
      ui.toast(ai.lastError, "error", 7000);
    }
  }
  async function reviewSuggestions() {
    review.excludeKinds = [];
    review.deferredView = false;
    await review.refreshCounts();
    // AI suggestions are unscored, so they match at every threshold
    await review.step(true);
  }
  function usd(v: number): string {
    return v < 0.01 ? "less than $0.01" : `about $${v.toFixed(2)}`;
  }
  function perMillion(v: number): string {
    return `$${v < 1 ? v.toFixed(3).replace(/0$/, "") : v.toFixed(2)}`;
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

  <div class="prompt">
    <div class="prompthead">
      <span class="label">Prompt</span>
      <span class="tag" class:edited={!!prefs.instructions}>{prefs.instructions ? "Edited" : "Default"}</span>
      <button type="button" class="link" onclick={togglePrompt} aria-expanded={promptOpen}>{promptOpen ? "Hide" : "View or edit"}</button>
    </div>
    {#if promptOpen}
      {#if preview}
        <label class="small promptlabel" for="ai-instructions">Instructions <span class="muted">(you can change these)</span></label>
        <textarea id="ai-instructions" rows="7" bind:value={draft} maxlength={preview.max_instructions}></textarea>
        <div class="promptfoot">
          <span class="muted small">{draft.trim().length} / {preview.max_instructions}</span>
          <button type="button" class="ctl" onclick={resetPrompt} disabled={draft.trim() === preview.default_instructions}>Reset to default</button>
          <button type="button" class="ctl primary" onclick={savePrompt} disabled={!promptDirty || !draft.trim()}>Save</button>
        </div>
        <div class="small promptlabel">Always added <span class="muted">(keeps the page text as data and the reply in the format SBWB checks)</span></div>
        <pre class="fixed">{preview.rules}</pre>
        <div class="small promptlabel">Sent with it, one message per page (page {view.page + 1})</div>
        {#if preview.user}
          <pre class="fixed">{dataLines.shown}{dataLines.more ? `\n… ${dataLines.more} more line${dataLines.more === 1 ? "" : "s"}` : ""}</pre>
        {:else}
          <p class="muted small">This page has no saved words yet.</p>
        {/if}
      {:else}
        <p class="muted small">Loading the prompt…</p>
      {/if}
    {/if}
  </div>

  <div class="card" aria-label="What leaves this machine">
    <div class="label">What leaves this machine</div>
    {#if estimate && estimate.pages.length}
      <ul class="small">
        {#each estimate.payload_kinds as k, i (i)}<li>{k}</li>{/each}
        <li>the prompt: {prefs.instructions ? "your edited instructions" : "the default instructions"} and the fixed rules</li>
        <li>{estimate.pages.length} page{estimate.pages.length === 1 ? "" : "s"}, {estimate.words} words, about {estimate.approx_tokens} tokens in {estimate.requests} request{estimate.requests === 1 ? "" : "s"}</li>
        <li>Not sent: the PDF, scan images, file paths, project metadata, other pages</li>
        {#if !prefs.model.trim()}
          <li>Cost: choose a model to see an estimate</li>
        {:else if !cost}
          <li>Cost: estimating…</li>
        {:else if cost.usd !== null && cost.input_per_mtok !== null && cost.output_per_mtok !== null}
          <li>
            Cost: {usd(cost.usd)}
            <span class="muted">({estimate.approx_tokens} in + about {estimate.approx_output_tokens} out tokens at {perMillion(cost.input_per_mtok)} / {perMillion(cost.output_per_mtok)} per million, prices as of {cost.prices_as_of}; thinking tokens, if the model uses them, cost extra)</span>
          </li>
        {:else}
          <li>Cost: unknown. SBWB has no price for {prefs.model.trim()}; check {providerStatus?.label ?? prefs.provider}'s pricing page.</li>
        {/if}
      </ul>
      <div class="grid">
        <label class="small">Max requests <input type="number" min="1" bind:value={prefs.maxRequests} /></label>
        <label class="small">Max characters per page <input type="number" min="1000" step="1000" bind:value={prefs.maxChars} /></label>
      </div>
      {#if overBudget}
        <p class="note warn">
          {estimate.pages.length} pages need {estimate.pages.length} requests (one per page), more than Max requests ({prefs.maxRequests}).
          <button type="button" class="link" onclick={() => estimate && (prefs.maxRequests = estimate.pages.length)}>Allow {estimate.pages.length} requests</button>
        </p>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={consent} disabled={ai.running !== null} /> I understand what is sent to {providerStatus?.label ?? prefs.provider} ({prefs.model || "no model"})</label>
    {:else}
      <p class="muted small">No processed pages in the selection.</p>
    {/if}
  </div>

  {#if ai.running}
    <div class="progress" role="status" aria-live="polite"><span class="spinner" aria-hidden="true"></span> Page {ai.running.page + 1} · {ai.running.done} of {ai.running.total} sent</div>
    <button type="button" class="ctl" onclick={() => api.aiCancel()}>Cancel after this page</button>
  {:else}
    <button type="button" class="ctl primary wide" onclick={send} disabled={!!sendBlocker} aria-describedby={sendBlocker ? "ai-send-why" : undefined}>
      Send {estimate?.pages.length ?? 0} page{(estimate?.pages.length ?? 0) === 1 ? "" : "s"}
    </button>
    {#if sendBlocker}<p id="ai-send-why" class="muted small why">{sendBlocker}</p>{/if}
  {/if}
  {#if ai.lastError}<p class="note err">{ai.lastError}. No request was retried.</p>{/if}

  {#if status?.last_run}
    <p class="muted small">Last run {when(status.last_run.ts)} · {status.last_run.provider}/{status.last_run.model} · {status.last_run.status} · {status.last_run.suggestions} suggestions{status.last_run.input_tokens !== null ? ` · ${status.last_run.input_tokens} in / ${status.last_run.output_tokens ?? 0} out tokens` : " · tokens unknown"}</p>
  {/if}
  {#if status && status.open_suggestions > 0}
    <button type="button" class="ctl primary wide" onclick={reviewSuggestions}>Review {status.open_suggestions} AI suggestions →</button>
  {/if}
  {#if ai.spend && ai.spend.runs.length}
    <div class="spend" aria-label="AI spend on this book">
      <div class="spendhead"><span class="label">AI spend on this book</span><span class="total">{formatSpend(ai.spend.total_usd)}</span></div>
      <p class="muted small">
        Computed by SBWB, not reported by the provider: the token counts each provider returned for {ai.spend.runs.length} run{ai.spend.runs.length === 1 ? "" : "s"}, at SBWB's list prices as of {ai.spend.prices_as_of}{ai.spend.unpriced ? `. ${ai.spend.unpriced} run${ai.spend.unpriced === 1 ? " has" : "s have"} an unknown cost and ${ai.spend.unpriced === 1 ? "is" : "are"} not counted` : ""}. Your actual charges are on the provider's billing page.
      </p>
      <div class="billing">
        {#each ai.spend.providers as id (id)}
          {@const p = status?.providers.find((x) => x.provider === id)}
          {#if p}<button type="button" class="link" title={p.billing_url} onclick={() => api.aiBillingOpen(id).catch((e) => ui.toast(errorMessage(e), "error"))}>{p.billing_label} ↗</button>{/if}
        {/each}
      </div>
    </div>
  {/if}
  <button type="button" class="link" onclick={() => (showRuns = !showRuns)}>{showRuns ? "Hide" : "Show"} run history ({runs.length})</button>
  {#if showRuns}
    <ul class="runs">
      {#each runs as r (r.id)}
        <li>
          <span class="runline"><span>{when(r.ts)} · {r.provider}/{r.model} · pages {r.pages.map((p) => p + 1).join(", ")}</span><span class="cost">{costOf(r.id)}</span></span>
          <span class="muted">{r.status} · {r.requests} req · {r.suggestions} ok · {r.rejected} rejected{r.input_tokens !== null ? ` · ${r.input_tokens} in / ${r.output_tokens ?? 0} out tokens` : ""}{r.error ? ` · ${r.error}` : ""}</span>
        </li>
      {/each}
      {#if ai.spend && ai.spend.runs.length > runs.length}<li class="muted">Showing the latest {runs.length}; the total covers all {ai.spend.runs.length} runs.</li>{/if}
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
  .prompt {
    display: grid;
    gap: 6px;
  }
  .prompthead {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .prompthead .label {
    margin-bottom: 0;
  }
  .tag.edited {
    background: var(--primary-bg);
    border-color: var(--primary-bg);
    color: var(--primary-fg);
  }
  .promptlabel {
    color: var(--text-2);
  }
  .promptfoot {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .promptfoot .muted {
    flex: 1;
  }
  textarea {
    padding: 6px;
    border: 1px solid var(--border-input);
    border-radius: 5px;
    background: var(--paper);
    color: var(--text);
    font: 12.5px/1.45 var(--font-ui);
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
  }
  textarea:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .fixed {
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--raised);
    color: var(--text-2);
    font: 11.5px/1.45 var(--font-mono);
    white-space: pre-wrap;
    word-break: break-word;
    tab-size: 4;
    max-height: 180px;
    overflow: auto;
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
  .why {
    margin: -4px 0 0;
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
  .spend {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--raised);
  }
  .spend p {
    margin: 0;
  }
  .billing {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
  }
  .spendhead {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .spendhead .label {
    margin-bottom: 0;
  }
  .total {
    font-weight: 600;
    font-size: 15px;
    font-variant-numeric: tabular-nums;
  }
  .runline {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .cost {
    flex: none;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .runs li {
    display: grid;
    gap: 2px;
    padding: 4px 6px;
    border-radius: 4px;
    background: var(--raised);
  }
</style>
