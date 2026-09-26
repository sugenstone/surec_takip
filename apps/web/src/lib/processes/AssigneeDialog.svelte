<script lang="ts">
  // Assignee picker (STEP 21C, ADR 0018): a single deliberate dialog — lists
  // stay lists. A native radio group keeps the choice keyboard-navigable
  // (arrow keys) without a combobox dependency. A stale current assignee is
  // shown for context but is not a selectable option.
  import { translate, type Locale } from '$lib/i18n';
  import type { ProcessPublic, WorkspaceMemberPublic } from '$lib/api/client';
  let {
    open,
    process,
    members,
    membersFailed,
    pending,
    error,
    locale,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    process: ProcessPublic | null;
    members: WorkspaceMemberPublic[] | null;
    membersFailed: boolean;
    pending: boolean;
    error: string;
    locale: Locale;
    onConfirm: (userId: string | null) => void;
    onCancel: () => void;
  } = $props();
  let dialog: HTMLDialogElement;
  const id = $props.id();
  // Selection mirrors the current assignee when the dialog opens; a stale
  // (ineligible) current assignee cannot be re-picked, so it maps to "keep".
  let selected = $state<string | null>(null);
  let lastOpened = $state<string | null>(null);
  const staleAssignee = $derived(
    process?.assignee && !process.assignee.eligible ? process.assignee : null,
  );
  $effect(() => {
    if (open && !dialog.open) {
      selected = process?.assignee?.eligible ? process.assignee.id : null;
      lastOpened = process?.id ?? null;
      dialog.showModal();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });
  function confirm() {
    onConfirm(selected);
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="{id}-title"
  aria-busy={pending}
  oncancel={(event) => {
    event.preventDefault();
    if (!pending) onCancel();
  }}
>
  {#if process && lastOpened === process.id}
    <h2 id="{id}-title">{translate(locale, 'processes.assignee.title', { name: process.name })}</h2>
    {#if members === null}
      <p class="muted">{translate(locale, 'processes.assignee.loading')}</p>
    {:else if membersFailed}
      <p class="error" role="alert">{translate(locale, 'processes.assignee.error.members')}</p>
    {:else}
      {#if staleAssignee}
        <p class="muted" data-testid="assignee-stale">
          {translate(locale, 'processes.assignee.current', {
            name: staleAssignee.display_name,
          })}
          · {translate(locale, 'processes.assignee.stale')}
        </p>
      {/if}
      <fieldset class="assignee-options" disabled={pending}>
        <legend class="sr-only"
          >{translate(locale, 'processes.assignee.title', { name: process.name })}</legend
        >
        <label class="assignee-option">
          <input
            type="radio"
            name="{id}-assignee"
            checked={selected === null}
            onchange={() => (selected = null)}
          />
          <span>{translate(locale, 'processes.assignee.none')}</span>
        </label>
        {#each members as member (member.id)}
          <label class="assignee-option">
            <input
              type="radio"
              name="{id}-assignee"
              checked={selected === member.id}
              onchange={() => (selected = member.id)}
            />
            <span>{member.display_name}</span>
          </label>
        {/each}
      </fieldset>
      {#if members.length === 0}
        <p class="muted">{translate(locale, 'processes.assignee.empty')}</p>
      {/if}
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="button-row">
      <button type="button" class="secondary" disabled={pending} onclick={onCancel}
        >{translate(locale, 'processes.cancel')}</button
      >
      <button type="button" disabled={pending || members === null} onclick={confirm}
        >{translate(locale, 'processes.assignee.save')}</button
      >
    </div>
  {/if}
</dialog>

<style>
  .assignee-options {
    display: flex;
    flex-direction: column;
    gap: var(--space-2, 0.5rem);
    border: none;
    padding: 0;
    margin: var(--space-3, 0.75rem) 0;
  }
  .assignee-option {
    display: flex;
    align-items: center;
    gap: var(--space-2, 0.5rem);
    min-height: 44px;
    padding: 0 var(--space-2, 0.5rem);
    border-radius: var(--radius, 8px);
    cursor: pointer;
  }
  .assignee-option:has(input:checked) {
    background: var(--surface-alt, rgba(127, 127, 127, 0.12));
  }
  .assignee-option:focus-within {
    outline: 2px solid var(--focus, currentColor);
    outline-offset: 2px;
  }
  .assignee-option input {
    min-width: 18px;
    min-height: 18px;
    margin: 0;
  }
</style>
