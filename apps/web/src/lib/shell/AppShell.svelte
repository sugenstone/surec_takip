<script lang="ts">
  // Professional application shell (ADR 0014, STEP 19.5C):
  // persistent sidebar (brand, context selection, primary navigation,
  // account) + slim topbar (mobile navigation trigger, global utilities).
  // Below 768px the sidebar becomes an off-canvas drawer; the URL stays the
  // single source of navigation truth — the drawer only surfaces links.
  import { onMount } from 'svelte';
  import { afterNavigate } from '$app/navigation';
  import { navigating, page } from '$app/state';
  import type { Theme } from '$lib/theme';
  import { translate } from '$lib/i18n';
  import { resolve } from '$app/paths';
  import Icon from '$lib/ui/Icon.svelte';

  let {
    locale,
    theme,
    organizations,
    currentOrganizationId,
    workspaces = [],
    currentWorkspaceId = null,
    user,
    children,
  }: {
    locale: 'tr-TR' | 'en';
    theme: Theme;
    organizations: { id: string; name: string; slug: string }[];
    currentOrganizationId: string;
    workspaces?: { id: string; name: string; slug: string; organization_id: string }[];
    currentWorkspaceId?: string | null;
    user: { display_name: string; email: string } | null;
    children: import('svelte').Snippet;
  } = $props();

  let navOpen = $state(false);
  let ready = $state(false);
  let sidebarElement = $state<HTMLElement>();
  let navToggle = $state<HTMLButtonElement>();
  onMount(() => {
    ready = true;
  });
  const inProjects = $derived(page.url.pathname.includes('/projects'));
  const inOrgHome = $derived(page.url.pathname === `/app/${currentOrganizationId}`);
  const initials = $derived(
    (user?.display_name ?? '')
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toLocaleUpperCase(locale) ?? '')
      .join(''),
  );
  afterNavigate(() => {
    navOpen = false;
  });

  function closeNav(refocus = false) {
    navOpen = false;
    if (refocus) navToggle?.focus();
  }

  // Drawer open state (mobile only): focus moves into the drawer, Escape and
  // page scroll lock apply, and cleanup restores the previous state.
  $effect(() => {
    if (!navOpen) return;
    sidebarElement?.querySelector<HTMLElement>('.drawer-close')?.focus();
    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        closeNav(true);
      }
    };
    window.addEventListener('keydown', onKeydown);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      window.removeEventListener('keydown', onKeydown);
      document.body.style.overflow = previousOverflow;
    };
  });

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

<a class="skip-link" href="#main-content">{translate(locale, 'navigation.skip')}</a>

<div class="shell">
  <aside bind:this={sidebarElement} id="app-sidebar" class="app-sidebar" class:open={navOpen}>
    <a class="brand" href={resolve('/app')}>
      <span class="brand-mark"><Icon name="layers" size={16} /></span>
      <span class="brand-name">{translate(locale, 'app.name')}</span>
    </a>

    <div class="sidebar-context">
      <span class="context-title">{translate(locale, 'shell.context')}</span>
      <div class="sidebar-field">
        <label for="org-switcher-shell">{translate(locale, 'org.switcher.label')}</label>
        <div class="select-wrap">
          <select
            id="org-switcher-shell"
            disabled={!ready}
            value={currentOrganizationId}
            onchange={switchOrg}
          >
            {#each organizations as org (org.id)}
              <option value={org.id}>{org.name}</option>
            {/each}
          </select>
          <Icon name="chevron-down" size={16} />
        </div>
      </div>

      {#if workspaces.length > 0}
        <div class="sidebar-field">
          <label for="ws-switcher-shell">{translate(locale, 'workspace.switcher.label')}</label>
          <div class="select-wrap">
            <select
              id="ws-switcher-shell"
              disabled={!ready}
              value={currentWorkspaceId ?? ''}
              onchange={switchWs}
            >
              <option value="" disabled>{translate(locale, 'ui.workspace.choose')}</option>
              {#each workspaces as ws (ws.id)}
                <option value={ws.id}>{ws.name}</option>
              {/each}
            </select>
            <Icon name="chevron-down" size={16} />
          </div>
        </div>
      {/if}
    </div>

    <nav class="side-nav" aria-label={translate(locale, 'shell.nav.label')}>
      <ul>
        <li>
          <a
            href={resolve(`/app/${currentOrganizationId}`)}
            class="nav-item"
            aria-current={inOrgHome ? 'page' : undefined}
            ><Icon name="layout-grid" size={18} />{translate(locale, 'workspace.title')}</a
          >
        </li>
        {#if currentWorkspaceId}
          <li>
            <a
              class="nav-item"
              aria-current={inProjects ? 'page' : undefined}
              href={resolve(`/app/${currentOrganizationId}/${currentWorkspaceId}/projects`)}
              ><Icon name="folder" size={18} />{translate(locale, 'projects.title')}</a
            >
          </li>
        {/if}
      </ul>
    </nav>

    <div class="sidebar-footer">
      {#if user}
        <div class="user-block">
          <span class="avatar" aria-hidden="true">{initials}</span>
          <div class="user-meta">
            <span class="user-name">{user.display_name}</span>
            <span class="user-email">{user.email}</span>
          </div>
        </div>
      {/if}
      <button type="button" class="logout-button" disabled={!ready} onclick={signOut}>
        <Icon name="log-out" size={18} />{translate(locale, 'auth.logout')}
      </button>
    </div>

    <button
      type="button"
      class="icon-btn drawer-close"
      aria-label={translate(locale, 'ui.close')}
      onclick={() => closeNav(true)}
    >
      <Icon name="x" size={20} />
    </button>
  </aside>
  {#if navOpen}<button
      type="button"
      class="drawer-backdrop"
      aria-label={translate(locale, 'ui.close')}
      tabindex="-1"
      onclick={() => closeNav()}
    ></button>{/if}

  <div class="shell-body">
    <header class="topbar">
      <button
        bind:this={navToggle}
        type="button"
        class="nav-toggle"
        disabled={!ready}
        aria-expanded={navOpen}
        aria-controls="app-sidebar"
        aria-label={translate(locale, 'shell.nav.toggle')}
        onclick={() => (navOpen = !navOpen)}
      >
        <Icon name="menu" size={22} />
      </button>
      <div class="topbar-utils">
        <label class="sr-only" for="locale-select"
          >{translate(locale, 'shell.account.language')}</label
        >
        <div class="select-wrap">
          <select
            id="locale-select"
            disabled={!ready}
            value={locale}
            onchange={(e) => setLocale((e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="tr-TR">Türkçe</option>
            <option value="en">English</option>
          </select>
          <Icon name="chevron-down" size={16} />
        </div>
        <label class="sr-only" for="theme-select">{translate(locale, 'shell.account.theme')}</label>
        <div class="select-wrap">
          <select
            id="theme-select"
            disabled={!ready}
            value={theme}
            onchange={(e) => setTheme((e.currentTarget as HTMLSelectElement).value)}
          >
            <option value="light">{translate(locale, 'shell.theme.light')}</option>
            <option value="dark">{translate(locale, 'shell.theme.dark')}</option>
            <option value="system">{translate(locale, 'shell.theme.system')}</option>
          </select>
          <Icon name="chevron-down" size={16} />
        </div>
      </div>
    </header>

    <main id="main-content" class="app-content" tabindex="-1" aria-busy={!!navigating.to}>
      {#if navigating.to}<p class="navigation-loading" role="status">
          {translate(locale, 'ui.loading')}
        </p>{/if}
      {@render children()}
    </main>
  </div>
</div>
