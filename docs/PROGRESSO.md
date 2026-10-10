# Progresso do plano

Estado das tarefas de `docs/PLANEJAMENTO.md`. Mantido pelo agente
`orquestrador`: uma tarefa só é marcada `[x]` depois de passar no gate
(`/verificar` + revisão do `revisor-arquitetura`).

Legenda: `[ ]` pendente · `[~]` em andamento · `[x]` concluída · `[!]` bloqueada (ver nota)

## Fase 0 — Consolidar a base · `engenheiro-dados`

- [x] 0.1 Faixa de dezenas e layout do volante no `LoteriaConfig`
- [x] 0.2 Comando `validar <loteria>`
- [x] 0.3 Validação aplicada ao gravar (`importar`, `baixar`, `atualizar`)
- [x] 0.4 Testes do parser com fixtures
- [x] 0.5 `todas` no lugar do slug
- [x] 0.6 Textos da CLI sem ".htm"
- [x] 0.7 Fechar buracos à mão (`buracos`, `inserir`) com cache de inserções manuais
- [x] 0.8 `atualizar` com salto: backup automático e caminho do `baixar`

## Fase 1a — Features · `engenheiro-motor`

- [x] 1a.1 Estado incremental por dezena e janela (`src/motor/`)
- [x] 1a.2 Tabela `features_dezena` + `motor_estado` + comando `sync features`
- [x] 1a.3 Testes: golden, vazamento, incremental = completo

## Fase 1b — Índice de força · `engenheiro-motor`

- [x] 1b.1 Leitura de perfis `.toml` (`perfis/`) + tabela `perfis`
- [x] 1b.2 Normalização, total, posição e variação de posição
- [x] 1b.3 Tabela `indice` + comando `sync indice`
- [x] 1b.4 Comando `sync tabela` (CSV da tabela geral por concurso)

## Fase 1c — Avaliação · `engenheiro-avaliacao`

- [x] 1c.1 Baseline do acaso + log loss / Brier
- [x] 1c.2 Top-k vs hipergeométrica
- [x] 1c.3 Walk-forward + conjunto final trancado
- [x] 1c.4 Testes de aleatoriedade (qui-quadrado, runs)
- [x] 1c.5 Comando `sync avaliar`

## Fase 1.5 — Ponte com Python · `cientista-ml`

- [x] 1.5.1 Comando `sync dataset --formato parquet|csv`
- [x] 1.5.2 `ml/`: leitura do dataset + baseline
- [x] 1.5.3 Busca de pesos com walk-forward → perfis `.toml`
- [x] 1.5.4 Vetores de conformidade (`testes/conformidade/`) em `cargo test` e `pytest`

## Fase 1d — Índice de evidência bayesiano · `engenheiro-avaliacao`

- [x] 1d.1 Conferir os desvios de frequência da 1c.4 contra fonte independente
- [x] 1d.2 Evidência por dezena (fator de Bayes, cinco graus) + `sync evidencia`
- [x] 1d.3 Evidência do índice de força (FB prequencial) no `sync evidencia`
- [ ] 1d.4 Grau e direção no CSV do `sync tabela` + `docs/GUIA.md`

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
- [x] Tamanho do conjunto trancado → 10%, corte fixo por número de concurso (ver nota de 2026-10-05)
- [x] Buraco já existente no banco deve bloquear `importar`/`baixar`? → não: só aviso (ver nota 0.7)
- [ ] Regra "data posterior a hoje" ao gravar (ver nota 0.2)
- [x] Ambiente Python, dependências e definição do "baseline" da `ml/` → resolvidos (ver notas de 2026-10-06)
- [x] Definição do walk-forward em Rust, dobras e uso do trancado — bloqueia 1c.3 → resolvida (ver notas de 2026-10-05)
- [x] Nível de significância e correção por múltiplas dezenas na 1c.4 → α = 5% por família, Holm (ver nota de 2026-10-05)
- [x] Conversão do índice em probabilidade e regras do log loss / Brier — bloqueia 1c.1 → resolvidas (ver notas de 2026-10-05)
- [x] Formato de `perfis.criado_em` e pontos ambíguos do `sync indice` — bloqueia 1b.3 → resolvidos (ver notas de 2026-10-05)
- [x] Pontos ambíguos da normalização, posição e variação de posição — bloqueia 1b.2 → resolvidos (ver notas de 2026-10-04)
- [x] Pontos ambíguos da especificação das features — bloqueia 1a.1 → resolvidos (ver notas de 2026-10-04)

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
- 2026-10-03 — 0.7: **decisões do usuário.** Buraco na numeração vira só aviso em toda
  gravação (`importar`, `baixar`, `atualizar`); no `validar` continua erro, para o buraco
  não ficar esquecido. Buracos podem ser fechados à mão, um a um, e as inserções ficam num
  cache versionado no git. A busca automática dos concursos que faltam fica para a 3.1,
  gravando no mesmo cache.
  Implementado: `src/manuais.rs` (cache `cache/manuais_<loteria>.json`, uma entrada por
  linha, ordenado por data de sorteio e concurso, dezenas em ordem crescente; conciliação
  com a fonte oficial), subcomandos `buracos` e `inserir`, opção `--cache` em `importar`,
  `baixar`, `atualizar` e `inserir`. `ModoGravacao` removido de `validacao.rs` (deixou de
  ter diferença entre os modos). `inserir` recusa concurso que já está no banco e trata
  data fora de ordem como erro. Gate: fmt ok, clippy ok, 60 testes. Teste de ponta a ponta
  em cópias dos bancos (scratchpad): `buracos`, `inserir` (data errada, concurso existente,
  dezena fora da faixa e caso certo), reaplicação do cache em banco recriado, conciliação
  igual (removida do cache) e divergente (aviso, mantida).
- 2026-10-03 — 0.7: revisão do `revisor-arquitetura`: aprovada com ressalvas, sem ALTA.
  Corrigidos: (MÉDIA) `inserir` grava o cache antes do banco, que é a fonte da verdade do
  dado digitado; (MÉDIA) entrada manual nunca sobrescreve concurso que já está no banco:
  `pendentes` considera o banco e, se o conteúdo diferir, gera aviso `DifereDoBanco`
  (testado de ponta a ponta com planilha antiga sem o concurso); (BAIXA) `atualizar`
  concilia o cache também quando já está atualizado; (BAIXA) erro próprio
  `CacheManualInvalido`; (BAIXA) `carregar` ordena as dezenas e recusa concurso repetido
  no cache; (BAIXA) `.gitattributes` com `cache/*.json text eol=lf`. Mantido de propósito:
  `inserir` aceita concurso acima do maior do banco (só avisa o buraco que cria).
  Gate: fmt ok, clippy ok, 63 testes.
- 2026-10-03 — 0.4: fixtures em `tests/fixtures/` (concursos 1–5 reais de cada loteria + bordas,
  ~50 KB), geradas por `tests/comum/mod.rs` imitando o XML que a Caixa publica (prefixo `x:`,
  dimension só do cabeçalho, linhas sem `r=`, data como texto `DD/MM/AAAA`, números com zero à
  esquerda lidos como `Float`). Regenerar: `cargo test --test parser -- --ignored gerar_fixtures`.
  Testes de integração em `tests/parser.rs` e de `normalizar_data` em `src/parser.rs`. Produção
  não mudou. Desvio: `[dev-dependencies] zip = "8.6"` (mesmo crate/versão que o calamine já usa).
  Revisão: aprovada com ressalvas, sem ALTA; corrigidos em uma rodada (MÉDIA) teste de fixtures
  compara o conteúdo descompactado, não o contêiner ZIP; (BAIXA) `tests/fixtures/*.xlsx binary`
  no `.gitattributes`; (BAIXA) `#![allow(dead_code)]` em `tests/comum/mod.rs`.
  Gate: fmt ok, clippy ok, 77 testes (1 ignorado: o gerador), `validar` sem problemas nos três bancos.
  **Pendência (sem tarefa no plano):** fragilidades do parser, sem efeito nos dados atuais —
  data em célula de data do Excel (`Data::DateTime`) faz a linha ser descartada em silêncio (teste
  das bordas registra esse comportamento); `normalizar_data` não confere dígitos nem dia/mês
  (a validação da 0.3 barra na gravação); `as_i64() as u32` trunca `Float` e dá a volta em negativo.
- 2026-10-04 — 0.5: interpretação adotada pelo orquestrador (revisável; sem como perguntar ao
  usuário nesta execução): `todas` vale em `baixar`, `atualizar`, `listar`, `validar` e `buracos`,
  percorrendo `config::LOTERIAS` na ordem do config; `importar` e `inserir` recusam `todas` com erro
  claro (arquivo e dezenas são de uma loteria só). `--db` com `todas` é erro (um banco por loteria).
  Uma loteria com falha não interrompe as demais; no fim, resumo por loteria e exit ≠ 0 se alguma
  falhou. `listar todas` imprime um JSON por loteria (objeto `{slug: [...]}`), sem juntar bancos.
- 2026-10-04 — 0.5 concluída. `config::TODAS`, `eh_todas`, `resolver` (src/config.rs) e, em
  src/bin/sync.rs, `resolver_alvos` (devolve `Alvos { multiplas, itens }` e concentra as recusas),
  `Execucao` (segue adiante por loteria, resumo, exit ≠ 0 se alguma falhou), `PorLoteria` (JSON de
  `listar todas` na ordem do config), `buscar_config_unica` (importar/inserir recusam `todas`).
  `baixar`/`atualizar` viraram funções async sem mudança de lógica. README atualizado.
  Desvios: `--seletor` com `todas` também é erro; cabeçalho e resumo vão para o stdout (exceto em
  `listar`, onde o stdout só tem o JSON), e o erro de cada loteria vai sempre para o stderr;
  **mudança de comportamento:** `listar` com banco inexistente agora falha com "banco não encontrado"
  em vez de criar um `.db` vazio (vale também para slug único). `listar todas` com falha parcial
  deixa a loteria que falhou fora do objeto (`{}` se todas falharem), com exit 1; rever no
  `exportar todas` da Fase 2 (o contrato pede null explícito para ausência).
  Revisão: 1ª aprovada com ressalvas (2 MÉDIA: `listar` criava banco vazio, faltavam testes da
  orquestração; 2 BAIXA), corrigidas numa rodada; 2ª aprovada com ressalvas, só 1 BAIXA.
  Gate: fmt ok, clippy ok, 92 testes (71 lib + 11 sync + 10 parser, 1 ignorado), `validar todas`
  sem problemas nos três bancos (exit 0).
  **Pendência BAIXA:** o README não diz o que `listar todas` faz numa falha parcial (JSON
  incompleto, exit 1). Pode entrar junto da 0.6, que também mexe nos textos da CLI.
- 2026-10-04 — 0.5, decisão do usuário: `listar` não aceita `todas` (cada loteria tem seu banco;
  um JSON com as três não tem uso). `listar` recusa `todas` via `buscar_config_unica`; saíram
  `PorLoteria` e o parâmetro `Saida` de `Execucao` (cabeçalho e resumo sempre no stdout). Mantido:
  `listar` com banco inexistente falha com "banco não encontrado". A pendência BAIXA acima deixa
  de existir. PLANEJAMENTO.md (0.5) e README atualizados com os comandos em que `todas` vale.
  `src/bin/sync.rs` voltou a LF, igual ao commitado. Gate: fmt ok, clippy ok, 92 testes
  (71 lib + 11 sync + 10 parser, 1 ignorado).
- 2026-10-04 — 0.6 concluída. Textos do `--help` em `src/bin/sync.rs` (`about`, `baixar`, `--dir`,
  `importar`, `<ARQUIVO>`) e o doc comment de `baixar_resultado` em `src/downloader.rs` passam a falar
  em "planilha (.xlsx)". Só texto: nenhuma flag, nome ou lógica mudou. Mantidos de propósito
  `src/downloader.rs:136`, `src/parser.rs:7` e `README.md:18`, que explicam corretamente por que o link
  parece `.htm`. Revisão: aprovada, só 1 BAIXA (cópias de trabalho em LF enquanto boa parte de `src/`
  está em CRLF; sem efeito no commit). Gate: fmt ok, clippy ok, 92 testes (71 lib + 11 sync +
  10 parser, 1 ignorado), `validar todas` sem problemas nos três bancos. **Fase 0 concluída.**
- 2026-10-04 — 1a.1 não iniciada: a especificação das features tem pontos ambíguos que mudam
  valores calculados (e o golden de bordas). Perguntas devolvidas ao usuário: (1) `repeticao_atual`
  é limitada pela janela? (2) `espaco_final` de dezena que nunca saiu antes de *t*: `NULL` ou outro
  valor? (3) janela pelo número do concurso, com o motor recusando histórico com buraco? (4) o motor
  gera também o snapshot de *t* = último + 1 (estado atual, `saiu` = `NULL`) para o app? (5) como a
  janela "total" é identificada na coluna `janela`?
- 2026-10-04 — Respostas do usuário, registradas no PLANEJAMENTO.md (Convenções, Componentes e
  Casos de borda): (1) `repeticao_atual` é limitada pela janela (no máximo *J*); (2) `espaco_final`
  de dezena que nunca saiu antes de *t* = `NULL`; (3) janela pelo número do concurso, motor recusa
  histórico com buraco e `sync features` recusa rodar se o `validar` apontar buraco; (4) o motor
  gera também o snapshot de *t* = último + 1, com `saiu` = `NULL` (gravar ou calcular na hora fica
  para a 1a.2); (5) janela total = `janela = 0`, por constante. 1a.1 desbloqueada.
- 2026-10-04 — 1a.1 concluída. `src/motor/` (`mod.rs`, `features.rs`, `estado.rs`, `tests.rs`), puro,
  sem I/O nem SQL: `FEATURE_VERSION = 1`, `JANELA_TOTAL = 0`, `JANELAS = [0, 10, 50, 100]`,
  `calcular(cfg, &[Resultado])`, `EstadoMotor` (serde; `novo`, `adicionar`, `avancar` tudo ou nada,
  `snapshot` do próximo concurso com `saiu = None`, `verificar_compatibilidade`), `FeaturesDezena`.
  Um único cálculo (`Acumulado::incorporar`): janela total incremental, janelas 10/50/100 recalculadas
  sobre as aparições dos últimos 100 concursos. Erros novos em `src/error.rs` (`MotorBuraco`,
  `MotorConcursoRepetido`, `MotorForaDeOrdem`, `MotorResultadoInvalido`, `MotorEstadoIncompativel`).
  Desvios (aceitos pela revisão, revisáveis): histórico precisa começar no concurso 1; `JANELAS` em
  ordem crescente, para casar com a PK; o motor recusa resultado que reprove em `validar_resultado`
  (inclusive data); `feature_version` fica no estado, não em `FeaturesDezena`.
  Medição (release, cópias dos bancos): Quina 7133 concursos → 2.282.560 linhas em ~125 ms.
  29 testes no módulo (golden, bordas, oráculo independente, vazamento em 7 valores de t,
  incremental = completo com retomada via JSON, erros). verificador-vazamento: sem vazamento.
  Revisão: aprovada com ressalvas, só BAIXA. Gate: fmt ok, clippy ok, 121 testes (100 lib + 11 sync +
  10 parser, 1 ignorado), `validar todas` ok nos três bancos.
  **Entrada para a 1a.2:** `db::listar` devolve `ORDER BY concurso DESC` — ordenar crescente antes
  do motor; ao carregar `motor_estado`, verificar uma vez (ex.: `EstadoMotor::retomar`), inclusive
  antes de `snapshot()`; o estado não detecta `resultados` corrigido abaixo do último concurso
  incorporado (guardar impressão dos resultados ou regenerar do zero); gravar `feature_version` a
  partir do estado. Opcional: reexport em `lib.rs`. Lacunas para a 1a.3: corte k=0/k=n no
  incremental = completo, vazamento na Lotofácil.
- 2026-10-04 — 1a.2: interpretação adotada pelo orquestrador (revisável; sem como perguntar ao
  usuário nesta execução): (1) o snapshot de *t* = último + 1 é **gravado** em `features_dezena`
  com `saiu = NULL` e substituído quando o concurso entra (assim `indice` e `exportar` leem tudo
  da tabela; o dataset da 1.5.1 filtra `saiu IS NOT NULL`); (2) `motor_estado` guarda uma linha
  com o `EstadoMotor` serializado em JSON, `feature_version`, último concurso incorporado e uma
  impressão determinística dos resultados incorporados; impressão divergente (resultado corrigido
  abaixo do último) ou `feature_version` diferente → regenera do zero; (3) `sync features` aceita
  uma loteria só (o plano, 0.5, não lista `features` entre os comandos com `todas`); (4) recusa
  rodar se `validar` apontar erro (buraco incluso), sem gravar nada.
