<script lang="ts">
  // Application sidebar on the official shadcn-svelte Sidebar primitive
  // (STEP 21D.5D): brand, org/workspace context, primary navigation and the
  // account block. Desktop collapses to an icon rail; mobile renders through
  // the Sheet branch of Sidebar.Root. The URL remains the single source of
  // navigation truth — links only reflect routes.
  import { afterNavigate } from '$app/navigation';
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { translate } from '$lib/i18n';
  // Per-icon subpath imports: the @lucide/svelte barrel would pull the entire
  // icon set through dev-mode SSR transform.
  import Folder from '@lucide/svelte/icons/folder';
  import Layers from '@lucide/svelte/icons/layers';
  import LayoutGrid from '@lucide/svelte/icons/layout-grid';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';
  import SidebarAccount from './SidebarAccount.svelte';
  import SidebarContext from './SidebarContext.svelte';

  let {
    locale,
    organizations,
    currentOrganizationId,
    workspaces = [],
    currentWorkspaceId = null,
    user,
  }: {
    locale: 'tr-TR' | 'en';
    organizations: { id: string; name: string; slug: string }[];
    currentOrganizationId: string;
    workspaces?: { id: string; name: string; slug: string; organization_id: string }[];
    currentWorkspaceId?: string | null;
    user: { display_name: string; email: string } | null;
  } = $props();

  const sidebar = Sidebar.useSidebar();
  // Route navigation always closes the mobile sheet; desktop collapse state
  // is untouched.
  afterNavigate(() => sidebar.setOpenMobile(false));

  const inProjects = $derived(page.url.pathname.includes('/projects'));
  const inOrgHome = $derived(page.url.pathname === `/app/${currentOrganizationId}`);
</script>

<Sidebar.Root
  collapsible="icon"
  mobileTitle={translate(locale, 'shell.nav.mobileTitle')}
  mobileDescription={translate(locale, 'shell.nav.mobileDescription')}
  closeLabel={translate(locale, 'ui.close')}
>
  <Sidebar.Header>
    <a class="brand" href={resolve('/app')}>
      <span class="brand-mark"><Layers size={16} /></span>
      <span class="brand-name group-data-[collapsible=icon]:hidden"
        >{translate(locale, 'app.name')}</span
      >
    </a>
  </Sidebar.Header>

  <Sidebar.Content>
    <SidebarContext
      {locale}
      {organizations}
      {currentOrganizationId}
      {workspaces}
      {currentWorkspaceId}
    />

    <Sidebar.Group>
      <Sidebar.GroupContent>
        <nav aria-label={translate(locale, 'shell.nav.label')}>
          <Sidebar.Menu>
            <Sidebar.MenuItem>
              <Sidebar.MenuButton
                isActive={inOrgHome}
                tooltipContent={translate(locale, 'workspace.title')}
              >
                {#snippet child({ props })}
                  <a
                    href={resolve(`/app/${currentOrganizationId}`)}
                    aria-current={inOrgHome ? 'page' : undefined}
                    {...props}><LayoutGrid /><span>{translate(locale, 'workspace.title')}</span></a
                  >
                {/snippet}
              </Sidebar.MenuButton>
            </Sidebar.MenuItem>
            {#if currentWorkspaceId}
              <Sidebar.MenuItem>
                <Sidebar.MenuButton
                  isActive={inProjects}
                  tooltipContent={translate(locale, 'projects.title')}
                >
                  {#snippet child({ props })}
                    <a
                      href={resolve(`/app/${currentOrganizationId}/${currentWorkspaceId}/projects`)}
                      aria-current={inProjects ? 'page' : undefined}
                      {...props}><Folder /><span>{translate(locale, 'projects.title')}</span></a
                    >
                  {/snippet}
                </Sidebar.MenuButton>
              </Sidebar.MenuItem>
            {/if}
          </Sidebar.Menu>
        </nav>
      </Sidebar.GroupContent>
    </Sidebar.Group>
  </Sidebar.Content>

  <Sidebar.Footer>
    <SidebarAccount {locale} {user} />
  </Sidebar.Footer>
  <Sidebar.Rail label={translate(locale, 'shell.nav.rail')} />
</Sidebar.Root>
