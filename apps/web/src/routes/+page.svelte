<script lang="ts">
  import { translate } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
</script>

<svelte:head>
  <title>{translate(data.locale, 'app.name')}</title>
</svelte:head>

<h1>{translate(data.locale, 'app.name')}</h1>
{#if data.user}
  <p>{translate(data.locale, 'home.welcome', { name: data.user.display_name })}</p>
{/if}
