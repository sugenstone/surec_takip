// Clients branch on stable API error codes, never on human-readable messages.
// All payload types are derived from the generated OpenAPI contract — do not
// hand-duplicate response shapes here.
import type { components } from '@platform/contracts/schema';

export type ApiErrorCode =
  | 'AUTH_INVALID_CREDENTIALS'
  | 'AUTH_REQUIRED'
  | 'VALIDATION_ERROR'
  | 'SERVICE_NOT_READY'
  | 'RESOURCE_NOT_FOUND'
  | (string & {});

export interface ApiError {
  code: ApiErrorCode;
  message: string;
  details: unknown;
  requestId: string;
}

export class ApiRequestError extends Error {
  readonly code: ApiErrorCode;
  readonly status: number;
  readonly requestId: string;
  // Structured error details (e.g. VALIDATION_ERROR field reports); never a
  // message string to branch on.
  readonly details: unknown;

  constructor(
    status: number,
    code: ApiErrorCode,
    message: string,
    requestId: string,
    details: unknown = null,
  ) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
    this.requestId = requestId;
    this.details = details;
  }
}

// Throws ApiRequestError for structured failures; returns parsed data for 2xx.
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(path, init);
  } catch {
    throw new ApiRequestError(0, 'NETWORK_ERROR', 'network', '');
  }
  if (response.ok) {
    return (await response.json()) as T;
  }
  let code: ApiErrorCode = 'UNEXPECTED_ERROR';
  let message = '';
  let requestId = '';
  let details: unknown = null;
  try {
    const body = (await response.json()) as { error?: ApiError };
    if (body.error) {
      code = body.error.code;
      message = body.error.message;
      requestId = body.error.requestId;
      details = body.error.details;
    }
  } catch {
    // Non-JSON error bodies keep the fallback code.
  }
  throw new ApiRequestError(response.status, code, message, requestId, details);
}

export type SessionUser = components['schemas']['UserPublic'];
export type OrganizationSummary = components['schemas']['OrganizationSummary'];
export type OrganizationPublic = components['schemas']['OrganizationPublic'];
export type WorkspacePublic = components['schemas']['WorkspacePublic'];
export type ProjectPublic = components['schemas']['ProjectPublic'];
export type ProjectStatus = ProjectPublic['status'];
export type SectionPublic = components['schemas']['SectionPublic'];
export type SectionStatus = SectionPublic['status'];
export type MeData = components['schemas']['MeData'];
// Derived progress (ADR 0017): same shape on project/section/work-item.
export type Progress = components['schemas']['Progress'];

export async function loginRequest(email: string, password: string): Promise<SessionUser> {
  const body = await request<{ data: { user: SessionUser } }>('/api/v1/auth/login', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ email, password }),
  });
  return body.data.user;
}

export async function logoutRequest(): Promise<void> {
  await request<{ data: unknown }>('/api/v1/auth/logout', { method: 'POST' });
}

export async function createOrganization(name: string): Promise<OrganizationPublic> {
  const body = await request<{
    data: { organization: OrganizationPublic };
  }>('/api/v1/organizations', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  return body.data.organization;
}

export async function listWorkspaces(organizationId: string): Promise<WorkspacePublic[]> {
  const body = await request<{ data: WorkspacePublic[] }>(
    `/api/v1/organizations/${organizationId}/workspaces`,
  );
  return body.data;
}

export async function createWorkspace(
  organizationId: string,
  name: string,
): Promise<WorkspacePublic> {
  const body = await request<{
    data: { workspace: WorkspacePublic };
  }>(`/api/v1/organizations/${organizationId}/workspaces`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  return body.data.workspace;
}

export async function listProjects(
  organizationId: string,
  workspaceId: string,
): Promise<ProjectPublic[]> {
  const body = await request<{ data: ProjectPublic[] }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects`,
  );
  return body.data;
}

export async function createProject(
  organizationId: string,
  workspaceId: string,
  input: { name: string; description?: string },
): Promise<ProjectPublic> {
  const body = await request<{ data: ProjectPublic }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    },
  );
  return body.data;
}

export async function updateProject(
  organizationId: string,
  workspaceId: string,
  projectId: string,
  input: { name?: string; description?: string; status?: ProjectStatus },
): Promise<ProjectPublic> {
  const body = await request<{ data: ProjectPublic }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects/${projectId}`,
    {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    },
  );
  return body.data;
}

export async function listSections(
  organizationId: string,
  workspaceId: string,
  projectId: string,
): Promise<SectionPublic[]> {
  const body = await request<{ data: SectionPublic[] }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects/${projectId}/sections`,
  );
  return body.data;
}

export async function createSection(
  organizationId: string,
  workspaceId: string,
  projectId: string,
  input: { name: string; parentSectionId?: string | null },
): Promise<SectionPublic> {
  const body = await request<{ data: SectionPublic }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects/${projectId}/sections`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: input.name, parent_section_id: input.parentSectionId ?? null }),
    },
  );
  return body.data;
}

