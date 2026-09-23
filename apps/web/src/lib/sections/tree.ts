// Safe client-side section tree assembly (ADR 0012). The backend returns a
// FLAT ordered list; this helper builds the hierarchy in memory. It is
// defensive by design: duplicate ids are ignored, rows pointing at missing
// parents (orphans) and rows only reachable through cycles can never attach
// to a root, so no corrupt input can loop or duplicate nodes. This is pure
// presentation — the backend remains the authority.
import type { SectionPublic } from '$lib/api/client';

export interface SectionNode {
  section: SectionPublic;
  children: SectionNode[];
}

export interface SectionRow {
  section: SectionPublic;
  depth: number;
}

export function buildSectionTree(sections: SectionPublic[]): SectionNode[] {
  const nodes = new Map<string, SectionNode>();
  for (const section of sections) {
    if (!nodes.has(section.id)) {
      nodes.set(section.id, { section, children: [] });
    }
  }
  const roots: SectionNode[] = [];
  for (const node of nodes.values()) {
    const parentId = node.section.parent_section_id;
    if (parentId === null || parentId === undefined) {
      roots.push(node);
      continue;
    }
    const parent = nodes.get(parentId);
    // Orphans (missing parent) and cycle members never reach a root and are
    // dropped instead of rendering unpredictably.
    if (parent && parent !== node) {
      parent.children.push(node);
    }
  }
  sortLevel(roots);
  return roots;
}

function sortLevel(nodes: SectionNode[]): void {
  nodes.sort(
    (a, b) => a.section.position - b.section.position || a.section.id.localeCompare(b.section.id),
  );
  for (const node of nodes) {
    sortLevel(node.children);
  }
}

// Iterative depth-first flattening for rendering: rows carry their depth so
// the UI indents without recursive components.
export function flattenTree(nodes: SectionNode[]): SectionRow[] {
  const rows: SectionRow[] = [];
  const stack: Array<{ node: SectionNode; depth: number }> = [];
  for (let index = nodes.length - 1; index >= 0; index -= 1) {
    stack.push({ node: nodes[index], depth: 0 });
  }
  while (stack.length > 0) {
    const { node, depth } = stack.pop() as { node: SectionNode; depth: number };
    rows.push({ section: node.section, depth });
    for (let index = node.children.length - 1; index >= 0; index -= 1) {
      stack.push({ node: node.children[index], depth: depth + 1 });
    }
  }
  return rows;
}

// Valid reparenting targets: every section except the node itself and its
// transitive descendants (mirrors the backend's cycle rule for UX only).
export function movableParents(sections: SectionPublic[], sectionId: string): SectionPublic[] {
  const descendants = new Set<string>([sectionId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const section of sections) {
      const parent = section.parent_section_id;
      if (parent != null && descendants.has(parent) && !descendants.has(section.id)) {
        descendants.add(section.id);
        changed = true;
      }
    }
  }
  return sections.filter((section) => !descendants.has(section.id));
}
