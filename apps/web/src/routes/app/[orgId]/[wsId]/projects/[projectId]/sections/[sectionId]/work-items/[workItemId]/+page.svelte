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
  import {
    cancelExecution,
    completeExecution,
    listWorkspaceMembers,
    reorderProcesses,
    startExecution,
    startTimeSession,
    stopTimeSession,
    updateProcess,
    updateProcessAssignment,
    type ExecutionPublic,
    type ProcessPublic,
    type TimeSessionPublic,
    type WorkspaceMemberPublic,
  } from '$lib/api/client';
  import {
    executionErrorMessageKey,
    processErrorMessageKey,
    sessionErrorMessageKey,
  } from '$lib/api/errors';
  import WorkItemForm from '$lib/work-items/WorkItemForm.svelte';
  import ProgressBlock from '$lib/ui/ProgressBlock.svelte';
  import ProcessForm from '$lib/processes/ProcessForm.svelte';
  import ProcessRow from '$lib/processes/ProcessRow.svelte';
  import AssigneeDialog from '$lib/processes/AssigneeDialog.svelte';
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
  let canAssign = $derived(data.permissions.includes('processes:assign'));
  // Execution permissions are backend-enforced; these only gate visibility.
  let canExecStart = $derived(data.permissions.includes('process_executions:start'));
  let canExecComplete = $derived(data.permissions.includes('process_executions:complete'));
  let canExecCancel = $derived(data.permissions.includes('process_executions:cancel'));
  // Time-session permissions are backend-enforced; these only gate controls.
  let canTrackStart = $derived(data.permissions.includes('time_sessions:start'));
  let canTrackStop = $derived(data.permissions.includes('time_sessions:stop'));
  // Attempts grouped per process definition, attempt_no ascending (API order).
  let attemptsByProcess = $derived.by(() => {
    const map: Record<string, ExecutionPublic[]> = {};
    for (const attempt of data.executions) (map[attempt.process_id] ??= []).push(attempt);
    return map;
  });
  // Open labor sessions grouped per process via the active execution id.
  // Sessions on terminal executions are never open (auto-close), so the
  // work-item list maps cleanly onto rows.
  let openSessionsByProcess = $derived.by(() => {
    const map: Record<string, TimeSessionPublic[]> = {};
    for (const session of data.openSessions) {
      const processId = Object.keys(attemptsByProcess).find((id) =>
        attemptsByProcess[id].some(
          (attempt) => attempt.id === session.process_execution_id && attempt.status === 'active',
        ),
      );
      if (processId) (map[processId] ??= []).push(session);
    }
    return map;
  });
  let cancelling = $state<ExecutionPublic | null>(null);
  let cancelReason = $state('');

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
  let assigning = $state<ProcessPublic | null>(null);
  // Member directory: fetched lazily once per page when the picker first
  // opens — never one request per process row (ADR 0018).
  let members = $state<WorkspaceMemberPublic[] | null>(null);
  let membersFailed = $state(false);
  let pending = $state(false);
  let errorFor = $state<{ id: string; message: string } | null>(null);
  let announcement = $state('');
  let error = $derived(errorFor?.id === data.item.id ? errorFor.message : null);
  $effect(() => {
    const id = data.item.id;
    if (creatingFor !== null && creatingFor !== id) creatingFor = null;
    if (editing && editing.work_item_id !== id) editing = null;
    if (archiving && archiving.work_item_id !== id) archiving = null;
    if (assigning && assigning.work_item_id !== id) assigning = null;
  });

  async function run(
    action: () => Promise<unknown>,
    mapError: (cause: unknown) => TranslationKey = processErrorMessageKey,
  ): Promise<boolean> {
    if (pending) return false;
    errorFor = null;
    pending = true;
    try {
      await action();
      await invalidateAll();
      return true;
    } catch (cause) {
      const key: TranslationKey = mapError(cause);
      errorFor = { id: data.item.id, message: translate(locale, key) };
      return false;
    } finally {
      pending = false;
    }
  }

  // Execution transitions (ADR 0016): the server writes all scope/actor/time
  // fields; the client sends at most an optional bounded reason.
  async function startAttempt(process: ProcessPublic) {
    await run(
      () => startExecution({ ...data.processScope, processId: process.id }),
      executionErrorMessageKey,
    );
  }
  async function completeAttempt(execution: ExecutionPublic) {
    await run(
      () =>
        completeExecution({ ...data.processScope, processId: execution.process_id }, execution.id),
      executionErrorMessageKey,
    );
  }
  function askCancel(execution: ExecutionPublic) {
    errorFor = null;
    cancelReason = '';
    cancelling = execution;
  }

  // Work sessions (STEP 21D): pause/resume is close+insert server-side.
  // Both commands resolve the process's ACTIVE attempt — sessions can
  // never target a terminal or missing execution from this UI.
  async function startWork(process: ProcessPublic) {
    const execution = attemptsByProcess[process.id]?.find((attempt) => attempt.status === 'active');
    if (!execution) return;
    await run(
      () => startTimeSession({ ...data.processScope, processId: process.id }, execution.id),
      sessionErrorMessageKey,
    );
  }
  async function stopWork(process: ProcessPublic, sessionId: string) {
    const execution = attemptsByProcess[process.id]?.find((attempt) => attempt.status === 'active');
    if (!execution) return;
    await run(
      () =>
        stopTimeSession({ ...data.processScope, processId: process.id }, execution.id, sessionId),
      sessionErrorMessageKey,
    );
  }
  async function confirmCancel() {
    const target = cancelling;
    if (!target) return;
    const ok = await run(
      () =>
        cancelExecution({ ...data.processScope, processId: target.process_id }, target.id, {
          cancel_reason: cancelReason,
        }),
      executionErrorMessageKey,
    );
    if (ok) {
      cancelling = null;
      cancelReason = '';
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

  // Assignment (STEP 21C): deliberate dialog, member directory fetched once.
  function askAssign(process: ProcessPublic) {
    errorFor = null;
    assigning = process;
    if (members === null && !membersFailed) void loadMembers();
  }
  async function loadMembers() {
    try {
      members = await listWorkspaceMembers(data.organization.id, data.workspace.id);
      membersFailed = false;
    } catch {
      membersFailed = true;
    }
  }
  async function confirmAssign(userId: string | null) {
    const target = assigning;
    if (!target) return;
    const ok = await run(() =>
      updateProcessAssignment({ ...data.processScope, processId: target.id }, { user_id: userId }),
    );
    if (ok) {
      announcement = translate(locale, 'processes.assignee.saved', { name: target.name });
      assigning = null;
    }
  }

  function processNameOf(execution: ExecutionPublic | null): string {
    return (
      data.processes.find((process) => process.id === execution?.process_id)?.name ??
      execution?.process_id ??
      ''
    );
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

  <div class="progress-strip">
    <ProgressBlock progress={data.item.progress} {locale} />
  </div>

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
              {canAssign}
              canStart={canExecStart}
              canComplete={canExecComplete}
              canCancel={canExecCancel}
              canTrack={canTrackStart}
              canStopWork={canTrackStop}
              currentUserId={data.user.id}
              sessions={openSessionsByProcess[process.id] ?? []}
              attempts={attemptsByProcess[process.id] ?? []}
              serverTime={data.serverTime}
              disabled={!ready || pending}
              onMove={(delta) => move(process, delta)}
              onEdit={() => (editing = process)}
              onArchive={() => {
                errorFor = null;
                archiving = process;
              }}
              onAssign={() => askAssign(process)}
              onStart={() => startAttempt(process)}
              onComplete={(id) => {
                const target = attemptsByProcess[process.id]?.find((attempt) => attempt.id === id);
                if (target) void completeAttempt(target);
              }}
              onCancel={(id) => {
                const target = attemptsByProcess[process.id]?.find((attempt) => attempt.id === id);
                if (target) askCancel(target);
              }}
              onWorkStart={() => startWork(process)}
              onWorkStop={(sessionId) => stopWork(process, sessionId)}
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

<AssigneeDialog
  open={assigning !== null}
  process={assigning}
  {members}
  {membersFailed}
  {pending}
  error={assigning ? (error ?? '') : ''}
  {locale}
  onCancel={() => {
    assigning = null;
    errorFor = null;
  }}
  onConfirm={(userId) => void confirmAssign(userId)}
/>

<ConfirmDialog
  open={cancelling !== null}
  title={translate(locale, 'executions.cancel.title')}
  description={translate(locale, 'executions.cancel.description', {
    name: processNameOf(cancelling),
  })}
  confirmLabel={translate(locale, 'executions.cancel.submit')}
  cancelLabel={translate(locale, 'processes.cancel')}
  {pending}
  error={cancelling ? (error ?? '') : ''}
  onCancel={() => {
    cancelling = null;
    cancelReason = '';
    errorFor = null;
  }}
  onConfirm={confirmCancel}
>
  <label class="field">
    <span>{translate(locale, 'executions.cancel.reasonLabel')}</span>
    <input
      type="text"
      maxlength="500"
      bind:value={cancelReason}
      disabled={pending}
      autocomplete="off"
    />
  </label>
</ConfirmDialog>
