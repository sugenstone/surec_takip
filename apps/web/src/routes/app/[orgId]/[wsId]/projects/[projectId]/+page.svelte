<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { invalidateAll } from '$app/navigation';
  import { updateProject, type ProjectStatus } from '$lib/api/client';
  import { projectErrorMessageKey } from '$lib/api/errors';
  import type { TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let canUpdate = $derived(data.permissions.includes('projects:update'));
  let canArchive = $derived(data.permissions.includes('projects:archive'));

  let editing = $state(false);
  let name = $state('');
  let description = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);

  const statusKey = (status: string): TranslationKey =>
    `projects.status.${status}` as TranslationKey;

  function startEditing() {
    name = data.project.name;
    description = data.project.description ?? '';
    errorMessage = null;
    editing = true;
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    errorMessage = null;
    pending = true;
    try {
      await updateProject(data.organization.id, data.workspace.id, data.project.id, {
        name: name.trim(),
        description: description.trim(),
      });
      editing = false;
      await invalidateAll();
    } catch (error) {
      const key: TranslationKey = projectErrorMessageKey(error);
      errorMessage = translate(locale, key);
    } finally {
      pending = false;
    }
  }

  // Status commands map 1:1 to the backend transition model: every non-
  // archive transition needs projects:update, entering archived needs
  // projects:archive (ADR 0011).
  async function transition(status: ProjectStatus) {
    if (pending) return;
    errorMessage = null;
    pending = true;
    try {
      await updateProject(data.organization.id, data.workspace.id, data.project.id, { status });
      await invalidateAll();
    } catch (error) {
      const key: TranslationKey = projectErrorMessageKey(error);
      errorMessage = translate(locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<svelte:head>
  <title>{data.project.name} — {data.workspace.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

<div class="header">
  <h1>{data.project.name}</h1>
  <span
    class="status-badge status-{data.project.status}"
    aria-label="{translate(locale, 'projects.column.status')}: {translate(
      locale,
      statusKey(data.project.status),
    )}"
  >
    {translate(locale, statusKey(data.project.status))}
  </span>
</div>
<p class="slug">{translate(locale, 'projects.detail.slug')}: {data.project.slug}</p>

{#if data.project.description}
  <section class="description">
    <h2>{translate(locale, 'projects.detail.description')}</h2>
    <p>{data.project.description}</p>
  </section>
{/if}

{#if canUpdate}
  {#if editing}
    <section class="edit-section" aria-label={translate(locale, 'projects.detail.edit')}>
      <h2>{translate(locale, 'projects.detail.edit')}</h2>
      <form onsubmit={save} method="post" novalidate>
        <label class="field-label" for="project-edit-name">
          {translate(locale, 'projects.create.label')}
        </label>
        <input
          id="project-edit-name"
          name="name"
          type="text"
          bind:value={name}
          required
          maxlength="200"
          autocomplete="off"
        />
        <label class="field-label" for="project-edit-description">
          {translate(locale, 'projects.detail.description')}
        </label>
        <textarea id="project-edit-description" name="description" bind:value={description} rows="3"
        ></textarea>
        <div class="button-row">
          <button type="submit" disabled={pending || name.trim().length === 0}>
            {pending
              ? translate(locale, 'projects.detail.saving')
              : translate(locale, 'projects.detail.save')}
          </button>
          <button
            type="button"
            class="secondary"
            onclick={() => {
              editing = false;
              errorMessage = null;
            }}
          >
            {translate(locale, 'projects.detail.cancel')}
          </button>
        </div>
      </form>
    </section>
  {:else}
    <button type="button" onclick={startEditing}>
      {translate(locale, 'projects.detail.edit')}
    </button>
  {/if}
{/if}

<div class="button-row status-actions">
  {#if data.project.status === 'active' && canUpdate}
    <button type="button" disabled={pending} onclick={() => transition('completed')}>
      {translate(locale, 'projects.detail.complete')}
    </button>
  {/if}
  {#if data.project.status === 'completed' && canUpdate}
    <button type="button" disabled={pending} onclick={() => transition('active')}>
      {translate(locale, 'projects.detail.reopen')}
    </button>
  {/if}
  {#if data.project.status === 'archived' && canUpdate}
    <button type="button" disabled={pending} onclick={() => transition('active')}>
      {translate(locale, 'projects.detail.reactivate')}
    </button>
  {/if}
  {#if data.project.status !== 'archived' && canArchive}
    <button
      type="button"
      class="secondary"
      disabled={pending}
      onclick={() => transition('archived')}
    >
      {translate(locale, 'projects.detail.archive')}
    </button>
  {/if}
</div>

{#if errorMessage}
  <p class="error" role="alert">{errorMessage}</p>
{/if}

<section class="sections-placeholder">
  <h2>{translate(locale, 'projects.detail.sections.title')}</h2>
  <p>{translate(locale, 'projects.detail.sections.placeholder')}</p>
</section>

<a class="back" href={resolve(`/app/${data.organization.id}/${data.workspace.id}/projects`)}>
  ← {translate(locale, 'projects.title')}
</a>

<style>
  .header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  h1 {
    font-size: var(--text-title);
    margin-block: 0 var(--space-1);
  }
  h2 {
    font-size: var(--text-heading);
    margin-block: var(--space-6) var(--space-2);
  }
  .slug {
    color: var(--muted-foreground);
    font-size: var(--text-small);
    margin-block: 0 var(--space-4);
  }
  .status-badge {
    font-size: var(--text-small);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    white-space: nowrap;
  }
  .status-active {
    border-color: var(--status-active);
    color: var(--status-active);
  }
  .status-completed {
    border-color: var(--status-completed);
    color: var(--status-completed);
  }
  .status-archived {
    opacity: 0.75;
  }
  .description {
    max-width: 40rem;
  }
  .description p {
    margin-block: 0;
    white-space: pre-line;
  }
  .edit-section {
    max-width: 28rem;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    margin-block: var(--space-4);
  }
  .field-label {
    display: block;
    font-size: var(--text-small);
    font-weight: 600;
    margin-block: var(--space-3) var(--space-2);
  }
  input,
  textarea {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    padding: var(--space-3);
    min-height: 44px;
  }
  .button-row {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-block-start: var(--space-4);
  }
  button {
    border: none;
    border-radius: var(--radius-md);
    background: var(--primary);
    color: var(--primary-foreground);
    font: inherit;
    font-weight: 600;
    padding: var(--space-3) var(--space-4);
    min-height: 44px;
    cursor: pointer;
  }
  button.secondary {
    background: var(--surface-muted, var(--surface));
    color: var(--foreground);
    border: 1px solid var(--border);
  }
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .status-actions {
    margin-block-start: var(--space-6);
  }
  .error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: var(--space-4) 0 0;
    max-width: 40rem;
  }
  .sections-placeholder {
    margin-block: var(--space-8) var(--space-6);
    max-width: 40rem;
    padding: var(--space-6);
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
  }
  .sections-placeholder p {
    color: var(--muted-foreground);
    margin-block: 0;
  }
  .back {
    display: inline-block;
    color: var(--primary);
    text-decoration: none;
  }
</style>
