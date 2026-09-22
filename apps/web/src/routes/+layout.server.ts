import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ locals }) => ({
  locale: locals.locale,
  theme: locals.theme,
  user: locals.user,
  organizations: locals.organizations,
  currentOrganizationId: locals.currentOrganizationId,
});
