<script lang="ts">
  // One ordered process DEFINITION. Real data only: order, name, optional
  // description and the required/optional flag. There is deliberately no
  // execution state here (no progress, timer or assignee) — future execution
  // metadata gets its own slot once real data exists (ADR 0015).
  import ActionMenu, { type MenuItem } from '@platform/ui/ActionMenu.svelte';
  import Icon from '$lib/ui/Icon.svelte';
  import { translate, type Locale } from '$lib/i18n';
  import type { ProcessPublic } from '$lib/api/client';
  let {
    process,
    index,
    count,
    locale,
    canUpdate,
    canArchive,
    canReorder,
    disabled,
    onMove,
    onEdit,
    onArchive,
  }: {
    process: ProcessPublic;
    index: number;
    count: number;
    locale: Locale;
    canUpdate: boolean;
    canArchive: boolean;
    canReorder: boolean;
    disabled: boolean;
    onMove: (delta: -1 | 1) => void;
    onEdit: () => void;
    onArchive: () => void;
  } = $props();
  const vars = $derived({ name: process.name });
  const menuItems = $derived.by<MenuItem[]>(() => {
    const items: (MenuItem | null)[] = [
      canUpdate ? { label: translate(locale, 'processes.edit'), onSelect: onEdit } : null,
      canUpdate && canArchive
        ? { label: translate(locale, 'processes.archive'), danger: true, onSelect: onArchive }
        : null,
    ];
    return items.filter((item): item is MenuItem => item !== null);
  });
</script>

<article class="process-row" data-process-id={process.id}>
  <span class="process-order" aria-hidden="true">{String(index + 1).padStart(2, '0')}</span>
  <div class="process-body">
    <h3 class="process-name">{process.name}</h3>
    {#if process.description}<p class="process-description">{process.description}</p>{/if}
  </div>
  <span class="requirement-badge" data-required={process.is_required}
    ><span aria-hidden="true">{process.is_required ? '●' : '○'}</span>
    {translate(locale, process.is_required ? 'processes.required' : 'processes.optional')}</span
  >
  {#if canReorder || menuItems.length > 0}
    <div class="process-actions">
      {#if canReorder}
        <button
          type="button"
          class="icon-btn"
          data-move="up"
          aria-label={translate(locale, 'processes.moveUp', vars)}
          title={translate(locale, 'processes.moveUp', vars)}
          disabled={disabled || index === 0}
          onclick={() => onMove(-1)}><Icon name="arrow-up" size={18} /></button
        >
        <button
          type="button"
          class="icon-btn"
          data-move="down"
          aria-label={translate(locale, 'processes.moveDown', vars)}
          title={translate(locale, 'processes.moveDown', vars)}
          disabled={disabled || index === count - 1}
          onclick={() => onMove(1)}><Icon name="arrow-down" size={18} /></button
        >
      {/if}
      {#if menuItems.length > 0}
        <ActionMenu
          label={translate(locale, 'processes.actions', vars)}
          items={menuItems}
          {disabled}
        />
      {/if}
    </div>
  {/if}
</article>
