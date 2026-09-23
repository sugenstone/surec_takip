import { describe, expect, it } from 'vitest';
import { buildSectionTree, flattenTree, movableParents } from './tree';
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
    ...partial,
  };
}

describe('section tree assembly safety', () => {
  it('builds nested trees and orders siblings by (position, id)', () => {
    const flat = [
      section({ id: 'floor-1', name: 'Kat 1', position: 0 }),
      section({ id: 'floor-2', name: 'Kat 2', position: 1 }),
      section({ id: 'apt-1', name: 'Daire 1', parent_section_id: 'floor-1', position: 1 }),
      section({ id: 'apt-0', name: 'Daire 0', parent_section_id: 'floor-1', position: 0 }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.name)).toEqual(['Kat 1', 'Daire 0', 'Daire 1', 'Kat 2']);
    expect(rows.map((row) => row.depth)).toEqual([0, 1, 1, 0]);
  });

  it('drops orphan rows instead of rendering them unpredictably', () => {
    const flat = [
      section({ id: 'root', name: 'Root' }),
      section({ id: 'orphan', name: 'Orphan', parent_section_id: 'missing' }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.id)).toEqual(['root']);
  });

  it('never loops or duplicates on cyclic corrupt input', () => {
    const flat = [
      section({ id: 'a', parent_section_id: 'b' }),
      section({ id: 'b', parent_section_id: 'a' }),
      section({ id: 'root', name: 'Root' }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.id)).toEqual(['root']);
  });

  it('ignores duplicate ids and self-parenting rows', () => {
    const flat = [
      section({ id: 'root', name: 'Root' }),
      section({ id: 'root', name: 'Root Duplicate' }),
      section({ id: 'self', parent_section_id: 'self' }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.name)).toEqual(['Root']);
  });

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

  it('builds the same tree when children arrive before their parents', () => {
    const flat = [
      section({ id: 'child', name: 'Daire 1', parent_section_id: 'parent', position: 0 }),
      section({ id: 'parent', name: 'Kat 1', position: 0 }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.name)).toEqual(['Kat 1', 'Daire 1']);
    expect(rows.map((row) => row.depth)).toEqual([0, 1]);
  });

  it('drops every node of a three-node cycle without touching valid roots', () => {
    const flat = [
      section({ id: 'root', name: 'Root' }),
      section({ id: 'x', parent_section_id: 'y' }),
      section({ id: 'y', parent_section_id: 'z' }),
      section({ id: 'z', parent_section_id: 'x' }),
    ];
    const rows = flattenTree(buildSectionTree(flat));
    expect(rows.map((row) => row.section.id)).toEqual(['root']);
  });
});
