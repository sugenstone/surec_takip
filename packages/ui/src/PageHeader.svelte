<script lang="ts">
  import type { Snippet } from 'svelte';
  let {
    title,
    description,
    context,
    metadata,
    actions,
  }: {
    title: string;
    description?: string;
    context?: Snippet;
    metadata?: Snippet;
    actions?: Snippet;
  } = $props();
</script>

<header class="page-header">
  {#if context}<div class="page-context">{@render context()}</div>{/if}
  <div class="page-heading">
    <div>
      <h1>{title}</h1>
      {#if description}<p class="page-description">{description}</p>{/if}
    </div>
    {#if metadata}<div class="page-metadata">{@render metadata()}</div>{/if}
    {#if actions}<div class="page-actions">{@render actions()}</div>{/if}
  </div>
</header>
