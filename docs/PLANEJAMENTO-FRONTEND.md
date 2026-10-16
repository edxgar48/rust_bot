# Planejamento — front-end (app PWA) (Fases 4 a 8)

Continuação de `docs/PLANEJAMENTO.md` (back-end, Fases 0 a 3, concluídas). O
ponto de contato entre os dois é o contrato de dados `docs/contrato-v1.md`.
Mudança de escopo ou de definição entra aqui primeiro, nunca só no código.

## Decisões (2026-10-14)

- **PWA**: uma base de código roda no navegador (desktop e celular) e se
  instala na tela inicial do Android e do iOS, sem loja. Funciona offline.
- **Pilha**: TypeScript + Svelte 5 com Vite (revisável na 4.1 se o protótipo
  mostrar problema), PWA por service worker (`vite-plugin-pwa`), testes com
  Vitest (lógica) e Playwright (ponta a ponta), gráficos em SVG próprio ou
  biblioteca leve (decidir na 7.1).
- **Local**: pasta `app/` neste repositório; um único repositório para
  contrato e app evoluírem juntos. `app/` tem `package.json` próprio e fica
  fora do `cargo build`.
- **Sem servidor**: o app só lê os JSON estáticos do contrato `v1`. Nada de
  API, conta, login ou banco no servidor. Dados do usuário (jogos salvos,
  preferências) ficam no aparelho (IndexedDB/localStorage).
- **Hospedagem: GitHub Pages** (decidido em 2026-10-15). O build é estático e
  funciona em qualquer hospedagem de arquivos; o deploy é por GitHub Actions
  a partir da `main`, em subcaminho (`/rust_bot/`), por isso caminhos relativos.
- **Escopo do MVP**: (1) último resultado das 3 loterias; (2) tabela de força
  por dezena; (3) histórico e conferência de jogo; (4) estatísticas e
  explicações. Fica fora do MVP: notificações push, premiação/rateio,
  outras loterias, contas, modelos de ML no app.
- **O app não calcula o motor.** Índice, posição, componentes e evidência vêm
  prontos de `tabela_geral.json`. Estatísticas simples sobre o histórico
  (frequência, atraso, pares/ímpares, soma) são calculadas no aparelho a
  partir de `historico.json`, em funções puras testadas; se alguma exigir a
  lógica do motor, vira pedido de campo novo no contrato (Fase 2), nunca
  reimplementação paralela.

## Princípios

- **Honestidade estatística.** Sorteios são independentes; o índice de força
  não prevê resultado. Toda tela que mostra ranking, índice ou evidência
  deixa isso claro, sem linguagem de "palpite certo", "dezena quente que vai
  sair" ou promessa de ganho. O texto vem do que o back-end já avalia (Fases
  1c, 1d, 1e). Toda tela de jogo mostra um aviso curto de jogo responsável.
- **Contrato é a fronteira.** O app segue `docs/contrato-v1.md` à risca:
  confere `schema_version` = 1, usa o `index.json` e o `sha256` (contrato,
  "Como o app deve usar o `index.json`"), ignora campos desconhecidos, trata
  ausência como `null`, nunca confia em ordem de chave. URL base numa
  configuração única.
- **Offline primeiro.** Última versão válida dos dados fica em cache; sem
  rede o app abre e diz de quando são os dados. Um arquivo que falha na
  conferência do hash não troca os dados locais.
- **Mobile primeiro, funciona em desktop.** Toque de 44 px, uso com uma mão,
  sem rolagem horizontal da página; tema claro e escuro; acessível (contraste,
  foco, leitor de tela; cor nunca é a única informação).
- **Português**: código, nomes e textos da interface, seguindo o estilo do
  projeto. Textos de tela em um só módulo (facilita revisão e futura
  tradução).
- **Sem segredo e sem rastreio**: sem analytics de terceiros no MVP; nada
  sai do aparelho.

## Telas do MVP

| Tela | Conteúdo | Dados |
|---|---|---|
| Início | Cartão por loteria: último concurso, data, dezenas sorteadas; "dados de <data>" e estado de atualização | `index.json`, `ultimo.json` |
| Tabela de força | Ranking das dezenas do próximo concurso: posição, variação (subiu/caiu), índice, grau de evidência; alternar lista e volante colorido; detalhe da dezena com os 9 componentes | `tabela_geral.json` |
| Histórico | Lista paginada/virtualizada de concursos, busca por número e data, filtro por dezena | `historico.json` |
| Conferir jogo | Usuário marca dezenas (volante da loteria), vê acertos em cada concurso e o melhor resultado histórico; salva jogos no aparelho | `historico.json` |
| Estatísticas | Frequência, atrasos, pares/ímpares, soma, repetição do concurso anterior, por loteria e janela | `historico.json` |
| Entenda | O que é o índice, o que a evidência significa, por que não prevê sorteio, de onde vêm os dados, versão do contrato e do perfil (`perfil`, `hash_toml`) | `tabela_geral.json` (cabeçalho) |
| Ajustes | Loteria padrão, tema, limpar dados locais, versão do app | local |

