import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  // Caminhos relativos: o build funciona em qualquer hospedagem de arquivos.
  base: './',
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'testes/**/*.test.ts'],
  },
});