- 2026-10-04 — 1a.2 concluída. `src/orquestracao.rs` (novo, na lib: `sincronizar_features`, sem SQL),
  `src/db.rs` (`listar_crescente`, funções de `features_dezena`/`motor_estado`, `iniciar_escrita` com
  BEGIN IMMEDIATE, INSERT em blocos de 64), `src/error.rs` (`ResultadosReprovados`, `SemResultados`),
  subcomando `features <loteria> [--db] [--completo]` em `src/bin/sync.rs`, README, `tests/features.rs`.
  Schema: `features_dezena` WITHOUT ROWID com as colunas do plano (`frequencia`, `repeticoes_total`,
  `repeticao_atual` NOT NULL; demais features e `saiu` aceitam NULL); `motor_estado(id=1, loteria,
  feature_version, ultimo_concurso, impressao "fnv1a64:<qtd>:<hex>", estado JSON)`. Regenera do zero
  quando: sem estado, versão diferente, JSON ilegível, incompatível, impressão dos resultados ≤ último
  mudou, contagem de linhas de features incoerente, ou `--completo`. Tudo numa transação; recusa e
  banco vazio não gravam nem criam tabelas. Desvios aceitos pela revisão: módulo `orquestracao` fora do
  diagrama de camadas; regeneração por contagem incoerente (COUNT(*) por execução); impressão FNV-1a
  própria (sem crate), inclui a data e ignora a ordem das dezenas; `rust-version = "1.88"` no
  Cargo.toml (`as_chunks`). Medição release (cópias): Quina 2.282.880 linhas ~1,7 s completo, ~0,16 s
  sem novidade; Mega-Sena 735.840 ~0,6 s; Lotofácil 379.600 ~0,45 s. Completo = incremental em lotes
  byte a byte nas três loterias. Os bancos reais ainda não receberam `sync features`.
  Revisão: aprovada com ressalvas, sem ALTA; verificador-vazamento: sem vazamento. Corrigidos 2 BAIXA
  (NOT NULL nas três colunas, `rust-version`). Gate: fmt ok, clippy ok, 184 testes (113 lib + 13 sync
  + 48 features + 10 parser, 1 ignorado), `validar todas` ok.
  **Pendências:** (MÉDIA) PLANEJAMENTO.md:54 e :156 ainda dizem "fica para a 1a.2" e `motor_estado(...)`
  — atualizar com a decisão acima, após aprovação do usuário; (BAIXA) incluir `orquestracao` no
  diagrama de camadas da skill `arquitetura-projeto`; para a 1a.3: teste de remoção de resultado no
  prefixo incorporado e teste de vazamento no nível do banco.
- 2026-10-04 — 1a.2, decisões confirmadas pelo usuário: snapshot de último + 1 gravado em
  `features_dezena` com `saiu` = `NULL` (datasets filtram `saiu IS NOT NULL`); `sync features` só
  com uma loteria; impressão inclui a data (corrigir só a data regenera tudo). PLANEJAMENTO.md
  atualizado (Convenções e esquema de `motor_estado`) e `orquestracao` incluído no diagrama de
  camadas da skill `arquitetura-projeto`, com aprovação do usuário. As duas pendências (MÉDIA e
  BAIXA) acima estão resolvidas; seguem para a 1a.3 os dois testes sugeridos pela revisão.
- 2026-10-04 — 1a.3 concluída. Só testes; nenhum arquivo de produção mudou. `src/motor/tests.rs`:
  `incremental_igual_a_completo` ampliado às três loterias com cortes k = 0 (estado vazio via JSON),
  1, 99, 100, 101, n−1 e n; `alterar_o_concurso_t_...` ampliado a Lotofácil e Mega-Sena, com
  contraprova em t+1; novo `features_de_t_nao_dependem_de_t_nem_do_que_vem_depois` (todo t de 1 a
  n+1, pega vazamento por contagem). `tests/features.rs`, seções 10 e 11: colunas e PK de
  `features_dezena` iguais ao plano; golden gravado (snapshot com `saiu` NULL, incremental e
  completo); casos de borda com NULL de verdade no SQLite e invariantes de NULL na tabela inteira;
  vazamento no nível do banco (dois bancos iguais até t−1, linhas ≤ t idênticas, exceto `saiu` de t);
  incremental concurso a concurso = completo; resultado apagado abaixo do último incorporado → recusa
  por buraco e preserva tudo; último concurso apagado → regenera sem ele (impressão mudou).
  verificador-vazamento: sem vazamento; 7 mutações injetadas numa cópia, todas pegas. Revisão:
  aprovada com ressalvas, só BAIXA. Gate: fmt ok, clippy ok, 193 testes (114 lib + 13 sync +
  56 features + 10 parser, 1 ignorado), `validar todas` ok. **Fase 1a concluída.**
  **Pendências BAIXA (não bloqueiam):** o caso t=1 de `vazamento_no_banco_...` não testa nada de
  fato (tirar ou comentar); vazamento por contagem em `avancar` só é coberto na unidade (a
  orquestração usa `adicionar`; reavaliar se mudar); `confere_com_o_oraculo` sem Mega-Sena;
  remoção do último concurso regenera — comportamento coerente com "resultado já incorporado mudou",
  mas o plano não cita remoção explicitamente (registrar no plano se o usuário quiser).
- 2026-10-04 — 1a.3, a pedido do usuário, dois testes a mais vindos da auditoria de vazamento
  (achados "importante"): (1) `estado_adulterado_e_recusado` agora aumenta também a `frequencia` no
  caso da aparição depois do último concurso, isolando a guarda `u <= limite` de
  `verificar_compatibilidade` (antes outra cláusula recusava primeiro); (2) novo
  `golden_nas_fronteiras_das_janelas_de_50_e_100` (aparições em t−101, t−100, t−51 e t−50, t = 151),
  que não depende mais só do oráculo. Conferido com mutantes (guarda removida; J=50 incluindo t−51;
  J=100 excluindo t−100): os três falham. Gate: fmt ok, clippy ok, 194 testes (115 lib + 13 sync +
  56 features + 10 parser, 1 ignorado).
- 2026-10-04 — 1a.3, decisão do usuário: comportamento com resultado apagado registrado no
  PLANEJAMENTO.md (último concurso → regenera sem ele; abaixo do último → buraco, recusa sem gravar).
- 2026-10-04 — 1b.1 não iniciada: o plano não define pontos que mudam o schema de `perfis` e o hash
  gravado com cada índice. Perguntas devolvidas ao usuário: (1) o que é `versao` em
  `perfis(nome, versao, hash_toml, criado_em)`; (2) onde ficam os arquivos e como se nomeia um perfil
  por loteria (o exemplo tem `loteria` dentro e `nome` sem loteria); quais pesos usar em Mega-Sena e
  Quina; (3) hash sobre os bytes crus ou com fim de linha normalizado (CRLF no Windows muda o hash);
  (4) mesmo `nome` com `.toml` alterado: erro ou nova linha; (5) validação: todos os 9 pesos
  obrigatórios, chave desconhecida é erro, `janela` só em {0, 10, 50, 100}; (6) a tabela guarda só o
  que o plano lista ou também loteria/janela/normalização/texto do `.toml`; (7) dependências novas
  `toml` e `sha2`; (8) quem grava na tabela `perfis` (1b.1 não tem comando; `sync indice` é a 1b.3).
- 2026-10-04 — 1b.1, decisões do usuário: aceitas as 8 sugestões. Registradas no PLANEJAMENTO.md
  ("Regras dos perfis", exemplo com `versao` e caminho `perfis/<loteria>/`, esquema de `perfis` com
  loteria, janela, normalização, texto do `.toml` e `criado_em` só no primeiro registro). 1b.1
  desbloqueada.
- 2026-10-04 — 1b.1 concluída. `src/perfil.rs` (novo, na lib, fora de `motor/` porque lê disco):
  `interpretar` (puro, a partir do texto), `carregar(raiz, cfg, nome)` (lê `<raiz>/<slug>/<nome>.toml` e
  confere pasta e nome do arquivo), `hash_toml` (SHA-256 hex depois de CRLF→LF), `Perfil`, `Pesos`
  (`deny_unknown_fields`, `componentes()` na ordem de `features_dezena`), `Normalizacao`. `src/db.rs`:
  `criar_tabela_perfis`, `buscar_perfil`, `inserir_perfil` (INSERT simples, nunca substitui).
  `src/orquestracao.rs`: `registrar_perfil(conn, cfg, carregado, criado_em) -> Novo | JaRegistrado`, com a
  regra de imutabilidade (hash diferente → `PerfilAlterado`, sugerindo `<prefixo>_v<N+1>`); recebe
  `&Connection` para a 1b.3 chamá-la dentro da transação do índice. Erros novos: `PerfilInvalido`,
  `PerfilAlterado`, `PerfilDeOutraLoteria`. Perfis `perfis/{lotofacil,megasena,quina}/padrao_v1.toml`
  (pesos do exemplo, janela 50, zscore, LF); `.gitattributes` com `perfis/**/*.toml text eol=lf`;
  dependências `toml = "1.1.6"` e `sha2 = "0.11.0"` (o lock subiu também `serde` 1.0.217→1.0.229 e
  `indexmap`, compatíveis). Nenhum comando novo na CLI.
  Desvios aceitos pela revisão: nome do perfil só com `[A-Za-z0-9_-]` (protege o caminho); sufixo
  `_vN` com N ≥ 1 e sem zeros à esquerda; `registrar_perfil` recusa perfil de outra loteria; teste que
  trava o hash dos três `padrao_v1.toml` (editar um deles, até um comentário, quebra o teste: é de
  propósito); coluna `toml` guarda o texto já em LF; `criado_em` vem do chamador como texto.
  33 testes novos (26 em `perfil.rs`, 2 em `db.rs`, 5 em `orquestracao.rs`). Revisão: aprovada com
  ressalvas, só BAIXA. Gate: fmt ok, clippy ok, 227 testes (148 lib + 13 sync + 56 features + 10 parser,
  1 ignorado), `validar todas` ok nos três bancos.
  **Para a 1b.3:** `criado_em` em ISO-8601 UTC (o projeto não tem `chrono`: formatar a partir de
  `SystemTime` ou decidir outro formato). **Pendências BAIXA (não bloqueiam):** `PerfilAlterado` sempre
  sugere `versao + 1`, mesmo se esse arquivo já existir; BOM UTF-8 não é tratado (pode mudar o hash ou
  dar erro de parse sem causa clara); falta doc comment em `PerfilDeOutraLoteria` (`src/error.rs`).
- 2026-10-04 — 1b.1, pendências BAIXA corrigidas a pedido do usuário: (1) `PerfilAlterado` sugere a
  próxima versão **livre** (`Perfil::proxima_versao` pula nomes com arquivo ao lado ou já registrados
  no banco; `PerfilCarregado` ganhou `origem`); (2) BOM UTF-8 removido antes de interpretar e do hash
  (`normalizar_fim_de_linha`), registrado no PLANEJAMENTO.md; (3) comentário de documentação em
  `PerfilDeOutraLoteria`. Testes novos: `bom_e_ignorado_no_texto_e_no_hash` e
  `sugestao_de_versao_pula_registradas_e_arquivos_existentes`. Gate: fmt ok, clippy ok, 229 testes
  (150 lib + 13 sync + 56 features + 10 parser, 1 ignorado).
- 2026-10-04 — 1b.2 não iniciada: a seção "Índice de força" do plano não define pontos que mudam o
  `total`, a `posicao` e a `variacao_posicao` calculados. Perguntas devolvidas ao usuário: (1) desvio
  padrão do z-score populacional (÷N) ou amostral (÷N−1); (2) média/desvio/mín/máx calculados só sobre
  as dezenas com valor não `NULL`, e `NULL` → neutro depois; (3) valor neutro de `NULL` no `minmax`
  (0,5? média dos normalizados? 0?); (4) componente constante no concurso (desvio 0 ou máx = mín) e
  componente todo `NULL` → todos 0 (ou neutro)?; (5) sinal de `variacao_posicao` (anterior − atual,
  positivo = subiu?) e valor no primeiro concurso do índice (`NULL`?); (6) empate no `total`: igualdade
  exata de `f64`, ou arredondar antes de ordenar (ex.: 1e-9) para não depender de ruído de ponto
  flutuante; ordem fixa da soma (a dos 9 componentes de `Pesos::componentes`)?; (7) a 1b.2 cobre todos
  os concursos de `features_dezena`, inclusive o snapshot de último + 1 (`variacao_posicao` dele
  relativa ao último concurso)?
- 2026-10-04 — 1b.2, decisões do usuário: aceitas as 7 sugestões, registradas no PLANEJAMENTO.md
  ("Índice de força" → "Regras do cálculo"): desvio populacional; NULL fora das estatísticas e
  neutro depois (0 no z-score, 0,5 no minmax); componente sem variação → neutro; soma na ordem de
  `Pesos::componentes`; total arredondado a 1e-9 antes de ordenar, desempate pela menor dezena;
  variação = anterior − atual, NULL no primeiro; todos os concursos de `features_dezena`, inclusive
  o último + 1. 1b.2 desbloqueada.
- 2026-10-04 — 1b.2 concluída. `src/motor/indice.rs` (novo, puro, sem I/O nem SQL):
  `calcular_indice(features, janela, normalizacao, pesos) -> Result<Vec<IndiceDezena>>`, com
  `IndiceDezena { concurso, dezena, total, posicao, variacao_posicao: Option<i64> }`, saída ordenada
  por (concurso, dezena) e sem a coluna `perfil`, que fica para quem grava. Reexportado em
  `src/motor/mod.rs`. Erro novo `MotorIndiceEntradaInvalida` (`src/error.rs`). Segue as "Regras do cálculo"
  do plano sem desvio. Decisões de implementação:
  - `total` devolvido já arredondado a 1e-9, com -0,0 trocado por +0,0, para a posição poder ser refeita
    a partir do total gravado;
  - "sem variação" decidido nos inteiros (mín = máx); média do z-score somada em u64;
  - peso casado com o componente pelo nome de `Pesos::componentes`; `saiu` nunca é lido;
  - entrada em qualquer ordem dá os mesmos bits;
  - erro para (concurso, dezena) repetido, buraco, conjunto de dezenas diferente entre concursos, janela
    ausente ou com faixa de concursos diferente das outras janelas, peso ou total não finito;
  - entrada vazia → vazio; o primeiro concurso da entrada tem variação `NULL`, mesmo que não seja o 1.
  40 testes em `motor::indice::tests`: goldens z-score e minmax feitos à mão e conferidos pelo revisor,
  NULL, componente constante e todo NULL, ruído 0,1+0,2, empate, variação, determinismo, ponto no tempo
  e erros. Medido na Quina: cerca de 0,1 s por perfil (570.720 linhas, release). verificador-vazamento:
  aprovado. Revisão: aprovada com ressalvas, só BAIXA. Gate: fmt ok, clippy ok, 269 testes (190 lib +
  13 sync + 56 features + 10 parser, 1 ignorado), `validar todas` ok.
  **Para a 1b.3:**
  - como o primeiro concurso da entrada recebe variação `NULL`, `sync indice` não pode passar só os
    concursos novos; recomendado regenerar o índice inteiro do perfil (cerca de 0,1 s), ou então incluir
    o último concurso já indexado e descartar as linhas dele;
  - conferir na orquestração que o conjunto de dezenas bate com o universo da `LoteriaConfig`.
  **Pendências BAIXA (não bloqueiam):**
  - dependência circular `motor` ↔ `perfil` (o motor usa `Pesos`/`Normalizacao`; `perfil` usa
    `JANELAS` e lê disco); mover os dois tipos para `motor` ou `model` e reexportar em `perfil`;
  - comentário impreciso sobre a ordenação instável (`indice.rs:83-85`);
  - |total| > ~1,8e299 dá erro de "total não finito" (só com pesos absurdos);
  - peso não finito é rejeitado mesmo com entrada vazia;
  - dois totais a menos de 1e-9 um do outro, mas em lados opostos de uma fronteira de arredondamento,
    não empatam (é inerente à regra; vale lembrar nos vetores de conformidade da 1.5).
- 2026-10-04 — 1b.2, pendências BAIXA 1 e 2 corrigidas a pedido do usuário: `Normalizacao` e `Pesos`
  movidos de `src/perfil.rs` para `src/motor/pesos.rs` (reexportados em `motor` e em `perfil`), o que
  tira a dependência circular — o motor não usa mais nada de `perfil`; comentário da ordenação
  instável em `indice.rs` corrigido (a checagem de repetidas vem depois, no laço). As BAIXA 3, 4 e 5
  ficam só registradas. Gate: fmt ok, clippy ok, 269 testes (190 lib + 13 sync + 56 features +
  10 parser, 1 ignorado).
- 2026-10-05 — 1b.3 não iniciada: o formato de `perfis.criado_em` segue sem decisão do usuário e o
  plano não define pontos que mudam o comportamento do `sync indice` e o schema de `indice`. Perguntas
  devolvidas ao usuário, com sugestões: (1) `criado_em` em ISO-8601 UTC com segundos
  (`2026-10-04T18:30:00Z`), formatado a partir de `SystemTime` na CLI, sem crate nova; (2) features
  desatualizadas ou ausentes (último concurso de `features_dezena` ≠ último de `resultados` + 1, ou
  `feature_version` diferente da atual): `sync indice` **recusa** e manda rodar `sync features` antes,
  ou roda as features sozinho? sugestão: recusar, sem gravar nada; (3) `--perfil` obrigatório ou com
  padrão `padrao_v1`? sugestão: obrigatório, um perfil por execução, só uma loteria (sem `todas`, como
  `features`); (4) cálculo: regenerar o índice inteiro do perfil a cada execução (DELETE das linhas do
  perfil + INSERT, numa transação, junto com o registro em `perfis`), ~0,1 s na Quina, em vez de
  incremental; (5) schema de `indice`: `concurso INTEGER, dezena INTEGER, perfil TEXT` (= `perfis.nome`),
  `total REAL NOT NULL`, `posicao INTEGER NOT NULL`, `variacao_posicao INTEGER` (NULL só no primeiro
  concurso), PK do plano, WITHOUT ROWID, sem FK; (6) usar as linhas de `features_dezena` da janela do
  perfil, conferindo que o conjunto de dezenas de cada concurso é exatamente o universo da
  `LoteriaConfig` (erro se não for); (7) banco/`features_dezena` vazios → erro, sem criar tabelas.
