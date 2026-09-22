import { describe, expect, it } from 'vitest';
import { ApiRequestError } from './client';
import {
  loginErrorMessageKey,
  organizationErrorMessageKey,
  workspaceErrorMessageKey,
} from './errors';
import { validOrganizationName, validWorkspaceName } from '../org';

describe('login error mapping by stable API code', () => {
  it('maps credential, network and unknown codes to distinct translation keys', () => {
    const invalid = new ApiRequestError(401, 'AUTH_INVALID_CREDENTIALS', 'msg', 'id-1');
    expect(loginErrorMessageKey(invalid)).toBe('auth.error.invalidCredentials');
    const network = new ApiRequestError(0, 'NETWORK_ERROR', 'msg', '');
    expect(loginErrorMessageKey(network)).toBe('auth.error.network');
    const other = new ApiRequestError(500, 'INTERNAL_ERROR', 'msg', 'id-2');
    expect(loginErrorMessageKey(other)).toBe('auth.error.unexpected');
    expect(loginErrorMessageKey(new Error('boom'))).toBe('auth.error.unexpected');
  });

  it('never derives the key from the human-readable message', () => {
    const misleading = new ApiRequestError(
      401,
      'AUTH_INVALID_CREDENTIALS',
      'Email or password is incorrect.',
      'id-3',
    );
    expect(loginErrorMessageKey(misleading)).not.toContain('incorrect');
  });
});

describe('organization error mapping by stable API code', () => {
  it('maps validation, network and unknown codes to distinct translation keys', () => {
    const invalid = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-4');
    expect(organizationErrorMessageKey(invalid)).toBe('org.error.invalidName');
    const network = new ApiRequestError(0, 'NETWORK_ERROR', 'msg', '');
    expect(organizationErrorMessageKey(network)).toBe('org.error.network');
    expect(organizationErrorMessageKey(new Error('x'))).toBe('org.error.unexpected');
  });
});

describe('workspace error mapping by stable API code', () => {
  it('maps validation, network and unknown codes to distinct translation keys', () => {
    const invalid = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-5');
    expect(workspaceErrorMessageKey(invalid)).toBe('workspace.error.invalidName');
    const network = new ApiRequestError(0, 'NETWORK_ERROR', 'msg', '');
    expect(workspaceErrorMessageKey(network)).toBe('workspace.error.network');
    const forbidden = new ApiRequestError(403, 'PERMISSION_DENIED', 'msg', 'id-6');
    expect(workspaceErrorMessageKey(forbidden)).toBe('workspace.error.forbidden');
    expect(workspaceErrorMessageKey(new Error('x'))).toBe('workspace.error.unexpected');
  });
});

describe('workspace name validation', () => {
  it('shares the organization name rules', () => {
    expect(validWorkspaceName('Atölye')).toBe(true);
    expect(validWorkspaceName('   ')).toBe(false);
    expect(validWorkspaceName('a'.repeat(201))).toBe(false);
  });
});

describe('organization name validation', () => {
  it('accepts trimmed names of 1 to 200 characters', () => {
    expect(validOrganizationName('Acme')).toBe(true);
    expect(validOrganizationName('  Acme  ')).toBe(true);
    expect(validOrganizationName('a'.repeat(200))).toBe(true);
  });
  it('rejects empty, whitespace-only and oversized names', () => {
    expect(validOrganizationName('')).toBe(false);
    expect(validOrganizationName('   ')).toBe(false);
    expect(validOrganizationName('a'.repeat(201))).toBe(false);
  });
});
