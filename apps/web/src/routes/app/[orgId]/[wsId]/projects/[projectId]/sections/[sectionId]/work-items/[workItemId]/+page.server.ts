import type { PageServerLoad } from './$types';
import { error, redirect } from '@sveltejs/kit';
import { workItemsPath, type WorkItemPublic } from '$lib/api/client';
const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';
export const load: PageServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workItemScope } = await parent();
  const token = cookies.get('platform_session');
  const response = await fetch(
    `${API_ORIGIN}${workItemsPath(workItemScope)}/${params.workItemId}`,
    { headers: token ? { cookie: `platform_session=${token}` } : {} },
  );
  if (response.status === 401) redirect(307, '/login');
  if (response.status === 404) error(404);
  if (!response.ok) error(503);
  return { item: (await response.json()) as WorkItemPublic };
};