- 2026-10-05 — 1b.3, decisões do usuário: aceitas as 7 sugestões, registradas no PLANEJAMENTO.md
  (esquema de `indice` e de `perfis.criado_em`; regras do `sync indice` na Fase 1b): `criado_em` ISO-8601
  UTC com segundos, feito na CLI a partir de `SystemTime`; recusa features desatualizadas ou de outra
  versão; `--perfil` obrigatório, sem `todas`; regenera o índice inteiro do perfil numa transação com o
  registro em `perfis`; esquema de `indice` WITHOUT ROWID sem FK; entrada só da janela do perfil com
  dezenas conferidas contra a `LoteriaConfig`; banco ou features vazios dão erro sem criar tabelas.
  1b.3 desbloqueada.
- 2026-10-05 — 1b.3 concluída. `src/tempo.rs` (novo, na lib: `iso8601_utc`, `data_civil`, sem crate);
  `src/error.rs` (`SemFeatures`, `FeaturesDesatualizadas`, `FeaturesDeOutraVersao`, `FeaturesIncoerentes`);
  `src/db.rs` (`criar_tabela_indice` com o esquema do plano, `apagar_indice_do_perfil`, `inserir_indice` em
  blocos de 64, `ler_indice`, `ler_features_da_janela`, `tabela_existe`, `maior_concurso_features`);
  `src/orquestracao.rs` (`sincronizar_indice` → `ResumoIndice`; todas as recusas antes de qualquer CREATE;
  depois `registrar_perfil`, DELETE do perfil e INSERT numa transação só); subcomando
  `indice <loteria> --perfil <nome> [--db] [--perfis perfis]` em `src/bin/sync.rs` (CLI lê o relógio e
  formata `criado_em`); README; `tests/indice.rs` (52 testes). Bancos reais não foram tocados.
  Desvios aceitos pela revisão: (1) flag extra `--perfis <pasta>`, padrão `perfis`; (2) módulo novo
  `src/tempo.rs`; (3) `feature_version` conferida só nas linhas da janela do perfil, não na tabela
  inteira — **leitura do plano a registrar** ("forem de outra feature_version"); (4) checagem extra: a
  série da janela tem de ir de 1 a último + 1 sem buraco, com as dezenas da config, senão
  `FeaturesIncoerentes` pedindo `sync features --completo`; (5) perfil de outra loteria é recusado antes
  de abrir a transação; `PerfilAlterado` só depois de calcular o índice (nada é gravado em nenhum dos dois).
  Ponta a ponta em cópias (release, máquina carregada): Lotofácil 94.900 linhas ~0,5 s, Mega-Sena
  183.960 ~0,9 s, Quina 570.720 ~2,4 s; segunda execução idêntica byte a byte, `criado_em` preservado;
  recusa por features desatualizadas preserva tudo; após `features` incremental, linhas ≤ 7134 idênticas.
  Revisão: aprovada com ressalvas, só BAIXA; corrigidas numa rodada (README com todas as recusas, a
  limitação abaixo e a ressalva sobre apagar `perfis`; asserção de prefixo estável entre lotes no teste
  incremental). verificador-vazamento: aprovado com ressalvas, sem vazamento; 3 regressões injetadas
  numa cópia (somar `saiu`, normalizar com estatísticas de outro concurso, variação contra t+1), todas
  pegas. Gate: fmt ok, clippy ok, 338 testes (203 lib + 17 sync + 56 features + 52 indice + 10 parser,
  1 ignorado), `validar todas` ok. **Fase 1b concluída.**
  **Pendências BAIXA (não bloqueiam):** a checagem de "desatualizado" compara só o último concurso — um
  resultado antigo corrigido sem rodar `sync features` não é detectado (documentado no README; opção:
  conferir `motor_estado.impressao` contra `resultados`, o que mudaria o plano); `saiu` é carregado em
  `FeaturesDezena` e chega ao motor sem ser lido (protegido só por teste; opção: tipo/consulta sem `saiu`);
  teste incremental cobre só um perfil (Quina, janela 10, minmax); `saiu` NULL no banco só testado na
  unidade; com `PerfilAlterado` o índice é calculado à toa (~2 s na Quina) antes da recusa.
- 2026-10-05 — 1b.3, decisões do usuário após a auditoria: (1) corrigido o achado "importante" do
  verificador-vazamento — `sincronizar_indice` agora confere `motor_estado` (loteria, último concurso
  e impressão de `resultados`) e recusa com `FeaturesDeResultadosAlterados` se não bater (resultado
  antigo corrigido, só a data corrigida, ou `motor_estado` ausente); a limitação saiu do README;
  5 testes novos em `tests/indice.rs`, e com a checagem desligada 4 deles falham; (2) registrado no
  PLANEJAMENTO.md que a `feature_version` é conferida só na janela do perfil. Também reforçado
  `saiu_nao_e_lido` (achado menor 2): `saiu` alternado por dezena com todos os pesos ≠ 0; pega
  leitura de `saiu` via `frequencia` e via `repeticao_atual` (conferido com mutantes). Gate: fmt ok,
  clippy ok, 343 testes (203 lib + 17 sync + 56 features + 57 indice + 10 parser, 1 ignorado).
- 2026-10-05 — Teste de ponta a ponta com sorteios novos (pedido do usuário). Backup dos três bancos em
  `backups/antes_teste_2026-10-05/` (hashes conferidos). Em cópias da Quina, com cache temporário:
  (A) buraco no 5000 → `validar`/`buracos` acusam, `inserir` fecha e registra no cache, `importar` da
  planilha oficial confirma e remove do cache — ok; (B) inserção errada → banco fica com o oficial e
  aviso de divergência — ok, mas a entrada errada continua no cache e o aviso se repete a cada
  importação até ser removida à mão; (C) `atualizar` com o banco em 7132 e a página em 7134 grava só o
  7134 e deixa o 7133 como buraco, com exit 0 — confirma o problema. Usuário decidiu a tarefa 0.8
  (backup automático + caminho do `baixar`, sem pergunta interativa), registrada no PLANEJAMENTO.md.
- 2026-10-05 — 0.8 concluída. `atualizar` com salto (página > último + 1): backup do banco em
  `backups/resultados_<loteria>_<AAAAMMDD-HHMMSS>.db` (horário em **UTC**, via `tempo::carimbo_utc`;
  cópia por `VACUUM INTO` em `db::copiar_banco`, origem só leitura, destino com conteúdo recusado) e depois
  o caminho do `baixar` (`baixar_planilha` + `processar_e_salvar`, reaproveitados). Planilha falha → erro
  citando o backup, nada gravado. Buraco restante → aviso + dica `buracos`/`inserir`, exit 0. Salto de 1 e
  "sem novidade" inalterados; vale com `todas`. Arquivos: `src/backup.rs` (novo), `src/db.rs`,
  `src/error.rs` (`SpiderError::Backup`), `src/tempo.rs`, `src/orquestracao.rs` (`Atualizacao`,
  `classificar_atualizacao`, função pura), `src/lib.rs`, `src/bin/sync.rs`, README. 26 testes novos, sem rede
  (relógio e download injetados; fixture `tests/fixtures/quina.xlsx` como planilha baixada).
  Interpretações do orquestrador (não estavam no plano, **a confirmar pelo usuário**): (1) banco inexistente
  ou sem concursos da loteria conta como último = 0 → se a página > 1 é salto, segue o caminho do `baixar`
  sem backup (antes gravava só o último concurso); (2) se a planilha estiver atrasada em relação à página, o
  concurso da página é gravado em seguida pelo caminho do salto de 1 (se só ele for recusado, a planilha já
  ficou gravada e o comando sai com erro citando o backup).
  Desvios aceitos pela revisão: flag extra `--backups <pasta>` (padrão `backups`) e `--dir` (padrão
  `downloads`) no `atualizar`; colisão no mesmo segundo ganha sufixo `_2`, `_3`… (limite 100), nunca
  sobrescreve. Teste C repetido com rede numa cópia da Quina (7132, página 7134): backup feito, planilha
  baixada, 2 registros gravados, `validar` 1..7134 ok; segunda execução "já está atualizado", sem backup.
  Revisão: aprovada com ressalvas, sem ALTA; BAIXAs corrigidas numa rodada (`faltando()` usado, mensagem de
  banco sem concursos, dica de buracos não sai mais em dobro, README). Gate: fmt ok, clippy ok, 369 testes
  (218 lib + 28 sync + 56 features + 57 indice + 10 parser, 1 ignorado), `validar todas` ok.
  **Pendência MÉDIA (não bloqueia):** `aplicar_atualizacao`, `processar_e_salvar` e `validar_lote` são fluxos
  de negócio no binário (`src/bin/sync.rs`), contra a regra de camadas (deviam estar em `orquestracao`);
  mover para a biblioteca antes ou durante a 3.1 (`sync ciclo` vai precisar deles). Notas soltas: a pasta
  `backups/` em si não está no `.gitignore` (só `*.db`); existem `src/resultados_lotofacil.db` e
  `src/bin/resultados_lotofacil.db` perdidos (anteriores à 0.8, não tocados).
- 2026-10-05 — 0.8, decisões do usuário: confirmadas as duas decisões do engenheiro (banco vazio = último 0;
  planilha atrasada → concurso da página gravado em seguida), registradas no PLANEJAMENTO.md (0.8); a
  pendência MÉDIA (lógica de gravação na CLI) entrou na descrição da 3.1. Teste C repetido em cópia com a
  regra nova: backup, planilha, 7133 e 7134 gravados, `validar` 1..7134 ok, segunda execução "já está
  atualizado" sem novo backup. `/backups` no `.gitignore`; removidos `src/resultados_lotofacil.db` e
  `src/bin/resultados_lotofacil.db` (bancos vazios de uma execução antiga, fora do git). Horário do
  nome do backup em UTC.
- 2026-10-05 — Teste de ponta a ponta nos bancos reais (backup em `backups/antes_teste_2026-10-05/`):
  `features` + `indice padrao_v1` completos sobre o histórico antigo; `atualizar todas` trouxe 1 sorteio
  novo por loteria (Lotofácil 3796, Mega-Sena 3066, Quina 7134, todos de 2026-10-03; salto de 1, sem
  backup); `validar todas` ok; `features` incremental (1 concurso, 0,05–0,17 s) e `indice`. Conferido:
  incremental = completo (`--completo` numa cópia) byte a byte em `features_dezena` e `indice` nas três;
  features 1..último antigo intactas e snapshot antigo igual exceto `saiu` (NULL → preenchido); índice
  1..antigo próximo intacto; `saiu = 1` exatamente nas dezenas sorteadas. Os bancos reais agora têm
  `features_dezena`, `motor_estado`, `perfis` e `indice`.
  Usuário conferiu no site da Caixa os três concursos novos: ok.
- 2026-10-05 — Conjunto trancado, decisão do usuário: 10% do histórico, com corte fixo pelo número do
  concurso (registrado no PLANEJAMENTO.md, 1c): busca até Lotofácil 3416, Mega-Sena 2759 e Quina 6421;
  trancados daí em diante (380, 307 e 713 concursos hoje), inclusive os sorteios futuros.
- 2026-10-05 — 1c.1 não iniciada: o plano pede log loss e Brier do perfil contra o acaso, mas o índice
  produz `total` e `posicao`, não probabilidade; sem definir a conversão, as métricas não têm valor
  definido. Perguntas devolvidas ao usuário, com sugestões: (1) **índice → probabilidade**: sugestão
  calibração pela posição, ponto no tempo — p_t(r) = (vezes em que a dezena na posição r saiu nos
  concursos < t + m·K/N) / (concursos < t + m), suavizada para o acaso; a soma das N probabilidades dá
  exatamente K e o primeiro concurso recebe o acaso. Alternativas: softmax do `total` escalado para somar
  K (estoura 1 na Lotofácil, precisa de ajuste de β), ou regressão logística de `saiu` sobre `total`
  ajustada até t−1 (é um modelo, se sobrepõe à 1c.3); (2) força do prior m (sugestão: 100 concursos
  fictícios) e histórico acumulado inteiro, sem janela; (3) concursos avaliados: os com `saiu` não NULL,
  só até o corte da busca (trancado fora por padrão), a partir do segundo concurso do índice;
  (4) log loss em logaritmo natural, p limitado a [1e-15, 1−1e-15], média por par (concurso, dezena);
  Brier = média de (p − saiu)²; (5) incerteza: erro padrão e IC 95% (normal) da diferença perfil − acaso
  calculada **por concurso** (as dezenas de um concurso não são independentes); (6) escopo da 1c.1: só
  funções puras em `src/avaliacao/` (baseline de `LoteriaConfig`, calibração, log loss, Brier, resumo)
  com testes contra valores calculados à mão; leitura do banco e CLI ficam para a 1c.5; o corte do
  trancado entra no `LoteriaConfig` na 1c.3. Nota à parte: a skill `avaliacao-estatistica` ainda trata
  o tamanho do trancado como decisão em aberto — atualizar com a decisão de 2026-10-05 (pede aprovação).
- 2026-10-05 — 1c.1, decisões do usuário: aceitas as 6 sugestões, registradas no PLANEJAMENTO.md (1c):
  calibração pela posição com prior m = 100 sobre o histórico inteiro; avaliação do 2º concurso do
  índice até o corte da busca, sem o trancado; log loss natural com p em [1e-15, 1−1e-15] e Brier, média
  por par; erro padrão e IC 95% da diferença perfil − acaso por concurso; 1c.1 só com funções puras em
  `src/avaliacao/`. Skill `avaliacao-estatistica` atualizada com o corte fixo do trancado (aprovação do
  usuário). 1c.1 desbloqueada.
- 2026-10-05 — 1c.1 concluída. `src/avaliacao/` (novo, puro, sem I/O nem SQL): `mod.rs` (`taxa_do_acaso(cfg)`
  = K/N do `LoteriaConfig`), `calibracao.rs` (`Observacao { concurso, dezena, posicao, saiu }`,
  `CONCURSOS_FICTICIOS = 100`, `calibrar`, calculada em inteiros com uma divisão no fim — o primeiro
  concurso dá exatamente o f64 de K/N), `metricas.rs` (`log_loss`, `brier`, `metricas_por_concurso`,
  `resumir`, `avaliar(cfg, obs, corte)` → `ResumoAvaliacao` com `Comparacao { perfil, acaso, diferenca,
  erro_padrao, ic95 }`), `tests.rs` (38 testes: valores à mão numa loteria fictícia N=4/K=2 com frações
  exatas, referências do acaso nas três loterias, soma = K, ponto no tempo, corte, determinismo,
  validação). `src/error.rs`: `AvaliacaoEntradaInvalida`, `AvaliacaoSemConcursos`.
  Interpretações (orquestrador/engenheiro, aceitas pela revisão, **a confirmar pelo usuário**): (1) EP =
  desvio amostral (÷(T−1)) de d_t / √T, IC normal ±1,96·EP; com T < 2, EP e IC ficam `None`; (2) Brier
  sem limitar p (o limite é só do log loss); (3) `saiu` NULL fora do último concurso é erro; (4) o corte
  é recebido como argumento e aplicado **antes** de validar e calibrar (nada acima do corte é lido);
  (5) série vazia é erro; (6) "primeiro concurso do índice" = primeiro da entrada.
  Revisão: aprovada com ressalvas, só BAIXA; corrigidas numa rodada (link de doc para item privado;
  mensagem de `AvaliacaoEntradaInvalida` cobre também erro de montagem). verificador-vazamento (via
  revisor): sem vazamento. Gate: fmt ok, clippy ok, 407 testes (256 lib + 28 sync + 56 features +
  57 indice + 10 parser, 1 ignorado), `validar todas` ok.
  **Para a 1c.3/1c.5:** passar as observações sempre a partir do 1º concurso do índice (a calibração
  recomeça do acaso no primeiro concurso da entrada), ou criar um limite inferior de avaliação separado
  da calibração para o walk-forward; aplicar o corte por loteria também na leitura do banco; `calibrar`
  é pública e não recebe corte — quem a usar fora de `avaliar` precisa filtrar antes (documentar ou
  reduzir visibilidade). **Pendências BAIXA (reforço de teste):** ponto no tempo mudando só o `saiu` de
  t e só t' > t; `nada_acima_do_corte_e_lido` comparar também a série de `metricas_por_concurso`.
