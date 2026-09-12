<script lang="ts">
  // Status bar (design section 3). Left: running stage and counters.
  // Right: save state and project file name. Announced politely (UX-05).
  type SaveState = "idle" | "saving" | "saved" | "failed";
  type Props = {
    stage?: string;
    running?: boolean;
    counters?: string;
    saveState?: SaveState;
    savedAgo?: string;
    projectFile?: string;
  };
  let {
    stage = "",
    running = false,
    counters = "",
    saveState = "idle",
    savedAgo = "",
    projectFile = "",
  }: Props = $props();

  const saveLabel = $derived(
    saveState === "saving"
      ? "Saving…"
      : saveState === "failed"
        ? "Save failed"
        : saveState === "saved"
          ? `Saved ${savedAgo || "just now"}`
          : "",
  );
</script>

<footer class="statusbar" aria-live="polite">
  {#if stage}
    <span class="stage">
      {#if running}<span class="spinner" aria-hidden="true"></span>{/if}
      {stage}
    </span>
  {/if}
  {#if counters}<span class="sep">|</span><span>{counters}</span>{/if}
  <span class="grow"></span>
  {#if saveLabel}
    <span class:failed={saveState === "failed"}>{saveLabel}</span>
  {/if}
  {#if projectFile}<span class="sep">|</span><span class="file">{projectFile}</span>{/if}
</footer>

<style>
  .statusbar {
    height: var(--statusbar-h);
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 14px;
    background: var(--chrome);
    border-top: 1px solid var(--border);
    font-size: 11.5px;
    color: var(--text-2);
    white-space: nowrap;
  }
  :global([data-theme="bench"]) .statusbar {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--muted);
    gap: 14px;
  }
  .stage {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .spinner {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    border-right-color: transparent;
    animation: spin 0.9s linear infinite;
  }
  :global([data-theme="bench"]) .spinner {
    width: 6px;
    height: 6px;
    border: none;
    background: var(--accent);
    box-shadow: var(--accent-glow);
    animation: none;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .sep {
    color: var(--disabled);
  }
  .grow {
    flex: 1;
  }
  .failed {
    color: var(--danger);
    font-weight: 600;
  }
  .file {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }
</style>
