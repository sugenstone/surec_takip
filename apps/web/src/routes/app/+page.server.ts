import type { PageServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';

/**
 * /app root page: resolves the default organization. When organizations
 * exist, redirects to the first (or remembered) org page. When none exist,
 * renders the no-organization onboarding state.
 */
export const load: PageServerLoad = ({ locals }) => {
  if (locals.organizations.length === 0) {
    return { mode: 'no-organizations' as const };
  }
  const selected =
    locals.organizations.find((org) => org.id === locals.currentOrganizationId) ??
    locals.organizations[0];
  redirect(307, `/app/${selected.id}`);
};
