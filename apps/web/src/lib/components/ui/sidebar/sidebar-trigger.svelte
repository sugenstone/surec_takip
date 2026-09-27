<script lang="ts">
  import PanelLeft from '@lucide/svelte/icons/panel-left';
  import { Button } from '$lib/components/ui/button/index.js';
  import { cn } from '$lib/utils.js';
  import { useSidebar } from './context.svelte.js';
  import type { ComponentProps } from 'svelte';

  // Adaptation (STEP 21D.5D): upstream hardcodes the sr-only "Toggle Sidebar"
  // label; we require a localized `label` prop like Sheet's `closeLabel`.
  let {
    ref = $bindable(null),
    class: className,
    label,
    onclick,
    ...restProps
  }: ComponentProps<typeof Button> & {
    label: string;
    onclick?: (e: MouseEvent) => void;
  } = $props();

  const sidebar = useSidebar();
</script>

<Button
  bind:ref
  data-sidebar="trigger"
  data-slot="sidebar-trigger"
  variant="ghost"
  size="icon-sm"
  class={cn('cn-sidebar-trigger', className)}
  type="button"
  aria-expanded={sidebar.isMobile ? sidebar.openMobile : sidebar.open}
  onclick={(e) => {
    onclick?.(e);
    sidebar.toggle();
  }}
  {...restProps}
>
  <PanelLeft class="cn-rtl-flip" />
  <span class="sr-only">{label}</span>
</Button>
