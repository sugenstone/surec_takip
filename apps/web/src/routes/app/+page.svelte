<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import { createOrganization } from '$lib/api/client';
  import { organizationErrorMessageKey } from '$lib/api/errors';
  import type { TranslationKey } from '$lib/i18n';
  let { data } = $props();
  let locale = $derived(data.locale);
  let name = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    errorMessage = null;
    pending = true;
    try {
      const org = await createOrganization(name.trim());
      name = '';
      await invalidateAll();
      await goto(resolve(`/app/${org.id}`));
    } catch (error) {
      const key: TranslationKey = organizationErrorMessageKey(error);
      errorMessage = translate(locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<svelte:head>
  <title>{translate(locale, 'shell.noOrg.title')} — {translate(locale, 'app.name')}</title>
</svelte:head>

<section class="empty-section">
  <h1>{translate(locale, 'shell.noOrg.title')}</h1>
  <p>{translate(locale, 'shell.noOrg.description')}</p>
  <form onsubmit={submit} method="post" novalidate>
    <label class="field-label" for="shell-org-name">
      {translate(locale, 'org.create.label')}
    </label>
    <div class="create-row">
      <input
        id="shell-org-name"
        name="name"
        type="text"
        bind:value={name}
        required
        maxlength="200"
        autocomplete="off"
      />
      <button type="submit" disabled={pending}>
        {pending ? translate(locale, 'org.create.pending') : translate(locale, 'org.create.submit')}
      </button>
    </div>
  </form>
  {#if errorMessage}
    <p class="error" role="alert">{errorMessage}</p>
  {/if}
</section>

<style>
  .empty-section {
    max-width: 28rem;
    margin: var(--space-16) auto;
    padding: var(--space-8);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  h1 {
    font-size: var(--text-title);
    margin-block: 0 var(--space-2);
  }
  p {
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
</style>
