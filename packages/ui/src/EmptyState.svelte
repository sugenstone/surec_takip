<script lang="ts">
  import type { Snippet } from 'svelte';
  let {
    title,
    description,
    icon,
    children,
  }: { title: string; description?: string; icon?: Snippet; children?: Snippet } = $props();
</script>

<section class="empty-state">
  {#if icon}<span class="empty-icon" aria-hidden="true">{@render icon()}</span>{/if}
  <h2>{title}</h2>
  {#if description}<p>{description}</p>{/if}{#if children}{@render children()}{/if}
</section>
