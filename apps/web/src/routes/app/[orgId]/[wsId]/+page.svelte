<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
</script>

<svelte:head>
  <title>{data.workspace.name} — {translate(data.locale, 'app.name')}</title>
</svelte:head>

<h2>{data.workspace.name}</h2>

<section class="modules">
  <a
    class="module-card"
    href={resolve(`/app/${data.organization.id}/${data.workspace.id}/projects`)}
  >
    <h3>{translate(data.locale, 'projects.title')}</h3>
    <p>{translate(data.locale, 'projects.entry.description')}</p>
  </a>
</section>

<style>
  h2 {
    font-size: var(--text-heading);
    margin-block: 0 var(--space-4);
  }
  .modules {
    max-width: 40rem;
  }
  .module-card {
    display: block;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    text-decoration: none;
    color: inherit;
  }
  .module-card:hover {
    border-color: var(--primary);
  }
  .module-card h3 {
    font-size: var(--text-body);
    font-weight: 600;
    margin-block: 0 var(--space-1);
  }
  .module-card p {
    color: var(--muted-foreground);
    font-size: var(--text-small);
    margin: 0;
  }
</style>