## Fases

### Fase 4 — Fundação do app

| # | Tarefa | Pronto quando |
|---|---|---|
| 4.1 | Esqueleto em `app/`: Vite + Svelte + TypeScript estrito, lint, formatação, Vitest, `npm run build` gerando estático; `app/README.md` e comandos no `CLAUDE.md` | build, lint e testes rodam do zero em máquina limpa |
| 4.2 | Camada de dados (`app/src/dados/`): cliente do contrato com URL base configurável, validação de `schema_version`, conferência de SHA-256 e `bytes`, tipos TypeScript dos 4 arquivos, tolerância a `null` e campo novo | testes com os JSON reais de `publico/v1/` copiados para `app/testes/fixtures/` e com arquivos adulterados (hash errado, `schema_version` 2, loteria `null` no índice) |
| 4.3 | Cache e offline: guardar a última versão válida por arquivo (IndexedDB), atualizar só o que mudou no `sha256`, service worker para o app shell | app abre em modo avião com dados da última sincronização; hash inválido não troca os dados locais |
| 4.4 | Sistema de design: tokens de cor/tipografia/espaçamento, tema claro/escuro, componentes base (cartão, bola de dezena, volante, lista, aviso), navegação por abas | catálogo de componentes verificado em 360 px e 1280 px |
| 4.5 | Decisão de estilo visual validada com mockups das telas principais (skill `app-mockup`) antes de construir as telas | usuário aprova os mockups |

### Fase 5 — Telas de resultado e ranking

| # | Tarefa | Pronto quando |
|---|---|---|
| 5.1 | Início: cartões das 3 loterias com último concurso, estado de atualização e erro de rede | mostra dados reais; sem rede mostra o cache com a data |
| 5.2 | Tabela de força: lista ordenada, variação de posição, grau e direção de evidência com rótulos de exibição (mapeamento dos identificadores do contrato, com teste que cobre todos os valores) | confere com `sync tabela` para as 3 loterias |
| 5.3 | Volante colorido e detalhe da dezena (9 componentes, `null` mostrado como "sem dados na janela", nunca zero) | `null` e valores extremos tratados |
| 5.4 | Tela "Entenda" e avisos: textos revisados por quem entende o motor (reaproveitar a leitura de `docs/GUIA.md`) | revisão de linguagem aprovada; nenhum texto sugere previsão |

### Fase 6 — Histórico e conferência de jogo

| # | Tarefa | Pronto quando |
|---|---|---|
| 6.1 | Histórico: lista virtualizada, busca por concurso/data, filtro por dezena | fluida com o maior histórico (cerca de 340 KB) em celular modesto |
| 6.2 | Conferir jogo: seleção no volante com a quantidade válida da loteria (Lotofácil 15–20, Mega-Sena 6–20, Quina 5–15), acertos por concurso e melhor resultado histórico | função pura de conferência testada com valores à mão |
| 6.3 | Jogos salvos no aparelho (IndexedDB): criar, nomear, apagar, exportar e importar como arquivo | sobrevive a recarregar a página e a atualizar dados |
| 6.4 | Conferência automática: ao chegar concurso novo, o app mostra o resultado dos jogos salvos | testado com concurso simulado |

### Fase 7 — Estatísticas

