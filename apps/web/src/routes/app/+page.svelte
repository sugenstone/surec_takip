<script lang="ts">
  import { onMount } from 'svelte';
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import { createOrganization } from '$lib/api/client';
  import { organizationErrorMessageKey } from '$lib/api/errors';
  import type { TranslationKey } from '$lib/i18n';
  let { data } = $props();
  let locale = $derived(data.locale);
  let name = $state('');
  // SSR renders the form before bindings and the submit handler attach;
  // typing before hydration would be silently reset, so gate input on mount.
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
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

<section class="form-panel onboarding">
  <h1>{translate(locale, 'shell.noOrg.title')}</h1>
  <p>{translate(locale, 'shell.noOrg.description')}</p>
  <form
    aria-busy={pending}
    aria-describedby={errorMessage ? 'org-error' : undefined}
    onsubmit={submit}
    method="post"
    novalidate
  >
    <label class="field-label" for="shell-org-name">
      {translate(locale, 'org.create.label')}
    </label>
    <div class="create-row">
      <input
        id="shell-org-name"
        name="name"
        type="text"
        disabled={!ready}
        bind:value={name}
        required
        maxlength="200"
        autocomplete="off"
      />
      <button type="submit" disabled={!ready || pending}>
        {pending ? translate(locale, 'org.create.pending') : translate(locale, 'org.create.submit')}
      </button>
    </div>
  </form>
  {#if errorMessage}
    <p id="org-error" class="error" role="alert">{errorMessage}</p>
  {/if}
</section>

<style>
  .onboarding {
    margin: var(--space-12) auto;
  }
</style>
