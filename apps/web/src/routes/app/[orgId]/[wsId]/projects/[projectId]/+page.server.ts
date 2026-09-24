import type { PageServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';
import type { ProjectPublic, SectionPublic } from '$lib/api/client';

const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * URL-authoritative project detail (ADR 0011/0012): the project and its
 * sections resolve through the backend against the URL org/workspace pair.
 * Unknown, foreign, deleted or wrong-parent resources all recover safely
 * without revealing whether anything exists elsewhere. Sections arrive as
 * ONE flat ordered list (query count 1, no N+1).
 */
export const load: PageServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workspace } = await parent();
  if (workspace.id !== params.wsId) redirect(307, `/app/${params.orgId}`);

  const token = cookies.get('platform_session');
  const headers: Record<string, string> = {};
  if (token) {
    headers.cookie = `platform_session=${token}`;
  }
  const base = `${API_ORIGIN}/api/v1/organizations/${params.orgId}/workspaces/${params.wsId}/projects/${params.projectId}`;
  const [projectResponse, sectionsResponse] = await Promise.all([
    fetch(base, { headers }),
    fetch(`${base}/sections`, { headers }),
  ]);
  if (!projectResponse.ok) {
    redirect(307, `/app/${params.orgId}/${params.wsId}/projects`);
  }
  // The single-project endpoint returns the bare ProjectPublic object (same
  // shape as the single-workspace endpoint); only lists are enveloped.
  const project = (await projectResponse.json()) as ProjectPublic;
  if (!project?.id) {
    redirect(307, `/app/${params.orgId}/${params.wsId}/projects`);
  }
  let sections: SectionPublic[] = [];
  let sectionsFailed = false;
  if (sectionsResponse.ok) {
    sections = ((await sectionsResponse.json()) as { data?: SectionPublic[] }).data ?? [];
  } else if (sectionsResponse.status !== 404) {
    // A 404 cannot happen here (the project resolved); connectivity issues
    // surface as an explicit error state instead of a silently empty list.
    sectionsFailed = true;
  }
  return { project, sections, sectionsFailed };
};
