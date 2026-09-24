<script lang="ts">
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import Breadcrumbs from '$lib/ui/Breadcrumbs.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
</script>

<svelte:head>
  <title>{data.workspace.name} — {translate(data.locale, 'app.name')}</title>
</svelte:head>

<PageHeader title={data.workspace.name}>
  {#snippet context()}<Breadcrumbs
      locale={data.locale}
      items={[
        { label: data.organization.name, href: `/app/${data.organization.id}` },
        { label: data.workspace.name },
      ]}
    />{/snippet}
</PageHeader>

<section class="modules">
  <a
    class="resource-card module-card"
    href={resolve(`/app/${data.organization.id}/${data.workspace.id}/projects`)}
  >
    <span class="card-glyph glyph-project" aria-hidden="true"><Icon name="folder" size={18} /></span
    >
    <span class="module-body">
      <h2>{translate(data.locale, 'projects.title')}</h2>
      <p>{translate(data.locale, 'projects.entry.description')}</p>
    </span>
    <span class="card-chevron" aria-hidden="true"><Icon name="chevron-right" size={18} /></span>
  </a>
</section>

<style>
  .modules {
    max-width: var(--form-width);
  }
  .module-card {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    text-decoration: none;
    color: inherit;
  }
  .module-body {
    flex: 1;
    min-width: 0;
  }
</style>
