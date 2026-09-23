<script lang="ts">
  import '../app.css';
  import { translate } from '$lib/i18n';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
</script>

<a class="skip-link" href="#main-content">{translate(data.locale, 'navigation.skip')}</a>
<main id="main-content" tabindex="-1">
  {@render children()}
</main>

<style>
  .skip-link {
    position: fixed;
    inset-block-start: var(--space-2);
    inset-inline-start: var(--space-2);
    padding: var(--space-3);
    background: var(--surface);
    transform: translateY(-200%);
    z-index: 100;
  }
  .skip-link:focus {
    transform: none;
  }
  main {
    min-height: 100vh;
  }
</style>
