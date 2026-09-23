<script lang="ts">
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';

  let {
    locale,
    organizations,
    currentOrganizationId,
    workspaces = [],
    currentWorkspaceId = null,
    permissions = [],
    user,
    children,
  }: {
    locale: string;
    organizations: { id: string; name: string; slug: string }[];
    currentOrganizationId: string;
    workspaces?: { id: string; name: string; slug: string; organization_id: string }[];
    currentWorkspaceId?: string | null;
    permissions?: string[];
    user: { display_name: string; email: string } | null;
    children: import('svelte').Snippet;
  } = $props();

  let navOpen = $state(false);
  const canCreateWorkspace = $derived(permissions.includes('workspaces:create'));

  function switchOrg(event: Event) {
    const target = event.currentTarget as HTMLSelectElement;
    document.cookie = `organization=${target.value}; Path=/; Max-Age=31536000; SameSite=Lax`;
    window.location.href = `/app/${target.value}`;
  }

  function switchWs(event: Event) {
    const target = event.currentTarget as HTMLSelectElement;
    document.cookie = `workspace=${target.value}; Path=/; Max-Age=31536000; SameSite=Lax`;
    window.location.href = `/app/${currentOrganizationId}/${target.value}`;
  }

  async function signOut() {
    const { logoutRequest } = await import('$lib/api/client');
    try {
      await logoutRequest();
    } finally {
      window.location.href = '/login';
    }
  }

  function setTheme(theme: string) {
    document.cookie = `theme=${theme}; Path=/; Max-Age=31536000; SameSite=Lax`;
    window.location.reload();
  }

  function setLocale(newLocale: string) {
    document.cookie = `locale=${newLocale}; Path=/; Max-Age=31536000; SameSite=Lax`;
    window.location.reload();
  }
</script>

<a class="skip-link" href="#main-content"
  >{translate(locale as 'tr-TR' | 'en', 'navigation.skip')}</a
>

