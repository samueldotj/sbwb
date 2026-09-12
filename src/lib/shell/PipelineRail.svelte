<script lang="ts">
  // Persistent left pipeline rail (design section 3). Rows carry a state
  // dot, label, and trailing value; the running row shows a progress bar.
  import type { Snippet } from "svelte";

  export type StageState = "done" | "running" | "queued" | "off" | "failed";
  export type StageRow = {
    id: string;
    label: string;
    state: StageState;
    value: string;
    progress?: number; // 0..1 when running
  };
  type Props = {
    stages: StageRow[];
    activeId?: string;
    onselect?: (id: string) => void;
    children?: Snippet; // review summary or job monitor
    footer?: Snippet;
  };
  let { stages, activeId = "", onselect, children, footer }: Props = $props();
</script>

<nav class="rail" aria-label="Pipeline">
  <div class="label">Pipeline</div>
  {#each stages as s (s.id)}
    <button
      type="button"
      class="row"
      class:active={s.id === activeId || s.state === "running"}
      aria-current={s.id === activeId ? "true" : undefined}
      onclick={() => onselect?.(s.id)}
    >
      <span class="dot {s.state}" aria-hidden="true"></span>
      <span class="name" class:bold={s.state === "running"}>{s.label}</span>
      <span class="value {s.state}">{s.value}</span>
    </button>
    {#if s.state === "running" && s.progress !== undefined}
      <div class="bar" role="progressbar" aria-valuenow={Math.round(s.progress * 100)} aria-valuemin="0" aria-valuemax="100" aria-label="{s.label} progress">
        <div class="fill" style="width:{Math.round(s.progress * 100)}%"></div>
      </div>
    {/if}
  {/each}
  {#if children}
    <div class="divider"></div>
    {@render children()}
  {/if}
  <div class="grow"></div>
  {#if footer}{@render footer()}{/if}
</nav>

<style>
  .rail {
    width: var(--rail-w);
    background: var(--chrome);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 12px 10px;
    gap: 2px;
    min-height: 0;
    overflow: hidden;
  }
  :global([data-theme="bench"]) .rail {
    background: var(--panel);
    padding: 10px 8px;
    gap: 1px;
    font-size: 12px;
  }
  .label {
    padding: 0 6px 8px;
  }
  .row {
    all: unset;
    display: grid;
    grid-template-columns: 16px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 7px 6px;
    border-radius: var(--radius-control);
    cursor: default;
  }
  :global([data-theme="bench"]) .row {
    grid-template-columns: 14px 1fr auto;
    padding: 6px;
  }
  .row:hover {
    background: var(--raised);
  }
  .row.active {
    background: var(--raised);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  :global([data-theme="bench"]) .row.active {
    box-shadow: none;
  }
  .row:focus-visible {
    outline: 2px solid var(--accent);
  }
  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-left: 3px;
    border: 1.5px solid var(--disabled);
  }
  :global([data-theme="bench"]) .dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    margin-left: 0;
    border: 1px solid var(--disabled);
  }
  .dot.done {
    background: var(--ok);
    border-color: var(--ok);
  }
  .dot.running {
    border: 2px solid var(--accent);
    border-right-color: transparent;
    margin-left: 2px;
    animation: spin 0.9s linear infinite;
  }
  :global([data-theme="bench"]) .dot.running {
    border: none;
    background: var(--accent);
    box-shadow: var(--accent-glow);
    animation: none;
    margin-left: 0;
  }
  .dot.failed {
    background: var(--danger);
    border-color: var(--danger);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .bold {
    font-weight: 600;
  }
  .value {
    color: var(--muted);
    font-size: 11px;
  }
  :global([data-theme="bench"]) .value {
    font-family: var(--font-mono);
    font-size: 10.5px;
  }
  .value.running {
    color: var(--accent);
    font-weight: 600;
  }
  .value.failed {
    color: var(--danger);
  }
  .bar {
    height: 3px;
    background: var(--track);
    border-radius: 2px;
    margin: 0 6px 4px;
  }
  :global([data-theme="bench"]) .bar {
    height: 2px;
    margin: 2px 6px 6px;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition: width 200ms ease-out;
  }
  .divider {
    height: 1px;
    background: var(--border);
    margin: 12px 6px;
  }
  .grow {
    flex: 1;
  }
</style>
