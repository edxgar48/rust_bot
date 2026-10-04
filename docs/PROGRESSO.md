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

## Fase 1a — Features · `engenheiro-motor`

- [x] 1a.1 Estado incremental por dezena e janela (`src/motor/`)
- [x] 1a.2 Tabela `features_dezena` + `motor_estado` + comando `sync features`
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
- [x] Buraco já existente no banco deve bloquear `importar`/`baixar`? → não: só aviso (ver nota 0.7)
- [ ] Regra "data posterior a hoje" ao gravar (ver nota 0.2)
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
