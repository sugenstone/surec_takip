<script lang="ts">
  // Professional application shell (STEP 21D.5D): official shadcn-svelte
  // Sidebar (persistent collapsible desktop rail + mobile Sheet) wrapped in a
  // SidebarProvider, plus a slim topbar (sidebar trigger, locale/theme
  // utilities). SSR resolves the `sidebar_state` cookie so the initial render
  // matches the persisted collapse state without a hydration flip.
  import { onMount } from 'svelte';
  import { navigating } from '$app/state';
  import type { Theme } from '$lib/theme';
  import { translate } from '$lib/i18n';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import AppSidebar from './AppSidebar.svelte';

  let {
    locale,
    theme,
    organizations,
    currentOrganizationId,
    workspaces = [],
    currentWorkspaceId = null,
    user,
    sidebarOpen = true,
    children,
  }: {
    locale: 'tr-TR' | 'en';
    theme: Theme;
    organizations: { id: string; name: string; slug: string }[];
    currentOrganizationId: string;
    workspaces?: { id: string; name: string; slug: string; organization_id: string }[];
    currentWorkspaceId?: string | null;
    user: { display_name: string; email: string } | null;
    sidebarOpen?: boolean;
    children: import('svelte').Snippet;
  } = $props();

  let ready = $state(false);
  onMount(() => {
    ready = true;
  });

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

<Sidebar.Provider open={sidebarOpen}>
  <AppSidebar
    {locale}
    {organizations}
    {currentOrganizationId}
    {workspaces}
    {currentWorkspaceId}
    {user}
  />

  <div class="shell-body">
    <header class="topbar">
      <Sidebar.Trigger label={translate(locale, 'shell.nav.toggle')} />
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
          <ChevronDown size={16} />
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
          <ChevronDown size={16} />
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
</Sidebar.Provider>
