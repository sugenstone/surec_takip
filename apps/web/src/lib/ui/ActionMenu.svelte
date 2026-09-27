<script lang="ts">
  // Overflow menu for secondary entity actions, composed from the shadcn
  // DropdownMenu primitives (Bits UI): menu/menuitem roles, roving focus,
  // Escape and outside dismissal, and focus restoration to the trigger are
  // provided by the primitive layer.
  import type { Snippet } from 'svelte';
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
  export type MenuItem = {
    label: string;
    danger?: boolean;
    disabled?: boolean;
    onSelect: () => void;
  };
  let {
    label,
    items,
    disabled = false,
    icon,
  }: {
    label: string;
    items: MenuItem[];
    disabled?: boolean;
    icon?: Snippet;
  } = $props();
</script>

<DropdownMenu.Root>
  <DropdownMenu.Trigger class="icon-btn menu-trigger" aria-label={label} {disabled}>
    {#if icon}{@render icon()}{:else}
      <svg
        class="icon"
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        aria-hidden="true"><path d="M5 12h.01" /><path d="M12 12h.01" /><path d="M19 12h.01" /></svg
      >
    {/if}
  </DropdownMenu.Trigger>
  <DropdownMenu.Content aria-label={label}>
    {#each items as item (item.label)}
      <DropdownMenu.Item
        variant={item.danger ? 'destructive' : 'default'}
        disabled={item.disabled}
        onSelect={() => item.onSelect()}>{item.label}</DropdownMenu.Item
      >
    {/each}
  </DropdownMenu.Content>
</DropdownMenu.Root>
