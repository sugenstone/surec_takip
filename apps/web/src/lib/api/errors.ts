import type { TranslationKey } from '$lib/i18n';
import { ApiRequestError } from './client';

// Maps stable API error codes to translation keys. Never parse messages.
export function loginErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'AUTH_INVALID_CREDENTIALS':
        return 'auth.error.invalidCredentials';
      case 'NETWORK_ERROR':
        return 'auth.error.network';
      default:
        return 'auth.error.unexpected';
    }
  }
  return 'auth.error.unexpected';
}

export function organizationErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR':
        return 'org.error.invalidName';
      case 'NETWORK_ERROR':
        return 'org.error.network';
      default:
        return 'org.error.unexpected';
    }
  }
  return 'org.error.unexpected';
}

export function workspaceErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR':
        return 'workspace.error.invalidName';
      case 'PERMISSION_DENIED':
        return 'workspace.error.forbidden';
      case 'NETWORK_ERROR':
        return 'workspace.error.network';
      default:
        return 'workspace.error.unexpected';
    }
  }
  return 'workspace.error.unexpected';
}
