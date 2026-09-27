<script lang="ts">
  // Application confirmation dialog composed from the shadcn AlertDialog
  // primitives (Bits UI). The public contract matches the former
  // packages/ui implementation: callers own `open` and close the dialog
  // through onCancel; the confirm action never closes the dialog itself,
  // so it stays open while `pending` and on backend failure. Escape and
  // the cancel control are suppressed while pending.
  import type { Snippet } from 'svelte';
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
  let {
    open,
    title,
    description,
    confirmLabel,
    cancelLabel,
    pending = false,
    error = '',
    onConfirm,
    onCancel,
    children,
  }: {
    open: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    cancelLabel: string;
    pending?: boolean;
    error?: string;
    onConfirm: () => void;
    onCancel: () => void;
    children?: Snippet;
  } = $props();

  // Writable derived: the caller's `open` prop drives the dialog, while
  // Bits UI close requests write back locally and notify onOpenChange.
  let internalOpen = $derived(open);
  let contentEl = $state<HTMLDivElement | null>(null);
</script>

<AlertDialog.Root
  bind:open={internalOpen}
  onOpenChange={(next) => {
    if (next) return;
    if (pending) {
      internalOpen = true;
      return;
    }
    onCancel();
  }}
>
  <AlertDialog.Content
    bind:ref={contentEl}
    class="confirm-dialog"
    escapeKeydownBehavior={pending ? 'ignore' : 'close'}
    aria-busy={pending}
    onOpenAutoFocus={(event) => {
      // Focus the cancel control on open — the least destructive action —
      // instead of the default focus on the dialog surface itself.
      event.preventDefault();
      setTimeout(() => {
        contentEl?.querySelector<HTMLElement>("[data-slot='alert-dialog-cancel']")?.focus();
      });
    }}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{title}</AlertDialog.Title>
      <AlertDialog.Description>{description}</AlertDialog.Description>
    </AlertDialog.Header>
    {#if children}{@render children()}{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={pending}>{cancelLabel}</AlertDialog.Cancel>
      <AlertDialog.Action variant="destructive" disabled={pending} onclick={onConfirm}
        >{confirmLabel}</AlertDialog.Action
      >
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
