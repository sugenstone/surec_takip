<script lang="ts">
  import { onMount } from 'svelte';
  import { translate } from '$lib/i18n';
  import { invalidateAll } from '$app/navigation';
  import { updateProject } from '$lib/api/client';
  import { projectErrorMessageKey } from '$lib/api/errors';
  import { childCounts, directChildren } from '$lib/sections/tree';
  import SectionCard from '$lib/sections/SectionCard.svelte';
  import SectionCreateForm from '$lib/sections/SectionCreateForm.svelte';
  import ProjectEditForm from '$lib/projects/ProjectEditForm.svelte';
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import ConfirmDialog from '@platform/ui/ConfirmDialog.svelte';
  import EmptyState from '@platform/ui/EmptyState.svelte';
  import FormDrawer from '@platform/ui/FormDrawer.svelte';
  import ActionMenu, { type MenuItem } from '@platform/ui/ActionMenu.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import Breadcrumbs from '$lib/ui/Breadcrumbs.svelte';
  import ProgressBlock from '$lib/ui/ProgressBlock.svelte';
  import type { TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let canUpdateProject = $derived(data.permissions.includes('projects:update'));
  let canArchiveProject = $derived(data.permissions.includes('projects:archive'));
  let canCreateSections = $derived(data.permissions.includes('sections:create'));
  // Project detail shows ROOT sections only; deeper levels live on their own
  // pages (drill-down navigation, ADR 0014).
  let roots = $derived(directChildren(data.sections, null));
  let counts = $derived(childCounts(data.sections));

  // ---- project lifecycle -------------------------------------------------
  let editing = $state(false);
  let creatingSection = $state(false);
  let pending = $state(false);
  let error: string | null = $state(null);
  let archiving = $state(false);
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });

  const menuItems = $derived.by<MenuItem[]>(() => {
    const items: (MenuItem | null)[] = [
      canUpdateProject
        ? { label: translate(locale, 'projects.detail.edit'), onSelect: () => (editing = true) }
        : null,
      data.project.status === 'active' && canUpdateProject
        ? {
            label: translate(locale, 'projects.detail.complete'),
            onSelect: () => transition('completed'),
          }
        : null,
      data.project.status === 'completed' && canUpdateProject
        ? {
            label: translate(locale, 'projects.detail.reopen'),
            onSelect: () => transition('active'),
          }
        : null,
      data.project.status === 'archived' && canUpdateProject
        ? {
            label: translate(locale, 'projects.detail.reactivate'),
            onSelect: () => transition('active'),
          }
        : null,
      data.project.status !== 'archived' && canArchiveProject
        ? {
            label: translate(locale, 'projects.detail.archive'),
            danger: true,
            onSelect: () => (archiving = true),
          }
        : null,
    ];
    return items.filter((item): item is MenuItem => item !== null);
  });

  function transition(status: 'completed' | 'archived' | 'active') {
    if (pending) return;
    error = null;
    pending = true;
    (async () => {
      try {
        await updateProject(data.organization.id, data.workspace.id, data.project.id, { status });
        archiving = false;
        await invalidateAll();
      } catch (cause) {
        const key: TranslationKey = projectErrorMessageKey(cause);
        error = translate(locale, key);
      } finally {
        pending = false;
      }
    })();
  }
</script>

<svelte:head>
  <title>{data.project.name} — {data.workspace.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

<PageHeader title={data.project.name} description={data.project.description ?? undefined}>
  {#snippet context()}<Breadcrumbs
      {locale}
      items={[
        { label: data.organization.name, href: `/app/${data.organization.id}` },
        { label: data.workspace.name, href: `/app/${data.organization.id}/${data.workspace.id}` },
        {
          label: translate(locale, 'projects.title'),
          href: `/app/${data.organization.id}/${data.workspace.id}/projects`,
        },
        { label: data.project.name },
      ]}
    />{/snippet}
  {#snippet metadata()}<StatusBadge
      status={data.project.status}
      label={translate(locale, `projects.status.${data.project.status}` as TranslationKey)}
    />{/snippet}
  {#snippet actions()}
    {#if canCreateSections && data.project.status !== 'archived'}
      <button type="button" disabled={!ready} onclick={() => (creatingSection = true)}>
        <Icon name="plus" size={18} />{translate(locale, 'sections.create.submit')}
      </button>
    {/if}
    {#if menuItems.length > 0}
      <ActionMenu
        label={translate(locale, 'ui.actions')}
        items={menuItems}
        disabled={!ready || pending}
      />
    {/if}
  {/snippet}
</PageHeader>

<div class="progress-strip">
  <ProgressBlock progress={data.project.progress} {locale} />
</div>

{#if error}
  <p id="project-error" class="error" role="alert">{error}</p>
{/if}

{#key data.project.id}
  <section class="page-section" aria-label={translate(locale, 'sections.title')}>
    <div class="section-head">
      <div>
        <h2>{translate(locale, 'sections.title')}</h2>
        <p class="muted">{translate(locale, 'ui.sections.description')}</p>
      </div>
    </div>

    {#if data.sectionsFailed}
      <div class="empty-inline">
        <p class="error" role="alert">{translate(locale, 'sections.error.network')}</p>
        <div class="button-row">
          <button class="secondary" onclick={() => invalidateAll()}
            >{translate(locale, 'ui.retry')}</button
          >
        </div>
      </div>
    {:else if roots.length === 0}
      <EmptyState
        title={translate(locale, 'sections.empty.title')}
        description={translate(locale, 'sections.empty.description')}
      >
        {#snippet icon()}<Icon name="layers" size={22} />{/snippet}
      </EmptyState>
    {:else}
      <ul class="collection">
        {#each roots as root (root.id)}
          <li>
            <SectionCard
              section={root}
              childCount={counts.get(root.id) ?? 0}
              orgId={data.organization.id}
              wsId={data.workspace.id}
              projectId={data.project.id}
              {locale}
            />
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/key}

<details class="record-details">
  <summary>{translate(locale, 'ui.details')}</summary>
  <p class="muted">{translate(locale, 'projects.detail.slug')}: {data.project.slug}</p>
</details>

<FormDrawer
  open={creatingSection}
  title={translate(locale, 'sections.create.submit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creatingSection = false)}
>
  <SectionCreateForm
    orgId={data.organization.id}
    wsId={data.workspace.id}
    projectId={data.project.id}
    parentId={null}
    {locale}
    inputId="section-root-name"
    onCreated={() => (creatingSection = false)}
    onCancel={() => (creatingSection = false)}
  />
</FormDrawer>

<FormDrawer
  open={editing}
  title={translate(locale, 'projects.detail.edit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (editing = false)}
>
  <ProjectEditForm
    orgId={data.organization.id}
    wsId={data.workspace.id}
    project={data.project}
    {locale}
    onSaved={() => (editing = false)}
    onCancel={() => (editing = false)}
  />
</FormDrawer>

<ConfirmDialog
  open={archiving}
  title={translate(locale, 'ui.archive.confirm')}
  description={translate(locale, 'ui.archive.description')}
  confirmLabel={translate(locale, 'ui.archive.confirm')}
  cancelLabel={translate(locale, 'ui.cancel')}
  {pending}
  error={error ?? ''}
  onCancel={() => {
    archiving = false;
  }}
  onConfirm={() => transition('archived')}
/>

<style>
  .record-details {
    margin-block-start: var(--space-4);
  }
</style>
