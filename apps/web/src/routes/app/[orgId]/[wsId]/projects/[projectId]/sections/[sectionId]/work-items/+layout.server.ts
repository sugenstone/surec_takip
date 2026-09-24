import type { LayoutServerLoad } from './$types';
import { error } from '@sveltejs/kit';

// Work item detail URLs keep their existing guard: the work-items API hides
// items under archived parents, so an archived section or project makes the
// detail uniformly not-found. The section page itself stays reachable.
export const load: LayoutServerLoad = async ({ parent }) => {
  const { project, section } = await parent();
  if (project.status === 'archived' || section.status === 'archived') error(404);
  return {};
};
