<script lang="ts">
  // approved / issues / unseen bar (design 4.1 and rail Review block).
  type Props = { approved: number; issues: number; unseen: number; width?: string; height?: string };
  let { approved, issues, unseen, width = "100%", height = "5px" }: Props = $props();
  const total = $derived(Math.max(1, approved + issues + unseen));
</script>

<div
  class="bar"
  style="width:{width};height:{height}"
  role="img"
  aria-label="{approved} approved, {issues} with issues, {unseen} unseen of {approved + issues + unseen}"
>
  <div class="seg ok" style="flex:{approved / total}"></div>
  <div class="seg warn" style="flex:{issues / total}"></div>
  <div class="seg unseen" style="flex:{unseen / total}"></div>
</div>

<style>
  .bar {
    display: flex;
    gap: 1px;
    border-radius: 3px;
    overflow: hidden;
  }
  .seg {
    height: 100%;
    min-width: 0;
  }
  .ok {
    background: var(--ok);
  }
  .warn {
    background: var(--warn);
  }
  .unseen {
    background: var(--track);
  }
</style>
