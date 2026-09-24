<script lang="ts">
  let { status, label }: { status: string; label: string } = $props();
</script>

<span class="status-badge" data-status={status}
  ><span aria-hidden="true">{status === 'completed' ? '✓' : status === 'archived' ? '−' : '●'}</span
  >
  {label}</span
>
