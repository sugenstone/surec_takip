import type { PageServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';
import type { ProjectPublic } from '$lib/api/client';

const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * URL-authoritative project list (ADR 0011): the parent layouts already
 * proved org and workspace access for the URL context; projects resolve
 * through the backend only. A failed fetch surfaces as an explicit error
 * state instead of a silently empty list.
 */
export const load: PageServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workspace } = await parent();
  if (workspace.id !== params.wsId) redirect(307, `/app/${params.orgId}`);

  const token = cookies.get('platform_session');
  const response = await fetch(
    `${API_ORIGIN}/api/v1/organizations/${params.orgId}/workspaces/${params.wsId}/projects`,
    { headers: token ? { cookie: `platform_session=${token}` } : {} },
  );
  if (!response.ok) {
    // Inaccessible/wrong-parent workspace: recover at the org root without
    // leaking why. Backend connectivity problems surface the same way as in
    // the shell layouts.
    if (response.status === 404) redirect(307, `/app/${params.orgId}`);
    return { projects: [] as ProjectPublic[], loadFailed: true };
  }
  const projects =
    ((await response.json()) as { data?: ProjectPublic[] }).data ?? ([] as ProjectPublic[]);
  return { projects, loadFailed: false };
};
