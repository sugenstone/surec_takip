<script lang="ts">
  // Overflow menu for secondary entity actions. Trigger is a labeled icon
  // button; the menu uses menu/menuitem roles, roving focus, Escape and
  // outside-click dismissal, and returns focus to the trigger on close.
  import type { Snippet } from 'svelte';
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

  let open = $state(false);
  let root = $state<HTMLDivElement>();
  let trigger = $state<HTMLButtonElement>();
  let menu = $state<HTMLDivElement>();

  const menuId = $props.id();

  function enabledItems(): HTMLElement[] {
    return [...(menu?.querySelectorAll<HTMLElement>('.menu-item:not(:disabled)') ?? [])];
  }

  function close(refocus = false) {
    open = false;
    if (refocus) trigger?.focus();
  }

  function select(item: MenuItem) {
    open = false;
    item.onSelect();
  }

  $effect(() => {
    if (!open || !menu) return;
    enabledItems()[0]?.focus();
    const onPointerDown = (event: PointerEvent) => {
      if (root && !root.contains(event.target as Node)) close();
    };
    document.addEventListener('pointerdown', onPointerDown);
    return () => document.removeEventListener('pointerdown', onPointerDown);
  });

  function onMenuKeydown(event: KeyboardEvent) {
    const items = enabledItems();
    const index = items.indexOf(event.target as HTMLElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      close(true);
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      items[(index + 1) % items.length]?.focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      items[(index - 1 + items.length) % items.length]?.focus();
    } else if (event.key === 'Home') {
      event.preventDefault();
      items[0]?.focus();
    } else if (event.key === 'End') {
      event.preventDefault();
      items[items.length - 1]?.focus();
    } else if (event.key === 'Tab') {
      close();
    }
  }
</script>

<div class="action-menu" bind:this={root}>
  <button
    bind:this={trigger}
    type="button"
    class="icon-btn menu-trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={open ? menuId : undefined}
    aria-label={label}
    {disabled}
    onclick={() => (open = !open)}
    onkeydown={(event) => {
      if ((event.key === 'ArrowDown' || event.key === 'Enter') && !open) {
        event.preventDefault();
        open = true;
      }
    }}
  >
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
  </button>
  {#if open}
    <div
      bind:this={menu}
      class="menu"
      role="menu"
      id={menuId}
      aria-label={label}
      tabindex="-1"
      onkeydown={onMenuKeydown}
    >
      {#each items as item (item.label)}
        <button
          type="button"
          role="menuitem"
          class="menu-item"
          class:danger={item.danger}
          disabled={item.disabled}
          onclick={() => select(item)}>{item.label}</button
        >
      {/each}
    </div>
  {/if}
</div>
