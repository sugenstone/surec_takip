<script lang="ts">
  // The AppShell is already rendered by the [orgId] parent layout; this
  // layout only adds workspace context for its children.
  import type { LayoutProps } from './$types';

  let { children }: LayoutProps = $props();
</script>

{@render children()}
