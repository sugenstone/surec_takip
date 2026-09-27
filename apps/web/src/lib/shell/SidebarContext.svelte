<script lang="ts">
  // Organization/workspace context controls inside the sidebar. Selection
  // writes a presentation-only cookie then performs a hard navigation so the
  // server re-resolves the permitted context (same semantics as the legacy
  // shell — the cookie is never an authorization input).
  import { onMount } from 'svelte';
  import { translate } from '$lib/i18n';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';

  let {
    locale,
    organizations,
    currentOrganizationId,
    workspaces = [],
    currentWorkspaceId = null,
  }: {
    locale: 'tr-TR' | 'en';
    organizations: { id: string; name: string; slug: string }[];
    currentOrganizationId: string;
    workspaces?: { id: string; name: string; slug: string; organization_id: string }[];
    currentWorkspaceId?: string | null;
  } = $props();

  let ready = $state(false);
  onMount(() => {
    ready = true;
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
</script>

<Sidebar.Group class="group-data-[collapsible=icon]:hidden py-1">
  <Sidebar.GroupLabel>{translate(locale, 'shell.context')}</Sidebar.GroupLabel>
  <Sidebar.GroupContent class="context-fields">
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
        <ChevronDown size={16} />
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
          <ChevronDown size={16} />
        </div>
      </div>
    {/if}
  </Sidebar.GroupContent>
</Sidebar.Group>
