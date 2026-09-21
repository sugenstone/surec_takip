import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

// Already-authenticated users skip the login screen.
export const load: PageServerLoad = ({ locals }) => {
  if (locals.user) redirect(307, '/');
  return {};
};
