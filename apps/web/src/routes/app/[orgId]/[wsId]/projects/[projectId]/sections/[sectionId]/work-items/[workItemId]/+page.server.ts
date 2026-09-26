import type { PageServerLoad } from './$types';
import { error, redirect } from '@sveltejs/kit';
import {
  processesPath,
  workItemExecutionsPath,
  workItemSessionsPath,
  workItemsPath,
  type ExecutionPublic,
  type ProcessPublic,
  type ProcessScope,
  type TimeSessionPublic,
  type WorkItemPublic,
} from '$lib/api/client';
const API_ORIGIN = process.env.API_ORIGIN ?? 'http://127.0.0.1:8080';
export const load: PageServerLoad = async ({ params, parent, cookies, fetch }) => {
  const { workItemScope } = await parent();
  const token = cookies.get('platform_session');
  const headers: Record<string, string> = token ? { cookie: `platform_session=${token}` } : {};
  // The URL work item id is only a lookup key: both requests resolve through
  // the backend's full parent chain, which stays the authorization boundary.
  const processScope: ProcessScope = { ...workItemScope, workItemId: params.workItemId };
  const [response, processesResponse, executionsResponse, sessionsResponse] = await Promise.all([
    fetch(`${API_ORIGIN}${workItemsPath(workItemScope)}/${params.workItemId}`, { headers }),
    fetch(`${API_ORIGIN}${processesPath(processScope)}`, { headers }).catch(() => null),
    fetch(`${API_ORIGIN}${workItemExecutionsPath(processScope)}`, { headers }).catch(() => null),
    // Open labor sessions only; a failed fetch degrades to "no workers
    // shown", never blocks the page (same tolerance as executions).
    fetch(`${API_ORIGIN}${workItemSessionsPath(processScope)}`, { headers }).catch(() => null),
  ]);
  if (response.status === 401) redirect(307, '/login');
  if (response.status === 404) error(404);
  if (!response.ok) error(503);
  let processes: ProcessPublic[] = [];
  const processesFailed = !processesResponse?.ok;
  if (processesResponse?.ok) {
    processes = ((await processesResponse.json()) as { data: ProcessPublic[] }).data;
  }
  let executions: ExecutionPublic[] = [];
  let serverTime = '';
  if (executionsResponse?.ok) {
    const body = (await executionsResponse.json()) as {
      data: ExecutionPublic[];
      server_time: string;
    };
    executions = body.data;
    serverTime = body.server_time;
  }
  let openSessions: TimeSessionPublic[] = [];
  if (sessionsResponse?.ok) {
    openSessions = ((await sessionsResponse.json()) as { data: TimeSessionPublic[] }).data;
  }
  return {
    item: (await response.json()) as WorkItemPublic,
    processScope,
    processes,
    processesFailed,
    executions,
    serverTime,
    openSessions,
  };
};
