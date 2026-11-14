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
});
