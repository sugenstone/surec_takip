<script lang="ts">
  // Derived progress primitive (ADR 0017): renders the backend-supplied
  // integer percent verbatim — the frontend never recomputes a ratio.
  // total = 0 arrives as percent = null and renders "—", never a fake 0% bar.
  let {
    percent,
    label,
  }: {
    percent: number | null;
    label: string;
  } = $props();
</script>

{#if percent === null}
  <span class="progress-meter progress-undefined" data-progress="undefined">—</span>
{:else}
  <span class="progress-meter">
    <span
      class="progress-track"
      role="progressbar"
      aria-valuenow={percent}
      aria-valuemin="0"
      aria-valuemax="100"
      aria-label={label}
    >
      <span class="progress-fill" style:inline-size="{percent}%"></span>
    </span>
    <span class="progress-value">{percent}%</span>
  </span>
{/if}

<style>
  .progress-meter {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .progress-track {
    inline-size: 5.5rem;
    block-size: 0.375rem;
    border-radius: var(--radius-pill);
    background: var(--border-subtle);
    overflow: hidden;
    flex-shrink: 0;
  }
  .progress-fill {
    display: block;
    block-size: 100%;
    background: var(--primary);
    border-radius: inherit;
    transition: inline-size 0.2s ease;
  }
  .progress-value {
    font-size: var(--text-caption);
    font-weight: 600;
    color: var(--foreground);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .progress-undefined {
    color: var(--muted-foreground);
    font-size: var(--text-caption);
  }
  @media (prefers-reduced-motion: reduce) {
    .progress-fill {
      transition: none;
    }
  }
</style>
