import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

// The root page requires an authenticated session; everything else redirects.
export const load: PageServerLoad = ({ locals }) => {
  if (!locals.user) redirect(307, '/login');
  return {};
};
