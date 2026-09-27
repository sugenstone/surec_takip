import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Browser requests stay same-origin: the dev server proxies /api to the
// backend, mirroring the reverse proxy used in production deployments.
const apiTarget = process.env.API_PROXY_TARGET ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // Dependency pre-bundling (STEP 21D.5D D1): SvelteKit's default dep-scan
  // entries are absolute paths built from kit.files.routes; on Windows those
  // backslash paths match zero files in the scan globber, so the startup
  // crawl finds nothing and every bare import is discovered lazily at
  // runtime — each discovery re-optimizes deps and full-reloads, killing
  // in-flight hydration. Scanning the whole client source graph with
  // relative forward-slash patterns discovers all imports upfront and cannot
  // drift when new modules (e.g. new @lucide icon subpaths) are imported.
  optimizeDeps: {
    entries: [
      'src/**/*.{svelte,js,ts}',
      '!src/**/*.server.{js,ts}',
      '!src/**/*.test.{js,ts}',
      '!src/**/*.d.ts',
    ],
  },
  server: {
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true },
    },
  },
});
