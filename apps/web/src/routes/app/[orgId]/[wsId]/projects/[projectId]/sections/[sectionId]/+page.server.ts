import type { PageServerLoad } from './$types';
import { error, redirect } from '@sveltejs/kit';
import { workItemsPath, type WorkItemPublic } from '$lib/api/client';
const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';
export const load: PageServerLoad = async ({ parent, cookies, fetch }) => {
  const { workItemScope, section, project } = await parent();
  // The work-items API intentionally 404s under an archived section or
  // project; the section page itself stays reachable so the section can be
  // reactivated, and simply does not list hidden items.
  const itemsHidden = section.status !== 'active' || project.status === 'archived';
  if (itemsHidden) return { items: [] as WorkItemPublic[], failed: false, itemsHidden };
  const token = cookies.get('platform_session');
  let items: WorkItemPublic[] = [];
  let failed = false;
  let response: Response;
  try {
    response = await fetch(`${API_ORIGIN}${workItemsPath(workItemScope)}`, {
      headers: token ? { cookie: `platform_session=${token}` } : {},
    });
  } catch {
    return { items, failed: true, itemsHidden };
  }
  if (response.status === 401) redirect(307, '/login');
  if (response.status === 404) error(404);
  if (response.ok) items = ((await response.json()) as { data: WorkItemPublic[] }).data;
  else failed = true;
  return { items, failed, itemsHidden };
};
