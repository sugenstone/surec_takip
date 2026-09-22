import type { OrganizationSummary, SessionUser } from '$lib/api/client';
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
    }
    interface PageData {
      locale: Locale;
      theme: Theme;
      user: SessionUser | null;
      organizations: OrganizationSummary[];
      currentOrganizationId: string | null;
    }
  }
}

export {};
