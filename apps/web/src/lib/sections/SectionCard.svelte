<script lang="ts">
  // Navigation-first section card (ADR 0014): the whole card enters the
  // section's own page via a stretched link; the trailing chevron signals
  // "drill into this level". Management actions live on the section page
  // itself, so the card stays free of nested interactive content.
  import { resolve } from '$app/paths';
  import { translate, type Locale } from '$lib/i18n';
  import type { SectionPublic } from '$lib/api/client';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import ProgressBlock from '$lib/ui/ProgressBlock.svelte';
  let {
    section,
    childCount,
    orgId,
    wsId,
    projectId,
    locale,
  }: {
    section: SectionPublic;
    childCount: number;
    orgId: string;
    wsId: string;
    projectId: string;
    locale: Locale;
  } = $props();
</script>

<article class="resource-card section-card">
  <header>
    <div class="card-heading">
      <span class="card-glyph glyph-section" aria-hidden="true"
        ><Icon name="layers" size={18} /></span
      >
      <h2>
        <a
          class="card-link"
          href={resolve('/app/[orgId]/[wsId]/projects/[projectId]/sections/[sectionId]', {
            orgId,
            wsId,
            projectId,
            sectionId: section.id,
          })}>{section.name}</a
        >
      </h2>
    </div>
    <div class="card-side">
      <StatusBadge
        status={section.status}
        label={translate(
          locale,
          section.status === 'archived' ? 'sections.status.archived' : 'sections.status.active',
        )}
      />
      <Icon name="chevron-right" size={18} />
    </div>
  </header>
  <p>{translate(locale, 'sections.childCount', { count: String(childCount) })}</p>
  <!-- Whole-subtree progress (ADR 0017), not only direct children. -->
  <ProgressBlock progress={section.progress} {locale} compact />
</article>

<style>
  .card-heading {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  .card-side {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .card-side :global(.icon) {
    color: var(--muted-foreground);
    transition:
      color 0.15s ease,
      transform 0.15s ease;
  }
  .section-card:hover .card-side :global(.icon) {
    color: var(--primary);
    transform: translateX(2px);
  }
</style>
