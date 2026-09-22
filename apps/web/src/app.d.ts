import type { OrganizationSummary, SessionUser, WorkspacePublic } from '$lib/api/client';
import type { Locale } from '$lib/i18n';
import type { Theme } from '$lib/theme';

declare global {
  namespace App {
    interface Locals {
      locale: Locale;
      theme: Theme;
      user: SessionUser | null;
      organizations: OrganizationSummary[];
      // Presentation-only org context (cookie); never an authorization input.
      currentOrganizationId: string | null;
      // Permitted workspaces of the current organization (server-validated).
      workspaces: WorkspacePublic[];
      // Presentation-only workspace context (cookie), resolved against the
      // permitted list above; never an authorization input.
      currentWorkspaceId: string | null;
    }
    interface PageData {
      locale: Locale;
      theme: Theme;
      user: SessionUser | null;
      organizations: OrganizationSummary[];
      currentOrganizationId: string | null;
      workspaces: WorkspacePublic[];
      currentWorkspaceId: string | null;
    }
  }
}

export {};
