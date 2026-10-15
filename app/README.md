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
- Aba Conferir (6.2): `src/telas/Conferir.svelte` alterna duas seções pelo hash
  (`#/conferir?loteria=quina` = Conferir jogo, o padrão; `...&secao=historico` = Histórico, helpers
  `hashDoJogo`/`hashDoHistorico` em `lib/rotas.ts`). Links antigos `#/conferir?loteria=X` sem `secao`
  agora abrem Conferir jogo; o Histórico exige `&secao=historico`. Conferir jogo: `ConferirJogo.svelte` + `conferir.ts`
  (puro). Volante em modo seleção, quantidade válida em `QUANTIDADE_JOGO` (Lotofácil 15-20, Mega-Sena
  6-20, Quina 5-15; o volante bloqueia novas dezenas no máximo). O resultado só aparece com quantidade
  válida e histórico carregado. `conferirJogo` faz uma passada com uma tabela de presença (`Uint8Array`),
  sem alocar por concurso: contagem por número de acertos, melhor resultado e todos os concursos em que
  ocorreu (mais recente primeiro); `concursosComAcertos` lista uma faixa; cada faixa da tela é um `<details>` (`FaixaAcertos.svelte`) que só
  calcula a lista (até 20 concursos, "e mais N") ao expandir. Faixas impossíveis (`minimoAcertos`) não aparecem.
  Testada com valores à mão e contra um cálculo ingênuo sobre as 3 fixtures reais. Contador em região viva
  (`role=status`); bolas são botões com `aria-pressed`; melhor faixa marcada com texto, não só cor. O
  botão "Salvar jogo" veio na 6.3 (abaixo). Estado comum das telas de dados (Força,
  Histórico, Conferir) em `telas/estado-tela.ts` (`SituacaoTela`, `EstadoTela`, `estadoEData`).
- Jogos salvos (6.3), `src/jogos/` + seção "Meus jogos" da aba Conferir (`#/conferir?loteria=quina&secao=jogos`,
  `hashDosJogos`). Modelo (`jogo.ts`, puro): `{ id, loteria, nome|null (até 40 caracteres), dezenas ordenadas e
válidas por `QUANTIDADE_JOGO`, criadoEm }`; `validarJogoSalvo` é a única porta de entrada (salvar, importar e
  leitura do banco; registro ruim no banco é pulado, nunca apagado). Persistência em IndexedDB num banco PRÓPRIO
  (`forca-loterias-jogos`, loja `jogos`, índice por loteria), separado do cache do contrato
  (`forca-loterias`): atualizar ou limpar o cache de dados não toca nos jogos (teste em `testes/jogos.test.ts`).
  `RepositorioResiliente` segue o padrão do armazenamento de dados (cai para memória com `aviso`, e a tela avisa
  que o jogo vale só até fechar o app). `servico.ts`: `salvarJogo` (não duplica: mesma loteria + dezenas),
  `exportarJogos`, `importarJogos`, `pedirArmazenamentoPersistente` (`navigator.storage.persist()`, best-effort,
  chamado ao salvar o primeiro jogo; nunca bloqueia). Backup = arquivo JSON do app
  `{ formato: "forca-loterias-jogos", versao: 1, exportadoEm, jogos: [...] }` (não é o contrato de dados);
  campos desconhecidos são ignorados, versão maior ou envelope errado é recusado sem alterar nada, e importar
  valida item a item e relata entraram / ignorados (já salvos) / inválidos; id repetido com outro conteúdo ganha
  id novo. Exportar baixa por Blob + `a[download]` (`lib/download.ts`); importar usa `input[type=file]` acionado
  por botão. Tela: `SalvarJogo.svelte` (nome opcional + "Salvar jogo", só habilitado com quantidade válida),
  `MeusJogos.svelte` (lista por loteria, Conferir carrega o jogo no volante, renomear inline, apagar com
  `FolhaModal` e "Desfazer", melhor resultado histórico só se o histórico estiver em cache, mensagens em
  região viva). Decisão: o backup é o arquivo exportado; sem nuvem; limpar os dados do navegador apaga os jogos
  (dito na tela). A conferência automática veio na 6.4 (abaixo).
  Falhas: `RepositorioResiliente` só cai para memória se o IndexedDB não existe ou NÃO ABRE (`ErroAbertura`, com
  aviso). Se abre e uma operação falha depois, ela é repetida uma vez com a conexão refeita; persistindo, o erro
  chega à tela (leitura: aviso "não foi possível ler" + "Tentar ler de novo", sem lista vazia e com Exportar
  bloqueado; gravação: mensagem de falha). Importação: máximo de 1.000 jogos por arquivo (acima, o arquivo é
  recusado inteiro) e ids já guardados (`listarIds`, inclusive registros inválidos) nunca são sobrescritos;
  `criadoEm` é normalizado para UTC.

