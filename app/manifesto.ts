// Manifesto do PWA (8.1). Fica fora de vite.config.ts para ser testado
// (testes/manifesto.test.ts confere os campos e se os ícones existem em public/).
//
// Caminhos relativos (`./`): o app funciona em qualquer hospedagem de arquivos,
// na raiz ou num subcaminho, sem URL final (decisão de hospedagem = 8.4). O
// `start_url` e o `scope` são resolvidos contra a URL do próprio manifesto, e
// ele fica na raiz do build, ao lado do index.html.
import type { ManifestOptions } from 'vite-plugin-pwa';

/** Cores iguais aos tokens de src/estilos/tokens.css (tema claro). */
export const COR_FUNDO_CLARO = '#F4FBF8'; // --cor-superficie
/** Barra do topo do app (--cor-cont); é a cor da barra de status do aparelho. */
export const COR_TEMA_CLARO = '#E8EFEC';
export const COR_TEMA_ESCURO = '#1A211F';

export const manifesto: Partial<ManifestOptions> = {
  name: 'Força das Dezenas',
  short_name: 'Dezenas',
  description:
    'Resultados, estatísticas e conferência de jogos da Lotofácil, Mega-Sena e Quina. Não prevê sorteios.',
  lang: 'pt-BR',
  dir: 'ltr',
  display: 'standalone',
  // "any": o app é responsivo e também roda em desktop e tablet; travar em
  // retrato só atrapalharia quem usa o aparelho deitado (tabelas e volantes).
  orientation: 'any',
  start_url: './',
  scope: './',
  theme_color: COR_TEMA_CLARO,
  // Splash do Android = background_color + ícone de 512 px.
  background_color: COR_FUNDO_CLARO,
  categories: ['utilities', 'entertainment'],
  icons: [
    { src: 'icone-192.png', sizes: '192x192', type: 'image/png', purpose: 'any' },
    { src: 'icone-512.png', sizes: '512x512', type: 'image/png', purpose: 'any' },
    { src: 'icone-maskable-192.png', sizes: '192x192', type: 'image/png', purpose: 'maskable' },
    { src: 'icone-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
    { src: 'icone.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
  ],
};
