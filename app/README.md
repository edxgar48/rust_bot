# App Força das Dezenas

Front-end estático (Vite + Svelte 5 + TypeScript estrito). Lê apenas os JSON do
contrato `v1` (`docs/contrato-v1.md`); sem servidor, sem analytics. Plano em
`docs/PLANEJAMENTO-FRONTEND.md`. Fica fora do `cargo build`.

Requisitos: Node 22+ e npm 11+.

## Comandos (a partir de `app/`)

```bash
npm install        # dependências (use npm ci em máquina limpa)
npm run dev        # servidor de desenvolvimento
npm run lint       # ESLint + checagem do Prettier
npm run formatar   # aplica o Prettier
npm run check      # tipos (svelte-check + tsc estrito)
npm test           # Vitest (sem rede)
npm run build      # gera o site estático em dist/
npm run preview    # serve dist/ localmente
```

## Estrutura

- `src/lib/textos.ts`: todos os textos de interface (um só módulo).
- `src/dados/`: camada de dados do contrato v1 (tipos, validação de forma,
  cliente do `index.json` com conferência de SHA-256/bytes, interface de
  armazenamento). URL base em `src/dados/config.ts`.
- Cache e offline (4.3): `src/dados/armazenamento-idb.ts` (IndexedDB direto, sem
  biblioteca; cai para memória com aviso se indisponível), `abertura.ts` (cache
  primeiro, depois sincroniza), `estado-sincronizacao.ts` (estado + texto).
  Service worker (vite-plugin-pwa) só faz precache do app shell; os dados do
  contrato NÃO passam por ele, o cache deles é o IndexedDB com conferência de
  SHA-256. SW existe só no build (`npm run preview`), nunca em `npm run dev`.
- Sistema de design (4.4): `src/estilos/tokens.css` (cor claro/escuro, tipografia da
  escala M3, forma, elevação, espaçamento) e `base.css`. Tema automático/claro/escuro:
  `src/lib/tema.ts` (preferência em localStorage, tolerante a falha) aplicada em
  `data-tema` no `<html>`; um script inline em `index.html` evita o flash. Fonte: pilha
  local (Roboto se existir, senão system-ui), sem CDN.
- Componentes em `src/componentes/` (BolaDezena, Volante, Cartao, Aviso, Chip,
  Segmentado, LinhaRanking, BarraNavegacao, SeletorTema, Icone) e lógica pura em
  `volante.ts`, `ranking.ts`, `segmentado.ts`. Catálogo: `npm run dev` e abrir `#catalogo`;
  para vê-lo num build, `npm run build:catalogo && npm run preview:catalogo`
  (`dist-catalogo/`, ignorado pelo git). O build normal não contém o catálogo.
- Telas e rotas (5.1): `src/lib/rotas.ts` (hash `#/inicio`, `#/forca`, `#/conferir`,
  `#/estatisticas`, `#/entenda`; padrão `#/inicio`; puro e testado), `src/lib/datas.ts`
  (AAAA-MM-DD para dd/mm/aaaa sem `Date`), `src/telas/Inicio.svelte` + `inicio.ts` (estado de
  sincronização -> o que a tela mostra). As outras abas são placeholders.
- Tabela de força (5.2): `src/telas/Forca.svelte` + `forca.ts` (visão pura: `tabela_geral` validada ->
  linhas em ordem de posição, subtítulo, perfil/janela e estado). Lê por `sessao.lerCache` (mostra o
  cache na hora) e `sessao.garantir` (`ResultadoGarantia.falha`: `sem_rede` ou `recusado`). Loteria
  escolhida: URL `#/forca?loteria=quina` (vale mais), senão a última escolha em localStorage
  (`lib/loteria-guardada.ts`, tolerante a falha), senão Lotofácil; trocar usa `history.replaceState`
  (sem poluir o histórico). A visão Volante é da 5.3 (hoje só um aviso "em breve"). O ponto de
  extensão do detalhe da dezena é a prop `aoEscolher(dezena)`: sem ela as linhas não são interativas.
  Os CSV em `testes/fixtures/sync-tabela/` são a saída de `sync tabela <loteria> --perfil padrao_v1`
  (gerada junto das fixtures); regerar com o `sync tabela` quando as fixtures de `publico/v1/` forem
  renovadas.
- Download sob demanda (`src/dados/sob-demanda.ts`): a abertura baixa só `index.json` +
  `ultimo.json` das 3 loterias. `tabela_geral` e `historico` vêm (1) quando a tela abre, por
  `sessao.garantir({ arquivos, loterias })` (devolve o cache se estiver sem rede), e (2) em
  segundo plano, quando a Início está atualizada: um tipo de arquivo por vez, em
  `requestIdleCallback`, em silêncio, e nunca com Save-Data ligado. Assim o app inteiro funciona
  sem rede mesmo que o usuário só tenha aberto a Início.
- `testes/`: fixtures (cópias de `publico/v1/`) e testes de dados.