export async function updateSection(
  organizationId: string,
  workspaceId: string,
  projectId: string,
  sectionId: string,
  input: {
    name?: string;
    parentSectionId?: string | null;
    position?: number;
    status?: SectionStatus;
  },
): Promise<SectionPublic> {
  const patch: Record<string, unknown> = {};
  if (input.name !== undefined) patch.name = input.name;
  if (input.parentSectionId !== undefined) patch.parent_section_id = input.parentSectionId;
  if (input.position !== undefined) patch.position = input.position;
  if (input.status !== undefined) patch.status = input.status;
  const body = await request<{ data: SectionPublic }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/projects/${projectId}/sections/${sectionId}`,
    {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(patch),
    },
  );
  return body.data;
}

export type WorkItemPublic = components['schemas']['WorkItemPublic'];
export type WorkItemScope = {
  organizationId: string;
  workspaceId: string;
  projectId: string;
  sectionId: string;
};
export function workItemsPath(scope: WorkItemScope): string {
  return `/api/v1/organizations/${scope.organizationId}/workspaces/${scope.workspaceId}/projects/${scope.projectId}/sections/${scope.sectionId}/work-items`;
}
export async function createWorkItem(
  scope: WorkItemScope,
  input: components['schemas']['CreateWorkItemRequest'],
): Promise<WorkItemPublic> {
  const body = await request<{ data: WorkItemPublic }>(workItemsPath(scope), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
  return body.data;
}
export async function updateWorkItem(
  scope: WorkItemScope,
  id: string,
  input: components['schemas']['UpdateWorkItemRequest'],
): Promise<WorkItemPublic> {
  const body = await request<{ data: WorkItemPublic }>(`${workItemsPath(scope)}/${id}`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
  return body.data;
}

// Process DEFINITIONS (ADR 0015): configuration only, no execution state.
export type ProcessPublic = components['schemas']['ProcessPublic'];
export type ProcessScope = WorkItemScope & { workItemId: string };
export function processesPath(scope: ProcessScope): string {
  return `${workItemsPath(scope)}/${scope.workItemId}/processes`;
}
export async function createProcess(
  scope: ProcessScope,
  input: components['schemas']['CreateProcessRequest'],
): Promise<ProcessPublic> {
  const body = await request<{ data: ProcessPublic }>(processesPath(scope), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
  return body.data;
}
export async function updateProcess(
  scope: ProcessScope,
  id: string,
  input: components['schemas']['UpdateProcessRequest'],
): Promise<ProcessPublic> {
  const body = await request<{ data: ProcessPublic }>(`${processesPath(scope)}/${id}`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
  return body.data;
}
// Assignment (STEP 21C / ADR 0018): dedicated command — responsibility is a
// separate permission boundary from processes:update. `user_id: null`
// unassigns; execution snapshots are server-written at start time.
export async function updateProcessAssignment(
  scope: ExecutionScope,
  input: components['schemas']['UpdateAssignmentRequest'],
): Promise<ProcessPublic> {
  const body = await request<{ data: ProcessPublic }>(
    `${processesPath(scope)}/${scope.processId}/assignment`,
    {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    },
  );
  return body.data;
}

// Eligible workspace members for the assignee picker (id + display name
// only). Read requires ordinary workspace access, not processes:assign.
export type WorkspaceMemberPublic = components['schemas']['WorkspaceMemberPublic'];
export async function listWorkspaceMembers(
  organizationId: string,
  workspaceId: string,
): Promise<WorkspaceMemberPublic[]> {
  const body = await request<{ data: WorkspaceMemberPublic[] }>(
    `/api/v1/organizations/${organizationId}/workspaces/${workspaceId}/members`,
  );
  return body.data;
}

// The server validates that the ids are exactly the active processes.
export async function reorderProcesses(
  scope: ProcessScope,
  processIds: string[],
): Promise<ProcessPublic[]> {
  const body = await request<{ data: ProcessPublic[] }>(`${processesPath(scope)}/reorder`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ process_ids: processIds }),
  });
  return body.data;
}

// Process EXECUTIONS (STEP 21A / ADR 0016): immutable operational attempts.
// Timestamps and `server_time` come from the database clock; the client never
// sends times, actor ids, scope ids, attempt numbers, or status.
export type ExecutionPublic = components['schemas']['ExecutionPublic'];
export type ExecutionScope = ProcessScope & { processId: string };
export function workItemExecutionsPath(scope: ProcessScope): string {
  return `${workItemsPath(scope)}/${scope.workItemId}/executions`;
}
export function executionsPath(scope: ExecutionScope): string {
  return `${processesPath(scope)}/${scope.processId}/executions`;
}
export async function listWorkItemExecutions(
  scope: ProcessScope,
): Promise<{ data: ExecutionPublic[]; server_time: string }> {
  return request<{ data: ExecutionPublic[]; server_time: string }>(workItemExecutionsPath(scope));
}
export async function startExecution(
  scope: ExecutionScope,
  input: components['schemas']['StartExecutionRequest'] = {},
): Promise<{ data: ExecutionPublic; server_time: string }> {
  return request<{ data: ExecutionPublic; server_time: string }>(executionsPath(scope), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(input),
  });
}
export async function completeExecution(
  scope: ExecutionScope,
  executionId: string,
): Promise<{ data: ExecutionPublic; server_time: string }> {
  return request<{ data: ExecutionPublic; server_time: string }>(
    `${executionsPath(scope)}/${executionId}/complete`,
    { method: 'POST' },
  );
}
export async function cancelExecution(
  scope: ExecutionScope,
  executionId: string,
  input: components['schemas']['CancelExecutionRequest'] = {},
): Promise<{ data: ExecutionPublic; server_time: string }> {
  return request<{ data: ExecutionPublic; server_time: string }>(
    `${executionsPath(scope)}/${executionId}/cancel`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(input),
    },
  );
}

// Time SESSIONS (STEP 21D / ADR 0019): tracked labor intervals on an
// execution. V1 is self-service — the worker is always the authenticated
// user, so the client sends no worker or timestamps; `server_time` keeps
// the same clock-anchor contract as executions.
export type TimeSessionPublic = components['schemas']['TimeSessionPublic'];
export function executionSessionsPath(scope: ExecutionScope, executionId: string): string {
  return `${executionsPath(scope)}/${executionId}/time-sessions`;
}
export function workItemSessionsPath(scope: ProcessScope): string {
  return `${workItemsPath(scope)}/${scope.workItemId}/time-sessions`;
}
export async function listWorkItemOpenSessions(
  scope: ProcessScope,
): Promise<{ data: TimeSessionPublic[]; server_time: string }> {
  return request<{ data: TimeSessionPublic[]; server_time: string }>(workItemSessionsPath(scope));
}
export async function startTimeSession(
  scope: ExecutionScope,
  executionId: string,
): Promise<{ data: TimeSessionPublic; server_time: string }> {
  return request<{ data: TimeSessionPublic; server_time: string }>(
    executionSessionsPath(scope, executionId),
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({}),
    },
  );
}
export async function stopTimeSession(
  scope: ExecutionScope,
  executionId: string,
  sessionId: string,
): Promise<{ data: TimeSessionPublic; server_time: string }> {
  return request<{ data: TimeSessionPublic; server_time: string }>(
    `${executionSessionsPath(scope, executionId)}/${sessionId}/stop`,
    { method: 'POST' },
  );
}
