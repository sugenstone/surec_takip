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

  constructor(status: number, code: ApiErrorCode, message: string, requestId: string) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
    this.code = code;
    this.requestId = requestId;
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
  try {
    const body = (await response.json()) as { error?: ApiError };
    if (body.error) {
      code = body.error.code;
      message = body.error.message;
      requestId = body.error.requestId;
    }
  } catch {
    // Non-JSON error bodies keep the fallback code.
  }
  throw new ApiRequestError(response.status, code, message, requestId);
}

export type SessionUser = components['schemas']['UserPublic'];
export type OrganizationSummary = components['schemas']['OrganizationSummary'];
export type OrganizationPublic = components['schemas']['OrganizationPublic'];
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
