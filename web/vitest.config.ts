import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  define: {
    'import.meta.env.PUBLIC_API_BASE_URL': JSON.stringify('http://hub.test/api/v1'),
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.ts'],
    restoreMocks: true,
    coverage: {
      provider: 'v8',
      include: ['src/**/*.{ts,svelte}'],
      exclude: ['src/test/**', 'src/**/*.d.ts'],
      reporter: ['text', 'text-summary'],
      thresholds: {
        lines: 75,
        functions: 75,
        statements: 75,
        branches: 55,
      },
    },
  },
});
