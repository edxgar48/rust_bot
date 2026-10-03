# Progresso do plano

Estado das tarefas de `docs/PLANEJAMENTO.md`. Mantido pelo agente
`orquestrador`: uma tarefa só é marcada `[x]` depois de passar no gate
(`/verificar` + revisão do `revisor-arquitetura`).

Legenda: `[ ]` pendente · `[~]` em andamento · `[x]` concluída · `[!]` bloqueada (ver nota)

## Fase 0 — Consolidar a base · `engenheiro-dados`

- [x] 0.1 Faixa de dezenas e layout do volante no `LoteriaConfig`
- [x] 0.2 Comando `validar <loteria>`
- [x] 0.3 Validação aplicada ao gravar (`importar`, `baixar`, `atualizar`)
- [ ] 0.4 Testes do parser com fixtures
- [ ] 0.5 `todas` no lugar do slug
- [ ] 0.6 Textos da CLI sem ".htm"

## Fase 1a — Features · `engenheiro-motor`

- [ ] 1a.1 Estado incremental por dezena e janela (`src/motor/`)
- [ ] 1a.2 Tabela `features_dezena` + `motor_estado` + comando `sync features`
- [ ] 1a.3 Testes: golden, vazamento, incremental = completo

## Fase 1b — Índice de força · `engenheiro-motor`

- [ ] 1b.1 Leitura de perfis `.toml` (`perfis/`) + tabela `perfis`
- [ ] 1b.2 Normalização, total, posição e variação de posição
- [ ] 1b.3 Tabela `indice` + comando `sync indice`

## Fase 1c — Avaliação · `engenheiro-avaliacao`

- [ ] 1c.1 Baseline do acaso + log loss / Brier
- [ ] 1c.2 Top-k vs hipergeométrica
- [ ] 1c.3 Walk-forward + conjunto final trancado
- [ ] 1c.4 Testes de aleatoriedade (qui-quadrado, runs)
- [ ] 1c.5 Comando `sync avaliar`

## Fase 1.5 — Ponte com Python · `cientista-ml`

- [ ] 1.5.1 Comando `sync dataset --formato parquet|csv`
- [ ] 1.5.2 `ml/`: leitura do dataset + baseline
- [ ] 1.5.3 Busca de pesos com walk-forward → perfis `.toml`
- [ ] 1.5.4 Vetores de conformidade (`testes/conformidade/`) em `cargo test` e `pytest`

## Fase 2 — Exportação · `engenheiro-publicacao`

- [ ] 2.1 Comando `sync exportar` (`index.json`, `ultimo.json`, `tabela_geral.json`, `historico.json`)
- [ ] 2.2 Geração determinística + hash SHA-256 + `schema_version`
- [ ] 2.3 `docs/contrato-v1.md`
- [ ] 2.4 Comando `sync publicar` (depende de decisão em aberto)

## Fase 3 — Automação · `engenheiro-operacao`

- [ ] 3.1 Comando `sync ciclo`
- [ ] 3.2 Calendário de sorteios no `config.rs`
- [ ] 3.3 Retentativas com backoff
- [ ] 3.4 Trava contra execução simultânea
- [ ] 3.5 Logs em arquivo
- [ ] 3.6 Alertas (depende de decisão em aberto)
- [ ] 3.7 Agendamento no Windows (`.ps1`)
- [ ] 3.8 Runbook no README

## Decisões em aberto

- [ ] Destino da publicação — bloqueia 2.4
- [ ] Canal de alerta — bloqueia 3.6
- [ ] Tamanho do conjunto trancado (sugestão ~10%) — necessário em 1c.3
- [ ] Buraco já existente no banco deve bloquear `importar`/`baixar`? (ver nota 0.3) — relevante para 3.1
- [ ] Regra "data posterior a hoje" ao gravar (ver nota 0.2)

## Notas

<!-- O orquestrador registra aqui bloqueios, desvios do plano e decisões tomadas, com data. -->

- 2026-10-03 — 0.1: "layout do volante" não está detalhado no plano. Interpretação
  adotada (sem decisão do usuário, revisável): grade linhas × colunas do volante
  oficial — Lotofácil 5×5 (1–25), Mega-Sena 6×10 (1–60), Quina 8×10 (1–80).
  **Confirmado pelo usuário em 2026-10-03**: dezenas em ordem crescente,
  preenchendo linha por linha (`posicao_no_volante`).
- 2026-10-03 — 0.1: implementada em `src/config.rs` (`dezena_min`/`dezena_max: u32`,
  struct `Volante`, `qtd_dezenas_possiveis()`, `dezenas()`, `contem()`, 7 testes).
  Desvios: `u32` em vez de `u8` (casa com `Resultado::dezenas`); struct `Volante`
  em vez de campos soltos. Revisão: aprovada com ressalvas, achado MÉDIA (doc comment)
  corrigido em uma rodada. clippy e testes ok. Fica `[~]`: `cargo fmt --check` falha
  por formatação pré-existente em `src/bin/sync.rs`, `src/db.rs`, `src/downloader.rs`
  (arquivos não tocados); aguarda autorização do usuário para um commit só de `cargo fmt`.
