<script lang="ts">
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { loginRequest } from '$lib/api/client';
  import { loginErrorMessageKey } from '$lib/api/errors';
  import { translate, type TranslationKey } from '$lib/i18n';
  import type { PageProps } from './$types';

  let { data }: PageProps = $props();
  let email = $state('');
  let password = $state('');
  let pending = $state(false);
  let errorMessage: string | null = $state(null);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (pending) return;
    pending = true;
    errorMessage = null;
    try {
      await loginRequest(email.trim(), password);
      await invalidateAll();
      await goto(resolve('/'));
    } catch (error) {
      const key: TranslationKey = loginErrorMessageKey(error);
      errorMessage = translate(data.locale, key);
    } finally {
      pending = false;
    }
  }
</script>

<svelte:head>
  <title>{translate(data.locale, 'auth.title')} — {translate(data.locale, 'app.name')}</title>
</svelte:head>

<div class="login-wrap">
  <section class="login-card">
    <h1>{translate(data.locale, 'auth.title')}</h1>
    <p class="login-description">{translate(data.locale, 'auth.description')}</p>
    <!-- method=post keeps credentials out of the URL if a native submit ever
         races hydration; the JS handler prevents navigation. -->
    <form onsubmit={submit} method="post" novalidate>
      <div class="field">
        <label for="email">{translate(data.locale, 'auth.email')}</label>
        <input
          id="email"
          name="email"
          type="email"
          autocomplete="email"
          inputmode="email"
          bind:value={email}
          required
          maxlength="254"
        />
      </div>
      <div class="field">
        <label for="password">{translate(data.locale, 'auth.password')}</label>
        <input
          id="password"
          name="password"
          type="password"
          autocomplete="current-password"
          bind:value={password}
          required
          maxlength="1024"
        />
      </div>
      {#if errorMessage}
        <p class="login-error" role="alert">{errorMessage}</p>
      {/if}
      <button type="submit" class="login-submit" disabled={pending}>
        {pending ? translate(data.locale, 'auth.pending') : translate(data.locale, 'auth.submit')}
      </button>
    </form>
  </section>
</div>

<style>
  .login-wrap {
    display: flex;
    justify-content: center;
    padding: var(--space-8) var(--space-4);
  }
  .login-card {
    width: 100%;
    max-width: 24rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: var(--space-8) var(--space-6);
  }
  .login-card h1 {
    margin-block: 0 var(--space-2);
  }
  .login-description {
    color: var(--muted-foreground);
    margin-block: 0 var(--space-6);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-block-end: var(--space-4);
  }
  .field label {
    font-size: var(--text-small);
    font-weight: 600;
  }
  .field input {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    padding: var(--space-3);
    min-height: 44px;
  }
  .login-error {
    color: var(--danger);
    background: var(--surface-muted);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    margin-block: 0 var(--space-4);
    overflow-wrap: anywhere;
  }
  .login-submit {
    width: 100%;
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
  .login-submit:hover:enabled {
    background: var(--primary-hover);
  }
</style>