- 2026-10-05 — 1c.2 não iniciada: o plano diz só "acertos das top-k dezenas vs. esperado pela
  hipergeométrica" e a skill `avaliacao-estatistica` pede também "intervalo e p-valor", sem definir
  pontos que mudam os valores calculados. Perguntas devolvidas ao usuário, com sugestões: (1) **quais k**:
  k = K da loteria (15/6/5) como métrica principal; a função pura aceita qualquer k em 1…N−1 (a 1c.5
  decide se expõe `--k`); (2) top-k = dezenas com `posicao` ≤ k (desempate já resolvido no índice);
  (3) concursos avaliados: o mesmo trecho da 1c.1 (do 2º concurso do índice até o corte, só sorteados,
  trancado fora), para as métricas serem comparáveis, embora o top-k não precise de calibração;
  (4) saída: média de acertos observada vs. k·K/N, diferença, e a distribuição observada (nº de concursos
  com 0…min(k,K) acertos) ao lado da esperada (T × pmf hipergeométrica); (5) incerteza: EP e IC 95% da
  diferença por concurso, como na 1c.1 (÷(T−1), normal); (6) p-valor **exato e unilateral** (acertos
  totais ≥ observado) pela soma de T hipergeométricas independentes, por convolução — válido mesmo com
  correlação serial do ranking, porque sob o acaso o sorteio de t é independente da posição calculada
  com t−1; (7) hipergeométrica em inteiros (C(80,40) cabe em u128) com testes contra valores tabelados;
  (8) escopo: só funções puras em `src/avaliacao/`, reaproveitando `Observacao`; leitura do banco e CLI
  na 1c.5. Achado à parte: `docs/PLANEJAMENTO.md:397-404` tem uma nota de progresso da 1c.1 colada
  depois do Backlog (commit 9672d3d); sugestão: mover para cá (pede aprovação para editar o plano).
- 2026-10-05 — 1c.1, decisões do usuário: confirmadas as 5 escolhas do implementador (EP amostral ÷(T−1),
  IC normal, Brier sem o limite, buraco e série vazia são erro, corte aplicado antes de validar) e
  registradas no PLANEJAMENTO.md (1c), junto com a ressalva do intervalo (otimista por correlação serial;
  1c.5 usa teste robusto). Achado baixo do verificador-vazamento coberto: novo teste
  `metricas_com_corte_menor_sao_prefixo_das_com_corte_maior` (3 loterias, cortes 2, 3, 40, 79 e 80,
  inclusive o snapshot); conferido com mutante (métrica dependente do tamanho do trecho até o corte),
  que ele pega. Pendências para 1c.3/1c.5: passar sempre o corte fixo da loteria (nunca `u32::MAX` em
  produção); concursos faltando no banco são erro na calibração.
- 2026-10-05 — Nota anterior (1c.1) movida do fim do PLANEJAMENTO.md, onde tinha sido colada por engano
  no commit 9672d3d. 1c.2, decisões do usuário: aceitas as 8 sugestões, registradas no PLANEJAMENTO.md
  (1c, "Ranking"). 1c.2 desbloqueada.
- 2026-10-05 — 1c.2 concluída. `src/avaliacao/topk.rs` (novo, puro): `binomial` (u128, com mdc no passo;
  só estoura a partir de C(132,66)), `Hipergeometrica` (`nova`, `da_loteria(cfg, k)` com k em 1..=N−1,
  `casos`/`pmf`/`media`/`variancia`; sem serde de propósito), `p_valor_soma(h, T, total) -> Cauda
  { probabilidade, log10 }`, `acertos_por_concurso`, `resumir_topk`, `avaliar_topk(cfg, obs, k, corte) ->
  ResumoTopK` (média de acertos vs k·K/N em `Comparacao` da 1c.1, distribuição observada × esperada de 0 a
  min(k,K), p-valor). `src/avaliacao/mod.rs`: funções privadas comuns `ate_o_corte` e `concursos_avaliados`,
  usadas também por `metricas_por_concurso` (refatoração sem mudança de comportamento, conferida pela
  revisão); `comparar` virou `pub(super)`; `src/avaliacao/tests.rs` só abriu visibilidade de auxiliares.
  Nenhuma variante de erro nova. **p-valor**: inclinação exponencial (identidade exata, θ por bissecção com
  passos fixos, fator em log, cauda somada das menores para as maiores), convolução parcela a parcela com
  poda; total ≤ média (conferido em inteiros) sem inclinação; total = máximo por fórmula fechada; total ≤
  T·mínimo dá 1; total impossível é erro. Com underflow, `probabilidade` vira 0 e `log10` segue finito
  (ex.: Lotofácil T=3415 tudo no máximo → −22240,9). Custo em release: ~0,9–1,2 s na Quina (k=5/40,
  T=6420), ~0,7–0,9 s Lotofácil k=15, ~0,2 s Mega-Sena; caudas extremas em µs.
  Testes: `src/avaliacao/topk/tests.rs` (binomiais e pmf contra as tabelas da Caixa, frações exatas N=4/K=2,
  média/variância da soma, caudas exatas recalculadas com inteiros grandes em Python — conferidas de novo pelo
  revisor, inclusive no ramo inclinado —, monotonicidade inclusive na troca de caminho com T=1000, resumo à
  mão, erros, ponto no tempo, corte, prefixo, determinismo); 2 ignorados: medição de custo e monotonicidade
  com T=2758 (~18 s em debug).
  Decisões do implementador (fora do plano, aceitas pela revisão): `Cauda` com `log10`; `nova` aceita 0..=N
  (a restrição 1..=N−1 fica em `da_loteria` e no top-k); `distribuicao_soma` só para testes
  (`#[cfg(test)]`). Revisão: aprovada com ressalvas, sem ALTA; corrigidos numa rodada só doc e teste (forma
  condicional e condição de validade do p-valor no doc do módulo, texto do custo, comentários de defesa
  inalcançável em `metricas.rs`, ressalva do IC em `ResumoTopK.acertos`, teste de monotonicidade com T
  grande). verificador-vazamento: sem vazamento. Gate: fmt ok, clippy ok, 455 testes (304 lib + 28 sync +
  56 features + 57 indice + 10 parser, 3 ignorados), `validar todas` ok.
  **Pendente de decisão do usuário (MÉDIA, revisão + verificador-vazamento):** o p-valor só vale se o
  perfil e o k foram fixados sem ver os sorteios do trecho avaliado; com busca de pesos no mesmo trecho ou
  escolhendo o melhor entre vários perfis/k, fica otimista (avaliar no trancado ou corrigir por
  Bonferroni/Holm, informando quantos foram testados). Já está no doc do módulo; sugerido registrar em
  PLANEJAMENTO.md (1c, "Ranking", p-valor) e tratar na 1c.5. **Para a 1c.3/1c.5:** ~1 s por chamada do
  p-valor; se virar laço (por dobra do walk-forward, por perfil ou por k), podar também por cima (massa
  relevante em alvo ± c·σ√T). **Pendências BAIXA:** `resumir_topk` é pública e não confere concursos
  repetidos na série (`avaliar_topk` é protegida); testes sugeridos e não feitos: simulação do nulo com
  ranking adaptativo conferindo p-valores uniformes, sorteio válido alternativo em corte+1.
- 2026-10-05 — 1c.2, decisão do usuário: registrada no PLANEJAMENTO.md (1c, "Ranking") a condição de
  validade do p-valor (perfil e k escolhidos sem ver o trecho avaliado; senão trancado ou Bonferroni/Holm,
  na 1c.5). Custo do p-valor em laço fica para decidir na 1c.3.
- 2026-10-05 — 1c.3 não iniciada: o plano diz só "walk-forward (treina até t, testa em t+1…)" e
  "trancado usado uma única vez", sem definir pontos que mudam o que é calculado. Um perfil `.toml` não
  é treinado e a calibração da 1c.1 já é acumulada e ponto no tempo, então "walk-forward" em Rust não
  tem sentido único. Perguntas devolvidas ao usuário, com sugestões: (1) **corte no config**: campo
  `ultimo_concurso_busca` no `LoteriaConfig` (3416/2759/6421) + função que diz a parte de um concurso
  (busca/trancado), usada também pelo dataset da 1.5.1; (2) **o que é o walk-forward em Rust**: avaliação
  em dobras consecutivas do trecho de busca (origem crescente, nunca embaralha), métricas da 1c.1 e
  top-k da 1c.2 por dobra contra o acaso, para ver estabilidade; a mesma definição de dobras serve à
  busca de pesos da 1.5.3 (que treina até a dobra e testa nela); (3) **dobras**: 5 dobras de tamanho
  igual (o resto na última) sobre o trecho do 2º concurso do índice até o corte; (4) **calibração
  separada da avaliação**: novo limite inferior de avaliação — a calibração sempre começa no 1º
  concurso do índice e só o trecho da dobra entra nas métricas (resolve a pendência da 1c.1);
  (5) **trancado**: avaliação só por escolha explícita (`Trecho::Busca` padrão / `Trecho::Trancado`),
  do corte + 1 até o último sorteado, com calibração usando todo o passado; o "uma única vez" fica
  registrado na 1c.5 (proposta: tabela derivada `avaliacoes_trancado(perfil, hash_toml, ate_concurso,
  avaliado_em)` e aviso/recusa se o perfil já foi avaliado — muda o esquema, decidir lá);
  (6) **custo do p-valor em laço**: calcular por dobra sem poda extra (T menor por dobra, custo total
  menor que o do trecho inteiro); podar por cima só se medir > 2 s no total; (7) **escopo**: só funções
  puras em `src/avaliacao/` + campo no `config.rs`; leitura do banco, CLI e teste robusto (bootstrap
  por blocos) na 1c.5.
- 2026-10-05 — 1c.3, decisões do usuário: aceitas as 7 sugestões, registradas no PLANEJAMENTO.md (1c,
  walk-forward). 1c.3 desbloqueada. 1c.4, decisões parciais: runs test por dezena (sequência saiu/não
  saiu), histórico inteiro; nível de significância e correção ainda em aberto (usuário pediu explicação).
- 2026-10-05 — 1c.3 concluída. `src/config.rs`: campo `ultimo_concurso_busca` no `LoteriaConfig` (3416 /
  2759 / 6421), enum `Trecho { Busca (padrão), Trancado }` com `nome()` ("busca"/"trancado") e
  `LoteriaConfig::trecho_do_concurso(concurso)` (≤ corte → Busca), para o dataset da 1.5.1 reutilizar.
  `src/avaliacao/walk_forward.rs` (novo, puro): `QTD_DOBRAS = 5`, `FaixaConcursos { primeiro, ultimo }`,
  `dividir_em_dobras(faixa, qtd)` (⌊T/qtd⌋ por dobra, sobra na última; só depende dos números dos
  concursos), `faixa_da_busca`/`dobras_da_busca` (do 2º concurso da entrada até `cfg.ultimo_concurso_busca`),
  `walk_forward(cfg, obs, k) -> WalkForward { corte, dobras: Vec<AvaliacaoFaixa> }` (log loss/Brier da 1c.1 e
  top-k com p-valor da 1c.2 por dobra) e `avaliar_trancado(cfg, obs, k) -> AvaliacaoFaixa` (corte+1 até o
  último sorteado, calibração com todo o passado recebido). Nenhuma das duas recebe corte: o modo busca só
  usa o corte do config. Variantes `_entre(…, inicio, fim)` em `metricas.rs`/`topk.rs`: o limite inferior só
  filtra os concursos avaliados; a calibração sempre começa no 1º concurso da entrada (resolve a pendência da
  1c.1). `avaliar`/`avaliar_topk` inalteradas (= `_entre` com início 0). `src/error.rs`: texto fixo de
  `AvaliacaoSemConcursos` generalizado ("avaliação: nada para avaliar: {0}") com detalhe específico em cada
  chamada. Dobras reais: Lotofácil 683×5; Mega-Sena 551×4 + 554 (2..552, …, 2206..2759); Quina 1284×5.
  Custo medido (release, séries do tamanho real, teste `#[ignore]`): walk-forward completo 0,54 s nas três
  loterias (Lotofácil 164 ms, Mega-Sena 82 ms, Quina 290 ms) — abaixo de 2 s, sem otimização.
  Decisões do implementador (aceitas pela revisão): `Trecho` em config.rs; dobras a partir da faixa, não da
  lista; menos de 5 concursos na busca → `AvaliacaoSemConcursos`, `qtd = 0` → `AvaliacaoEntradaInvalida`;
  trancado vazio → `AvaliacaoSemConcursos`; walk-forward e trancado como funções separadas (a 1c.5 mapeia a
  opção do trancado para `avaliar_trancado`); `k` é parâmetro; se a entrada começar acima do corte, o
  trancado calibra só com o que recebeu e o 1º da entrada fica de fora (documentado: a 1c.5 deve passar a
  série desde o 1º concurso do índice).
  Testes: `src/avaliacao/walk_forward/tests.rs` (37 + 1 ignorado de medição: dobras e sobra, limites reais,
  corte padrão do config nas 3 loterias, nada acima do corte lido, calibração não recomeça na dobra — conta à
  mão e dobra = trecho da série inteira bit a bit —, ponto no tempo por dobra, trancado, erros, determinismo)
  e 2 em `config.rs`. Revisão: 1ª rodada aprovada com ressalvas (3 BAIXA: texto do erro, derives sem uso em
  `Trecho`, doc de `avaliar_trancado`), corrigidas numa rodada; 2ª rodada aprovada com ressalvas, só BAIXA.
  verificador-vazamento: **sem vazamento**. Gate: fmt ok, clippy ok, 494 testes (343 lib + 28 sync +
  56 features + 57 indice + 10 parser, 4 ignorados), `validar todas` ok.
  **Para a 1c.5:** a CLI/orquestração deve chamar só `walk_forward` e `avaliar_trancado` (as `_entre`,
  `avaliar` e `avaliar_topk` são públicas e aceitam `u32::MAX`; alternativa: reduzi-las a `pub(crate)`);
  montar as observações a partir de `indice` + `features_dezena` desde o 1º concurso do índice (avisar ou
  recusar se a entrada começar depois); ajustar o texto de `AvaliacaoSemConcursos` se precisar.
  **Para a 1.5.1:** usar `trecho_do_concurso` (a avaliação repete a comparação `<= corte` em `ate_o_corte`).
  **Pendências BAIXA (revisão/verificador):** comentário do módulo `walk_forward.rs:19-21` diz que cada
  dobra não lê nada acima do seu fim "nem para validar", mas a divisão em dobras valida a busca inteira até
  o corte (dado inválido na dobra 5 derruba a dobra 1; não é vazamento); testes sugeridos e não feitos:
  trancado com as configs reais (3417/2760/6422), walk-forward completo com config real em Lotofácil e
  Quina, dobras iguais com `saiu`/posições diferentes, dado inválido em dobra posterior.
- 2026-10-05 — 1c.3/1c.4, decisões do usuário: aceitas as recomendações. 1c.4: α = 5% por família, Holm
  nos testes por dezena, qui-quadrado global sem correção, p-valor original e corrigido por dezena
  (registrado no PLANEJAMENTO.md); 1c.4 desbloqueada. Para a 1c.5: as funções de avaliação que aceitam
  corte arbitrário (`avaliar`, `avaliar_topk`, variantes `_entre`) passam a `pub(crate)`, e a CLI chama
  só `walk_forward` e `avaliar_trancado`.
- 2026-10-05 — 1c.4: interpretações do orquestrador para pontos que o plano não fixa (**a confirmar pelo
  usuário**, depois registrar no PLANEJAMENTO.md): (1) só funções puras em `src/avaliacao/`, CLI na 1c.5;
  (2) entrada `cfg` + `&[Resultado]` (histórico de `resultados`, trancado incluído; série consecutiva, não
  precisa começar no 1); (3) qui-quadrado de frequência **corrigido para sorteio sem reposição**,
  X² = (N−1)/(N−K)·Σ(O−E)²/E com N−1 gl (o Pearson puro tem média N−K, não N−1), p-valor pela gama
  incompleta regularizada; (4) runs test de Wald–Wolfowitz por dezena, aproximação normal, sem correção de
  continuidade, bilateral; dezena com n1 = 0 ou n0 = 0 fica sem p-valor e fora da família do Holm;
  (5) Holm step-down público (reutilizável na 1c.5), `ALFA = 0.05`, empate pela menor dezena.
