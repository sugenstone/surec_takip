// Safe client-side section hierarchy helpers (ADR 0012/0014). The backend
// returns a FLAT ordered list; drill-down pages filter it to the direct
// children of the current level and resolve ancestor trails for breadcrumbs.
// Recursion lives in the data model and the URL — never in a single-page
// expanded tree. Helpers are defensive by design: duplicate ids, orphans and
// cycles can never loop or fabricate parents. This is pure presentation —
// the backend remains the authority.
import type { SectionPublic } from '$lib/api/client';

function byPosition(a: SectionPublic, b: SectionPublic): number {
  return a.position - b.position || a.id.localeCompare(b.id);
}

// Direct children of one level only — never descendants. `null` selects the
// project root. Cyclic input can attach nodes to each other but can never
// fabricate a root or loop the caller (output is a plain filtered list);
// duplicate ids keep their first occurrence instead of rendering twice.
export function directChildren(
  sections: SectionPublic[],
  parentId: string | null,
): SectionPublic[] {
  const seen = new Set<string>();
  return sections
    .filter((section) => {
      if ((section.parent_section_id ?? null) !== parentId || seen.has(section.id)) return false;
      seen.add(section.id);
      return true;
    })
    .sort(byPosition);
}

// Direct-child counts per section id, for secondary card metadata. Shares
// directChildren's dedupe rule so card counts match the rendered list.
export function childCounts(sections: SectionPublic[]): Map<string, number> {
  const seen = new Set<string>();
  const counts = new Map<string, number>();
  for (const section of sections) {
    const parent = section.parent_section_id;
    if (parent == null || seen.has(section.id)) continue;
    seen.add(section.id);
    counts.set(parent, (counts.get(parent) ?? 0) + 1);
  }
  return counts;
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

// Breadcrumbs use only the parent-scoped SSR list. Corrupt/missing chains never
// produce guessed links; this helper is presentation, not authorization.
export function sectionTrail(sections: SectionPublic[], id: string): SectionPublic[] {
  const byId = new Map(sections.map((section) => [section.id, section]));
  const seen = new Set<string>();
  const trail: SectionPublic[] = [];
  let current = byId.get(id);
  while (current) {
    if (seen.has(current.id)) return [];
    seen.add(current.id);
    trail.unshift(current);
    if (!current.parent_section_id) return trail;
    current = byId.get(current.parent_section_id);
    if (!current) return [];
  }
  return trail;
}
