import { describe, expect, it } from 'vitest';
import { ApiRequestError } from './client';
import { loginErrorMessageKey } from './errors';

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
