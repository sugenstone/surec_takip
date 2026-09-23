import type { LayoutServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';
import type { WorkspacePublic } from '$lib/api/client';

const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

/** Server-side fetch that forwards the session cookie to the backend. */
function backendFetch(event: {
  cookies: { get(name: string): string | undefined };
  fetch: typeof fetch;
}) {
  const token = event.cookies.get('platform_session');
  return (path: string) =>
    event.fetch(`${API_ORIGIN}${path}`, {
      headers: token ? { cookie: `platform_session=${token}` } : {},
    });
}

/**
 * Workspace-scoped layout: validates that the workspace belongs to the URL
 * organization and that the user has access (the layout server load above
 * already proved org membership). Stale/wrong-parent wsIds redirect to the
 * org root for workspace selection.
 *
 * Workspace-level effective permissions are resolved through the backend so
 * workspace-scoped grants surface too; they drive action visibility only —
 * the backend remains the authorization authority (ADR 0011).
 */
export const load: LayoutServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workspaces } = await parent();
  const ws = workspaces.find((w) => w.id === params.wsId);
  if (!ws) redirect(307, `/app/${params.orgId}`);

  const doFetch = backendFetch({ cookies, fetch });
  const permResponse = await doFetch(
    `/api/v1/organizations/${params.orgId}/workspaces/${ws.id}/effective-permissions`,
  );
  let permissions: string[] = [];
  if (permResponse.ok) {
    permissions =
      ((await permResponse.json()) as { data?: { permissions?: string[] } }).data?.permissions ??
      [];
  }

  return { workspace: ws satisfies WorkspacePublic, permissions };
};