- 2026-10-05 — 1c.4 concluída. `src/avaliacao/aleatoriedade.rs` (novo, puro, sem crate nova):
  `testar_aleatoriedade(cfg, &[Resultado]) -> ResumoAleatoriedade`, `qui_quadrado_frequencia`,
  `teste_de_runs`, `holm`, `p_valor_qui_quadrado`, `p_valor_normal_bilateral`, `ALFA`; gama incompleta
  (série + fração contínua de Lentz) com ln Γ exato para a inteiro/semi-inteiro. Usa
  `validacao::validar_resultado` por concurso; repetido/buraco checados no próprio laço (`validar_sequencia`
  exige começar no 1). `src/error.rs`: `AleatoriedadeEntradaInvalida` (erros do histórico, orienta para
  `sync validar`/`buracos`/`inserir`); config inválida e argumentos das funções numéricas seguem em
  `AvaliacaoEntradaInvalida`, série vazia em `AvaliacaoSemConcursos`. Testes em
  `src/avaliacao/aleatoriedade/tests.rs` (30): p-valores de χ² e normal contra referências em `decimal` de
  1200 dígitos (erro ≤ 6e-14, caudas até 1e-299), Holm igual ao `p.adjust` do R, contas à mão na loteria
  MINI, média exata da corrigida = N−1 por enumeração, μ/σ² do runs contra a distribuição exata, erros,
  determinismo, simulação do nulo (Mega-Sena 400×500, 1,5 s).
  **Bancos reais (histórico inteiro, só leitura):** qui-quadrado rejeita a uniformidade nas três —
  Lotofácil X² = 57,27 (24 gl, p = 1,5e-4; desvio concentrado em 1899–3796: dezenas 25, 20, 10 acima, 16 e
  23 abaixo), Mega-Sena 89,47 (59 gl, p = 6,4e-3), Quina 101,92 (79 gl, p = 0,042). Runs após Holm: só a
  Quina 25 (z = −3,94, p Holm 0,0066; p exato ≈ 3,0e-4 confirma). Pela regra do plano, conferir primeiro os
  dados (ex.: segunda metade da Lotofácil contra fonte independente).
  Revisão: aprovada com ressalvas, sem ALTA; corrigidos numa rodada a variante de erro própria (MÉDIA), o
  reuso de `validar_resultado` e o texto dos arredondamentos (BAIXA). Efeito colateral: concurso 0 e data
  inválida agora reprovam, como no motor. verificador-vazamento (via revisor): sem vazamento; a 1c.5 não pode
  usar este resultado para escolher perfil/pesos, só como diagnóstico. Gate: fmt ok, clippy ok, 524 testes
  (373 lib + 28 sync + 56 features + 57 indice + 10 parser, 4 ignorados), `validar todas` ok.
  **Pendente de decisão do usuário (MÉDIA):** a normal do runs subestima a cauda esquerda com n1 pequeno —
  chance de alarme falso após o Holm medida em ~5,7% Lotofácil, 6,0% Mega-Sena, 7,0% Quina (≈9–11% com
  Mega-Sena T = 500), acima do α = 5% por família do plano; o teste de simulação aceita até 10%. Proposta:
  p-valor exato pela distribuição condicional de R (em log, barato), decidindo a definição bilateral
  (2·min das caudas ou soma das probabilidades ≤ a observada), e apertar o teto do teste para ~7%.
- 2026-10-05 — 1c.4, decisão do usuário: o runs test passa a usar **p-valor exato** (a aproximação normal
  dá alarme falso acima dos 5% depois do Holm: ~7% na Quina, ~6% na Mega-Sena). Commit da 1c.4 feito com a
  aproximação normal; a troca para o exato é um ajuste à parte, antes da 1c.5, e depende de escolher a
  regra do teste bilateral (dobro da menor cauda ou soma das probabilidades até a observada). Ainda sem
  confirmação do usuário: as 3 escolhas do implementador (qui-quadrado corrigido para sorteio sem
  reposição; dezena que nunca/sempre saiu fora do Holm; histórico inclui o trancado). Resultado nos bancos
  reais a investigar: uniformidade rejeitada na Lotofácil (p 1,5e-4, desvio concentrado em 1899–3796),
  Mega-Sena (p 6,4e-3) e Quina (p 0,042); runs rejeita a dezena 25 da Quina.
- 2026-10-05 — 1c.4, decisões do usuário: runs test com p-valor exato bilateral pela soma das probabilidades
  até a observada; qui-quadrado corrigido sem reposição (confirmado); dezena que nunca/sempre saiu
  **entra no Holm com p = 1** (muda o código atual, que a deixa de fora); histórico inteiro com o trancado
  (confirmado). Investigação dos desvios de frequência registrada no Backlog do PLANEJAMENTO.md, para
  depois da 1c.5. Ajuste da 1c.4 (p-valor exato + dezena fixa no Holm) a fazer antes da 1c.5.
- 2026-10-05 — 1c.4, ajuste feito (continua [x]; sem commit): runs test com **p-valor exato**
  (`p_valor_runs_exato`, distribuição exata de R em log por recorrência, O(min(n1,n0)), soma compensada,
  bilateral pela soma das probabilidades com P(r) ≤ P(obs)·(1+1e-7), limitado a 1); aproximação normal
  removida por inteiro (`p_valor_normal_bilateral`, campo `z`, `testavel()`); dezena fixa entra no Holm com
  p = 1 (`p_valor`/`p_valor_holm` viraram f64; novos `TesteRuns::fixa()` e
  `ResumoAleatoriedade::dezenas_fixas_runs`). Desvio: uma linha de reexport em `src/avaliacao/mod.rs`.
  Testes do módulo 30 → 37 (enumeração para n ≤ 14, 39 referências de Python com `math.comb`/`Fraction`
  com erro ≤ 1,7e-14, subnormal e zeros, dezena fixa mudando o m do Holm; teto da simulação 10% → 7%).
  Alarme falso após Holm com o exato (2000 históricos): Lotofácil 4,65%, Mega-Sena 4,20%, Quina 4,50%,
  Mega-Sena com 500 concursos 2,95%. Gate: fmt ok, clippy ok, 531 testes (380 lib + 28 + 56 + 57 + 10,
  4 ignorados), `validar todas` ok. Revisão: aprovada com ressalvas, sem ALTA.
  **Bancos reais (cópias, só leitura):** χ² sem mudança (Lotofácil 57,27/24 gl p 1,53e-4; Mega-Sena
  89,47/59 gl p 6,41e-3; Quina 101,92/79 gl p 4,24e-2); nenhuma dezena fixa; runs após Holm rejeita só a
  Quina 25 (n1 = 416, R = 748, p exato 6,59e-5, p Holm 5,27e-3). **Correção da nota anterior:** o
  "p exato ≈ 3,0e-4" da Quina 25 era o dobro da cauda esquerda, não a regra decidida.
  **Pendente de decisão do usuário (MÉDIA, revisor e implementador):** a distribuição de R é em serrote
  (P(2k+1)/P(2k) = (n−2k)/(2k)) e, pela soma das probabilidades, a rejeição depende muito da paridade de R
  (só diz se o primeiro e o último concurso têm o mesmo estado): Quina, nível 5%, rejeita 26,7% com R par
  contra 1,95% com R ímpar (Mega-Sena 15,9% vs 2,1%); o tamanho segue ≤ α. Alternativa: "dobro da menor
  cauda" (monótona em R). Nos dados reais a Quina 25 (R par) seria rejeitada nas duas regras (Holm 0,024
  com o dobro da cauda). Pendências BAIXA: `p_valor_runs_exato` (pública) valida R só depois de montar os
  pesos (validar a faixa antes); script Python das referências não está no repositório (scratchpad).
- 2026-10-05 — 1c.4, decisão do usuário após o ajuste: bilateral do runs exato passa a ser o **dobro da menor
  cauda** (a soma das probabilidades até a observada fazia a paridade de R decidir a rejeição), registrado no
  PLANEJAMENTO.md. Junto: validar R (e n1, n0) antes de montar a distribuição em `p_valor_runs_exato`, e
  colar como comentário no teste o trecho Python que gerou as referências.
- 2026-10-05 — 1c.4, ajuste rodada 2 (continua [x]; sem commit): decisão do usuário, runs exato bilateral
  pelo **dobro da menor cauda**, p = min(1, 2·min(P(R ≤ r), P(R ≥ r))) (registrado no PLANEJAMENTO.md),
  por causa da dependência da paridade de R na regra anterior. `p_valor_runs_exato`: caudas em
  log-soma-exp relativas ao modo, limite 1, folga 1+1e-7 removida; nova `validar_runs` confere n1/n0
  (vazio, overflow) e a faixa de R antes de montar a distribuição (`AvaliacaoEntradaInvalida` com a faixa
  possível). Testes do módulo 37 → 38: força bruta n ≤ 14 com a regra nova, 59 referências de Python
  (trecho `math.comb` + `Fraction` colado como comentário em tests.rs), p unimodal com pico na mediana
  (não no modo), `rejeicao_nao_depende_da_paridade_de_r` (Quina 446/6688: 5,36% par vs 4,35% ímpar a 5%;
  a regra antiga daria razão > 5), dezena fixa mudando a decisão do Holm (m = 4 vs 3), simulação com teto
  de 7% e janela por dezena 2,5–3,7%. Alarme falso após Holm (2000 históricos): Lotofácil 4,40%,
  Mega-Sena 3,55%, Quina 4,10%, Mega-Sena 500 concursos 1,15%. Gate: fmt ok, clippy ok, 532 testes
  (381 lib + 28 + 56 + 57 + 10, 4 ignorados), `validar todas` ok. Revisão: aprovada com ressalvas, só
  BAIXA; os três achados da rodada 1 resolvidos.
  **Bancos reais (cópias, só leitura):** χ² sem mudança (Lotofácil 57,27/24 gl p 1,54e-4; Mega-Sena
  89,47/59 gl p 6,41e-3; Quina 101,92/79 gl p 4,24e-2); nenhuma dezena fixa; runs após Holm rejeita só a
  Quina 25 (n1 = 416, R = 748, μ = 784,5, p 3,04e-4, p Holm 0,0243). Menores das outras: Lotofácil 7
  (p 0,0154, Holm 0,385), Mega-Sena 26 (p 0,0100, Holm 0,600). As notas anteriores com p 6,59e-5 da
  Quina 25 valem para a regra descartada.
  Pendências BAIXA: comentário de `dobro_produto` em `teste_de_runs` diz que cabe em u128 para todo n em
  u64 (só vale até n ≈ 2^32; inalcançável na prática); `p_valor_runs_exato` pública aloca O(min(n1,n0))
  (documentar ou tornar `pub(crate)`); alinhar o número da paridade da regra antiga (26,7% no plano,
  26,6% no teste, ≈ 27% na doc do módulo).
- 2026-10-05 — 1c.5 não iniciada (continua [ ]): o plano diz só "Comando `sync avaliar <loteria> --perfil X`"
  e deixa para a 1c.5 pontos que mudam esquema, valores e saída. Perguntas devolvidas ao usuário, com
  sugestões: (1) **teste robusto do veredito**: bootstrap circular por blocos (Politis–Romano) sobre as
  diferenças por concurso perfil − acaso; bloco b = ⌈T^(1/3)⌉ (busca: Lotofácil 16, Mega-Sena 15, Quina 19)
  — ou fixo (ex.: 50), já que a calibração acumulada tem memória longa; B = 10 000 reamostragens; PRNG
  próprio (SplitMix64/xoshiro256**, sem crate nova) com semente fixa no código (ex.: 20261005), para a saída
  ser determinística; p-valor unilateral (fração das médias reamostradas e centradas ≥ média observada) e IC
  95% percentil; alternativa: teste de permutação recalculando a calibração (mais fiel ao nulo, mais caro);
  (2) **qual métrica decide**: veredito "supera o acaso" pelo log loss (bootstrap, α = 5%); Brier com o
  mesmo bootstrap, só informativo; top-k com o p-valor exato da 1c.2 (já robusto à correlação serial);
  (3) **trecho do veredito**: na busca, o trecho inteiro (2º concurso do índice até o corte = união das 5
  dobras), com as dobras só descritivas (estabilidade); no trancado, o trecho do trancado;
  (4) **trancado**: flag `--trancado` (só `avaliar_trancado`; padrão = walk-forward); "uma única vez"
  pela tabela `avaliacoes_trancado(perfil, hash_toml, ate_concurso, avaliado_em)` — **muda o esquema e
  não é regenerável** (é um registro, foge da regra das tabelas derivadas; alternativa: arquivo de
  registro por loteria fora do banco); política sugerida: mesmo `(perfil, hash_toml)` já avaliado →
  recusa sem `--repetir`, que avalia e mostra as avaliações anteriores; perfis diferentes no trancado →
  p-valores também com Bonferroni pelo nº de `(perfil, hash_toml)` distintos registrados na loteria;
  (5) **correção por vários perfis/k na busca**: o comando avalia um perfil por vez e não sabe quantos
  foram testados; sugestão: `--k` opcional (padrão K), aviso fixo na saída sobre a condição de validade do
  p-valor e opção `--testados N` para Bonferroni (p·N, limitado a 1) junto do original;
  (6) **aleatoriedade**: não depende de perfil; sugestão: comando à parte `sync aleatoriedade <loteria|todas>`
  (é comando novo, fora do plano — precisa de aprovação) ou, sem mudar o plano, flag `--aleatoriedade` no
  `sync avaliar`; (7) **saída**: só texto no terminal (cabeçalho com loteria/perfil/hash/trecho/corte,
  tabela por dobra com log loss, Brier e top-k perfil × acaso, diferença, EP, IC normal, p exato, e linha
  final do veredito com o bootstrap); nada gravado além do registro do trancado; sem `--json` por ora;
  (8) **pré-condições** (como no `sync indice`): uma loteria e um perfil por execução, sem `todas`; recusa
  se o perfil não estiver em `perfis`, se o hash do `.toml` em disco diferir do gravado, se o índice não
  cobrir até o último concurso de `resultados` + 1, se as features estiverem desatualizadas ou se faltar
  concurso (buraco = erro); observações de `indice` + `features_dezena` (janela do perfil) desde o 1º
  concurso do índice; o SQL fica em `db.rs`, o fluxo em `orquestracao`, a CLI só imprime.
  Já decididos e a aplicar: `avaliar`, `avaliar_topk`, variantes `_entre` e `p_valor_runs_exato` passam a
  `pub(crate)`; a CLI chama só `walk_forward` e `avaliar_trancado`; nunca `u32::MAX` como corte.
- 2026-10-05 — 1c.5, decisões do usuário: aceitas as 8 sugestões com 3 ajustes, registradas no
  PLANEJAMENTO.md (1c): bloco fixo de 50 no bootstrap (em vez de ⌈T^(1/3)⌉); registro do trancado em
  `registros/trancado_<loteria>.json` versionado no git (em vez de tabela no banco); testes de
  aleatoriedade num comando à parte, `sync aleatoriedade <loteria|todas>`, que entra junto com a 1c.5.
  1c.5 desbloqueada.