- Conferência automática (6.4), `src/jogos/conferencia-automatica.ts` (puro), `novidades.ts` (liga ao repositório
  e ao cache, sem rede) e `servico.marcarComoVisto`. Cada jogo guarda `conferidoAte` (último concurso cujo
  resultado a pessoa já viu): campo OPCIONAL, `null` = sem marca. Compatibilidade: arquivos de backup antigos
  sem o campo seguem válidos (importam como `null`), exportar grava o campo e importar o preserva (valor
  inválido vira `null` sem recusar o jogo); a `versao` do arquivo continua 1, pois apps antigos só ignoram o
  campo. Jogo novo nasce com `conferidoAte` = último concurso do histórico mostrado (o passado não vira
  novidade); jogo sem marca recebe o último concurso conhecido na primeira conferência. Regra:
  `concursosPosteriores` junta o histórico em cache e o `ultimo.json` (um único concurso novo é conferido só
  com o `ultimo.json`, sem baixar o histórico); se o histórico em cache está atrás e há buraco, o jogo fica
  `pendente` (nunca "0 acertos" por falta de dado). `conferirConcursosNovos` devolve acertos por concurso,
  melhor (empate: o mais recente) e `naFaixaPremio` (`FAIXA_PREMIO`: Lotofácil 11, Mega-Sena 4, Quina 2;
  rótulo "acertos", o app não calcula valor de prêmio e diz isso na tela). Quando roda: `sessao.svelte.ts` chama
  `calcularNovidades` ao fim da abertura/atualização (ultimo.json já sincronizado), depois que o segundo plano
  baixa o histórico e ao voltar à aba Início; a abertura enxuta NÃO baixa o histórico só para isso. Em Meus
  jogos, se há jogos e o histórico falta ou está atrás do último concurso, `garantir` baixa sob demanda.
  Início: cartão "Concurso N saiu: seus jogos salvos da Quina" com resumo ("2 jogos conferidos; melhor: 3
  acertos"), jogos na faixa e link para Meus jogos (nada se há 0 jogos ou nada novo). Meus jogos: jogo com
  concurso novo ganha borda grossa + selo em texto "Novo: concurso N", acertos por concurso (até 5, "e mais N")
  e "Marcar como visto" (só avança `conferidoAte`; nunca é marcado sozinho). Offline usa o último cache; sem
  notificação push (backlog). Testes: puros com valores à mão, `testes/conferencia-automatica.test.ts`
  (servidor falso + concurso 7142 sintético via `ServidorFalso.publicarConcursoQuina`, IndexedDB falso,
  recarregar, offline, ultimo.json adulterado) e de componente (`Inicio.novidade.test.ts`,
  `MeusJogos.novidade.test.ts`). Capturas 360/1280 px, claro e escuro, conferidas (2026-10-15).
- Estatísticas, funções puras (7.1), `src/estatisticas/estatisticas.ts`: `calcularEstatisticas(slug,
concursos, janela)` com janelas 10/50/100/`'total'` (últimos N; com menos concursos usa todos e informa
  `tamanho`), numa só passada: frequência por dezena (+ esperado N·k/total), atraso atual (`null` = não saiu
  na janela, listado em `semAparicao`; `rankingAtraso` só tem atraso conhecido), pares/ímpares (distribuição,
  média, esperado), soma (média, mín., máx., esperado) e repetidas do anterior (só concursos com o anterior
  na janela: `comparacoes = tamanho - 1`). Descrevem o passado, não preveem o sorteio. Verificação:
  `testes/fixtures/estatisticas-esperado.json` é gerado por SQL direto nos bancos `resultados_*.db` com
  `py -3.12 app/testes/gerar-esperado-estatisticas.py` (a partir da raiz; confere antes que o
  `historico.json` da fixture é idêntico ao banco até o último concurso da fixture). Regerar quando as
  fixtures forem renovadas. Gráficos e biblioteca: tarefa 7.2.
- Download sob demanda (`src/dados/sob-demanda.ts`): a abertura baixa só `index.json` +
  `ultimo.json` das 3 loterias. `tabela_geral` e `historico` vêm (1) quando a tela abre, por
  `sessao.garantir({ arquivos, loterias })` (devolve o cache se estiver sem rede), e (2) em
  segundo plano, quando a Início está atualizada: um tipo de arquivo por vez, em
  `requestIdleCallback`, em silêncio, e nunca com Save-Data ligado. Assim o app inteiro funciona
  sem rede mesmo que o usuário só tenha aberto a Início.
- `testes/`: fixtures (cópias de `publico/v1/`) e testes de dados.
