<script lang="ts">
  // One ordered process DEFINITION plus its execution surface (ADR 0016):
  // real attempt history from the API only — no fake timers, assignees or
  // progress. The live timer is display-only: it derives elapsed time from
  // the server-provided `started_at`/`serverTime` anchor and ticks locally
  // (no per-second writes, client clock skew is corrected by the anchor).
  import ActionMenu, { type MenuItem } from '@platform/ui/ActionMenu.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { translate, type Locale } from '$lib/i18n';
  import type { ExecutionPublic, ProcessPublic, TimeSessionPublic } from '$lib/api/client';
  import { assigneeView } from './assignee';
  let {
    process,
    index,
    count,
    locale,
    canUpdate,
    canArchive,
    canReorder,
    canAssign,
    canStart,
    canComplete,
    canCancel,
    canTrack,
    canStopWork,
    currentUserId,
    sessions,
    attempts,
    serverTime,
    disabled,
    onMove,
    onEdit,
    onArchive,
    onAssign,
    onStart,
    onComplete,
    onCancel,
    onWorkStart,
    onWorkStop,
  }: {
    process: ProcessPublic;
    index: number;
    count: number;
    locale: Locale;
    canUpdate: boolean;
    canArchive: boolean;
    canReorder: boolean;
    canAssign: boolean;
    canStart: boolean;
    canComplete: boolean;
    canCancel: boolean;
    canTrack: boolean;
    canStopWork: boolean;
    currentUserId: string;
    sessions: TimeSessionPublic[];
    attempts: ExecutionPublic[];
    serverTime: string;
    disabled: boolean;
    onMove: (delta: -1 | 1) => void;
    onEdit: () => void;
    onArchive: () => void;
    onAssign: () => void;
    onStart: () => void;
    onComplete: (id: string) => void;
    onCancel: (id: string) => void;
    onWorkStart: () => void;
    onWorkStop: (sessionId: string) => void;
  } = $props();
  const vars = $derived({ name: process.name });
  const menuItems = $derived.by<MenuItem[]>(() => {
    const items: (MenuItem | null)[] = [
      canUpdate ? { label: translate(locale, 'processes.edit'), onSelect: onEdit } : null,
      canAssign
        ? { label: translate(locale, 'processes.assignee.change'), onSelect: onAssign }
        : null,
      canUpdate && canArchive
        ? { label: translate(locale, 'processes.archive'), danger: true, onSelect: onArchive }
        : null,
    ];
    return items.filter((item): item is MenuItem => item !== null);
  });
  // Assignee presentation is a derived view-model — the backend's eligible
  // flag passes through; the chip never recomputes membership itself.
  const assignee = $derived(assigneeView(process, locale));

  const active = $derived(attempts.find((a) => a.status === 'active'));
  // Open labor sessions on the active attempt (STEP 21D): worker chips are
  // the "who is working" surface; the viewer's own open session drives the
  // single start/pause control. Closed intervals live in the attempt
  // history — labor duration is a reporting concern, not a row timer.
  const myOpenSession = $derived(sessions.find((s) => s.worker.id === currentUserId));
  const latest = $derived(attempts[attempts.length - 1]);
  const terminal = $derived(latest && latest.status !== 'active' ? latest : undefined);

  // Timer anchor: elapsed at the moment `serverTime` was generated. Every
  // invalidation delivers a fresh server_time, re-anchoring the local tick.
  let now = $state(Date.now());
  let anchoredAt = $state(Date.now());
  $effect(() => {
    if (!active) return;
    anchoredAt = Date.now();
    now = anchoredAt;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  const elapsedMs = $derived(
    active
      ? Math.max(0, Date.parse(serverTime) - Date.parse(active.started_at) + (now - anchoredAt))
      : 0,
  );
  function formatDuration(ms: number): string {
    const total = Math.floor(ms / 1000);
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  }
  function terminalDuration(attempt: ExecutionPublic): string {
    const end = Date.parse(attempt.completed_at ?? attempt.cancelled_at ?? '');
    const start = Date.parse(attempt.started_at);
    return formatDuration(Math.max(0, end - start));
  }
  function shortTime(iso: string | null | undefined): string {
    if (!iso) return '';
    // ISO-8601 UTC "…T17:04:32Z" → display the HH:MM:SS wall time.
    return iso.slice(11, 19);
  }
</script>

<article class="process-row" data-process-id={process.id}>
  <span class="process-order" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
  <div class="process-body">
    <h3 class="process-name">{process.name}</h3>
    {#if process.description}<p class="process-description">{process.description}</p>{/if}
  </div>
  <span class="requirement-badge" data-required={process.is_required}
    ><span aria-hidden="true">{process.is_required ? '●' : '○'}</span>
    {translate(locale, process.is_required ? 'processes.required' : 'processes.optional')}</span
  >
  <span
    class="assignee-chip"
    class:assignee-unassigned={!assignee.assigned}
    class:assignee-stale={assignee.stale}
    data-testid="assignee-{process.id}"
  >
    {#if assignee.assigned}
      <span class="assignee-avatar" aria-hidden="true">{assignee.initials}</span>
      <span class="assignee-name">{assignee.name}</span>
      {#if assignee.stale}
        <span class="assignee-warning">{translate(locale, 'processes.assignee.stale')}</span>
      {/if}
    {:else}
      <span class="assignee-name">{translate(locale, 'processes.assignee.unassigned')}</span>
    {/if}
  </span>
  {#if canReorder || menuItems.length > 0}
    <div class="process-actions">
      {#if canReorder}
        <button
          type="button"
          class="icon-btn"
          data-move="up"
          aria-label={translate(locale, 'processes.moveUp', vars)}
          title={translate(locale, 'processes.moveUp', vars)}
          disabled={disabled || index === 0}
          onclick={() => onMove(-1)}><Icon name="arrow-up" size={18} /></button
        >
        <button
          type="button"
          class="icon-btn"
          data-move="down"
          aria-label={translate(locale, 'processes.moveDown', vars)}
          title={translate(locale, 'processes.moveDown', vars)}
          disabled={disabled || index === count - 1}
          onclick={() => onMove(1)}><Icon name="arrow-down" size={18} /></button
        >
      {/if}
      {#if menuItems.length > 0}
        <ActionMenu
          label={translate(locale, 'processes.actions', vars)}
          items={menuItems}
          {disabled}
        />
      {/if}
    </div>
  {/if}

  <div class="exec-bar" data-testid="exec-bar-{process.id}">
    {#if active}
      <span class="exec-state exec-active">
        <span class="exec-dot" aria-hidden="true"></span>
        {translate(locale, 'executions.inProgress')}
      </span>
      <span
        class="exec-timer"
        role="timer"
        aria-label={translate(locale, 'executions.timer.elapsed', {
          time: formatDuration(elapsedMs),
        })}
        data-testid="exec-timer">{formatDuration(elapsedMs)}</span
      >
      <span class="exec-buttons">
        {#if canTrack}
          {#if myOpenSession && canStopWork}
            <button
              type="button"
              class="secondary exec-btn work-btn"
              {disabled}
              data-testid="work-pause-{process.id}"
              onclick={() => onWorkStop(myOpenSession.id)}
              >{translate(locale, 'sessions.pause')}</button
            >
          {:else if !myOpenSession}
            <button
              type="button"
              class="secondary exec-btn work-btn"
              {disabled}
              data-testid="work-start-{process.id}"
              onclick={onWorkStart}>{translate(locale, 'sessions.startWork')}</button
            >
          {/if}
        {/if}
        {#if canComplete}
          <button
            type="button"
            class="secondary exec-btn"
            {disabled}
            onclick={() => onComplete(active.id)}>{translate(locale, 'executions.complete')}</button
          >
        {/if}
        {#if canCancel}
          <button
            type="button"
            class="secondary exec-btn"
            {disabled}
            onclick={() => onCancel(active.id)}>{translate(locale, 'executions.cancel')}</button
          >
        {/if}
      </span>
      {#if sessions.length > 0}
        <span class="work-sessions" data-testid="workers-{process.id}">
          <span class="muted">{translate(locale, 'sessions.working')}:</span>
          {#each sessions as session (session.id)}
            <span class="worker-chip"
              >{session.worker.display_name}{#if session.worker.id === currentUserId}
                ({translate(locale, 'sessions.you')}){/if}</span
            >
          {/each}
        </span>
      {/if}
    {:else}
      {#if terminal}
        <span
          class="exec-state"
          class:exec-done={terminal.status === 'completed'}
          class:exec-cancelled={terminal.status === 'cancelled'}
        >
          {translate(
            locale,
            terminal.status === 'completed' ? 'executions.completed' : 'executions.cancelled',
          )}
        </span>
        {#if terminal.status === 'completed'}
          <span class="exec-duration muted">
            {translate(locale, 'executions.duration')}: {terminalDuration(terminal)}
          </span>
        {/if}
      {/if}
      {#if canStart}
        <button type="button" class="secondary exec-btn" {disabled} onclick={onStart}>
          {translate(locale, terminal ? 'executions.retryStart' : 'executions.start')}
        </button>
      {/if}
    {/if}
    {#if attempts.length > 0}
      <details class="exec-history">
        <summary
          >{translate(locale, 'executions.attempts', { count: String(attempts.length) })}</summary
        >
        <ul class="exec-attempts">
          {#each attempts as attempt (attempt.id)}
            <li class="exec-attempt" data-status={attempt.status}>
              <span class="exec-attempt-label"
                >{translate(locale, 'executions.attempt', { n: String(attempt.attempt_no) })}</span
              >
              <span class="exec-attempt-status"
                >{translate(
                  locale,
                  attempt.status === 'active'
                    ? 'executions.inProgress'
                    : attempt.status === 'completed'
                      ? 'executions.completed'
                      : 'executions.cancelled',
                )}</span
              >
              <span class="muted">
                {translate(locale, 'executions.startedAt')}: {shortTime(attempt.started_at)}
                {#if attempt.status !== 'active'}
                  · {translate(locale, 'executions.finishedAt')}: {shortTime(
                    attempt.completed_at ?? attempt.cancelled_at,
                  )} · {terminalDuration(attempt)}
                {/if}
              </span>
              {#if attempt.start_reason}
                <span class="muted exec-reason"
                  >{translate(locale, 'executions.startReason')}: {attempt.start_reason}</span
                >
              {/if}
              {#if attempt.cancel_reason}
                <span class="muted exec-reason"
                  >{translate(locale, 'executions.cancelReason')}: {attempt.cancel_reason}</span
                >
              {/if}
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  </div>
</article>
