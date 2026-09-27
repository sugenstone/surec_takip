<script lang="ts">
  // Transient creation/edit surface composed from the shadcn Sheet
  // primitives (Bits UI). The right-anchored panel keeps the `form-drawer`
  // class so the existing field/label styling applies inside, and the
  // mobile behaviour matches the previous implementation (full width).
  // Modality, focus trapping and restoration come from the primitive
  // layer. Children mount only while open, so drafts reset on cancel but
  // survive backend failures (the drawer stays open on error by the
  // caller's choice). Escape, outside interaction and the close control
  // are suppressed while `busy`.
  import type { Snippet } from 'svelte';
  import * as Sheet from '$lib/components/ui/sheet/index.js';
  let {
    open,
    title,
    description,
    closeLabel,
    busy = false,
    onClose,
    children,
  }: {
    open: boolean;
    title: string;
    description?: string;
    closeLabel: string;
    busy?: boolean;
    onClose: () => void;
    children: Snippet;
  } = $props();

  // Writable derived: the caller's `open` prop drives the drawer, while
  // Bits UI close requests write back locally and notify onOpenChange.
  let internalOpen = $derived(open);
  let contentEl = $state<HTMLDivElement | null>(null);
</script>

<Sheet.Root
  bind:open={internalOpen}
  onOpenChange={(next) => {
    if (next) return;
    if (busy) {
      internalOpen = true;
      return;
    }
    onClose();
  }}
>
  <Sheet.Content
    bind:ref={contentEl}
    side="right"
    class="form-drawer w-full max-sm:border-s-0 gap-0 p-0 sm:max-w-[30rem]"
    {closeLabel}
    closeDisabled={busy}
    escapeKeydownBehavior={busy ? 'ignore' : 'close'}
    interactOutsideBehavior="ignore"
    aria-busy={busy}
    onOpenAutoFocus={(event) => {
      // Move focus to the first field, matching the previous native
      // <dialog> behaviour, instead of focusing the panel itself.
      event.preventDefault();
      setTimeout(() => {
        contentEl?.querySelector<HTMLElement>('input, select, textarea')?.focus();
      });
    }}
  >
    <div class="drawer-head">
      <div class="drawer-heading">
        <Sheet.Title class="text-lg">{title}</Sheet.Title>
        {#if description}
          <Sheet.Description class="drawer-description">{description}</Sheet.Description>
        {/if}
      </div>
    </div>
    <div class="drawer-body">{@render children()}</div>
  </Sheet.Content>
</Sheet.Root>
