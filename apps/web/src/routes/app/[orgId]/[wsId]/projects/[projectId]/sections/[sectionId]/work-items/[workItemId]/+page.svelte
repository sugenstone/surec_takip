<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import PageHeader from '@platform/ui/PageHeader.svelte';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import ConfirmDialog from '@platform/ui/ConfirmDialog.svelte';
  import FormDrawer from '@platform/ui/FormDrawer.svelte';
  import SectionBreadcrumbs from '$lib/ui/SectionBreadcrumbs.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { translate, type TranslationKey } from '$lib/i18n';
  import { reorderProcesses, updateProcess, type ProcessPublic } from '$lib/api/client';
  import { processErrorMessageKey } from '$lib/api/errors';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
  import ProcessForm from '$lib/processes/ProcessForm.svelte';
  import ProcessRow from '$lib/processes/ProcessRow.svelte';
  import type { PageProps } from './$types';
  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let listParams = $derived({
    orgId: data.organization.id,
    wsId: data.workspace.id,
    projectId: data.project.id,
    sectionId: data.section.id,
  });
  let canCreate = $derived(data.permissions.includes('processes:create'));
  let canUpdate = $derived(data.permissions.includes('processes:update'));
  let canArchive = $derived(data.permissions.includes('processes:archive'));
  let canReorder = $derived(data.permissions.includes('processes:reorder'));

  // SSR renders management controls before their handlers are attached.
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  // Management state is bound to the work item it was opened for, so a
  // client navigation to another item never shows a stale drawer/dialog.
  let creatingFor = $state<string | null>(null);
  let editing = $state<ProcessPublic | null>(null);
  let archiving = $state<ProcessPublic | null>(null);
  let pending = $state(false);
  let errorFor = $state<{ id: string; message: string } | null>(null);
  let announcement = $state('');
  let error = $derived(errorFor?.id === data.item.id ? errorFor.message : null);
  $effect(() => {
    const id = data.item.id;
    if (creatingFor !== null && creatingFor !== id) creatingFor = null;
    if (editing && editing.work_item_id !== id) editing = null;
    if (archiving && archiving.work_item_id !== id) archiving = null;
  });

  async function run(action: () => Promise<unknown>): Promise<boolean> {
    if (pending) return false;
    errorFor = null;
    pending = true;
    try {
      await action();
      await invalidateAll();
      return true;
    } catch (cause) {
      const key: TranslationKey = processErrorMessageKey(cause);
      errorFor = { id: data.item.id, message: translate(locale, key) };
      return false;
    } finally {
      pending = false;
    }
  }

  // Adjacent swap; the server validates the complete permutation.
  async function move(process: ProcessPublic, delta: -1 | 1) {
    const ids = data.processes.map((p) => p.id);
    const from = ids.indexOf(process.id);
    const to = from + delta;
    if (from < 0 || to < 0 || to >= ids.length) return;
    [ids[from], ids[to]] = [ids[to], ids[from]];
    if (!(await run(() => reorderProcesses(data.processScope, ids)))) return;
    announcement = translate(locale, 'processes.moved', {
      name: process.name,
      position: String(to + 1),
    });
    // Keep keyboard focus on the moved row; fall back to the opposite
    // control when the row reached an end and this one became disabled.
    await tick();
    const row = document.querySelector(`[data-process-id="${process.id}"]`);
    const preferred = row?.querySelector<HTMLButtonElement>(
      `[data-move="${delta < 0 ? 'up' : 'down'}"]`,
    );
    const fallback = row?.querySelector<HTMLButtonElement>(
      `[data-move="${delta < 0 ? 'down' : 'up'}"]`,
    );
    (preferred && !preferred.disabled ? preferred : fallback)?.focus();
  }

  async function confirmArchive() {
    const target = archiving;
    if (!target) return;
    if (await run(() => updateProcess(data.processScope, target.id, { status: 'archived' })))
      archiving = null;
  }
</script>

<svelte:head
  ><title>{data.item.name} — {translate(data.locale, 'workItems.title')}</title></svelte:head
