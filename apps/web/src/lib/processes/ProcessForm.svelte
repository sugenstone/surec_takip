<script lang="ts">
  // Create/edit a process DEFINITION (ADR 0015) inside a FormDrawer. The
  // drawer mounts children only while open, so every opening starts from a
  // fresh draft and no SSR/hydration gating is needed; a failed save keeps
  // the draft and the drawer open. Configuration only — no execution fields.
  import { untrack } from 'svelte';
  import { translate, type Locale, type TranslationKey } from '$lib/i18n';
  import {
    createProcess,
    updateProcess,
    type ProcessPublic,
    type ProcessScope,
  } from '$lib/api/client';
  import { processErrorMessageKey } from '$lib/api/errors';
  let {
    scope,
    locale,
    process,
    onSaved,
    onCancel,
  }: {
    scope: ProcessScope;
    locale: Locale;
    process?: ProcessPublic;
    onSaved: () => Promise<void>;
    onCancel: () => void;
  } = $props();
  const id = $props.id();
  let name = $state(untrack(() => process?.name ?? ''));
  let slug = $state(untrack(() => process?.slug ?? ''));
  let description = $state(untrack(() => process?.description ?? ''));
  let isRequired = $state(untrack(() => process?.is_required ?? true));
  let pending = $state(false);
  let error: string | null = $state(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || pending) return;
    error = null;
    pending = true;
    try {
      if (process) {
        // An empty description explicitly clears the stored value.
        await updateProcess(scope, process.id, {
          name: trimmed,
          slug,
          description,
          is_required: isRequired,
        });
      } else {
        await createProcess(scope, {
          name: trimmed,
          slug: slug.trim() || undefined,
          description: description.trim() || undefined,
          is_required: isRequired,
        });
      }
      await onSaved();
    } catch (cause) {
      const key: TranslationKey = processErrorMessageKey(cause);
      error = translate(locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<form
  aria-describedby={error ? `${id}-error` : undefined}
  aria-busy={pending}
  onsubmit={submit}
  method="post"
  novalidate
>
  <label for="{id}-name">{translate(locale, 'processes.name')}</label>
  <input id="{id}-name" bind:value={name} required maxlength="200" autocomplete="off" />
  <label for="{id}-slug">{translate(locale, 'processes.slug')}</label>
  <input id="{id}-slug" bind:value={slug} maxlength="64" autocomplete="off" />
  <label for="{id}-description">{translate(locale, 'processes.descriptionLabel')}</label>
  <textarea id="{id}-description" bind:value={description} maxlength="2000" rows="3"></textarea>
  <div class="check-field">
    <label class="check">
      <input type="checkbox" bind:checked={isRequired} aria-describedby="{id}-required-hint" />
      <span>{translate(locale, 'processes.requiredLabel')}</span>
    </label>
    <p id="{id}-required-hint" class="muted">{translate(locale, 'processes.requiredHint')}</p>
  </div>
  {#if error}<p id="{id}-error" class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="submit" disabled={pending || name.trim().length === 0}>
      {translate(locale, pending ? 'processes.saving' : process ? 'processes.save' : 'ui.create')}
    </button>
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}>
      {translate(locale, 'processes.cancel')}
    </button>
  </div>
</form>

<style>
  .check-field {
    margin-block-start: var(--space-4);
  }
  .check-field label.check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-height);
    margin: 0;
    cursor: pointer;
  }
  .check-field label.check input {
    width: 1.125rem;
    height: 1.125rem;
    min-height: 0;
    margin: 0;
    padding: 0;
    accent-color: var(--primary);
    flex: none;
  }
  .check-field p {
    margin: 0;
    font-size: var(--text-small);
  }
</style>
