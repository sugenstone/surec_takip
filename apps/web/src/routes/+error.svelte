<script lang="ts">
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { resolveLocale, translate } from '$lib/i18n';

  const locale = $derived(resolveLocale(page.data.locale));
  const kind = $derived(page.status === 404 ? 'notFound' : 'unexpected');
</script>

<svelte:head>
  <title>{translate(locale, `error.${kind}.title`)}</title>
</svelte:head>

<h1>{translate(locale, `error.${kind}.title`)}</h1>
<p>{translate(locale, `error.${kind}.description`)}</p>
<a href={resolve('/')}>{translate(locale, 'error.home')}</a>