<header class="shell-header" role="banner">
  <button
    type="button"
    class="nav-toggle"
    aria-expanded={navOpen}
    aria-label={translate(locale as 'tr-TR' | 'en', 'shell.nav.toggle')}
    onclick={() => (navOpen = !navOpen)}
  >
    <span class="nav-toggle-bar"></span>
    <span class="nav-toggle-bar"></span>
    <span class="nav-toggle-bar"></span>
  </button>

  <nav
    class="shell-nav"
    class:open={navOpen}
    aria-label={translate(locale as 'tr-TR' | 'en', 'shell.nav.label')}
  >
    <ul class="nav-list">
      <li>
        <a href={resolve(`/app/${currentOrganizationId}`)} class="nav-link"
          >{translate(locale as 'tr-TR' | 'en', 'shell.nav.home')}</a
        >
      </li>
    </ul>
  </nav>

  <div class="switchers">
    <label class="switcher-label" for="org-switcher-shell">
      {translate(locale as 'tr-TR' | 'en', 'org.switcher.label')}
    </label>
    <select
      id="org-switcher-shell"
      class="switcher"
      value={currentOrganizationId}
      onchange={switchOrg}
    >
      {#each organizations as org (org.id)}
        <option value={org.id}>{org.name}</option>
      {/each}
    </select>

    {#if workspaces.length > 0}
      <label class="switcher-label" for="ws-switcher-shell">
        {translate(locale as 'tr-TR' | 'en', 'workspace.switcher.label')}
      </label>
      <select
        id="ws-switcher-shell"
        class="switcher"
        value={currentWorkspaceId ?? ''}
        onchange={switchWs}
      >
        {#each workspaces as ws (ws.id)}
          <option value={ws.id}>{ws.name}</option>
        {/each}
      </select>
    {/if}
  </div>

  <div class="account">
    {#if user}<span class="account-name">{user.display_name}</span>{/if}
    <div class="account-controls">
      <label class="sr-only" for="locale-select"
        >{translate(locale as 'tr-TR' | 'en', 'shell.account.language')}</label
      >
      <select
        id="locale-select"
        class="mini-select"
        value={locale}
        onchange={(e) => setLocale((e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="tr-TR">Türkçe</option>
        <option value="en">English</option>
      </select>
      <label class="sr-only" for="theme-select"
        >{translate(locale as 'tr-TR' | 'en', 'shell.account.theme')}</label
      >
      <select
        id="theme-select"
        class="mini-select"
        onchange={(e) => setTheme((e.currentTarget as HTMLSelectElement).value)}
      >
        <option value="light">{translate(locale as 'tr-TR' | 'en', 'shell.theme.light')}</option>
        <option value="dark">{translate(locale as 'tr-TR' | 'en', 'shell.theme.dark')}</option>
        <option value="system">{translate(locale as 'tr-TR' | 'en', 'shell.theme.system')}</option>
      </select>
      <button type="button" class="logout" onclick={signOut}>
        {translate(locale as 'tr-TR' | 'en', 'auth.logout')}
      </button>
    </div>
  </div>
</header>

{#if canCreateWorkspace && workspaces.length === 0 && currentOrganizationId}
  <!-- permission-aware empty state is handled by the org page -->
{/if}

<main id="main-content" tabindex="-1">
  {@render children()}
</main>

<style>
  .skip-link {
    position: fixed;
    inset-block-start: var(--space-2);
    inset-inline-start: var(--space-2);
    padding: var(--space-3);
    background: var(--surface);
    transform: translateY(-200%);
    z-index: 100;
  }
  .skip-link:focus {
    transform: none;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .shell-header {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex-wrap: wrap;
    min-height: 56px;
  }
  .nav-toggle {
    display: none;
    flex-direction: column;
    gap: 4px;
    background: none;
    border: none;
    padding: var(--space-2);
    cursor: pointer;
    min-width: 44px;
    min-height: 44px;
  }
  .nav-toggle-bar {
    display: block;
    width: 20px;
    height: 2px;
    background: var(--foreground);
    border-radius: 1px;
  }
  .shell-nav ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    gap: var(--space-4);
  }
  .nav-link {
    color: var(--foreground);
    text-decoration: none;
    font-weight: 500;
    padding: var(--space-2) var(--space-1);
  }
  .nav-link:hover {
    color: var(--primary);
  }
  .switchers {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-inline-start: auto;
    flex-wrap: wrap;
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
    min-height: 40px;
    max-width: 14rem;
  }
  .account {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .account-name {
    font-weight: 600;
    font-size: var(--text-small);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 10rem;
  }
  .account-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .mini-select {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--foreground);
    font-size: var(--text-caption);
    padding: var(--space-1) var(--space-2);
    min-height: 36px;
  }
  .logout {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    font-size: var(--text-small);
    padding: var(--space-2) var(--space-3);
    min-height: 36px;
    cursor: pointer;
  }
  .logout:hover {
    background: var(--surface-muted);
  }
  main {
    padding: var(--space-6);
    min-height: calc(100vh - 56px);
  }
  @media (max-width: 768px) {
    .nav-toggle {
      display: flex;
    }
    .shell-nav {
      display: none;
      position: absolute;
      top: 56px;
      left: 0;
      right: 0;
      background: var(--surface);
      border-bottom: 1px solid var(--border);
      padding: var(--space-4);
      z-index: 50;
    }
    .shell-nav.open {
      display: block;
    }
    .shell-nav ul {
      flex-direction: column;
      gap: var(--space-2);
    }
    .switchers {
      margin-inline-start: 0;
      width: 100%;
      order: 3;
    }
    .switcher {
      flex: 1;
      max-width: none;
    }
    .account {
      margin-inline-start: auto;
    }
    .account-name {
      display: none;
    }
    main {
      padding: var(--space-4);
    }
  }
</style>
