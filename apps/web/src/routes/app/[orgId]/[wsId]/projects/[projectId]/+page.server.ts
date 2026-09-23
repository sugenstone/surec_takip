import type { PageServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';
import type { ProjectPublic } from '$lib/api/client';

const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * URL-authoritative project detail (ADR 0011): the project resolves through
 * the backend against the URL org/workspace pair. Unknown, foreign, deleted
 * or wrong-parent projects all recover to the workspace projects list
 * without revealing whether the project exists elsewhere.
 */
export const load: PageServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workspace } = await parent();
  if (workspace.id !== params.wsId) redirect(307, `/app/${params.orgId}`);

  const token = cookies.get('platform_session');
  const response = await fetch(
    `${API_ORIGIN}/api/v1/organizations/${params.orgId}/workspaces/${params.wsId}/projects/${params.projectId}`,
    { headers: token ? { cookie: `platform_session=${token}` } : {} },
  );
  if (!response.ok) {
    redirect(307, `/app/${params.orgId}/${params.wsId}/projects`);
  }
  // The single-project endpoint returns the bare ProjectPublic object (same
  // shape as the single-workspace endpoint); only the list is enveloped.
  const project = (await response.json()) as ProjectPublic;
  if (!project?.id) {
    redirect(307, `/app/${params.orgId}/${params.wsId}/projects`);
  }
  return { project };
};
