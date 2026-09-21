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
