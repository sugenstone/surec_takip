<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { goto, invalidateAll } from '$app/navigation';
  import { createProject } from '$lib/api/client';
  import { projectErrorMessageKey } from '$lib/api/errors';
  import type { TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let canCreate = $derived(data.permissions.includes('projects:create'));

  let name = $state('');
  let description = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);

  const statusKey = (status: string): TranslationKey =>
    `projects.status.${status}` as TranslationKey;

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    errorMessage = null;
    pending = true;
    try {
      const trimmedDescription = description.trim();
      const project = await createProject(data.organization.id, data.workspace.id, {
        name: name.trim(),
        ...(trimmedDescription ? { description: trimmedDescription } : {}),
      });
      name = '';
      description = '';
      await invalidateAll();
      await goto(
        resolve(`/app/${data.organization.id}/${data.workspace.id}/projects/${project.id}`),
      );
    } catch (error) {
      const key: TranslationKey = projectErrorMessageKey(error);
      errorMessage = translate(locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<svelte:head>
  <title
    >{translate(locale, 'projects.title')} — {data.workspace.name} — {translate(
      locale,
      'app.name',
    )}</title
  >
</svelte:head>

<h1>{translate(locale, 'projects.title')}</h1>

{#if data.loadFailed}
  <p class="error" role="alert">{translate(locale, 'projects.error.network')}</p>
{:else if data.projects.length === 0}
  <section class="empty-section">
    <h2>{translate(locale, 'projects.empty.title')}</h2>
    <p>
      {translate(locale, canCreate ? 'projects.empty.description' : 'projects.empty.noPermission')}
    </p>
  </section>
{:else}
  <ul class="project-list">
    {#each data.projects as project (project.id)}
      <li>
        <a
          class="project-link"
          href={resolve(`/app/${data.organization.id}/${data.workspace.id}/projects/${project.id}`)}
        >
          <span class="project-name">{project.name}</span>
          {#if project.description}
            <span class="project-description">{project.description}</span>
          {/if}
          <span
            class="status-badge status-{project.status}"
            aria-label="{translate(locale, 'projects.column.status')}: {translate(
              locale,
              statusKey(project.status),
            )}"
          >
            {translate(locale, statusKey(project.status))}
          </span>
        </a>
      </li>
    {/each}
  </ul>
{/if}

{#if canCreate}
  <section class="create-section">
    <h2>{translate(locale, 'projects.create.submit')}</h2>
    <form onsubmit={submit} method="post" novalidate>
      <label class="field-label" for="project-create-name">
        {translate(locale, 'projects.create.label')}
      </label>
      <input
        id="project-create-name"
        name="name"
        type="text"
        bind:value={name}
        required
        maxlength="200"
        autocomplete="off"
      />
      <label class="field-label" for="project-create-description">
        {translate(locale, 'projects.create.description')}
      </label>
      <textarea id="project-create-description" name="description" bind:value={description} rows="2"
      ></textarea>
      <button type="submit" disabled={pending || name.trim().length === 0}>
        {pending
          ? translate(locale, 'projects.create.pending')
          : translate(locale, 'projects.create.submit')}
      </button>
    </form>
  </section>
{/if}

{#if errorMessage}
  <p class="error" role="alert">{errorMessage}</p>
{/if}

<style>
  h1 {
    font-size: var(--text-title);
    margin-block: 0 var(--space-6);
  }
  h2 {
    font-size: var(--text-heading);
    margin-block: 0 var(--space-3);
  }
  .empty-section {
    max-width: 28rem;
    padding: var(--space-6);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  .empty-section p {
    color: var(--muted-foreground);
    margin-block: 0;
  }
  .project-list {
    list-style: none;
    margin: 0 0 var(--space-6);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    max-width: 40rem;
  }
  .project-link {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    text-decoration: none;
    color: inherit;
  }
  .project-link:hover {
    border-color: var(--primary);
  }
  .project-name {
    font-weight: 600;
  }
  .project-description {
    color: var(--muted-foreground);
    font-size: var(--text-small);
    flex-basis: 100%;
  }
  .status-badge {
    margin-inline-start: auto;
    font-size: var(--text-small);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    color: var(--muted-foreground);
    white-space: nowrap;
  }
  /* Status color is decorative only; the badge text carries the meaning. */
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
  .create-section {
    max-width: 28rem;
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
  button {
    margin-block-start: var(--space-4);
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
  button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: var(--space-4) 0 0;
    max-width: 40rem;
  }
</style>
