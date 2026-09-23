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
