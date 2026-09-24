<script lang="ts">
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
  } = $props();
  let dialog: HTMLDialogElement;
  const id = $props.id();
  $effect(() => {
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="{id}-title"
  aria-describedby="{id}-description"
  aria-busy={pending}
  oncancel={(event) => {
    event.preventDefault();
    if (!pending) onCancel();
  }}
>
  <h2 id="{id}-title">{title}</h2>
  <p id="{id}-description">{description}</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="button-row">
    <button type="button" class="secondary" disabled={pending} onclick={onCancel}
      >{cancelLabel}</button
    >
    <button type="button" class="danger" disabled={pending} onclick={onConfirm}
      >{confirmLabel}</button
    >
  </div>
</dialog>
