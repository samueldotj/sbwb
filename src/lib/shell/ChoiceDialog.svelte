<script lang="ts">
  // In-app modal with named choices (PRJ-04 close flow). Focus is trapped,
  // Esc picks the cancel choice, the first button gets focus.
  export type Choice = { id: string; label: string; kind?: "primary" | "danger" | "secondary" };
  type Props = { open: boolean; title: string; message: string; choices: Choice[]; onchoose: (id: string) => void; cancelId?: string };
  let { open, title, message, choices, onchoose, cancelId = "cancel" }: Props = $props();

  let panel = $state<HTMLElement | null>(null);
  $effect(() => {
    if (open && panel) {
      const first = panel.querySelector<HTMLElement>("button");
      first?.focus();
    }
  });
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.stopPropagation();
      onchoose(cancelId);
    } else if (e.key === "Tab" && panel) {
      const items = Array.from(panel.querySelectorAll<HTMLElement>("button"));
      if (!items.length) return;
      const first = items[0]!;
      const last = items[items.length - 1]!;
      if (e.shiftKey && document.activeElement === first) {
        last.focus();
        e.preventDefault();
      } else if (!e.shiftKey && document.activeElement === last) {
        first.focus();
        e.preventDefault();
      }
    }
  }
</script>

{#if open}
  <div class="scrim" role="presentation"></div>
  <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="choice-title" aria-describedby="choice-msg" tabindex="-1" bind:this={panel} onkeydown={onkeydown}>
    <div class="title" id="choice-title">{title}</div>
    <p class="msg" id="choice-msg">{message}</p>
    <div class="buttons">
      {#each choices as c (c.id)}
        <button type="button" class="ctl {c.kind ?? 'secondary'}" onclick={() => onchoose(c.id)}>{c.label}</button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(42, 38, 34, 0.35);
    z-index: 60;
  }
  .dialog {
    position: fixed;
    top: 30%;
    left: 50%;
    transform: translateX(-50%);
    width: 460px;
    max-width: calc(100vw - 40px);
    background: var(--paper);
    color: var(--text);
    border-radius: var(--radius-sheet);
    box-shadow: var(--shadow-sheet);
    padding: 22px 24px 18px;
    z-index: 61;
    display: grid;
    gap: 12px;
  }
  .title {
    font-family: var(--font-serif);
    font-size: 20px;
  }
  .msg {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .ctl {
    padding: 7px 12px;
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
  .ctl.danger {
    border-color: var(--danger);
    color: var(--danger);
  }
  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
