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

// Project mutations distinguish field-level validation failures (slug/name,
// invalid transition) from permission and connectivity problems using the
// stable VALIDATION_ERROR code plus the details.fields payload.
export function projectErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR': {
        const fields = (error.details as { fields?: Record<string, unknown> } | undefined)?.fields;
        if (fields && 'status' in fields) return 'projects.error.invalidTransition';
        if (fields && 'slug' in fields) return 'projects.error.slugTaken';
        return 'projects.error.invalidName';
      }
      case 'PERMISSION_DENIED':
        return 'projects.error.forbidden';
      case 'NETWORK_ERROR':
        return 'projects.error.network';
      default:
        return 'projects.error.unexpected';
    }
  }
  return 'projects.error.unexpected';
}

// Section mutations: the parent field covers invalid parents AND cycle
// attempts — both share one honest localized message (details values are
// backend strings we deliberately never branch on).
export function sectionErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR': {
        const fields = (error.details as { fields?: Record<string, unknown> } | undefined)?.fields;
        if (fields && 'parent_section_id' in fields) return 'sections.error.invalidParent';
        if (fields && 'status' in fields) return 'sections.error.invalidTransition';
        if (fields && 'slug' in fields) return 'sections.error.slugTaken';
        return 'sections.error.invalidName';
      }
      case 'PERMISSION_DENIED':
        return 'sections.error.forbidden';
      case 'NETWORK_ERROR':
        return 'sections.error.network';
      default:
        return 'sections.error.unexpected';
    }
  }
  return 'sections.error.unexpected';
}

export function workItemErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR': {
        const fields = (error.details as { fields?: Record<string, unknown> } | undefined)?.fields;
        if (fields && 'slug' in fields) return 'workItems.error.slug';
        if (fields && 'position' in fields) return 'workItems.error.position';
        if (fields && 'status' in fields) return 'workItems.error.status';
        return 'workItems.error.validation';
      }
      case 'PERMISSION_DENIED':
        return 'workItems.error.forbidden';
      case 'RESOURCE_NOT_FOUND':
        return 'workItems.error.notFound';
      case 'AUTH_REQUIRED':
        return 'workItems.error.auth';
      case 'NETWORK_ERROR':
        return 'workItems.error.network';
    }
  }
  return 'workItems.error.unexpected';
}

// A stale reorder (someone changed the list meanwhile) surfaces as a
// process_ids field error: ask for a refresh instead of a generic failure.
export function processErrorMessageKey(error: unknown): TranslationKey {
  if (error instanceof ApiRequestError) {
    switch (error.code) {
      case 'VALIDATION_ERROR': {
        const fields = (error.details as { fields?: Record<string, unknown> } | undefined)?.fields;
        if (fields && 'slug' in fields) return 'processes.error.slug';
        if (fields && 'description' in fields) return 'processes.error.description';
        if (fields && 'process_ids' in fields) return 'processes.error.order';
        return 'processes.error.validation';
      }
      case 'PERMISSION_DENIED':
        return 'processes.error.forbidden';
      case 'RESOURCE_NOT_FOUND':
        return 'processes.error.notFound';
      case 'AUTH_REQUIRED':
        return 'processes.error.auth';
      case 'NETWORK_ERROR':
        return 'processes.error.network';
    }
  }
  return 'processes.error.unexpected';
}
