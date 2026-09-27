<script lang="ts">
  // Account block at the bottom of the sidebar: deterministic initials avatar,
  // user identity, and the real logout action. Collapsed (icon) mode reduces
  // to the avatar; logout stays reachable via its icon button + tooltip.
  import { onMount } from 'svelte';
  import { translate } from '$lib/i18n';
  import LogOut from '@lucide/svelte/icons/log-out';
  import * as Sidebar from '$lib/components/ui/sidebar/index.js';

  let {
    locale,
    user,
  }: {
    locale: 'tr-TR' | 'en';
    user: { display_name: string; email: string } | null;
  } = $props();

  let ready = $state(false);
  onMount(() => {
    ready = true;
  });

  const initials = $derived(
    (user?.display_name ?? '')
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toLocaleUpperCase(locale) ?? '')
      .join(''),
  );

  async function signOut() {
    const { logoutRequest } = await import('$lib/api/client');
    try {
      await logoutRequest();
    } finally {
      window.location.href = '/login';
    }
  }
</script>

{#if user}
  <div class="sidebar-account">
    <span class="avatar" aria-hidden="true">{initials}</span>
    <div class="user-meta group-data-[collapsible=icon]:hidden">
      <span class="user-name">{user.display_name}</span>
      <span class="user-email">{user.email}</span>
    </div>
  </div>
{/if}
<Sidebar.Menu>
  <Sidebar.MenuItem>
    <Sidebar.MenuButton
      disabled={!ready}
      tooltipContent={translate(locale, 'auth.logout')}
      onclick={signOut}
    >
      <LogOut /><span>{translate(locale, 'auth.logout')}</span>
    </Sidebar.MenuButton>
  </Sidebar.MenuItem>
</Sidebar.Menu>
