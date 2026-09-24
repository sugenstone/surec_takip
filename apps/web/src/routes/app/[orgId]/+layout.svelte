<script lang="ts">
  import { page } from '$app/state';
  import AppShell from '$lib/shell/AppShell.svelte';
  import type { LayoutProps } from './$types';

  let { data, children }: LayoutProps = $props();
</script>

<AppShell
  locale={data.locale}
  theme={data.theme}
  currentWorkspaceId={page.data.workspace?.id ?? null}
  organizations={data.organizations}
  currentOrganizationId={data.organization.id}
  workspaces={data.workspaces}
  user={data.user}
>
  {@render children()}
</AppShell>
