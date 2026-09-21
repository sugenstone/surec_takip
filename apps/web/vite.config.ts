import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Browser requests stay same-origin: the dev server proxies /api to the
// backend, mirroring the reverse proxy used in production deployments.
const apiTarget = process.env.API_PROXY_TARGET ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    proxy: {
      '/api': { target: apiTarget, changeOrigin: true },
    },
  },
});
