<script lang="ts">
  // Full hierarchy breadcrumb for drill-down pages:
  // Organization / Workspace / Projects / Project / …ancestors / current.
  // `trail` is the section chain resolved from backend data (root → current).
  // When `leaf` is given (a work item name), every trail section links to its
  // own page and the leaf is the current item; otherwise the last trail
  // section is the current page. Corrupt trails never produce guessed links —
  // the loader already falls back to the verified current section.
  import Breadcrumbs from './Breadcrumbs.svelte';
  import { translate, type Locale } from '$lib/i18n';
  import type { SectionPublic } from '$lib/api/client';
  let {
    locale,
    organization,
    workspace,
    project,
    trail,
    leaf,
  }: {
    locale: Locale;
    organization: { id: string; name: string };
    workspace: { id: string; name: string };
    project: { id: string; name: string };
    trail: SectionPublic[];
    leaf?: string;
  } = $props();

  const wsBase = $derived(`/app/${organization.id}/${workspace.id}` as const);
  const projectBase = $derived(`${wsBase}/projects/${project.id}` as const);
  const items = $derived.by(() => {
    const crumbs: { label: string; href?: `/app/${string}` }[] = [
      { label: organization.name, href: `/app/${organization.id}` },
      { label: workspace.name, href: wsBase },
      { label: translate(locale, 'projects.title'), href: `${wsBase}/projects` },
      { label: project.name, href: projectBase },
    ];
    const linked = leaf ? trail.length : trail.length - 1;
    trail.forEach((s, index) => {
      crumbs.push(
        index < linked
          ? { label: s.name, href: `${projectBase}/sections/${s.id}` }
          : { label: s.name },
      );
    });
    if (leaf) crumbs.push({ label: leaf });
    return crumbs;
  });
</script>

<Breadcrumbs {locale} {items} />
