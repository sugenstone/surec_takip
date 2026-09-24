<script lang="ts">
  // Work item card: represents executable work, not hierarchy navigation.
  // The `children` slot is reserved for the future authorized process summary
  // (STEP 20) — nothing is rendered there today.
  import type { Snippet } from 'svelte';
  import { resolve } from '$app/paths';
  import { translate, type Locale } from '$lib/i18n';
  import type { WorkItemPublic, WorkItemScope } from '$lib/api/client';
  import StatusBadge from '@platform/ui/StatusBadge.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  let {
    item,
    scope,
    locale,
    children,
  }: { item: WorkItemPublic; scope: WorkItemScope; locale: Locale; children?: Snippet } = $props();
</script>

<article class="resource-card work-item-card" data-status={item.status}>
  <header>
    <div class="card-heading">
      <span class="card-glyph glyph-work" aria-hidden="true"
        ><Icon name="square-check" size={18} /></span
      >
      <h2>
        <a
          class="card-link"
          href={resolve(
            `/app/${scope.organizationId}/${scope.workspaceId}/projects/${scope.projectId}/sections/${scope.sectionId}/work-items/${item.id}`,
          )}>{item.name}</a
        >
      </h2>
    </div>
    <StatusBadge
      status={item.status}
      label={translate(
        locale,
        item.status === 'completed' ? 'workItems.status.completed' : 'workItems.status.active',
      )}
    />
  </header>
  <p class="item-reference">{item.slug}</p>
  {#if children}<div class="work-item-content">{@render children()}</div>{/if}
</article>

<style>
  .card-heading {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }
  .item-reference {
    margin: 0;
    font-size: var(--text-caption);
    color: var(--muted-foreground);
  }
  .work-item-content {
    margin-block-start: var(--space-4);
  }
</style>
