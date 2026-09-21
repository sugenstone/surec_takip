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
