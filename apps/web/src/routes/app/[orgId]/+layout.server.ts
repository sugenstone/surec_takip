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

export const load: LayoutServerLoad = async ({ params, locals, cookies, fetch }) => {
  if (!locals.user) redirect(307, '/login');
  const org = locals.organizations.find((o) => o.id === params.orgId);
  if (!org) redirect(307, '/app');

  const doFetch = backendFetch({ cookies, fetch });
  const [wsResponse, permResponse] = await Promise.all([
    doFetch(`/api/v1/organizations/${org.id}/workspaces`),
    doFetch(`/api/v1/organizations/${org.id}/effective-permissions`),
  ]);

  let workspaces: WorkspacePublic[] = [];
  if (wsResponse.ok) {
    workspaces = ((await wsResponse.json()) as { data?: WorkspacePublic[] }).data ?? [];
  }
  let permissions: string[] = [];
  if (permResponse.ok) {
    permissions =
      ((await permResponse.json()) as { data?: { permissions?: string[] } }).data?.permissions ??
      [];
  }

  return { organization: org, workspaces, permissions };
};
