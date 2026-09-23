<script lang="ts">
  import { translate } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
</script>

<h2>{data.workspace.name}</h2>
<p class="placeholder">{translate(data.locale, 'shell.ws.placeholder')}</p>

<style>
  h2 {
    font-size: var(--text-heading);
    margin-block: 0 var(--space-2);
  }
  .placeholder {
    color: var(--muted-foreground);
  }
</style>
