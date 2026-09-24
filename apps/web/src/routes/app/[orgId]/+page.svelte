<script lang="ts">
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import EmptyState from '@platform/ui/EmptyState.svelte';
  import FormDrawer from '@platform/ui/FormDrawer.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import WorkspaceCreateForm from '$lib/workspaces/WorkspaceCreateForm.svelte';
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  const canCreate = $derived(data.permissions.includes('workspaces:create'));
  let creating = $state(false);
  // Creation opens a drawer that needs client-side state.
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
</script>

<svelte:head>
  <title>{data.organization.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

<PageHeader title={data.organization.name} description={translate(locale, 'shell.context.org')}>
  {#snippet actions()}
    {#if canCreate && data.workspaces.length > 0}
      <button type="button" disabled={!ready} onclick={() => (creating = true)}>
        <Icon name="plus" size={18} />{translate(locale, 'workspace.create.submit')}
      </button>
    {/if}
  {/snippet}
</PageHeader>

{#if data.workspaces.length === 0}
  <EmptyState
    title={translate(locale, 'shell.noWs.title')}
    description={translate(
      locale,
      canCreate ? 'shell.noWs.description' : 'shell.noWs.noPermission',
    )}
  >
    {#snippet icon()}<Icon name="layout-grid" size={22} />{/snippet}
    {#if canCreate}
      <div class="button-row">
        <button type="button" disabled={!ready} onclick={() => (creating = true)}>
          <Icon name="plus" size={18} />{translate(locale, 'workspace.create.submit')}
        </button>
      </div>
    {/if}
  </EmptyState>
{:else}
  <section class="page-section" aria-label={translate(locale, 'workspace.title')}>
    <div class="section-head">
      <div><h2>{translate(locale, 'workspace.title')}</h2></div>
    </div>
    <ul class="collection">
      {#each data.workspaces as ws (ws.id)}
        <li class="resource-card ws-card">
          <div class="card-heading">
            <span class="card-glyph glyph-workspace" aria-hidden="true"
              ><Icon name="layout-grid" size={18} /></span
            >
            <h2>
              <a class="card-link" href={resolve(`/app/${data.organization.id}/${ws.id}`)}
                >{ws.name}</a
              >
            </h2>
            <span class="card-chevron" aria-hidden="true"
              ><Icon name="chevron-right" size={18} /></span
            >
          </div>
          <p class="card-slug">{ws.slug}</p>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<FormDrawer
  open={creating}
  title={translate(locale, 'workspace.create.submit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creating = false)}
>
  <WorkspaceCreateForm
    orgId={data.organization.id}
    {locale}
    onCancel={() => (creating = false)}
    onCreated={async (ws) => {
      creating = false;
      await goto(resolve(`/app/${data.organization.id}/${ws.id}`));
    }}
  />
</FormDrawer>

<style>
  .card-heading {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  .ws-card h2 {
    margin: 0;
    font-size: var(--text-body);
    flex: 1;
    min-width: 0;
  }
  .card-slug {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-caption);
    color: var(--muted-foreground);
  }
</style>
