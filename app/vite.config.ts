import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { VitePWA } from 'vite-plugin-pwa';

export default defineConfig({
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
        globPatterns: ['**/*.{js,css,html,svg,png,webmanifest}'],
        navigateFallback: 'index.html',
        runtimeCaching: [],
        cleanupOutdatedCaches: true,
      },
      manifest: {
        name: 'Força das Dezenas',
        short_name: 'Dezenas',
        description: 'Resultados e estatísticas das loterias da Caixa.',
        lang: 'pt-BR',
        display: 'standalone',
        start_url: './',
        scope: './',
        theme_color: '#006B5E',
        background_color: '#F4FBF8',
        // Ícones PNG (192/512, maskable) ficam para a tarefa 8.1.
        icons: [{ src: 'icone.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' }],
      },
    }),
  ],
  // Caminhos relativos: o build funciona em qualquer hospedagem de arquivos.
  base: './',
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts', 'testes/**/*.test.ts'],
  },
});
