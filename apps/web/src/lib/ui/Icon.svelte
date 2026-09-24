<script lang="ts">
  // Inline stroke icons (Lucide-style, 24px grid). Decorative by default:
  // icons support adjacent text labels and stay aria-hidden.
  const icons: Record<string, string[]> = {
    menu: ['M4 7h16', 'M4 12h16', 'M4 17h16'],
    x: ['M6 6l12 12', 'M18 6L6 18'],
    plus: ['M12 5v14', 'M5 12h14'],
    'chevron-right': ['M9 6l6 6-6 6'],
    'chevron-down': ['M6 9l6 6 6-6'],
    'arrow-up': ['M12 19V5', 'M5 12l7-7 7 7'],
    'arrow-down': ['M12 5v14', 'M19 12l-7 7-7-7'],
    'list-ordered': [
      'M10 6h11',
      'M10 12h11',
      'M10 18h11',
      'M4 6h1v4',
      'M4 10h2',
      'M6 18H4c0-1 2-2 2-3s-1-1.5-2-1',
    ],
    'square-check': [
      'M3 5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z',
      'm9 12l2 2 4-4',
    ],
    'more-horizontal': ['M5 12h.01', 'M12 12h.01', 'M19 12h.01'],
    layers: ['M12 3l9 5-9 5-9-5 9-5z', 'M3 13l9 5 9-5'],
    folder: ['M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z'],
    'log-out': ['M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4', 'M16 17l5-5-5-5', 'M21 12H9'],
    'layout-grid': ['M4 4h6v6H4z', 'M14 4h6v6h-6z', 'M4 14h6v6H4z', 'M14 14h6v6h-6z'],
  };

  let { name, size = 20 }: { name: keyof typeof icons; size?: number } = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {#each icons[name] as d (d)}<path {d} />{/each}
</svg>