- 2026-10-05 — 1c.5 concluída (sem commit). Comandos `sync avaliar <loteria> --perfil X [--k K]
  [--testados N] [--trancado [--repetir]]` e `sync aleatoriedade <loteria|todas>`. Novos:
  `src/avaliacao/bootstrap.rs` (puro: SplitMix64 próprio, sorteio sem viés, bootstrap circular com blocos
  fixos de 50, B = 10 000, semente 20261005, p unilateral (1+#{m*−m ≤ m})/(B+1), IC 95% percentil sem
  interpolação), `src/registros.rs` (`trancado_<loteria>.json` determinístico, LF, gravação atômica,
  `deny_unknown_fields`; pasta padrão `registros/` **ao lado do banco usado**), `tests/avaliar.rs` (85
  testes). Alterados: `src/avaliacao/{aleatoriedade, metricas, mod, topk, walk_forward}.rs` (`veredito_da_busca`,
  `veredito_do_trancado`, `bonferroni`; `avaliar`/`avaliar_topk`/`_entre` viraram `#[cfg(test)] pub(crate)`,
  `p_valor_runs_exato` `pub(crate)`; `faixa_do_trancado` sem `u32::MAX`), `src/orquestracao.rs`
  (`avaliar_perfil`/`avaliar_perfil_e_formatar`, `testar_aleatoriedade_do_banco`, `ler_features_em_dia`
  extraída de `sincronizar_indice`), `src/db.rs` (`iniciar_leitura`, `abrir_somente_leitura`; banco só
  leitura), `src/error.rs`, `src/bin/sync.rs`, `src/lib.rs`, `README.md`, `.gitattributes`
  (`registros/*.json text eol=lf`). A execução anterior caiu por limite de API (HTTP 429) e foi retomada
  sobre o trabalho parcial.
  Rodadas: (1) revisão aprovada com ressalvas — MÉDIA pasta de registros relativa ao diretório de trabalho
  (corrigida: ancorada no banco, `--registros` escondido, avisos com caminho absoluto); BAIXAS corrigidas:
  relatório montado antes de gravar o uso e impresso sem panic (saída fechada no trancado → relatório na
  saída de erro, código 1), recarga do registro antes de salvar (até 3 tentativas, senão
  `RegistroTrancadoConcorrente`), campo duplicado removido. (2) auditoria de vazamento, duas MÉDIAS
  corrigidas: índice gravado agora é recalculado em memória (`motor::calcular_indice`) e conferido bit a bit
  com o gravado (`IndiceIncoerente` se divergir; fecha "resultado antigo corrigido + `sync features` sem
  `sync indice`"; custo +0,02 a +0,2 s); com `--trancado` o cabeçalho sempre diz qual registro foi lido e
  quantos usos tinha (inclusive "0 (arquivo inexistente…)"). 2ª revisão: **aprovada com ressalvas, só
  BAIXA**; verificador-vazamento: **sem vazamento**.
  Gate: fmt ok, clippy ok, 645 testes (409 lib + 28 sync + 85 avaliar + 56 features + 57 indice + 10 parser,
  4 ignorados), `validar todas` ok; sha256 dos bancos da raiz inalterado; sem `registros/` na raiz; o
  trancado não foi rodado em banco real nem cópia.
  **Cópias, padrao_v1, walk-forward:** log loss perfil − acaso (bootstrap) Lotofácil +3,51e-4 (p 0,9999),
  Mega-Sena +4,24e-4 (p 1,0), Quina +2,53e-4 (p 1,0) → **não supera o acaso** nas três; top-k p exato
  Lotofácil 0,0014 (30950 vs 30735), Mega-Sena 0,545, Quina 0,0104 (2105 vs 2006) — coincide com a não
  uniformidade da 1c.4 (backlog), não é sinal do perfil. Tempos 1,1 s / 0,65 s / 2,2 s. `sync
  aleatoriedade` igual à 1c.4 (χ² rejeita nas três; runs rejeita só Quina 25 após Holm).
  **Desvios a aprovar pelo usuário:** `--k` e `--testados` recusados com `--trancado` (k fixo = K; o
  registro não guarda k); bootstrap exige T ≥ 100 (2 blocos), senão sai sem veredito e sem erro; `--repetir`
  sem uso anterior é aceito; Bonferroni do trancado conta pares (perfil, hash) distintos incluindo o atual,
  também no top-k; `--repetir` não aumenta o Bonferroni; avisos de pasta na saída de erro; registro ao lado
  do banco (o plano diz `registros/` sem fixar a âncora); `PLANEJAMENTO.md:314` diz "só a CLI chama
  `walk_forward`/`avaliar_trancado`", mas quem chama é a orquestração (ajustar o texto).
  **Pendências BAIXA:** `avaliacao::avaliar_trancado`/`veredito_do_trancado` seguem `pub` (sem uso de
  produção a primeira; sugestão: `pub(crate)`/`#[cfg(test)]`, para a 1.5 não avaliar o trancado sem
  registro); `--registros` escondido e `--db` em outra pasta ou apagar o arquivo contornam o uso único (só
  aviso; opção mais forte: recusar `--trancado` fora da pasta padrão); janela de corrida entre a última
  recarga e o rename (sem trava; README pede não rodar em paralelo); doc de `avaliar` em sync.rs diz que
  toda a saída é determinística (no trancado variam data e caminho); `saiu` vem de `features_dezena` sem
  recomparar com `resultados`; checagem de chaves duplicada entre `juntar_indice_e_features` e
  `conferir_indice_recalculado`; segunda cópia do índice em memória; erro do trancado depois de ler o
  registro não mostra a contagem de usos; a busca lê e valida também as linhas do trancado (só integridade).
- 2026-10-05 — 1c.5, decisões do usuário: aceitos os itens 1–5 (k fixo no trancado, mínimo de 100
  concursos no bootstrap, Bonferroni do trancado, `--repetir` sem uso anterior, `registros/` ao lado do
  banco), registrados no PLANEJAMENTO.md; frase "só a CLI chama" corrigida (quem chama é a
  orquestração); `avaliar_trancado` virou `#[cfg(test)] pub(crate)` (sem uso em produção) e
  `veredito_do_trancado` `pub(crate)`, ambos fora do reexport público. Item 8 (registro contornável
  por `--db`/`--registros`/apagar o arquivo) segue só com aviso, sem decisão nova. Gate: fmt, clippy e
  645 testes (409 lib + 28 sync + 85 avaliar + 56 features + 57 indice + 10 parser, 4 ignorados).
- 2026-10-05 — 1b.4 concluída (pedido do usuário, registrada no PLANEJAMENTO.md antes do código):
  `sync tabela <loteria|todas> --perfil X [--concurso N] [--saida tabelas]`. As conferências do
  `avaliar` foram extraídas para `ler_indice_conferido` em `src/orquestracao.rs`, usada pelo `avaliar`
  (sem mudança de comportamento) e pela nova `tabela_do_concurso`; `src/tabela.rs` (novo, puro) monta o
  CSV (`;`, vírgula decimal, 19 colunas, uma linha por dezena em ordem de posição); erro novo
  `TabelaConcursoInvalido`; `/tabelas` no `.gitignore`; README e `docs/GUIA.md` atualizados. Testes:
  5 unitários do CSV e 10 de integração em `tests/tabela.rs` (próximo concurso igual ao índice e às
  features, concurso sorteado com `saiu` = dezenas sorteadas, recusas, CSV, CLI sem tocar no banco e
  determinística). Gate: fmt, clippy, 660 testes (4 ignorados). Rodado nos bancos reais (só leitura,
  hashes iguais): tabelas dos concursos 3797, 3067 e 7135 gravadas em `tabelas/`. Sem revisão do
  `revisor-arquitetura` (feito direto, a pedido do usuário).
- 2026-10-06 — 1.5.1 não iniciada (continua [ ]): o plano diz só "`sync dataset <loteria> --formato
  parquet|csv`" e "o dataset marca a qual parte cada concurso pertence"; faltam pontos que mudam o
  arquivo gerado, as dependências e a proteção do trancado. Perguntas devolvidas ao usuário, com
  sugestões: (1) **trancado no arquivo**: por padrão só concursos ≤ corte (`trecho_do_concurso` =
  Busca), coluna `trecho` sempre presente; `--trancado` inclui o resto (só para a validação final)
  — ou sempre tudo, só com a coluna?; (2) **formato das linhas**: longo, uma linha por (concurso,
  dezena, janela), igual a `features_dezena` (o Python pivota), em vez de largo (9×4 colunas);
  (3) **colunas**: `concurso, data, dezena, janela, trecho`, os 9 componentes, `saiu`,
  `feature_version`; sem índice/perfil (o Python recalcula o índice a partir dos pesos; os vetores de
  conformidade da 1.5.4 garantem a paridade); só `saiu IS NOT NULL`; (4) **Parquet**: o crate `parquet`
  (arrow-rs) é pesado (dezenas de dependências, compilação longa); alternativas: (a) `parquet` sem
  `arrow`, só a API de baixo nível, com compressão desligada ou zstd; (b) entregar só CSV agora e
  Parquet depois; (c) Python converte CSV→Parquet (fere "Rust gera o dataset"); sugestão (a);
  (5) **determinismo**: mesmos dados → mesmos bytes nos dois formatos (ordem concurso, dezena, janela;
  sem carimbo de tempo dentro do arquivo; metadados do Parquet fixos), testado; (6) **CSV**: padrão
  de máquina (`,` separador, `.` decimal, cabeçalho, NULL = campo vazio, LF, UTF-8 sem BOM),
  diferente do `sync tabela` (que é para planilha); (7) **saída e manifesto**: `--saida datasets/`
  (no `.gitignore`), arquivo `datasets/<loteria>/features_v<feature_version>.<ext>` + `manifesto.json`
  ao lado (loteria, feature_version, último concurso, corte, trecho incluído, nº de linhas, SHA-256 do
  arquivo, impressão de `motor_estado`), para o Python conferir o que leu; (8) **pré-condições**: uma
  loteria por execução (sem `todas`, como `features`/`indice`), recusa se as features estiverem
  desatualizadas ou de resultados alterados (reusar `ler_features_em_dia`), banco só leitura; SQL em
  `db.rs`, fluxo em `orquestracao`, montagem do arquivo num módulo puro (ex.: `src/dataset.rs`).
- 2026-10-06 — 1.5.1 implementada pelo `cientista-ml` (continua [~], aguardando decisões de plano):
  `sync dataset <loteria> [--formato parquet|csv] [--trancado] [--saida datasets]`. Novos
  `src/dataset.rs` (montagem de CSV, Parquet e manifesto, sem banco) e `tests/dataset.rs`; fluxo
  `dataset_de_features` em `src/orquestracao.rs` (reusa `ler_features_em_dia` por janela, numa
  transação de leitura); SQL `ler_features_ate` em `db.rs`; erros `DatasetIncoerente` e `Parquet`;
  `parquet = 60.0.0` sem arrow (MSRV 1.88, sem compressão, `created_by` fixo); chrono 0.4.39 → 0.4.45
  no Cargo.lock (exigido pelo parquet); `/datasets` no `.gitignore`; README e GUIA atualizados.
  Gate: fmt ok, clippy ok, 697 testes (4 ignorados, já existentes), `validar todas` ok. Nas cópias
  dos bancos: duas execuções com bytes iguais nos dois formatos (Quina 2.054.720 linhas até 6421),
  bancos intactos; lido de volta com pyarrow/pandas numa venv fora de `ml/`, valores iguais ao SQLite.
  `revisor-arquitetura`: aprovado com ressalvas, sem achado alto; `verificador-vazamento`
  (independente): sem vazamento. Desvios do plano a confirmar: (1) `manifesto_<ext>.json` (um por
  formato) em vez de `manifesto.json`; (2) `--formato` com padrão `parquet`; (3) sem coluna `loteria`
  (fica no manifesto, nos metadados do Parquet e na pasta). Perguntas ao usuário: (A) `--trancado`
  grava no mesmo nome `features_v<fv>.<ext>` do dataset de busca, então uma validação final pode deixar
  o trancado onde a busca de pesos vai ler; sugestão: nome próprio (`features_v<fv>_trancado.<ext>` +
  manifesto próprio), em vez de depender do Python recusar `trecho != busca`; (B) a impressão no
  manifesto é a do banco inteiro (a de `motor_estado`, inclui o trancado): sugestão: manter e rotular
  como impressão do banco, e acrescentar uma impressão só dos concursos incluídos. Ajustes baixos para a
  próxima rodada: `Dataset::novo` recusar concurso > corte sem `--trancado` (hoje confia no rótulo);
  variante de erro própria para "valor não cabe em INT32"/falha ao serializar o manifesto (a mensagem
  atual manda regenerar as features); teste de CLI com concurso de fato trancado, se viável.
- 2026-10-06 — 1.5.1, decisões do usuário após a 1ª rodada: aprovados os desvios (manifesto por formato,
  `--formato` padrão `parquet`, sem coluna `loteria`); (A) `--trancado` grava com nome próprio
  (`features_v<fv>_trancado.<ext>` e manifesto próprio); (B) manifesto com a impressão do banco
  rotulada como tal e mais a impressão só dos concursos incluídos. Registrado no PLANEJAMENTO.md.
  Corrigida a mensagem de `sync tabela --concurso` com `todas` (espaços no meio da frase, erro de
  edição da 1b.4).
- 2026-10-06 — 1.5.1 concluída ([x]), rodada 2 (`cientista-ml`), com as decisões aprovadas pelo usuário
  (plano atualizado: desvios 1–3 da rodada 1, (A) e (B)): `--trancado` grava
  `features_v<fv>_trancado.<ext>` + `manifesto_trancado_<ext>.json` e nunca toca no par da busca
  (teste de CLI compara os bytes da busca antes e depois); manifesto com `impressao_resultados_banco`
  (a de `motor_estado`) e `impressao_resultados_incluidos` (mesma função, só até o último concurso
  incluído; teste: não muda com concurso novo acima do corte); `Dataset::novo` recusa concurso acima do
  corte sem `--trancado` também pelo número; erros próprios `ValorForaDoInt32` e `ManifestoJson`; teste
  de CLI com concursos de fato trancados (banco sintético da Mega-Sena com 2762 concursos, ~15 s em
  debug; o gerador de datas do teste passou a avançar o ano a cada 336 concursos). README e GUIA
  atualizados. Gate: fmt ok, clippy ok, 703 testes (4 ignorados, já existentes), `validar todas` ok.
  Ponta a ponta na cópia da Mega-Sena: busca 1–2759 (662.160 linhas) e trancado 1–3066 (735.840) na
  mesma pasta, arquivos da busca com SHA igual antes e depois; segunda rodada com `diff -r` vazio.
  `revisor-arquitetura`: aprovado com ressalvas, só baixas, não corrigidas: (1) a mensagem de
  `DatasetIncoerente` manda regenerar as features mesmo quando a causa é erro de programação; (2) com
  `--trancado` sem nenhum concurso acima do corte, o manifesto diz `busca+trancado` (o campo descreve o
  modo, não o conteúdo). Em aberto para o usuário: documentar o manifesto do dataset num contrato
  (ex.: `docs/contrato-v1.md` ou doc próprio), hoje só no README/GUIA.
- 2026-10-06 — 1.5.2 não iniciada (continua [ ]): o plano diz só "Pasta `ml/`: leitura do dataset,
  baseline", sem definir ambiente, dependências, o que é o "baseline" nem como o gate roda o pytest.
  Situação da máquina: Python 3.14 (padrão) e 3.12 instalados; o Python global não tem pytest, numpy,
  pandas nem pyarrow, então `python -m pytest ml/` da skill `verificar` falharia assim que `ml/`
  existir. Perguntas devolvidas ao usuário, com sugestões: (1) **ambiente**: venv em `ml/.venv` (fora
  do git), Python 3.12, `ml/pyproject.toml` com versões fixadas (`==`); o gate passa a chamar
  `ml/.venv/Scripts/python -m pytest ml/` (muda a skill `verificar` e o CLAUDE.md, pede aprovação);
  (2) **dependências**: `pyarrow` (Parquet) + `numpy` + `pytest`, sem pandas/polars/scikit-learn;
  (3) **leitura**: só via manifesto — confere SHA-256, nº de linhas, `feature_version`, último concurso
  ≤ corte e recusa qualquer linha com `trecho != busca`; a leitura de busca recusa arquivo/manifesto
  `_trancado` (o trancado só por uma função separada, que a 1.5.2 não implementa); lê Parquet e CSV
  e devolve a mesma estrutura (numpy, pivotada por janela); (4) **o que é o "baseline"**: (a) o acaso
  K/N com log loss, Brier e top-k esperado nas mesmas 5 dobras do Rust, e (b) a avaliação de um perfil
  `.toml` em Python (índice zscore/minmax + calibração pela posição m = 100 + log loss/Brier/top-k),
  conferida com a saída de `sync avaliar <loteria> --perfil padrao_v1` (tolerância ~1e-12); é a métrica
  que a 1.5.3 vai otimizar. Sem modelo de ML "de verdade" (ex.: regressão logística) nesta tarefa;
  (5) **K e N**: derivados do próprio dataset (N dezenas por concurso, K = soma de `saiu`), com recusa
  se algum concurso não bater, sem tabela fixa no Python; (6) **dobras**: as da 1c.3 (5 iguais, do 2º
  concurso até o corte, sobra na última), reimplementadas e testadas contra os limites reais da 1c.3;
  (7) **experimento**: `ml/experimentos/001_baseline.py`, saída só em texto, resultado registrado aqui;
  (8) **testes**: pytest com dados sintéticos pequenos + fixture pequena gerada pelo `sync dataset` em
  banco sintético (CSV e Parquet) em `ml/tests/fixtures/`, versionada.
  Achados à parte: `docs/PLANEJAMENTO.md:567-568` tem de novo uma nota de progresso (1.5.1) colada no
  fim do Backlog — sugestão: apagar de lá (já está aqui); a skill `padroes-python-ml` diz
  `perfis/<nome>.toml`, mas o plano manda `perfis/<loteria>/<nome>.toml` (corrigir a skill antes da 1.5.3).
- 2026-10-06 — 1.5.1, decisões do usuário: aceitas as 8 sugestões, registradas no PLANEJAMENTO.md
  (Fase 1.5, item do `sync dataset`). 1.5.1 desbloqueada.
- 2026-10-06 — Nota acima (1.5.1) movida do fim do PLANEJAMENTO.md, onde tinha sido colada por engano
  no commit a9b64cd. 1.5.2, decisões do usuário: aceitas as 8 sugestões, registradas no PLANEJAMENTO.md
  (Fase 1.5, item da pasta `ml/`); skill `verificar` e CLAUDE.md passam a rodar o pytest pela venv
  `ml/.venv`; skill `padroes-python-ml` corrigida para `perfis/<loteria>/<nome>.toml`. 1.5.2 desbloqueada.
- 2026-10-06 — 1.5.2 concluída ([x]) pelo `cientista-ml`. Nova pasta `ml/`: `pyproject.toml` (Python
  `==3.12.*`; `numpy==2.5.3`, `pyarrow==25.0.1`, `pytest==9.1.1`, grupo `ambiente`; instalar com
  `ml/.venv/Scripts/python -m pip install --group ml/pyproject.toml:ambiente`, pip ≥ 25.1), pacote
  `ml/rust_spider_ml/` (`dataset.py` leitura pelo manifesto, `perfil.py`, `indice.py`, `avaliacao.py`
  calibração/métricas/dobras/top-k, `conferencia.py` comparação com o Rust a 1e-12),
  `ml/experimentos/001_baseline.py`, `ml/ferramentas/gerar_fixture.py` (regenera as fixtures pelo
  próprio Rust: banco sintético de 40 concursos da Lotofácil, semente fixa, bytes iguais em duas
  gerações), 64 testes pytest e fixtures em `ml/tests/fixtures/` (CSV, Parquet, manifestos, saídas do
  `sync avaliar`, referências JSON, perfil `teste_minmax_v1`, `dobras_rust.json` com os limites reais
  das 5 dobras das três loterias). `.gitignore` (`/ml/.venv`, `__pycache__/`, `.pytest_cache/`),
  `.gitattributes` (`ml/tests/fixtures/** -text`, para o autocrlf não quebrar o SHA-256 do manifesto),
  README com seção sobre `ml/`. Nada em `src/`. Desvios aceitos pelo revisor (a confirmar pelo
  usuário): (1) `examples/referencia_avaliacao.rs`, ferramenta Rust fora de `src/` que roda
  `orquestracao::avaliar_perfil` só no trecho de busca e grava JSON em precisão total (o `sync avaliar`
  imprime 6 casas); (2) `dobras_rust.json` gerado por ele a partir de cópias dos bancos reais; (3) a
  impressão dos resultados é recalculada em Python (FNV-1a) e conferida com
  `impressao_resultados_incluidos`, por isso o dataset tem de começar no concurso 1; (4) p-valor exato
  e bootstrap do top-k ficam só no Rust; (5) pacote não instalado (pytest via `pythonpath`).
  Gate: fmt ok, clippy ok, `cargo test` verde (4 ignorados, já existentes), pytest 64 ok, `validar
  todas` ok. `revisor-arquitetura`: aprovado com ressalvas, só baixas; `verificador-vazamento`
  (acionado direto pelo orquestrador, porque os pareceres aninhados não voltaram): sem vazamento, só
  baixas. **Conferência Python × Rust** (cópias dos bancos, trecho de busca, `padrao_v1`, CSV e
  Parquet): diferença máxima 0,0 em todas as métricas, dobras, T, acertos e distribuição, nas três
  loterias. **Resultado do baseline (experimento 001)**, log loss perfil − acaso no total da busca:
  Lotofácil +3,51e-4 (0,673363 × 0,673012), Mega-Sena +4,2e-4 (0,325506 × 0,325083), Quina +2,5e-4
  (0,234045 × 0,233792) — o perfil fica **pior que o acaso em todas as 15 dobras**; Brier idem. Top-k
  por concurso: Lotofácil 9,0630 × 9,0000 (p exato 0,0014 no Rust), Mega-Sena 0,5986 × 0,6000 (p 0,55),
  Quina 0,3279 × 0,3125 (p 0,0104); ganhos pequenos, instáveis entre dobras, a 1ª dobra sempre acima
  do acaso (coerente com os desvios de frequência da 1c.4) — tratar como suspeita, não como sinal.
  Esse é o ponto de partida que a 1.5.3 tenta superar. Achados baixos não corrigidos: (a)
  `001_baseline.py:44-46,76-81` formata EP/IC `None` (dataset com < 10 concursos) com `TypeError`;
  (b) `001_baseline.py:113` deixa `DivergenciaRust` sair como traceback com referência de outro
  perfil; (c) o corte no Python vem do manifesto, não de uma constante por loteria — só um manifesto
  e um CSV forjados à mão com SHA refeito passam (decisão do plano: sem tabela fixa no Python;
  pergunta ao usuário se quer o reforço); (d) com manifesto adulterado, o arquivo trancado é
  decodificado antes de ser recusado; (e) `test_avaliacao_sintetica_e_vazamento` só compara as dobras
  1–3 (o vazamento é coberto por `test_calibracao_so_usa_o_passado`). Corrigir (a), (b) e (d) no
  começo da 1.5.3.
- 2026-10-06 — 1.5.2, decisões do usuário: aprovados os 5 desvios; para o começo da 1.5.3, tabela de cortes
  no Python (reforço contra o manifesto) e contrato do manifesto em `docs/contrato-v1.md`, registrados no
  PLANEJAMENTO.md, junto com as 3 correções baixas do experimento e da leitura do dataset.
- 2026-10-06 — 1.5.3 não iniciada (continua [ ]): o plano diz só "busca de pesos com walk-forward →
  perfis `.toml`" e "ajusta até uma dobra, testa nela"; faltam pontos que mudam o perfil gerado e a
  validade estatística. Perguntas devolvidas ao usuário, com sugestões: (1) **espaço de busca**: 9 pesos
  com sinal livre + janela {0,10,50,100} + normalização {zscore,minmax}; como a posição só depende da
  ordem, pesos normalizados (soma dos |w| = 1) e o vetor todo zero proibido; (2) **método**: busca
  aleatória com semente fixa e orçamento fixo por loteria (ex.: 2 000 candidatos em cada uma das 8
  combinações janela × normalização), sem crate nova (só numpy), opcionalmente seguida de refinamento
  por coordenadas; (3) **protocolo walk-forward** ("ajusta até uma dobra, testa nela"), aninhado: para
  i = 2…5, escolhe o melhor candidato nas dobras 1…i−1 e mede na dobra i; a estimativa honesta da busca
  é o log loss fora da amostra nas dobras 2…5 contra o acaso; o perfil final é o melhor nas dobras 1…5
  (trecho de busca inteiro); (4) **objetivo**: log loss médio por concurso (mesma calibração m = 100 da
  1.5.2); Brier e top-k só informativos; (5) **quando gravar**: grava sempre o campeão como
  `perfis/<loteria>/busca_v1.toml` (versão seguinte se existir), com cabeçalho em comentário (experimento,
  semente, orçamento, nº de candidatos, dataset: feature_version, último concurso, impressão dos
  incluídos, log loss fora da amostra × acaso), pesos arredondados a 6 casas e reavaliados já
  arredondados; **não** registra no banco, não roda o trancado e não vira perfil ativo — isso fica para o
  usuário (o trancado é uso único); alternativa: só gravar se o fora da amostra superar o acaso;
  (6) **múltiplas comparações**: o relatório informa o nº de candidatos e avisa que o p-valor do `sync
  avaliar` na busca é otimista para o campeão; a validação que vale é o trancado; (7) **entrega**: módulo
  `ml/rust_spider_ml/busca.py`, experimento `ml/experimentos/002_busca_pesos.py` (saída em texto,
  resultado anotado aqui), testes pytest (determinismo pela semente, dobra de teste não influencia a
  escolha, invariância de escala, `.toml` gerado relido pelo `perfil.py`), e conferência ponta a ponta numa
  cópia do banco: `sync indice` + `sync avaliar` com o perfil gerado batendo com o Python a 1e-12;
  (8) **ordem**: primeira rodada só com o que já está decidido para o começo da 1.5.3 (tabela de cortes no
  Python, contrato do manifesto em `docs/contrato-v1.md`, correções baixas (a), (b) e (d) da 1.5.2), que
  não depende das perguntas acima.
- 2026-10-06 — 1.5.3, decisões do usuário: aceitas as 8 sugestões (inclusive gravar sempre o melhor perfil e
  a ordem do item 8), registradas no PLANEJAMENTO.md (Fase 1.5, "Regras da 1.5.3"). 1.5.3 desbloqueada.
- 2026-10-07 — 1.5.3 concluída ([x]) pelo `cientista-ml`, em 3 rodadas. **Começo da 1.5.3**: tabela
  `CORTES_BUSCA` (3416, 2759, 6421) em `ml/rust_spider_ml/dataset.py`, imutável, conferida contra o
  `ultimo_concurso_busca` do manifesto e contra cada concurso lido (loteria desconhecida, corte divergente
  ou concurso acima do corte são recusados; testes sintéticos injetam `cortes=` por argumento nomeado, que
  não consegue trocar o corte das loterias reais); `docs/contrato-v1.md` criado com a seção do dataset
  (arquivos, 14 campos do manifesto, 16 colunas, CSV/Parquet, determinismo, o que o Python confere,
  compatibilidade; a Fase 2 completa o resto); correções (a) EP/IC `None` → `n/d`, (b) `DivergenciaRust`
  → mensagem e código 1 (entrada recusada → código 2), (d) trancado recusado antes de decodificar
  (manifesto, nome, rodapé do Parquet; busca de `,trancado,` nos bytes do CSV como recusa precoce; a
  conferência linha a linha continua). **Busca**: `ml/rust_spider_ml/busca.py`,
  `ml/experimentos/002_busca_pesos.py`, `ml/tests/test_busca.py` (34 testes), README com a seção da busca;
  perfis gerados `perfis/{lotofacil,megasena,quina}/busca_v1.toml` (não rastreados, para revisão do
  usuário). Semente 20261006, 2.000 × 8 = 16.000 candidatos por loteria (orçamento completo, sem desvio de
  tempo: 19 min 14 s as três, 4 processos), sem refinamento peso a peso (opcional). Gate: fmt ok, clippy
  ok, `cargo test` 703 passados / 4 ignorados (já existentes), pytest 125, `validar todas` ok.
  `revisor-arquitetura`: aprovado com ressalvas, só baixas; 4 corrigidas (002 tudo ou nada: valida todos os
  manifestos antes de gravar e recusa loteria repetida; `gravar_perfil` recusa semente/orçamento `None`;
  aviso de nome já registrado no banco; ressalva da correlação serial no IC normal). `verificador-vazamento`:
  sem vazamento (teste de mutação "olhar uma dobra a mais" detectado). `verificador-conformidade`: Python ×
  Rust com diferença 0 (bit a bit) em log loss, Brier e top-k, por dobra e total, nas 3 loterias, com `sync
  indice` + `sync avaliar --testados 16000` numa cópia dos bancos (parser Rust aceitou os 3 perfis).
  **Resultado** (log loss perfil − acaso; walk-forward, fora da amostra nas dobras 2–5, IC 95% normal):
  Lotofácil +4,98e-4 [+2,21e-4; +7,74e-4] (0,6735093 × 0,6730117, T = 2732); Mega-Sena +5,66e-4
  [+3,90e-4; +7,42e-4] (0,3256488 × 0,3250830, T = 2207); Quina +2,56e-4 [+1,91e-4; +3,21e-4] (0,2340477 ×
  0,2337917, T = 5136) — **pior que o acaso nas três e em todas as dobras de teste**; os escolhidos mudam de
  janela/normalização a cada passo (instáveis). Perfil final (dentro da amostra, trecho inteiro): Lotofácil
  janela 0 minmax, log loss +1,26e-4 sobre o acaso, top-15 9,0712 × 9,0 (p 0,83; 1,0 corrigido); Mega-Sena
  janela 0 zscore, +2,79e-4, top-6 0,5852 × 0,6; Quina janela 0 minmax, +2,00e-4, top-5 0,3101 × 0,3125;
  `sync avaliar` com 16.000 testados: "NÃO supera o acaso" nas três. Leitura: nem o melhor de 16.000 ganha
  da constante K/N dentro da amostra — provável custo da calibração pela posição (N taxas com m = 100
  somam ruído que a constante não tem; o mesmo aparece no `padrao_v1`), ou seja, "empatar com o acaso"
  aparece como pequeno excesso positivo nessa métrica. Desvios aceitos pelo revisor: (1) pesos arredondados
  a 6 casas **antes** de avaliar (soma dos |w| = 1 ± 5e-6; o avaliado é exatamente o gravado); (2) pesos
  de cada passo do walk-forward só na saída do 002, não no cabeçalho do `.toml`; (3) Parquet sem
  estatísticas de `trecho`/`concurso` passa a ser recusado (documentado no contrato); (4) códigos de saída
  1/2 no 001. Pendências: achado baixo para tarefa Rust futura — fixar `set_statistics_enabled` em
  `src/dataset.rs:352-366` com teste (o Python agora depende dessas estatísticas); achado (e) da 1.5.2
  continua aberto; decisão do usuário: levar algum `busca_v1` ao trancado (uso único) ou não, e se os
  perfis gerados entram no git.
- 2026-10-09 — 1.5.4 não iniciada (continua [ ]): o plano diz só "`testes/conformidade/*.json`: entrada →
  features/índice esperados, verificados por `cargo test` e `pytest`". Hoje o Python **não calcula
  features** (lê do dataset) nem `variacao_posicao` (`ml/rust_spider_ml/indice.py` só faz total e
  posição), e já existe uma conferência Python × Rust por fixture (`ml/tests/test_conformidade_rust.py`).
  Perguntas devolvidas ao usuário, com sugestões: (1) **alcance no Python**: (a) o pytest confere só o
  trecho features → índice (total, posição e, novo, `variacao_posicao`), e o trecho resultados →
  features é conferido só pelo `cargo test`; ou (b) o Python ganha uma implementação de referência das
  features (só para conformidade, fora do caminho de produção), conferida pelo pytest; sugestão (b),
  coerente com "migração futura para 100% Rust" e com a paridade nos dois sentidos; (2) **formato do
  vetor**: um JSON por caso, `{versao_formato, descricao, loteria (K, N, faixa), feature_version,
  entrada: {resultados | features}, perfil: {janela, normalizacao, pesos}, esperado: {features, indice}}`,
  `NULL` como `null`, floats em JSON com 17 dígitos significativos (ida e volta exata do `f64`);
  (3) **quem gera o esperado**: casos feitos à mão (golden do plano, bordas de NULL, janela 10/50/100 e
  fronteiras, componente constante, todo NULL, empate exato e quase empate a 1e-9, zscore e minmax,
  variação no primeiro concurso) com o esperado **gerado pelo Rust** por uma ferramenta
  (`examples/gerar_conformidade.rs`, como a `referencia_avaliacao.rs`) e conferido à mão nos casos
  pequenos; o `cargo test` falha se o JSON versionado divergir do que o Rust calcula hoje (vetor nunca é
  regerado em silêncio); (4) **comparação**: inteiros e posição exatos; `total` igual bit a bit (já é
  arredondado a 1e-9 e o Python reproduz operação por operação) — ou tolerância 1e-12?; sugestão: exato;
  (5) **local**: `testes/conformidade/` na raiz, como diz o plano (ao lado de `tests/`, que é do Rust),
  com `testes/conformidade/** -text` no `.gitattributes`; (6) **tamanho**: vetores pequenos (≤ ~150
  concursos sintéticos, para cobrir a janela de 100), dezenas de KB no total; sem dados reais;
  (7) **fora de escopo**: calibração/log loss/top-k (já conferidos pela fixture da 1.5.2); de carona, o
  achado baixo da 1.5.3 (`set_statistics_enabled` fixo em `src/dataset.rs` com teste) entra ou fica
  para outra tarefa?
- 2026-10-09 — Respostas do usuário: todas as sugestões aceitas (1b, formato, esperado pelo Rust,
  `total` bit a bit, `testes/conformidade/`, vetores pequenos); o `set_statistics_enabled` entra na
  1.5.4. Regras registradas no plano. Pendências da 1.5.3 já resolvidas em 2026-10-08: os
  `busca_v1.toml` entraram no git (cf0cbec) e nenhum vai ao trancado.
- 2026-10-09 — 1.5.4 concluída (cientista-ml). 11 vetores sintéticos em `testes/conformidade/` (~94 KB):
  golden, bordas de NULL, janelas 10/50/100 e fronteiras (140 concursos), componente constante, todo
  NULL, empate exato, quase empate a 1e-9, zscore janela 10, minmax janela 50, variação no primeiro
  concurso, formato Lotofácil com `padrao_v1`. Casos definidos em `src/conformidade.rs` (módulo de
  biblioteca, só ferramenta; testes à mão em `src/conformidade/tests.rs`), gerados por
  `examples/gerar_conformidade.rs`; `tests/conformidade.rs` compara bytes gerados × versionados, pega
  arquivo sobrando/faltando, determinismo e tamanho < 100 KB. Python: `referencia_features.py`
  (referência não incremental), `conformidade.py` (total bit a bit, inteiros exatos com tipo),
  `variacao_posicao` em `indice.py`; bate bit a bit nos 11 casos. `src/dataset.rs`:
  `set_statistics_enabled(EnabledStatistics::Page)` explícito (padrão atual do crate, bytes iguais),
  com teste de estatísticas por row group. Gate: fmt/clippy ok, cargo test verde (principal 444
  passed), pytest 171 passed, validar todas ok (em cópias). Revisor: aprovado, só achados baixos.
  Desvios aceitos: formato com `colunas_features`/`colunas_indice`, linhas como arrays posicionais e
  `esperado.concursos_conferidos` (filtra as linhas escritas; o índice é calculado sobre a série
  inteira nos dois lados), para caber em dezenas de KB; maior caso com 140 concursos. Achados baixos em
  aberto: (a) `flutuante` em `src/conformidade.rs:174` normaliza −0.0 para 0.0 ao escrever, então um
  −0.0 do motor não seria pego pelos vetores (só por `tests.rs:235` e testes do motor); (b) `pub mod
  conformidade` na API pública (sugestão: `#[doc(hidden)]`); (c) limite de 100 KB a ~5% do atual;
  (d) `feature_version == 1` literal em `ml/tests/test_vetores_conformidade.py:104`. Parecer do
  verificador-vazamento não chegou a tempo; o revisor conferiu o ponto no tempo à mão sem achados.
- 2026-10-09 — 1.5.4, achados baixos do revisor corrigidos (todas as sugestões aceitas pelo usuário):
  o gravador dos vetores não troca mais −0.0 por 0.0 (um zero negativo do motor passa a mudar o
  arquivo; nenhum vetor tinha −0.0, os 11 antigos não mudaram); limite de tamanho dos vetores subiu
  para 150 KB (hoje ~99 KB); `FEATURE_VERSION` única no Python (`referencia_features.py`), usada pelo
  teste; `conformidade` com `#[doc(hidden)]` em `src/lib.rs`. Dois casos novos, das lacunas apontadas
  pelo verificador-conformidade: `12_meio_exato_arredondamento` (total·1e9 = ±2,5 e 0,5 exatos, onde
  meio para longe do zero e meio para o par divergem) e `13_empate_exato_zscore` (empate exato e
  empate que só existe depois do arredondamento a 1e-9, em z-score). Gate: fmt e clippy ok, Rust
  721 passaram, pytest 175 passaram.
- 2026-10-09 — 1d.1: interpretação adotada pelo orquestrador (revisável; sem como perguntar ao
  usuário nesta execução): investigação **só de leitura** (nada no repositório nem nos bancos; cópias
  e scripts no scratchpad da sessão); "fonte independente" = API JSON da Caixa por concurso
  (`servicebus2.caixa.gov.br/portaldeloterias/api/<loteria>/<n>`) + espelho de terceiros com o
  histórico completo (`loteriascaixa-api.herokuapp.com/api/<loteria>`); erro de dado seria só
  proposto, não corrigido.
- 2026-10-09 — 1d.1 concluída (engenheiro-avaliacao). **Fontes e cobertura** (acesso 2026-10-09):
  API da Caixa — Lotofácil 1899–3796 inteiro + 1–14 (1912 de 3799), Mega-Sena 3067/3067, Quina
  7137/7137, sem bloqueio; espelho de terceiros — 100% nas três. Comparados concurso, data e dezenas
  (conjunto; dezena repetida no concurso checada à parte). **Divergências: só a data da Quina 380**,
  que as duas fontes trazem com o mesmo erro da planilha (1997) e já está em `CORRECOES_DATA`.
  Ressalva: as duas fontes são independentes do nosso pipeline de planilha, **não da Caixa** —
  descartam erro nosso de coleta/parse, não um erro na fonte primária (não existe outra). χ² da 1c.4
  reproduzidos exatamente no snapshot da época (57,27 / 89,47 / 101,92); com o histórico atual
  57,43 (p 1,5e-4) / 89,87 (p 5,9e-3) / 101,69 (p 0,044). Diagnóstico com ~150–200 olhares por
  loteria (blocos, janelas, pontos de mudança, eventos, locais): tratar como exploração, não prova.
  **Conclusões por trecho:**
  - **Lotofácil "1899–3796"**: não é erro de dado. A "concentração" não se sustenta: o corte em 1899
    (2019-12-04) foi escolhido depois de olhar os dados e não coincide com nenhuma mudança de regra,
    local ou equipamento. 1–1898 dá X² 27,5 (p 0,28) e 1899–3796 dá 52,1 (p 7,6e-4), mas as metades
    não diferem (homogeneidade 23,2, p 0,55 analítico e Monte Carlo; se o desvio estivesse só na 2ª
    metade, o esperado seria ≈ 38) e os z por dezena das duas metades correlacionam +0,46 (p perm.
    0,008). Leitura: o mesmo padrão nas duas metades, significativo só no total e na 2ª metade —
    dezena 16 abaixo (z −3,5, abaixo nos 4 quartos), 20/25/10 acima, repetido entre locais de sorteio.
    Sem explicação: acaso ou viés pequeno e persistente (p ≈ 4e-4 após Bonferroni ×3 loterias).
  - **Mega-Sena**: não é erro de dado (3067/3067, 0 divergências). O desvio **global sobrevive** à
    correção entre loterias (p 0,018 com Bonferroni ×3). O que é fraco é a localização no tempo:
    nenhum bloco de ~307 passa sozinho; ponto de mudança marginal em 1751 (2015-10-15, p global
    0,047); homogeneidade na mudança para o Espaço Caixa Loterias/Tietê (concurso 1772, evento
    definido sem olhar os dados) p nominal 7,2e-3, ≈ 0,04 corrigido pelos ~6 eventos da Mega-Sena e
    ≈ 0,13 pelos ~18 das três loterias; Estúdio de TV (2014-08-27) p 0,012 e troca para globo de
    acrílico (concurso 1140, 2009-12-31, fonte: Wikipédia) p 0,04, nominais. Possível associação com
    a mudança de local em 2014–2015, não confirmada.
  - **Quina**: não é erro de dado (7137/7137; só a data do 380). X² p 0,044 vira 0,13 com Bonferroni
    ×3 loterias; único bloco forte 715–1427 (2000–2005, p 0,0012), sem ponto de mudança (p global
    0,87). Compatível com o acaso.
  - **Quina, runs da dezena 25**: não é erro de dado (sequências iguais nas duas fontes). Excesso de
    saídas em concursos seguidos: 42 pares contra 24,2 esperados dado n1 = 416 (cauda exata
    condicional 2,7e-4), espalhados pelo histórico (15/14/12 por terço), sem coincidir com mudança de
    local ou equipamento; a sobreposição geral entre sorteios seguidos é normal (0,311 × 0,3125).
    Marginal depois da multiplicidade: ×80 dezenas ≈ 0,022 unilateral; entre as 165 dezenas das três
    loterias ≈ 0,09 bilateral; agregado de runs das 80 dezenas Σz² = 93,0 (p ≈ 0,15). Sem explicação.
  **Linha do tempo levantada** (campo `local` do espelho = dado; demais pela Wikipédia, confiança
  média): Caminhão da Sorte desde 2000; Estúdio de TV (2014-08-27); Espaço Caixa Loterias, Terminal
  Tietê (2015-12, Lotofácil 1300 / Mega-Sena 1772 / Quina 3963); fim do Caminhão (2019-06); Espaço da
  Sorte, Av. Paulista (2021-11-17, Lotofácil 2374 / Mega-Sena 2429 / Quina 5707); mudanças de
  frequência semanal; globo de acrílico na Mega-Sena em 2009-12-31 (sem fonte primária). Sem
  interrupção na pandemia. Nenhuma data de troca de globos/bolas depois de 2009 encontrada. Teste por
  local: Lotofácil com excesso de variação (p 0,0013) mas o padrão se repete entre locais; Mega-Sena
  p 0,55; Quina p 0,50. **Correções de dado propostas: nenhuma.** A base segue como está para a 1d.2.
  Gate: fmt ok, clippy ok, `cargo test` verde (nenhum código mudou), pytest 175, `validar todas` ok.
  Revisão: aprovada com ressalvas, sem ALTA; recálculos independentes conferiram χ², homogeneidade e a
  cauda exata da Quina 25; as ressalvas de redação (Mega-Sena global × localização, "fonte
  independente", p ajustado da Quina 25, "persistente") estão incorporadas acima.
  **Correção de registros antigos:** a expressão "desvio concentrado em 1899–3796" (notas da 1c.4,
  acima, e `docs/GUIA.md:270`) fica refutada por esta nota. **Pendência (MÉDIA, revisor):** corrigir
  o texto de `docs/GUIA.md:270` — sugerido fazer junto da 1d.4, que já atualiza o GUIA.
- 2026-10-09 — 1d.2: interpretação adotada pelo orquestrador (revisável; sem como perguntar ao
  usuário nesta execução): (1) priori Beta(a, b) com média p = K/N e desvio s·p (s = 5 %; 2,5 % e
  10 % na sensibilidade) → a + b = (1 − p)/(s²·p) − 1; FB = beta-binomial ÷ binomial, via `ln_gamma`;
  (2) janela secundária = últimos min(500, t−1) concursos antes de t; (3) o relatório usa o snapshot
  de t = último + 1 (histórico inteiro); a função pura aceita qualquer t, para a 1d.4; (4) esperado
  sob o acaso: simulação de históricos com sorteio sem reposição (K de N), `SplitMix64` já existente,
  semente 20261008, R = 1000 réplicas, média da contagem por grau — e teste que a confere contra o
  valor exato N·P(grau) da binomial (linearidade da esperança); (5) `sync evidencia <loteria> [--db]`,
  sem `todas`, recusa se `validar` apontar erro, sem gravar nada nem criar tabela; fluxo em
  `orquestracao`, CLI só imprime.
- 2026-10-10 — 1d.2 concluída (engenheiro-avaliacao). `src/avaliacao/evidencia.rs` (novo, puro:
  `ln_gamma` próprio por Stirling + recorrência, `Priori::nova`, `log10_fator_bayes`, `classificar`,
  `Grau`, `Direcao`, `evidencia_por_dezena(cfg, resultados, t, janela)` para qualquer t,
  `esperado_sob_o_acaso` (SplitMix64, semente 20261008, R = 1000) e `esperado_exato` (binomial));
  `src/avaliacao/evidencia/tests.rs` (36), `tests/evidencia.rs` (10), `src/error.rs`
  (`EvidenciaEntradaInvalida`), `src/orquestracao.rs` (`evidencia_do_banco`, sem SQL), subcomando
  `evidencia <loteria> [--db]` em `src/bin/sync.rs` (+2 testes; banco aberto só leitura, tempo no
  stderr, stdout determinístico), README. Segue as interpretações da nota de 2026-10-09 sem desvio.
  Desvio de leitura: a evidência A usa o histórico inteiro, **trancado incluso** (descritiva do
  sorteio, como o `sync aleatoriedade`); a revisão considerou que não fere a regra do trancado.
  Resultado (priori 5 %, histórico inteiro, moderada ou mais observadas / esperadas sob o acaso):
  Lotofácil 2 / 0,14 (dezena 16 forte, log10 FB +1,906; dezena 20 moderada, +1,170), Mega-Sena
  1 / 0,04, Quina 0 / 0,14; últimos 500: nenhuma nas três. Tempo (release): 0,2–0,5 s.
  verificador-vazamento: sem vazamento. Revisão: aprovada com ressalvas, sem ALTA. Gate: fmt ok,
  clippy ok, `cargo test` verde (480 lib + integração), pytest 175, `validar todas` ok.
  **Pendências (MÉDIA, mudam o plano — aguardam o usuário):** (1) teto matemático da direção
  "a favor do acaso": FB máximo a favor de H0 = √(1 + Var_priori/Var_amostral), em x = n·p; com
  s = 5 % dá |log10 FB| ≤ ~0,13 na Mega-Sena, ~0,17 na Quina e ~0,59 na Lotofácil (janela de 500:
  ~0,03) — a evidência contra viés nunca passa de "muito fraca"/"fraca", o que contradiz o "mesmo
  destaque" do plano se não for explicado; sugestão: registrar no plano e no GUIA (1d.4) e imprimir
  no relatório o máximo atingível por janela; (2) escrever no plano que A lê o trancado por ser
  descritiva e que seu resultado não alimenta busca nem escolha de perfil.
  **Pendências BAIXA:** `esperado_exato` e `Grade::get` públicos sem uso externo; `evidencia_por_dezena`
  com t acima do último concurso devolve o snapshot do último em silêncio (documentar ou recusar na
  1d.4); `tests/evidencia.rs:186-205` não confere a unicidade da dezena "muito forte" que o comentário
  afirma; `tests/evidencia.rs:287` depende do alinhamento exato das colunas. Para a 1d.4:
  `evidencia_por_dezena` é O(t) por chamada — versão incremental para o CSV por concurso.
- 2026-10-10 — Decisões do usuário sobre as pendências da 1d.1 e 1d.2 (todas as sugestões
  aceitas), registradas em `PLANEJAMENTO.md`: (1) teto da evidência a favor do acaso documentado
  na Fase 1d; o relatório do `sync evidencia` passa a imprimir o máximo atingível por janela (entra
  na 1d.3) e o GUIA explica a assimetria (1d.4); (2) A lê o trancado por ser descritiva e não
  alimenta busca nem escolha de perfil; (3) descrição da 1d.1 marca a concentração 1899–3796 como
  refutada e guarda a linha do tempo dos locais de sorteio; (4) correção de `docs/GUIA.md:270`
  entra na 1d.4. Pendências BAIXA da 1d.2 seguem para a 1d.3/1d.4.
- 2026-10-10 — 1d.3: interpretação adotada pelo orquestrador (revisável; sem como perguntar ao
  usuário nesta execução): (1) "perfil ativo" não existe no código; passa a ser `padrao_v1` (o plano diz
  que a busca não trocou o perfil ativo), num campo do `LoteriaConfig`, não em `match slug`;
  (2) B = Σ, nos pares (concurso, dezena) das dobras de teste 2 a 5 do walk-forward do `sync avaliar`,
  de ln p_modelo − ln p_acaso (Bernoulli, com o mesmo limite [1e-15, 1 − 1e-15] do log loss), guardado
  como `log10_fb`; graus e limites da escala de A; direção de B rotulada "a favor do índice" (FB > 1) /
  "a favor do acaso" (FB ≤ 1); por dobra só informativo; (3) pré-condições de B iguais às do
  `sync avaliar` (perfil registrado com hash igual ao `.toml`, índice até último + 1, features em dia);
  se B não puder ser calculada, A sai do mesmo jeito e B mostra o motivo e o comando a rodar — exit 0
  com o perfil padrão, exit ≠ 0 com `--perfil` explícito; (4) teto a favor do acaso de A impresso por
  janela e por largura de priori, `log10 √(1 + Var_priori/Var_amostral)` em x = n·p.
- 2026-10-10 — 1d.3 concluída (engenheiro-avaliacao). `src/avaliacao/evidencia_indice.rs` (novo, puro:
  `evidencia_do_indice`, `DirecaoIndice`, por dobra 2–5; reaproveita `calibrar`, `dobras_da_busca`,
  `log_loss`) + 13 testes; `config.rs` (`perfil_ativo = "padrao_v1"`); `evidencia.rs`
  (`teto_a_favor_do_acaso`, `Grade::get` privado, `esperado_exato` só em teste); `orquestracao.rs`
  (`evidencia_completa`, `SecaoIndice`, B indisponível → A sai e B mostra motivo e comando); `sync
  evidencia --perfil --perfis`; `tests/evidencia.rs` (+9); README. Segue a interpretação acima sem desvio.
  Conferência: ln FB = −pares × (log loss perfil − acaso) dobra a dobra contra `walk_forward` e
  `avaliar_perfil`, tolerância 1e-13 × pares × log loss do acaso (erro medido ~50× menor). Resultado
  (`padrao_v1`, dobras 2–5): log10 FB Lotofácil −7,31, Mega-Sena −13,59, Quina −25,00, todas "muito
  forte, a favor do acaso". Teto de A a favor do acaso (priori 5 %, inteiro / últimos 500): Lotofácil
  0,592 / 0,229; Mega-Sena 0,134 / 0,028; Quina 0,170 / 0,017. verificador-vazamento: sem vazamento.
  Revisão: aprovada com ressalvas, sem ALTA. Gate: fmt ok, clippy ok, cargo test verde (lib 500,
  avaliar 85, evidencia 19, indice 57, features 56, ...), pytest 175, `validar todas` ok.
  **Pendência (MÉDIA, muda o plano, aguarda o usuário):** "muito forte a favor do acaso" em B é o
  esperado de um índice **sem informação**: a calibração estima N taxas por posição e paga um custo de
  Occam (estimativa do revisor, ln FB esperado sob o acaso ≈ −17,5 / −41 / −60 contra observados −16,8
  / −31,3 / −57,6). Sugestão: registrar a assimetria de B no plano e imprimir no relatório o ln FB
  esperado de um índice inerte (fórmula fechada ou simulação com semente fixa), ou ao menos ressalva
  no relatório e no GUIA (1d.4). **Pendências BAIXA:** plano diz "~0,03 na janela de 500" para o
  teto, só vale na Mega-Sena (Lotofácil 0,23, Quina 0,02) — corrigir PLANEJAMENTO.md com aprovação;
  tabela por dobra de B sem a coluna "diferença de log loss por par" para conferir com o `avaliar`;
  `comando_para_destravar_o_indice` ignora `--perfis` e usa `&&` (não existe no PowerShell 5.1);
  `IndiceEvidenciado.primeiro_indice`/`ultimo_concurso` sem uso na CLI; README:113 "não lê o
  trancado" → "o trancado não entra no cálculo de B"; duas funções públicas `evidencia_do_indice`
  (pura e com banco) — renomear a da orquestração para `evidencia_do_indice_do_banco`.
- 2026-10-10 — Decisões do usuário sobre a 1d.3 ("aceito as sugestões"): interpretações (perfil
  ativo `padrao_v1`, rótulo "a favor do índice", regra de exit) mantidas. **Índice inerte** registrado
  no plano (Fase 1d, B) e implementado: `esperado_indice_inerte` (200 réplicas, semente 20261010,
  `REPLICAS_INERTE`/`SEMENTE_INERTE`) simula, com as mesmas posições do perfil, resultados sorteados
  ao acaso e devolve média, faixa de 95 % e réplicas ≥ observado; réplica em laço único, conferida
  contra `evidencia_do_indice` nos testes (Quina: 86 s → 12,7 s em debug, 2,9 s em release). Relatório
  imprime o inerte e uma ressalva. Resultado (`padrao_v1`, log10 FB observado / média do inerte /
  faixa 95 % / réplicas ≥ observado): Lotofácil −7,31 / −7,48 / [−11,67; −3,93] / 91 de 200;
  Mega-Sena −13,59 / −17,58 / [−22,42; −12,75] / 12 de 200; Quina −25,00 / −26,18 / [−32,39; −19,71]
  / 72 de 200 — nas três o índice é indistinguível de um inerte (ln FB médio do inerte ≈ −17,2 /
  −40,5 / −60,3, batendo com a estimativa do revisor). Pendências BAIXA resolvidas: plano corrigido
  (teto na janela de 500: ~0,03 / ~0,02 / ~0,23); coluna "Δ log loss/par" na tabela por dobra;
  comandos a rodar um por linha (sem `&&`), com `--perfis` quando a pasta não é a padrão
  (`IndiceIndisponivel.comandos: Vec<String>`); `primeiro_indice`/`ultimo_concurso` impressos ("Índice
  lido"); README ("o trancado não entra no cálculo"); função da orquestração renomeada para
  `evidencia_do_indice_no_banco`. 1d.4 passa a incluir a ressalva do inerte no GUIA. Gate: fmt,
  clippy, cargo test verde (lib 506, evidencia 19, ...), pytest 175.
