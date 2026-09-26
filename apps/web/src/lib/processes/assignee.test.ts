import { describe, expect, it } from 'vitest';
import { assigneeView } from './assignee';
import { translate } from '../i18n';
import type { ProcessPublic } from '../api/client';

// Presentation contract (ADR 0018): eligibility is a server-computed flag the
// frontend passes through — stale display is derived, never recomputed.
function process(assignee: ProcessPublic['assignee']): ProcessPublic {
  return {
    id: 'proc',
    organization_id: 'org',
    workspace_id: 'ws',
    project_id: 'prj',
    section_id: 'sec',
    work_item_id: 'wi',
    name: 'Montaj',
    slug: 'montaj',
    description: null,
    position: 0,
    is_required: true,
    status: 'active',
    assignee,
  } as ProcessPublic;
}

describe('assigneeView', () => {
  it('unassigned process reports the empty state', () => {
    const view = assigneeView(process(null), 'tr-TR');
    expect(view.assigned).toBe(false);
    expect(view.stale).toBe(false);
    expect(view.name).toBe('');
  });

  it('eligible assignee shows the name and initials', () => {
    const view = assigneeView(
      process({ id: 'u1', display_name: 'Harun Usta', eligible: true }),
      'tr-TR',
    );
    expect(view.assigned).toBe(true);
    expect(view.stale).toBe(false);
    expect(view.name).toBe('Harun Usta');
    expect(view.initials).toBe('HU');
  });

  it('stale assignee keeps the name but flags the warning', () => {
    const view = assigneeView(
      process({ id: 'u1', display_name: 'Harun Usta', eligible: false }),
      'en',
    );
    expect(view.assigned).toBe(true);
    expect(view.stale).toBe(true);
    expect(view.name).toBe('Harun Usta');
  });

  it('initials use at most the first two words, locale-cased', () => {
    const view = assigneeView(process({ id: 'u1', display_name: 'a b c d', eligible: true }), 'en');
    expect(view.initials).toBe('AB');
  });
});

describe('assignee translations', () => {
  it('unassigned + stale labels exist in both locales', () => {
    expect(translate('tr-TR', 'processes.assignee.unassigned')).toBe('Atanmamış');
    expect(translate('en', 'processes.assignee.unassigned')).toBe('Unassigned');
    expect(translate('tr-TR', 'processes.assignee.stale')).toBe('Artık üye değil');
    expect(translate('en', 'processes.assignee.stale')).toBe('No longer a member');
    expect(translate('tr-TR', 'processes.assignee.change')).toBe('Sorumlu değiştir');
  });
});
