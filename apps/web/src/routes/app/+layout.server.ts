import type { LayoutServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';

/**
 * Authenticated shell layout: guards authentication only. Organization
 * selection/redirect happens in the /app page, not the layout, so nested
 * routes (/app/[orgId]/[wsId]) are not redirected by the parent.
 */
export const load: LayoutServerLoad = ({ locals }) => {
  if (!locals.user) redirect(307, '/login');
  return { user: locals.user };
};
