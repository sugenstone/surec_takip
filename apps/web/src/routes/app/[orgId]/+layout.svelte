<script lang="ts">
  import AppShell from '$lib/shell/AppShell.svelte';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
</script>

<AppShell
  locale={data.locale}
  organizations={data.organizations}
  currentOrganizationId={data.organization.id}
  workspaces={data.workspaces}
  permissions={data.permissions}
  user={data.user}
>
  {@render children()}
</AppShell>
