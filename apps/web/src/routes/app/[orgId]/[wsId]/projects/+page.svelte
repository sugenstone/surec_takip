<script lang="ts">
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import EmptyState from '@platform/ui/EmptyState.svelte';
  import FormDrawer from '@platform/ui/FormDrawer.svelte';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import ProjectCreateForm from '$lib/projects/ProjectCreateForm.svelte';
  import { translate, type TranslationKey } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  const canCreate = $derived(data.permissions.includes('projects:create'));
  const orgId = $derived(data.organization.id);
  const wsId = $derived(data.workspace.id);
  let creating = $state(false);
  // Creation opens a drawer that needs client-side state.
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
</script>

<svelte:head>
  <title>{translate(locale, 'projects.title')} — {translate(locale, 'app.name')}</title>
</svelte:head>

<PageHeader title={translate(locale, 'projects.title')} description={data.workspace.name}>
  {#snippet metadata()}
    {#if data.permissions.length === 0}
      <span class="muted">{translate(locale, 'ui.readOnly')}</span>
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if canCreate && data.projects.length > 0}
      <button type="button" disabled={!ready} onclick={() => (creating = true)}>
        <Icon name="plus" size={18} />{translate(locale, 'projects.create.submit')}
      </button>
    {/if}
  {/snippet}
</PageHeader>

{#if data.projects.length === 0}
  <EmptyState
    title={translate(locale, 'projects.empty.title')}
    description={translate(
      locale,
      canCreate ? 'projects.empty.description' : 'projects.empty.noPermission',
    )}
  >
    {#snippet icon()}<Icon name="folder" size={22} />{/snippet}
    {#if canCreate}
      <div class="button-row">
        <button type="button" disabled={!ready} onclick={() => (creating = true)}>
          <Icon name="plus" size={18} />{translate(locale, 'projects.create.submit')}
        </button>
      </div>
    {/if}
  </EmptyState>
{:else}
  <ul class="collection">
    {#each data.projects as project (project.id)}
      <li class="resource-card project-card">
        <header>
          <div class="card-heading">
            <span class="card-glyph glyph-project" aria-hidden="true"
              ><Icon name="folder" size={18} /></span
            >
            <h2>
              <a class="card-link" href={resolve(`/app/${orgId}/${wsId}/projects/${project.id}`)}
                >{project.name}</a
              >
            </h2>
          </div>
          <div class="card-side">
            <StatusBadge
              status={project.status}
              label={translate(locale, `projects.status.${project.status}` as TranslationKey)}
            />
            <span class="card-chevron" aria-hidden="true"
              ><Icon name="chevron-right" size={18} /></span
            >
          </div>
        </header>
        {#if project.description}<p class="card-description">{project.description}</p>{/if}
      </li>
    {/each}
  </ul>
{/if}

<FormDrawer
  open={creating}
  title={translate(locale, 'projects.create.submit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creating = false)}
>
  <ProjectCreateForm
    {orgId}
    {wsId}
    {locale}
    onCancel={() => (creating = false)}
    onCreated={async (project) => {
      creating = false;
      await goto(resolve(`/app/${orgId}/${wsId}/projects/${project.id}`));
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
  .card-side {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .card-description {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
</style>
