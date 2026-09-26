import { describe, expect, it } from 'vitest';
import { ApiRequestError } from './client';
import {
  executionErrorMessageKey,
  loginErrorMessageKey,
  sessionErrorMessageKey,
  organizationErrorMessageKey,
  processErrorMessageKey,
  projectErrorMessageKey,
  sectionErrorMessageKey,
  workspaceErrorMessageKey,
  workItemErrorMessageKey,
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

describe('section error mapping by stable API code and field payload', () => {
  it('maps parent-field validation failures to the move/cycle message', () => {
    const parent = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-s1', {
      fields: { parent_section_id: ['Would create a cycle'] },
    });
    expect(sectionErrorMessageKey(parent)).toBe('sections.error.invalidParent');
  });

  it('splits slug, transition, name, permission and network failures', () => {
    const slug = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-s2', {
      fields: { slug: ['Already taken'] },
    });
    expect(sectionErrorMessageKey(slug)).toBe('sections.error.slugTaken');
    const transition = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-s3', {
      fields: { status: ['Invalid transition'] },
    });
    expect(sectionErrorMessageKey(transition)).toBe('sections.error.invalidTransition');
    const name = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-s4', {
      fields: { name: ['Required'] },
    });
    expect(sectionErrorMessageKey(name)).toBe('sections.error.invalidName');
    const forbidden = new ApiRequestError(403, 'PERMISSION_DENIED', 'msg', 'id-s5');
    expect(sectionErrorMessageKey(forbidden)).toBe('sections.error.forbidden');
    const network = new ApiRequestError(0, 'NETWORK_ERROR', 'msg', '');
    expect(sectionErrorMessageKey(network)).toBe('sections.error.network');
    expect(sectionErrorMessageKey(new Error('x'))).toBe('sections.error.unexpected');
  });
});

describe('workspace name validation', () => {
  it('shares the organization name rules', () => {
    expect(validWorkspaceName('Atölye')).toBe(true);
    expect(validWorkspaceName('   ')).toBe(false);
    expect(validWorkspaceName('a'.repeat(201))).toBe(false);
  });
});

describe('project error mapping by stable API code and field payload', () => {
  it('splits VALIDATION_ERROR into transition, slug and name messages', () => {
    const transition = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-7', {
      fields: { status: ['Invalid transition'] },
    });
    expect(projectErrorMessageKey(transition)).toBe('projects.error.invalidTransition');
    const slug = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-8', {
      fields: { slug: ['Already taken'] },
    });
    expect(projectErrorMessageKey(slug)).toBe('projects.error.slugTaken');
    const name = new ApiRequestError(422, 'VALIDATION_ERROR', 'msg', 'id-9', {
      fields: { name: ['Required'] },
    });
    expect(projectErrorMessageKey(name)).toBe('projects.error.invalidName');
  });

  it('maps permission, network and unknown failures without reading messages', () => {
    const forbidden = new ApiRequestError(403, 'PERMISSION_DENIED', 'msg', 'id-10');
    expect(projectErrorMessageKey(forbidden)).toBe('projects.error.forbidden');
    const network = new ApiRequestError(0, 'NETWORK_ERROR', 'msg', '');
    expect(projectErrorMessageKey(network)).toBe('projects.error.network');
    const other = new ApiRequestError(500, 'INTERNAL_ERROR', 'msg', 'id-11');
    expect(projectErrorMessageKey(other)).toBe('projects.error.unexpected');
    expect(projectErrorMessageKey(new Error('x'))).toBe('projects.error.unexpected');
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

describe('work item error mapping', () => {
  it('maps status codes without inspecting server messages', () => {
    for (const [code, key] of [
      ['PERMISSION_DENIED', 'forbidden'],
      ['RESOURCE_NOT_FOUND', 'notFound'],
      ['AUTH_REQUIRED', 'auth'],
      ['NETWORK_ERROR', 'network'],
      ['INTERNAL_ERROR', 'unexpected'],
    ]) {
      expect(
        workItemErrorMessageKey(new ApiRequestError(400, code, 'misleading slug message', '')),
      ).toBe(`workItems.error.${key}`);
    }
  });
  it('maps validation fields and preserves a generic fallback', () => {
    for (const field of ['slug', 'position', 'status']) {
      expect(
        workItemErrorMessageKey(
          new ApiRequestError(422, 'VALIDATION_ERROR', 'ignored', '', {
            fields: { [field]: ['ignored'] },
          }),
        ),
      ).toBe(`workItems.error.${field}`);
    }
    expect(workItemErrorMessageKey(new ApiRequestError(422, 'VALIDATION_ERROR', '', ''))).toBe(
      'workItems.error.validation',
    );
  });
});

describe('process error mapping', () => {
  it('maps status codes without inspecting server messages', () => {
    for (const [code, key] of [
      ['PERMISSION_DENIED', 'forbidden'],
      ['RESOURCE_NOT_FOUND', 'notFound'],
      ['AUTH_REQUIRED', 'auth'],
      ['NETWORK_ERROR', 'network'],
      ['INTERNAL_ERROR', 'unexpected'],
    ]) {
      expect(
        processErrorMessageKey(new ApiRequestError(400, code, 'misleading slug message', '')),
      ).toBe(`processes.error.${key}`);
    }
    expect(processErrorMessageKey(new Error('slug'))).toBe('processes.error.unexpected');
  });
  it('maps validation fields, including a stale reorder, with a generic fallback', () => {
    for (const [field, key] of [
      ['slug', 'slug'],
      ['description', 'description'],
      ['process_ids', 'order'],
      ['name', 'validation'],
    ]) {
      expect(
        processErrorMessageKey(
          new ApiRequestError(422, 'VALIDATION_ERROR', 'ignored', '', {
            fields: { [field]: ['ignored'] },
          }),
        ),
      ).toBe(`processes.error.${key}`);
    }
  });
});

describe('execution error mapping', () => {
  it('maps status codes without inspecting server messages', () => {
    for (const [code, key] of [
      ['VALIDATION_ERROR', 'validation'],
      ['STATE_CONFLICT', 'conflict'],
      ['PERMISSION_DENIED', 'forbidden'],
      ['RESOURCE_NOT_FOUND', 'notFound'],
      ['AUTH_REQUIRED', 'auth'],
      ['NETWORK_ERROR', 'network'],
      ['INTERNAL_ERROR', 'unexpected'],
    ]) {
      expect(
        executionErrorMessageKey(new ApiRequestError(409, code, 'misleading message', '')),
      ).toBe(`executions.error.${key}`);
    }
    expect(executionErrorMessageKey(new Error('x'))).toBe('executions.error.unexpected');
  });
});

describe('time session error mapping', () => {
  it('maps status codes without inspecting server messages', () => {
    for (const [code, key] of [
      ['ACTIVE_SESSION_EXISTS', 'activeElsewhere'],
      ['STATE_CONFLICT', 'conflict'],
      ['VALIDATION_ERROR', 'validation'],
      ['PERMISSION_DENIED', 'forbidden'],
      ['RESOURCE_NOT_FOUND', 'notFound'],
      ['NETWORK_ERROR', 'network'],
      ['INTERNAL_ERROR', 'unexpected'],
    ]) {
      expect(sessionErrorMessageKey(new ApiRequestError(409, code, 'misleading message', ''))).toBe(
        `sessions.error.${key}`,
      );
    }
    expect(sessionErrorMessageKey(new Error('x'))).toBe('sessions.error.unexpected');
  });
});
