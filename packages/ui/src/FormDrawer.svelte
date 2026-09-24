<script lang="ts">
  // Transient creation/edit surface: a right-anchored sheet built on the
  // native <dialog> element, so modality, focus trapping, backdrop and focus
  // restoration to the trigger come from the platform. Children mount only
  // while open, so drafts reset on cancel but survive backend failures
  // (the drawer stays open on error by the caller's choice).
  import type { Snippet } from 'svelte';
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
  let dialog: HTMLDialogElement;
  const id = $props.id();
  $effect(() => {
    if (open && !dialog.open) {
      dialog.showModal();
      dialog.querySelector<HTMLElement>('input, select, textarea')?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });
</script>

<dialog
  bind:this={dialog}
  class="form-drawer"
  aria-labelledby="{id}-title"
  aria-busy={busy}
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) onClose();
  }}
>
  {#if open}
    <div class="drawer-head">
      <div class="drawer-heading">
        <h2 id="{id}-title">{title}</h2>
        {#if description}<p class="drawer-description">{description}</p>{/if}
      </div>
      <button
        type="button"
        class="icon-btn drawer-close"
        aria-label={closeLabel}
        disabled={busy}
        onclick={onClose}
      >
        <svg
          class="icon"
          width="20"
          height="20"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg
        >
      </button>
    </div>
    <div class="drawer-body">{@render children()}</div>
  {/if}
</dialog>
