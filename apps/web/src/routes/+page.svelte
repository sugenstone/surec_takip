<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import { createOrganization } from '$lib/api/client';
  import { organizationErrorMessageKey } from '$lib/api/errors';
  import { translate, type TranslationKey } from '$lib/i18n';
  import { validOrganizationName } from '$lib/org';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let name = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    errorMessage = null;
    if (pending) return;
    if (!validOrganizationName(name)) {
      errorMessage = translate(data.locale, 'org.error.invalidName');
      return;
    }
    pending = true;
    try {
      await createOrganization(name.trim());
      name = '';
      await invalidateAll();
    } catch (error) {
      const key: TranslationKey = organizationErrorMessageKey(error);
      errorMessage = translate(data.locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<svelte:head>
  <title>{translate(data.locale, 'app.name')}</title>
</svelte:head>

<h1>{translate(data.locale, 'app.name')}</h1>
{#if data.user}
  <p>{translate(data.locale, 'home.welcome', { name: data.user.display_name })}</p>

  <section class="org-section" aria-labelledby="org-title">
    <h2 id="org-title">{translate(data.locale, 'org.title')}</h2>
    {#if data.organizations.length === 0}
      <div class="empty-state">
        <p class="empty-title">{translate(data.locale, 'org.empty.title')}</p>
        <p class="empty-description">{translate(data.locale, 'org.empty.description')}</p>
      </div>
    {:else}
      <ul class="org-list">
        {#each data.organizations as organization (organization.id)}
          <li>
            <span class="org-name">{organization.name}</span>
            <span class="org-slug">{organization.slug}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <form onsubmit={submit} method="post" novalidate class="create-form">
      <label class="field-label" for="organization-name">
        {translate(data.locale, 'org.create.label')}
      </label>
      <div class="create-row">
        <input
          id="organization-name"
          name="name"
          type="text"
          bind:value={name}
          required
          maxlength="200"
          autocomplete="off"
        />
        <button type="submit" disabled={pending}>
          {pending
            ? translate(data.locale, 'org.create.pending')
            : translate(data.locale, 'org.create.submit')}
        </button>
      </div>
    </form>
    {#if errorMessage}
      <p class="org-error" role="alert">{errorMessage}</p>
    {/if}
  </section>
{/if}

<style>
  .org-section {
    margin-block-start: var(--space-8);
    max-width: 40rem;
  }
  .org-section h2 {
    font-size: var(--text-heading);
    margin-block: 0 var(--space-4);
  }
  .empty-state {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--surface);
    padding: var(--space-6);
    margin-block-end: var(--space-4);
  }
  .create-form {
    margin-block-start: var(--space-4);
  }
  .empty-title {
    font-weight: 600;
    margin-block: 0 var(--space-1);
  }
  .empty-description {
    color: var(--muted-foreground);
    margin-block: 0 var(--space-4);
  }
  .field-label {
    display: block;
    font-size: var(--text-small);
    font-weight: 600;
    margin-block-end: var(--space-2);
  }
  .create-row {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .create-row input {
    flex: 1 1 12rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    padding: var(--space-3);
    min-height: 44px;
  }
  .create-row button {
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
  .create-row button:hover:enabled {
    background: var(--primary-hover);
  }
  .org-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .org-list li {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    padding: var(--space-3) var(--space-4);
  }
  .org-name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .org-slug {
    color: var(--muted-foreground);
    font-size: var(--text-small);
    overflow-wrap: anywhere;
  }
  .org-error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: var(--space-4) 0 0;
    overflow-wrap: anywhere;
  }
</style>