>
<div class="work-items">
  <PageHeader title={data.item.name}>
    {#snippet context()}<SectionBreadcrumbs
        locale={data.locale}
        organization={data.organization}
        workspace={data.workspace}
        project={data.project}
        trail={data.sectionTrail}
        leaf={data.item.name}
      />{/snippet}
    {#snippet metadata()}<StatusBadge
        status={data.item.status}
        label={translate(
          data.locale,
          data.item.status === 'completed'
            ? 'workItems.status.completed'
            : 'workItems.status.active',
        )}
      />{/snippet}
  </PageHeader>

  <section class="page-section" aria-labelledby="processes-heading">
    <div class="section-head">
      <div>
        <h2 id="processes-heading">{translate(locale, 'processes.title')}</h2>
        <p class="muted">{translate(locale, 'processes.description')}</p>
      </div>
      {#if canCreate && data.processes.length > 0}
        <button type="button" disabled={!ready} onclick={() => (creatingFor = data.item.id)}>
          <Icon name="plus" size={18} />{translate(locale, 'processes.create')}
        </button>
      {/if}
    </div>
    {#if error && !archiving}<p class="error" role="alert">{error}</p>{/if}
    <!-- Polite live region without role="status": the page's form keeps the
         single status message ("Saved"), reorders are still announced. -->
    <p class="sr-only" aria-live="polite" data-testid="process-announcement">{announcement}</p>
    {#if data.processesFailed}
      <div class="empty-inline">
        <p class="error" role="alert">{translate(locale, 'processes.error.load')}</p>
        <div class="button-row">
          <button class="secondary" type="button" onclick={() => invalidateAll()}
            >{translate(locale, 'processes.retry')}</button
          >
        </div>
      </div>
    {:else if data.processes.length === 0}
      <div class="empty-inline process-empty">
        <span class="process-empty-icon" aria-hidden="true"
          ><Icon name="list-ordered" size={20} /></span
        >
        <div>
          <p class="process-empty-title">{translate(locale, 'processes.empty')}</p>
          {#if canCreate}
            <p class="muted">{translate(locale, 'processes.empty.create')}</p>
            <div class="button-row">
              <button type="button" disabled={!ready} onclick={() => (creatingFor = data.item.id)}>
                <Icon name="plus" size={18} />{translate(locale, 'processes.create')}
              </button>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <ol class="process-list">
        {#each data.processes as process, index (process.id)}
          <li>
            <ProcessRow
              {process}
              {index}
              count={data.processes.length}
              {locale}
              {canUpdate}
              {canArchive}
              {canReorder}
              disabled={!ready || pending}
              onMove={(delta) => move(process, delta)}
              onEdit={() => (editing = process)}
              onArchive={() => {
                errorFor = null;
                archiving = process;
              }}
            />
          </li>
        {/each}
      </ol>
    {/if}
  </section>

  {#if data.permissions.includes('work_items:update')}
    <section class="form-panel">
      <h2>{translate(data.locale, 'workItems.edit')}</h2>
      {#key data.item.id}<WorkItemForm
          scope={data.workItemScope}
          locale={data.locale}
          item={data.item}
          canArchive={data.permissions.includes('work_items:archive')}
          onSaved={async (item) => {
            if (item.status === 'archived')
              await goto(
                resolve(
                  '/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]',
                  listParams,
                ),
              );
            else await invalidateAll();
          }}
        />{/key}
    </section>
  {:else}<section class="panel">
      <p class="muted">{translate(data.locale, 'ui.readOnly')}</p>
      <p>{translate(data.locale, 'workItems.slug')}: {data.item.slug}</p>
    </section>{/if}
</div>

<FormDrawer
  open={creatingFor === data.item.id}
  title={translate(locale, 'processes.create')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (creatingFor = null)}
>
  <ProcessForm
    scope={data.processScope}
    {locale}
    onCancel={() => (creatingFor = null)}
    onSaved={async () => {
      await invalidateAll();
      creatingFor = null;
    }}
  />
</FormDrawer>

<FormDrawer
  open={editing !== null}
  title={translate(locale, 'processes.edit')}
  closeLabel={translate(locale, 'ui.close')}
  onClose={() => (editing = null)}
>
  {#if editing}
    {#key editing.id}<ProcessForm
        scope={data.processScope}
        {locale}
        process={editing}
        onCancel={() => (editing = null)}
        onSaved={async () => {
          await invalidateAll();
          editing = null;
        }}
      />{/key}
  {/if}
</FormDrawer>

<ConfirmDialog
  open={archiving !== null}
  title={translate(locale, 'processes.archive')}
  description={translate(locale, 'processes.archive.description', {
    name: archiving?.name ?? '',
  })}
  confirmLabel={translate(locale, 'processes.archive.submit')}
  cancelLabel={translate(locale, 'processes.cancel')}
  {pending}
  error={archiving ? (error ?? '') : ''}
  onCancel={() => {
    archiving = null;
    errorFor = null;
  }}
  onConfirm={confirmArchive}
/>