- 2026-10-03 — 0.1 concluída. `cargo fmt` aplicado em `src/bin/sync.rs`, `src/db.rs`,
  `src/downloader.rs` (só formatação, sem mudança de lógica — dispensa teste manual
  do downloader). Adicionado `posicao_no_volante()` + 2 testes. Gate aprovado:
  fmt ok, clippy ok, 9 testes passando.
- 2026-10-03 — 0.2: interpretação adotada pelo orquestrador (revisável): buracos contados
  de 1 até o maior concurso e reportados em faixas; data não pode ser anterior à do
  concurso anterior (datas iguais são permitidas); duplicatas na entrada contam como problema.
  Implementada em `src/validacao.rs` (funções puras `validar`, `validar_resultado`,
  `validar_sequencia`, reutilizáveis na 0.3) + subcomando `validar` em `sync.rs`.
  Desvios: regras extras `LoteriaDiferente` e `ConcursoZero`; banco vazio ou inexistente dá erro.
  Gate de código ok: fmt, clippy e 27 testes. Revisão: aprovada com ressalvas, só achados BAIXA.
  Limitações registradas: `listar` filtra por loteria, então `LoteriaDiferente` não dispara
  pela CLI; quando a data errada é posterior, o problema aponta o concurso seguinte.
  **Fica `[~]`: os dados reais reprovam.** megasena ok. lotofacil: falta o 3793.
  quina: falta o 7131, e o 380 tem data 1997-03-12 entre 1998-03-08 e 1998-03-15
  (provável erro de digitação na planilha da Caixa). Aguarda decisão do usuário.
- 2026-10-03 — 0.2: **decisões do usuário.** (1) Erros conhecidos da planilha da Caixa
  são corrigidos antes de gravar, por uma lista de exceções por loteria + concurso
  (`config::CORRECOES_DATA`, aplicada por `src/correcoes.rs` em `importar`, `baixar` e
  `atualizar`). A correção só é aplicada se a data vier exatamente com o erro conhecido;
  se vier com outro valor, gera aviso no stderr e não altera nada. Primeira entrada:
  Quina 380, 1997-03-12 → 1998-03-12. (2) Regra ajustada: data fora de ordem passa a ser
  **aviso** (não reprova o `validar`) e a mensagem orienta a cadastrar a correção; os
  demais problemas continuam sendo erro. (3) `sync baixar lotofacil` e `sync baixar quina`
  executados: entraram lotofacil 3793 e quina 7131, e a quina 380 foi corrigida.
  `validar` nos três bancos: nenhum problema (lotofacil 1–3795, megasena 1–3065,
  quina 1–7133). Gate: fmt ok, clippy ok, 34 testes. **0.2 concluída.**
- 2026-10-03 — 0.3: decisão do usuário: `atualizar` mantém a regra atual para buracos
  (avisa e sugere `baixar`, sem recusar a gravação) por enquanto.
- 2026-10-03 — 0.2: segunda revisão (só das mudanças novas): aprovada com ressalvas, sem
  achado ALTA. Corrigidos: unicidade de (loteria, concurso) em `CORRECOES_DATA` garantida
  por teste (MÉDIA); texto do `--help` do `validar`; comentário orientando a não remover
  correções enquanto a planilha mantiver o erro. **Limitação para a 0.3:** com data fora
  de ordem sendo aviso, uma data no futuro no último concurso (ex.: 2062 em vez de 2026)
  não gera problema nenhum — avaliar um erro para "data posterior a hoje" ao gravar.
- 2026-10-03 — 0.3: interpretação adotada pelo orquestrador (revisável): validar depois das
  correções e antes de gravar, em `importar`, `baixar` e `atualizar`; erro em qualquer registro
  → nada é gravado e o comando sai com exit ≠ 0; a sequência é validada sobre banco existente +
  lote (o lote substitui concursos iguais); duplicata no lote é erro; data fora de ordem é aviso
  (só os pares que envolvem concursos do lote, para não repetir avisos antigos); buraco é erro em
  `importar`/`baixar` e aviso em `atualizar`. Nenhuma regra nova.
  Implementada em `src/validacao.rs` (função pura `avaliar_gravacao`, `ModoGravacao`,
  `DecisaoGravacao`) e `src/bin/sync.rs` (`validar_lote`, `carregar_existentes`). O banco só é
  aberto/criado depois de a validação aprovar, então um lote recusado não deixa `.db` vazio.
  Mudanças de comportamento: `atualizar` com banco vazio agora avisa o buraco 1..N-1 e grava;
  `atualizar` lê o histórico inteiro para validar. `db::ultimo_concurso` ficou sem uso (mantida).
  Gate: fmt ok, clippy ok, 50 testes, `validar` sem problemas nos três bancos. Revisão: 1ª aprovada
  com ressalvas (1 MÉDIA, achados BAIXA corrigidos em uma rodada); 2ª aprovada.
  **Pendente de decisão (achado MÉDIA):** um buraco que já está no banco (ex.: `atualizar` gravou
  o 105 com o banco em 100) faz `baixar` recusar o lote inteiro se a planilha ainda não chegou ao
  104, inclusive os concursos que reduziriam o buraco. Alternativa sugerida pela revisão: em
  `importar`/`baixar`, tratar como erro só o buraco abaixo do maior concurso do lote.
