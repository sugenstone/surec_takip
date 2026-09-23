import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

// Authenticated root redirects into the app shell; unauthenticated to login.
// The shell handles organization/workspace resolution (ADR 0010).
export const load: PageServerLoad = ({ locals }) => {
  if (!locals.user) redirect(307, '/login');
  redirect(307, '/app');
};
