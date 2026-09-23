import type { LayoutServerLoad } from './$types';
import { error, redirect } from '@sveltejs/kit';
import type { ProjectPublic, SectionPublic, WorkItemScope } from '$lib/api/client';
const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';
export const load: LayoutServerLoad = async ({ params, parent, cookies, fetch }) => {
  await parent();
  const token = cookies.get('platform_session');
  const headers: Record<string, string> = token ? { cookie: `platform_session=${token}` } : {};
  const base = `${API_ORIGIN}/api/v1/organizations/${params.orgId}/workspaces/${params.wsId}/projects/${params.projectId}`;
  const [p, s] = await Promise.all([
    fetch(base, { headers }),
    fetch(`${base}/sections/${params.sectionId}`, { headers }),
  ]);
  if (p.status === 401 || s.status === 401) redirect(307, '/login');
  if (p.status === 404 || s.status === 404) error(404);
  if (!p.ok || !s.ok) error(503);
  const project = (await p.json()) as ProjectPublic;
  const section = (await s.json()) as SectionPublic;
  if (project.status === 'archived' || section.status === 'archived') error(404);
  const workItemScope: WorkItemScope = {
    organizationId: params.orgId,
    workspaceId: params.wsId,
    projectId: params.projectId,
    sectionId: params.sectionId,
  };
  return { project, section, workItemScope };
};
