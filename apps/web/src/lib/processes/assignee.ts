import type { ProcessPublic } from '../api/client';

// Assignee presentation view-model (STEP 21C, ADR 0018). The backend is the
// single authority for eligibility: the flag passes through verbatim — the
// frontend derives only display flags, never re-evaluates membership.
export type AssigneeView = {
  assigned: boolean;
  /** Stale: assignee exists but no longer satisfies membership rules. */
  stale: boolean;
  name: string;
  /** Two-letter initials for the decorative avatar glyph. */
  initials: string;
};

export function assigneeView(process: ProcessPublic, locale: string): AssigneeView {
  const assignee = process.assignee;
  if (!assignee) {
    return { assigned: false, stale: false, name: '', initials: '' };
  }
  const initials = assignee.display_name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => word[0]?.toLocaleUpperCase(locale) ?? '')
    .join('');
  return {
    assigned: true,
    stale: !assignee.eligible,
    name: assignee.display_name,
    initials,
  };
}
