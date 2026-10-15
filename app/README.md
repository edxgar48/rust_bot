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
  (sem poluir o histórico). Forma de exibição (Lista/Volante) e dezena aberta
  também ficam na URL (`#/forca?loteria=quina&visao=volante&dezena=20`; precedência URL, depois
  localStorage, depois lista). Ver 5.3 abaixo.
  Os CSV em `testes/fixtures/sync-tabela/` são a saída de `sync tabela <loteria> --perfil padrao_v1`
  (gerada junto das fixtures); regerar com o `sync tabela` quando as fixtures de `publico/v1/` forem
  renovadas.
- Volante e detalhe da dezena (5.3): `componentes/Volante.svelte` em modo leitura com `aoEscolher`
  (cada célula é um botão "Dezena 20, 1º lugar", colorida por uma das 5 faixas iguais de
  `volante.ts`, com legenda). O detalhe é uma folha modal (`componentes/FolhaModal.svelte` + `folha.ts`:
  foco preso, Esc/Fechar/toque no fundo fecham, foco volta à origem; em tela larga vira janela
  centralizada), igual para lista e volante. Abrir faz `pushState` (voltar fecha); aberto direto pela
  URL, Fechar só tira o parâmetro; `dezena` inexistente é ignorada. Conteúdo montado em
  `telas/detalhe-dezena.ts` (puro, testado): índice com 3 casas, variação, evidência (5 graus x 2
  direções, frases em `textos.ts`), janela e os 9 componentes com explicação; `null` vira "sem dados na
  janela" (espaço final nulo = "nunca saiu", porque não depende da janela). A Força não tem mais a prop
  `aoEscolher`: o detalhe é interno.
- Entenda e avisos (5.4): `src/telas/Entenda.svelte` + `entenda.ts` (visão pura). Conteúdo em
  `textos.entenda`; frases dos graus/direções e dos 9 componentes são importadas de `textos.ts`
  (sem duplicar). "De onde vêm os dados" lê só o cache (`sessao.lerCache`, relido quando a situação
  muda; nunca exige rede): data da atualização, último concurso, e do cabeçalho da tabela_geral o
  perfil, a janela, a normalização, `feature_version` e o `hash_toml` abreviado (8 caracteres; completo
  no `title` e num `<details>`). Versão do app: constante `__VERSAO_APP__` injetada pelo Vite
  (`define`, vinda do package.json; declarada em `src/globais.d.ts`). Avisos centralizados em
  `textos.avisos` (não prevê / jogo responsável). `src/lib/textos-varredura.test.ts` falha se o
  módulo de textos tiver linguagem de previsão (lista e exceções documentadas no próprio teste).
- Histórico (6.1), aba Conferir (`#/conferir?loteria=quina`): `src/telas/Historico.svelte` +
  `LinhaConcurso.svelte` + `historico.ts` (tudo que é lógica fica em funções puras testadas: leitura da
  busca digitada, `consultarHistorico`, `janelaVirtual`, visão por estado). Lê `historico.json` do cache
  (`sessao.lerCache`, depois `sessao.garantir`); mesma precedência de loteria da Força (URL, `localStorage`,
  Lotofácil; a chave guardada é a mesma). Lista do concurso mais recente ao mais antigo. Busca por número
  ou por data (dd/mm/aaaa com máscara; também aceita AAAA-MM-DD; sem `Date`): mostra só o(s) concurso(s)
  encontrado(s); inexistente ou data sem sorteio dão mensagem (com o sorteio anterior e o seguinte mais
  próximos). Filtro por dezenas: o volante em modo seleção (`<details>`), concursos que contêm TODAS as
  dezenas, no máximo quantas saem por concurso; busca e filtro valem juntos. Dezena do filtro na linha:
  anel e negrito (não só cor) e frase para leitor de tela. Lista virtual própria, sem dependência: linhas de
  altura fixa em rem por loteria (`ALTURA_LINHA_REM`), a página rola (sem rolagem aninhada), só as linhas
  perto da tela existem no DOM (cerca de 13 a 15, 3 de folga de cada lado), `padding` do `<ul>` reserva o
  resto. O estado de `situacao` usa `$state.raw` (milhares de listas, sem proxy reativo).
- Download sob demanda (`src/dados/sob-demanda.ts`): a abertura baixa só `index.json` +
  `ultimo.json` das 3 loterias. `tabela_geral` e `historico` vêm (1) quando a tela abre, por
  `sessao.garantir({ arquivos, loterias })` (devolve o cache se estiver sem rede), e (2) em
  segundo plano, quando a Início está atualizada: um tipo de arquivo por vez, em
  `requestIdleCallback`, em silêncio, e nunca com Save-Data ligado. Assim o app inteiro funciona
  sem rede mesmo que o usuário só tenha aberto a Início.
- `testes/`: fixtures (cópias de `publico/v1/`) e testes de dados.
