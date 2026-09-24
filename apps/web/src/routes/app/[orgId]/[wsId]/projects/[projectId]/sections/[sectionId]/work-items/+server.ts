import { redirect } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

// The work item list moved onto the section page (drill-down navigation,
// ADR 0014). The old collection URL stays a stable redirect so existing deep
// links, reloads and shared bookmarks keep their exact hierarchy context.
export const GET: RequestHandler = ({ params }) => {
  redirect(
    307,
    `/app/${params.orgId}/${params.wsId}/projects/${params.projectId}/sections/${params.sectionId}`,
  );
};
