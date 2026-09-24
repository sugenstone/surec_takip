import type { LayoutServerLoad } from './$types';
import { error, redirect } from '@sveltejs/kit';
import { sectionTrail } from '$lib/sections/tree';
import type { ProjectPublic, SectionPublic, WorkItemScope } from '$lib/api/client';
const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';

/**
 * URL-authoritative section context (ADR 0012/0014): every hierarchy level is
 * its own page. The project, the current section and the flat section list
 * (for the ancestor trail and direct-child filtering) resolve through the
 * backend against the URL org/workspace pair. Archived sections still resolve
 * — their page renders read-only so reactivation stays reachable.
 */
export const load: LayoutServerLoad = async ({ params, parent, cookies, fetch }) => {
  await parent();
  const token = cookies.get('platform_session');
  const headers: Record<string, string> = token ? { cookie: `platform_session=${token}` } : {};
  const base = `${API_ORIGIN}/api/v1/organizations/${params.orgId}/workspaces/${params.wsId}/projects/${params.projectId}`;
  const [p, s, tree] = await Promise.all([
    fetch(base, { headers }),
    fetch(`${base}/sections/${params.sectionId}`, { headers }),
    fetch(`${base}/sections`, { headers }),
  ]);
  if (p.status === 401 || s.status === 401) redirect(307, '/login');
  if (p.status === 404 || s.status === 404) error(404);
  if (!p.ok || !s.ok) error(503);
  const project = (await p.json()) as ProjectPublic;
  const section = (await s.json()) as SectionPublic;
  const workItemScope: WorkItemScope = {
    organizationId: params.orgId,
    workspaceId: params.wsId,
    projectId: params.projectId,
    sectionId: params.sectionId,
  };
  const sections = tree.ok ? ((await tree.json()) as { data: SectionPublic[] }).data : [];
  const trail = sectionTrail(sections, section.id);
  return {
    project,
    section,
    sections,
    workItemScope,
    sectionTrail: trail.length ? trail : [section],
  };
};
