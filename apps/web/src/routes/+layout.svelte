<script lang="ts">
  import '../app.css';
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import { logoutRequest } from '$lib/api/client';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
  let signingOut = $state(false);

  async function signOut() {
    if (signingOut) return;
    signingOut = true;
    try {
      await logoutRequest();
    } catch {
      // The cookie is cleared below regardless; a failed server call keeps
      // the session revocable server-side on the next request.
    } finally {
      signingOut = false;
      await invalidateAll();
      await goto(resolve('/login'));
    }
  }
</script>

<a class="skip-link" href="#main-content">{translate(data.locale, 'navigation.skip')}</a>
{#if data.user}
  <header class="session-bar">
    <span class="session-user">
      <span class="session-label">{translate(data.locale, 'auth.signedIn')}</span>
      <span class="session-name">{data.user.display_name}</span>
    </span>
    <button type="button" class="logout-button" onclick={signOut} disabled={signingOut}>
      {translate(data.locale, 'auth.logout')}
    </button>
  </header>
{/if}
<main id="main-content" tabindex="-1">
  {@render children()}
</main>

<style>
  .session-bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-6);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .session-user {
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    min-width: 0;
  }
  .session-label {
    color: var(--muted-foreground);
    font-size: var(--text-small);
  }
  .session-name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .logout-button {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    font-size: var(--text-small);
    padding: var(--space-2) var(--space-4);
    min-height: 44px;
    cursor: pointer;
  }
  .logout-button:hover {
    background: var(--surface-muted);
  }
</style>
