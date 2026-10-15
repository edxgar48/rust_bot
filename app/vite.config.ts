import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';
import { VitePWA } from 'vite-plugin-pwa';
import { manifesto } from './manifesto';

// Versão do app = a do package.json, injetada na compilação (constante __VERSAO_APP__).
// Importar o JSON no código levaria o arquivo inteiro para o pacote.
const { version: versaoApp } = JSON.parse(
  readFileSync(new URL('./package.json', import.meta.url), 'utf-8'),
) as { version: string };

export default defineConfig(({ mode }) => ({
  plugins: [
    svelte(),
    VitePWA({
      // 'prompt': a versão nova fica esperando e vale na próxima abertura; o app
      // nunca recarrega sozinho sob o usuário (ver src/pwa.ts).
      registerType: 'prompt',
      injectRegister: false,
      // Service worker desativado em dev (só existe no build).
      devOptions: { enabled: false },
      workbox: {
        // Só o app shell é pré-cacheado. Os dados do contrato NÃO passam pelo
        // service worker: o cache deles é o IndexedDB, com conferência de hash.
        globPatterns: ['**/*.{js,css,html,svg,png,ico,webmanifest}'],
        navigateFallback: 'index.html',
        runtimeCaching: [],
        cleanupOutdatedCaches: true,
      },
      manifest: manifesto,
    }),
  ],
  define: { __VERSAO_APP__: JSON.stringify(versaoApp) },
  // Caminhos relativos: o build funciona em qualquer hospedagem de arquivos.
  base: './',
  // Vitest: o Svelte precisa da build de navegador para montar componentes no jsdom.
  resolve: mode === 'test' ? { conditions: ['browser'] } : undefined,
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'testes/**/*.test.ts'],
  },
}));
