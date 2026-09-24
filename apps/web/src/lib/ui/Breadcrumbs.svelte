<script lang="ts">
  import { resolve } from '$app/paths';
  import { translate, type Locale } from '$lib/i18n';
  let {
    items,
    locale,
  }: { items: { label: string; href?: '/app' | `/app/${string}` }[]; locale: Locale } = $props();
</script>

<nav class="breadcrumbs" aria-label={translate(locale, 'ui.breadcrumbs')}>
  <ol>
    {#each items as item, index (index)}<li>
        {#if item.href}<a href={resolve(item.href)}>{item.label}</a>{:else}<span
            aria-current={index === items.length - 1 ? 'page' : undefined}>{item.label}</span
          >{/if}
      </li>{/each}
  </ol>
</nav>
