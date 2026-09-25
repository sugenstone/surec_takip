import { describe, expect, it } from 'vitest';
import { childCounts, directChildren, movableParents, sectionTrail } from './tree';
import type { SectionPublic } from '$lib/api/client';

function section(partial: Partial<SectionPublic> & { id: string }): SectionPublic {
  return {
    organization_id: 'org',
    workspace_id: 'ws',
    project_id: 'prj',
    parent_section_id: null,
    name: partial.id,
    slug: partial.id,
    position: 0,
    status: 'active',
    progress: { completed: 0, active: 0, total: 0 },
    ...partial,
  };
}

describe('drill-down child selection', () => {
  const flat = [
    section({ id: 'floor-1', name: 'Kat 1', position: 1 }),
    section({ id: 'floor-0', name: 'Kat 0', position: 0 }),
    section({ id: 'apt-1', name: 'Daire 1', parent_section_id: 'floor-1', position: 1 }),
    section({ id: 'apt-0', name: 'Daire 0', parent_section_id: 'floor-1', position: 0 }),
    section({ id: 'kitchen', name: 'Mutfak', parent_section_id: 'apt-0' }),
  ];

  it('exposes only root sections at project level, ordered by (position, id)', () => {
    expect(directChildren(flat, null).map((s) => s.id)).toEqual(['floor-0', 'floor-1']);
  });

  it('exposes only the direct children of the current section', () => {
    expect(directChildren(flat, 'floor-1').map((s) => s.id)).toEqual(['apt-0', 'apt-1']);
  });

  it('never surfaces grandchildren at the parent level', () => {
    const names = directChildren(flat, 'floor-1').map((s) => s.id);
    expect(names).not.toContain('kitchen');
    expect(directChildren(flat, 'apt-0').map((s) => s.id)).toEqual(['kitchen']);
  });

  it('supports arbitrary depth without a fixed maximum', () => {
    const deep = [section({ id: 'd0' })];
    for (let i = 1; i < 8; i += 1) {
      deep.push(section({ id: `d${i}`, parent_section_id: `d${i - 1}` }));
    }
    let level = 'd0';
    for (let i = 1; i < 8; i += 1) {
      const next = directChildren(deep, level);
      expect(next.map((s) => s.id)).toEqual([`d${i}`]);
      level = `d${i}`;
    }
    expect(directChildren(deep, level)).toEqual([]);
    expect(sectionTrail(deep, 'd7').map((s) => s.id)).toEqual([
      'd0',
      'd1',
      'd2',
      'd3',
      'd4',
      'd5',
      'd6',
      'd7',
    ]);
  });

  it('keeps malformed input safe: cycles, orphans, duplicates never loop or invent roots', () => {
    const corrupt = [
      section({ id: 'a', parent_section_id: 'b' }),
      section({ id: 'b', parent_section_id: 'a' }),
      section({ id: 'root', name: 'Root' }),
      section({ id: 'root', name: 'Root Duplicate' }),
      section({ id: 'self', parent_section_id: 'self' }),
      section({ id: 'orphan', parent_section_id: 'missing' }),
    ];
    // A two-node cycle attaches to each other but never to the root level.
    expect(directChildren(corrupt, null).map((s) => s.id)).toEqual(['root']);
    expect(directChildren(corrupt, 'a').map((s) => s.id)).toEqual(['b']);
    expect(directChildren(corrupt, 'b').map((s) => s.id)).toEqual(['a']);
    expect(directChildren(corrupt, 'self').map((s) => s.id)).toEqual(['self']);
    expect(childCounts(corrupt).get('root')).toBeUndefined();
  });
});

describe('direct child counts', () => {
  it('counts only direct children per parent', () => {
    const flat = [
      section({ id: 'root' }),
      section({ id: 'child', parent_section_id: 'root' }),
      section({ id: 'child2', parent_section_id: 'root' }),
      section({ id: 'grand', parent_section_id: 'child' }),
    ];
    const counts = childCounts(flat);
    expect(counts.get('root')).toBe(2);
    expect(counts.get('child')).toBe(1);
    expect(counts.get('grand')).toBeUndefined();
  });
});

describe('reparent candidates', () => {
  it('excludes a section and its descendants from movable parents', () => {
    const flat = [
      section({ id: 'a' }),
      section({ id: 'b' }),
      section({ id: 'a1', parent_section_id: 'a' }),
      section({ id: 'a1x', parent_section_id: 'a1' }),
    ];
    const movable = movableParents(flat, 'a').map((item) => item.id);
    expect(movable).toEqual(['b']);
  });

  it('excludes a full depth-5 subtree while unrelated nodes stay selectable', () => {
    const flat = [
      section({ id: 'root-unrelated-1' }),
      section({ id: 'a' }),
      section({ id: 'b', parent_section_id: 'a' }),
      section({ id: 'c', parent_section_id: 'b' }),
      section({ id: 'd', parent_section_id: 'c' }),
      section({ id: 'e', parent_section_id: 'd' }),
      section({ id: 'unrelated-2' }),
    ];
    const movable = movableParents(flat, 'a').map((item) => item.id);
    expect(movable).toEqual(['root-unrelated-1', 'unrelated-2']);
  });
});

describe('ancestor trail for breadcrumbs', () => {
  const input = [
    section({ id: 'root' }),
    section({ id: 'child', parent_section_id: 'root' }),
    section({ id: 'leaf', parent_section_id: 'child' }),
    section({ id: 'other' }),
  ];

  it('resolves ancestors in root-to-leaf order from a flat list', () => {
    expect(sectionTrail([...input].reverse(), 'leaf').map((s) => s.id)).toEqual([
      'root',
      'child',
      'leaf',
    ]);
  });

  it('does not guess parent links for missing rows or cycles', () => {
    expect(sectionTrail(input, 'unknown')).toEqual([]);
    expect(sectionTrail([section({ id: 'a', parent_section_id: 'missing' })], 'a')).toEqual([]);
    expect(
      sectionTrail(
        [
          section({ id: 'a', parent_section_id: 'b' }),
          section({ id: 'b', parent_section_id: 'a' }),
        ],
        'a',
      ),
    ).toEqual([]);
  });
});