| # | Tarefa | Pronto quando |
|---|---|---|
| 7.1 | Funções puras de estatística sobre o histórico (frequência, atraso atual, pares/ímpares, soma, repetidas do anterior, janelas 10/50/100/total) e escolha da biblioteca de gráficos | resultados idênticos a cálculo independente (ex.: SQL sobre os bancos) nas 3 loterias |
| 7.2 | Telas de estatísticas com gráficos acessíveis (cor não é a única informação; tabela alternativa) | revisão de acessibilidade ok |
| 7.3 | Comparação com o esperado ao acaso (hipergeométrica/binomial) junto de cada estatística, para o usuário ver que o desvio é normal | números conferem com os da avaliação do back-end |
| 7.4 | Seção "Por evidência" na aba Estatísticas (pedido do usuário, 2026-10-15): as dezenas da `tabela_geral.json` agrupadas por grau e direção de evidência, mantendo a ordem de posição dentro de cada grupo, para ver quais dezenas têm o mesmo grau juntas | dados idênticos à tabela do motor; textos sem sugerir que o agrupamento prevê o sorteio; acessível em 360 px |
| 7.5 | Botão "Inserir" por dezena na tabela da Força (pedido do usuário, 2026-10-16): entre a variação e o índice, leva a dezena para o volante de Conferir jogo já marcada, para completar e salvar. Rascunho só em memória de módulo (`telas/rascunho-jogo.ts`): sem duplicar, respeita o máximo da loteria (feedback em texto), outra loteria começa rascunho novo, tocar de novo remove; atalho "N dezenas no jogo · Abrir em Conferir" leva ao hash do jogo. O rascunho só é consumido após esse atalho, na montagem de Conferir jogo (volante vazio; jogo de Meus jogos tem precedência), sem nome | alvo de toque >= 44 px e `aria-label` por dezena; 360 px sem rolagem horizontal; textos só em `textos.ts` e sem linguagem proibida; testes do store, da Força e da integração com Conferir |

### Fase 8 — Qualidade, PWA e publicação

| # | Tarefa | Pronto quando |
|---|---|---|
| 8.1 | Instalação como PWA: manifesto, ícones, tela inicial, splash; teste no Android (Chrome) e iOS (Safari) | instalado nos dois sistemas; abre offline |
| 8.2 | Testes ponta a ponta (Playwright) com contrato de teste local: fluxos de início, ranking, conferência, offline, hash inválido | verdes no CI local |
| 8.3 | Desempenho e acessibilidade (Lighthouse): meta PWA válido, acessibilidade ≥ 90, carregamento inicial rápido em 4G | metas atingidas e registradas |
| 8.4 | **Decisão de hospedagem** (em aberto) e deploy do build estático; URL final entra no `app/README.md` | app abre pela URL pública no celular |
| 8.5 | Runbook do front-end no `app/README.md`: contrato mudou de versão, dados atrasados, como atualizar o app, como mudar a URL base | cobre os cenários acima |
| 8.6 | Verificação de contrato no build: teste que roda o app contra a saída real do `sync exportar` (e falha se o contrato mudar sem atualização do app) | quebra de contrato é detectada antes do deploy |

**Pronto quando:** o app instalado no celular e aberto no navegador mostra
os resultados e o ranking do mesmo dia em que o `sync ciclo` publica; abre
sem rede com os últimos dados; confere um jogo salvo; e nenhuma tela sugere
que o índice prevê o sorteio.

## Ordem e dependências

4 → 5 → 6 → 7 → 8. Dentro da Fase 4, a 4.5 (mockups) pode andar em paralelo
com a 4.2/4.3, mas as telas só começam depois dela. A 6 e a 7 dependem só da
camada de dados da 4.2 e podem trocar de ordem. O back-end só muda se o
front-end pedir **campo novo opcional** no contrato (ex.: evidência do índice
B em `tabela_geral.json`, hoje fora): o pedido vira tarefa na Fase 2 e entra
em `docs/contrato-v1.md` antes.

## Agentes e skills

Falta um agente de domínio para o front-end. Proposta: `engenheiro-app`
(Fases 4 a 8; Read/Edit/Write/Bash; usa `contrato-dados` e o `auditor-contrato`
para conferir leitura), a criar em `.claude/agents/` antes da 4.1, e ajustar
o `orquestrador` para delegar a ele. O gate de qualidade do app (lint, tipos,
Vitest, build) entra na skill `verificar` quando `app/` existir.

## Decisões em aberto

- ~~Hospedagem do app~~ — decidido em 2026-10-15: GitHub Pages, deploy por GitHub Actions a partir da `main` do repositório `edxgar48/rust_bot` (público); URL esperada `https://edxgar48.github.io/rust_bot/` (8.4).
- ~~Biblioteca de gráficos~~ — decidido em 2026-10-15: SVG próprio, sem dependência (7.2).
- Estilo visual e identidade (4.5).
- ~~Backup dos jogos salvos~~ — decidido em 2026-10-15: o backup é o arquivo exportado (sem nuvem); o app avisa que os jogos ficam só no aparelho (6.3).

## Backlog do front-end

- Notificação de concurso novo (push exige servidor; avaliar alternativas).
- Evidência do índice (B) na tela de ranking, depois de entrar no contrato.
- Popularidade e rateio (depende da fase de premiação do back-end).
- Outras loterias (depende do back-end).
- Empacotar como app de loja (Capacitor/TWA), se o PWA não bastar.
- Tradução e internacionalização.
