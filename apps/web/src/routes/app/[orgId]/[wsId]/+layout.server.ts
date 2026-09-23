import type { LayoutServerLoad } from './$types';
import { redirect } from '@sveltejs/kit';

/**
 * Workspace-scoped layout: validates that the workspace belongs to the
 * URL organization and that the user has access (the layout server load
 * above already proved org membership). Stale/wrong-parent wsIds redirect
 * to the org root for workspace selection.
 */
export const load: LayoutServerLoad = ({ params, parent }) =>
  (async () => {
    const { workspaces } = await parent();
    const ws = workspaces.find((w) => w.id === params.wsId);
    if (!ws) redirect(307, `/app/${params.orgId}`);
    return { workspace: ws };
  })();
