import { defineConfig } from 'astro/config';
import svelte from '@astrojs/svelte';

export default defineConfig({
  integrations: [svelte()],
  server: {
    host: true,
    allowedHosts: ['.e2b.app'],
    port: 4321,
    strictPort: true,
  },
  vite: {
    server: {
      proxy: {
        '^/api/v1/projects/[^/]+/chat/channels': {
          target: 'http://127.0.0.1:4011',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/v1(?=\/|$)/, ''),
        },
        '/api/v1': {
          target: 'http://127.0.0.1:4010',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/v1(?=\/|$)/, ''),
        },
      },
    },
    preview: {
      proxy: {
        '^/api/v1/projects/[^/]+/chat/channels': {
          target: 'http://127.0.0.1:4011',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/v1(?=\/|$)/, ''),
        },
        '/api/v1': {
          target: 'http://127.0.0.1:4010',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/v1(?=\/|$)/, ''),
        },
      },
    },
  },
});
