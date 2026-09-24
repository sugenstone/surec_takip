<script lang="ts">
  import '../app.css';
  import { page } from '$app/state';
  import { translate } from '$lib/i18n';
  import type { LayoutProps } from './$types';
  let { data, children }: LayoutProps = $props();
</script>

{#if page.data.organization}
  {@render children()}
{:else}
  <a class="skip-link" href="#main-content">{translate(data.locale, 'navigation.skip')}</a>
  <main id="main-content" class="app-content standalone" tabindex="-1">{@render children()}</main>
{/if}
