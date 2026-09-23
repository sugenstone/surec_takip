<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import { goto, invalidateAll } from '$app/navigation';
  import type { TranslationKey } from '$lib/i18n';
  import { createWorkspace } from '$lib/api/client';
  import { workspaceErrorMessageKey } from '$lib/api/errors';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let locale = $derived(data.locale);
  let name = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);
  const canCreate = $derived(data.permissions.includes('workspaces:create'));
</script>

<svelte:head>
  <title>{data.organization.name} — {translate(locale, 'app.name')}</title>
</svelte:head>

<h1>{data.organization.name}</h1>
<p class="context">
  {translate(locale, 'shell.context.org')} · {data.organization.slug}
</p>

{#if data.workspaces.length === 0}
  <section class="empty-section">
    <h2>{translate(locale, 'shell.noWs.title')}</h2>
    <p>{translate(locale, canCreate ? 'shell.noWs.description' : 'shell.noWs.noPermission')}</p>
    {#if canCreate}
      <form
        onsubmit={async (event) => {
          event.preventDefault();
          if (pending) return;
          errorMessage = null;
          pending = true;
          try {
            const ws = await createWorkspace(data.organization.id, name.trim());
            name = '';
            await invalidateAll();
            await goto(resolve(`/app/${data.organization.id}/${ws.id}`));
          } catch (error) {
            const key: TranslationKey = workspaceErrorMessageKey(error);
            errorMessage = translate(locale, key);
          } finally {
            pending = false;
          }
        }}
        method="post"
        novalidate
      >
        <label class="field-label" for="ws-create-name">
          {translate(locale, 'workspace.create.label')}
        </label>
        <div class="create-row">
          <input
            id="ws-create-name"
            name="name"
            type="text"
            bind:value={name}
            required
            maxlength="200"
            autocomplete="off"
          />
          <button type="submit" disabled={pending}>
            {pending
              ? translate(locale, 'workspace.create.pending')
              : translate(locale, 'workspace.create.submit')}
          </button>
        </div>
      </form>
    {/if}
    {#if errorMessage}
      <p class="error" role="alert">{errorMessage}</p>
    {/if}
  </section>
{:else}
  <section class="ws-list-section">
    <h2>{translate(locale, 'workspace.title')}</h2>
    <ul class="ws-list">
      {#each data.workspaces as ws (ws.id)}
        <li>
          <a class="ws-link" href={resolve(`/app/${data.organization.id}/${ws.id}`)}>
            <span class="ws-name">{ws.name}</span>
            <span class="ws-slug">{ws.slug}</span>
          </a>
        </li>
      {/each}
    </ul>
    {#if canCreate}
      <details>
        <summary>{translate(locale, 'workspace.create.submit')}</summary>
        <form
          onsubmit={async (event) => {
            event.preventDefault();
            if (pending) return;
            errorMessage = null;
            pending = true;
            try {
              const ws = await createWorkspace(data.organization.id, name.trim());
              name = '';
              await invalidateAll();
              await goto(resolve(`/app/${data.organization.id}/${ws.id}`));
            } catch (error) {
              const key: TranslationKey = workspaceErrorMessageKey(error);
              errorMessage = translate(locale, key);
            } finally {
              pending = false;
            }
          }}
          method="post"
          novalidate
        >
          <label class="field-label" for="ws-create-2">
            {translate(locale, 'workspace.create.label')}
          </label>
          <div class="create-row">
            <input
              id="ws-create-2"
              name="name"
              type="text"
              bind:value={name}
              required
              maxlength="200"
              autocomplete="off"
            />
            <button type="submit" disabled={pending}>
              {pending
                ? translate(locale, 'workspace.create.pending')
                : translate(locale, 'workspace.create.submit')}
            </button>
          </div>
        </form>
      </details>
    {/if}
  </section>
{/if}

<style>
  h1 {
    font-size: var(--text-title);
    margin-block: 0 var(--space-1);
  }
  h2 {
    font-size: var(--text-heading);
    margin-block: 0 var(--space-3);
  }
  .context {
    color: var(--muted-foreground);
    margin-block: 0 var(--space-6);
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
  }
  .create-row input {
    flex: 1;
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
  .error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: var(--space-4) 0 0;
  }
  .ws-list-section {
    max-width: 40rem;
  }
  .ws-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .ws-link {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    align-items: baseline;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    text-decoration: none;
    color: inherit;
  }
  .ws-link:hover {
    border-color: var(--primary);
  }
  .ws-name {
    font-weight: 600;
  }
  .ws-slug {
    color: var(--muted-foreground);
    font-size: var(--text-small);
  }
  details {
    margin-block-start: var(--space-4);
  }
  summary {
    cursor: pointer;
    color: var(--primary);
    font-weight: 500;
  }
</style>
