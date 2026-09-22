<script lang="ts">
  import '../app.css';
  import { goto, invalidateAll } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  import { logoutRequest } from '$lib/api/client';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
  let signingOut = $state(false);

  // Presentation-only context cookie; the backend re-verifies membership on
  // every request regardless of what is stored here.
  async function switchOrganization(id: string) {
    document.cookie = `organization=${id}; Path=/; Max-Age=31536000; SameSite=Lax`;
    await invalidateAll();
  }

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
    <div class="context">
      {#if data.organizations.length > 1}
        <label class="switcher-label" for="organization-switcher">
          {translate(data.locale, 'org.switcher.label')}
        </label>
        <select
          id="organization-switcher"
          class="switcher"
          value={data.currentOrganizationId ?? ''}
          onchange={(event) => switchOrganization(event.currentTarget.value)}
        >
          {#each data.organizations as organization (organization.id)}
            <option value={organization.id}>{organization.name}</option>
          {/each}
        </select>
      {:else if data.organizations.length === 1}
        <span class="single-org">{data.organizations[0].name}</span>
      {/if}
    </div>
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
    flex-wrap: wrap;
  }
  .context {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-inline-end: auto;
    min-width: 0;
  }
  .switcher-label {
    color: var(--muted-foreground);
    font-size: var(--text-small);
  }
  .switcher {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    font-size: var(--text-small);
    padding: var(--space-2);
    min-height: 44px;
    max-width: 16rem;
  }
  .single-org {
    font-weight: 600;
    overflow-wrap: anywhere;
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
