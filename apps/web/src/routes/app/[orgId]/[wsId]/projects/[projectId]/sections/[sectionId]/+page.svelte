<script lang="ts">
  // One hierarchy level per page (ADR 0014): the current section is the page
  // context, its DIRECT children are navigation cards and its work items are
  // listed below — descendants are never expanded inline. Deeper levels are
  // reached by navigating, and the breadcrumb carries the ancestor chain.
  // Creation/edit open in drawers; secondary lifecycle actions live in the
  // page action menu so the reading surface stays clean.
  import { onMount } from 'svelte';
  import { translate } from '$lib/i18n';
  import { invalidateAll } from '$app/navigation';
  import { navigating } from '$app/state';
  import { updateSection } from '$lib/api/client';
  import { sectionErrorMessageKey } from '$lib/api/errors';
  import { childCounts, directChildren } from '$lib/sections/tree';
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import ConfirmDialog from '@platform/ui/ConfirmDialog.svelte';
  import EmptyState from '@platform/ui/EmptyState.svelte';
  import FormDrawer from '@platform/ui/FormDrawer.svelte';
  import ActionMenu, { type MenuItem } from '@platform/ui/ActionMenu.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import SectionBreadcrumbs from '$lib/ui/SectionBreadcrumbs.svelte';
  import SectionCard from '$lib/sections/SectionCard.svelte';
  import SectionCreateForm from '$lib/sections/SectionCreateForm.svelte';
  import SectionEditForm from '$lib/sections/SectionEditForm.svelte';
  import WorkItemCard from '$lib/work-items/WorkItemCard.svelte';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
  import type { TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let section = $derived(data.section);
  let canUpdateSections = $derived(data.permissions.includes('sections:update'));
  let canArchiveSections = $derived(data.permissions.includes('sections:archive'));
  let canCreateSections = $derived(data.permissions.includes('sections:create'));
  let canCreateItems = $derived(data.permissions.includes('work_items:create'));

  let children = $derived(directChildren(data.sections, section.id));
  let counts = $derived(childCounts(data.sections));
  let active = $derived(section.status === 'active' && data.project.status !== 'archived');
  let emptySection = $derived(
    !data.itemsHidden && !data.failed && children.length === 0 && data.items.length === 0,
  );

  // SSR renders management controls before their handlers are attached.
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  let pending = $state(false);
  // Drilling between sections reuses this page component, so local management
  // state is bound to the section id it was opened for: navigation to another
  // level automatically hides it without an async reset racing user input.
  let editingId = $state<string | null>(null);
  let archivingId = $state<string | null>(null);
  let creatingSectionFor = $state<string | null>(null);
  let creatingItemFor = $state<string | null>(null);
  let errorFor = $state<{ id: string; message: string } | null>(null);
  let editing = $derived(editingId === section.id);
  let archiving = $derived(archivingId === section.id);
  let creatingSection = $derived(creatingSectionFor === section.id);
  let creatingItem = $derived(creatingItemFor === section.id);
  let error = $derived(errorFor?.id === section.id ? errorFor.message : null);
  $effect(() => {
    const id = section.id;
    if (editingId !== null && editingId !== id) editingId = null;
    if (archivingId !== null && archivingId !== id) archivingId = null;
    if (creatingSectionFor !== null && creatingSectionFor !== id) creatingSectionFor = null;
    if (creatingItemFor !== null && creatingItemFor !== id) creatingItemFor = null;
    if (errorFor && errorFor.id !== id) errorFor = null;
  });

  const menuItems = $derived.by<MenuItem[]>(() => {
    const items: (MenuItem | null)[] = [
      canUpdateSections
        ? {
            label: translate(locale, 'sections.detail.edit'),
            onSelect: () => (editingId = section.id),
          }
        : null,
      canUpdateSections
        ? {
            label: translate(locale, 'sections.up'),
            disabled: section.position === 0,
            onSelect: () => reorder(section.position - 1),
          }
        : null,
      canUpdateSections
        ? {
            label: translate(locale, 'sections.down'),
            onSelect: () => reorder(section.position + 1),
          }
        : null,
      section.status === 'archived' && canUpdateSections
        ? {
            label: translate(locale, 'sections.reactivate'),
            onSelect: () => setStatus('active'),
          }
        : null,
      section.status === 'active' && canArchiveSections
        ? {
            label: translate(locale, 'sections.archive'),
            danger: true,
            onSelect: () => (archivingId = section.id),
          }
        : null,
    ];
    return items.filter((item): item is MenuItem => item !== null);
  });

  async function run(action: () => Promise<unknown>) {
    if (pending) return;
    errorFor = null;
    pending = true;
    try {
      await action();
      archivingId = null;
      await invalidateAll();
    } catch (cause) {
      const key: TranslationKey = sectionErrorMessageKey(cause);
      errorFor = { id: section.id, message: translate(locale, key) };
    } finally {
      pending = false;
    }
  }

  function reorder(position: number) {
    run(() =>
      updateSection(data.organization.id, data.workspace.id, data.project.id, section.id, {
        position,
      }),
    );
  }

  function setStatus(status: 'active' | 'archived') {
    run(() =>
      updateSection(data.organization.id, data.workspace.id, data.project.id, section.id, {
        status,
      }),
    );
  }
</script>

<svelte:head>
  <title>{section.name} — {data.project.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

{#key section.id}
  <PageHeader title={section.name}>
    {#snippet context()}<SectionBreadcrumbs
        {locale}
        organization={data.organization}
        workspace={data.workspace}
        project={data.project}
        trail={data.sectionTrail}
      />{/snippet}
    {#snippet metadata()}<StatusBadge
        status={section.status}
        label={translate(
          locale,
          section.status === 'archived' ? 'sections.status.archived' : 'sections.status.active',
        )}
      />{/snippet}
    {#snippet actions()}
      {#if menuItems.length > 0}
        <ActionMenu
          label={translate(locale, 'ui.actions')}
          items={menuItems}
          disabled={!ready || pending}
        />
      {/if}
    {/snippet}
  </PageHeader>

  {#if error}
    <p id="section-error" class="error" role="alert">{error}</p>
  {/if}

  {#if emptySection}
    <EmptyState
      title={translate(locale, 'sections.detail.empty.title')}
      description={translate(locale, 'sections.detail.empty.description')}
    >
      {#snippet icon()}<Icon name="layers" size={22} />{/snippet}
      {#if active && (canCreateSections || canCreateItems)}
        <div class="button-row">
          {#if canCreateSections}
            <button
              type="button"
              disabled={!ready}
              onclick={() => (creatingSectionFor = section.id)}
            >
              <Icon name="plus" size={18} />{translate(locale, 'sections.createChild')}
            </button>
          {/if}
          {#if canCreateItems}
            <button
              type="button"
              class="secondary"
              disabled={!ready}
              onclick={() => (creatingItemFor = section.id)}
            >
              <Icon name="plus" size={18} />{translate(locale, 'workItems.create')}
            </button>
          {/if}
        </div>
      {/if}
    </EmptyState>
  {:else}
    <section class="page-section" aria-label={translate(locale, 'sections.subsections')}>
      <div class="section-head">
        <div><h2>{translate(locale, 'sections.subsections')}</h2></div>
        {#if canCreateSections && active}
          <button type="button" disabled={!ready} onclick={() => (creatingSectionFor = section.id)}>
            <Icon name="plus" size={18} />{translate(locale, 'sections.createChild')}
          </button>
        {/if}
      </div>
      {#if children.length === 0}
        <p class="empty-inline">{translate(locale, 'sections.subsections.empty')}</p>
      {:else}
        <ul class="collection">
          {#each children as child (child.id)}
            <li>
              <SectionCard
                section={child}
                childCount={counts.get(child.id) ?? 0}
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

    {#if !data.itemsHidden}
      <section class="page-section" aria-label={translate(locale, 'workItems.title')}>
        <div class="section-head">
          <div>
            <h2>{translate(locale, 'workItems.title')}</h2>
            <p class="muted">{translate(locale, 'ui.workItems.description')}</p>
          </div>
          {#if canCreateItems && active}
            <button type="button" disabled={!ready} onclick={() => (creatingItemFor = section.id)}>
              <Icon name="plus" size={18} />{translate(locale, 'workItems.create')}
            </button>
          {/if}
        </div>
        {#if navigating.to}<p role="status">{translate(locale, 'workItems.loading')}</p>{/if}
        {#if data.failed}
          <div class="empty-inline">
            <p class="error" role="alert">{translate(locale, 'workItems.error.load')}</p>
            <div class="button-row">
              <button class="secondary" type="button" onclick={() => invalidateAll()}
                >{translate(locale, 'workItems.retry')}</button
              >
            </div>
          </div>
        {:else if data.items.length === 0}
          <p class="empty-inline">
            {translate(locale, canCreateItems ? 'ui.empty.workItems' : 'workItems.empty')}
          </p>
        {:else}
          <ul class="collection">
            {#each data.items as item (item.id)}
              <li><WorkItemCard {item} scope={data.workItemScope} {locale} /></li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}
  {/if}
{/key}

<FormDrawer
  open={creatingSection}
  title={translate(locale, 'sections.createChild')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creatingSectionFor = null)}
>
  <SectionCreateForm
    orgId={data.organization.id}
    wsId={data.workspace.id}
    projectId={data.project.id}
    parentId={section.id}
    {locale}
    inputId="section-child-name"
    onCreated={() => (creatingSectionFor = null)}
    onCancel={() => (creatingSectionFor = null)}
  />
</FormDrawer>

<FormDrawer
  open={creatingItem}
  title={translate(locale, 'workItems.create')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creatingItemFor = null)}
>
  <WorkItemForm
    scope={data.workItemScope}
    {locale}
    onCancel={() => (creatingItemFor = null)}
    onSaved={async () => {
      creatingItemFor = null;
      await invalidateAll();
    }}
  />
</FormDrawer>

<FormDrawer
  open={editing}
  title={translate(locale, 'sections.detail.edit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (editingId = null)}
>
  <SectionEditForm
    orgId={data.organization.id}
    wsId={data.workspace.id}
    projectId={data.project.id}
    {section}
    sections={data.sections}
    {locale}
    onSaved={() => (editingId = null)}
    onCancel={() => (editingId = null)}
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
    archivingId = null;
  }}
  onConfirm={() => setStatus('archived')}
/>
