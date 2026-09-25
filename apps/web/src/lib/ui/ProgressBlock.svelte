<script lang="ts">
  // Localized progress block (ADR 0017): meter + "x/y süreç" + an in-flight
  // hint. `compact` is for cards — undefined work collapses to just "—";
  // the full form adds the localized empty-work sentence for detail strips.
  import { translate, type Locale } from '$lib/i18n';
  import type { Progress } from '$lib/api/client';
  import { progressView } from '$lib/progress/progress';
  import ProgressMeter from '@platform/ui/ProgressMeter.svelte';
  let {
    progress,
    locale,
    compact = false,
  }: { progress: Progress; locale: Locale; compact?: boolean } = $props();
  let view = $derived(progressView(progress));
</script>

<div class="progress-block" class:compact>
  <ProgressMeter percent={view.percent} label={translate(locale, 'progress.label')} />
  {#if !view.hasWork}
    {#if !compact}
      <span class="progress-note">{translate(locale, 'progress.empty')}</span>
    {/if}
  {:else}
    <span class="progress-counts"
      >{translate(locale, 'progress.processes', {
        completed: String(view.completed),
        total: String(view.total),
      })}</span
    >
    {#if view.hasActive}
      <span class="progress-running"
        >{translate(locale, 'progress.running', { count: String(view.active) })}</span
      >
    {/if}
  {/if}
</div>

<style>
  .progress-block {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    min-width: 0;
  }
  .progress-counts,
  .progress-running,
  .progress-note {
    font-size: var(--text-caption);
    color: var(--muted-foreground);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .progress-running {
    color: var(--primary);
  }
</style>
