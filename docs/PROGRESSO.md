# Progresso do plano

Estado das tarefas de `docs/PLANEJAMENTO.md`. Mantido pelo agente
`orquestrador`: uma tarefa só é marcada `[x]` depois de passar no gate
(`/verificar` + revisão do `revisor-arquitetura`).

Legenda: `[ ]` pendente · `[~]` em andamento · `[x]` concluída · `[!]` bloqueada (ver nota)

## Fase 0 — Consolidar a base · `engenheiro-dados`

- [x] 0.1 Faixa de dezenas e layout do volante no `LoteriaConfig`
- [ ] 0.2 Comando `validar <loteria>`
- [ ] 0.3 Validação aplicada ao gravar (`importar`, `baixar`, `atualizar`)
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
