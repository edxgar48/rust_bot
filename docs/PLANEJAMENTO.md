# Planejamento — back-end de estatísticas de loterias (Fases 0 a 3)

Este documento cobre o back-end: coleta, validação, motor estatístico,
experimentos de ML e exportação/automação. O front-end (app mobile) terá um
planejamento próprio; o ponto de contato entre os dois é o contrato de dados
da Fase 2 (`docs/contrato-v1.md`).

## Decisões de arquitetura

- **Um banco SQLite por loteria** (`resultados_<loteria>.db`), como já é hoje.
  Nada de banco único.
- **Evolução incremental**: a tabela `resultados` existente não muda. Tudo o
  que é novo entra como **tabelas derivadas**, que podem ser apagadas e
  regeneradas a partir de `resultados` a qualquer momento.
- **Foco nas dezenas.** Premiação (ganhadores, rateio, arrecadação, cidades)
  fica no backlog — os dados já existem nas planilhas da Caixa quando forem
  necessários.
- **O motor estatístico é o núcleo do sistema.** Ele calcula as estatísticas
  como **série temporal**: o estado de cada dezena imediatamente *antes* de
  cada concurso. O último snapshot alimenta o app; todos os snapshots
  alimentam os experimentos de ML. Uma única fonte da verdade.
- **"Banco por dezena" = consulta, não arquivo.** A tabela de features tem uma
  linha por (concurso, dezena, janela); a série de uma dezena é um
  `WHERE dezena = N`. Isso preserva o contexto entre dezenas, que os modelos
  precisam.
- **Python para experimentos, Rust para produção.** O Python nunca é dono de
  lógica de que a produção dependa: ele lê os datasets gerados pelo Rust e
  propõe perfis de pesos. Vetores de conformidade compartilhados garantem que
  uma migração futura para 100% Rust seja possível.

### Ressalva sobre ML

Sorteios são independentes; por construção, nenhum modelo deve prever
dezenas melhor que o acaso. Os experimentos são válidos como estudo, mas o
sistema precisa de uma **régua de avaliação embutida**: um modelo que pareça
"acertar" é, até prova em contrário, sinal de vazamento de informação do
futuro ou de sobreajuste.

## Especificação das features

### Convenções

- O snapshot do concurso *t* usa **apenas** os concursos até *t−1*.
- Janelas: **10, 50, 100 e total**. Para uma janela de tamanho *J*,
  consideram-se as aparições da dezena nos concursos *t−J* … *t−1*.
- **Espaço** entre duas aparições consecutivas *a* e *b* = `b − a − 1`
  (0 quando saiu em concursos seguidos).

### Componentes

| Coluna | Definição |
|---|---|
| `frequencia` | número de aparições na janela |
| `maior_espaco` | maior espaço na janela |
| `maior_espaco_ocorrencias` | quantas vezes o maior espaço ocorreu |
| `menor_espaco` | menor espaço na janela |
| `menor_espaco_ocorrencias` | quantas vezes o menor espaço ocorreu |
| `soma_variacoes` | Σ \|espaçoᵢ − espaçoᵢ₋₁\| entre espaços seguidos |
| `espaco_final` | (t−1) − última aparição; não é limitado pela janela |
| `repeticoes_total` | vezes em que saiu em dois concursos seguidos, dentro da janela |
| `repeticao_atual` | sequência corrente de concursos seguidos em que saiu (0 se não saiu em *t−1*) |
| `saiu` | alvo: 1 se saiu no concurso *t*, senão 0 |

### Exemplo de referência (teste golden)

Dezena sai nos concursos 3, 4, 8, 9, 10, 15. Snapshot em *t* = 20, janela total:

- espaços = [0, 3, 0, 0, 4]
- `maior_espaco` = 4, `maior_espaco_ocorrencias` = 1
- `menor_espaco` = 0, `menor_espaco_ocorrencias` = 3
- `soma_variacoes` = 3 + 3 + 0 + 4 = 10
- `espaco_final` = 19 − 15 = 4
- `repeticoes_total` = 3, `repeticao_atual` = 0
- `frequencia` = 6

### Casos de borda

- Menos de 2 aparições na janela → sem espaços: colunas de espaço = `NULL`.
- Menos de 3 aparições → sem variações: `soma_variacoes` = `NULL`.
- Na normalização do índice, `NULL` vira valor neutro (0 em z-score = média).
- Isso será frequente na janela de 10 da Mega-Sena e da Quina.
- Colinearidade: quando `menor_espaco = 0`, `menor_espaco_ocorrencias` é igual
  a `repeticoes_total`. Irrelevante para o índice ponderado, mas relevante
  para modelos lineares no Python.

## Índice de força

- **Total da classificação** = Σ (pesoₖ × componenteₖ normalizado).
- **Normalização** por concurso, entre as dezenas daquele concurso
  (`zscore` ou `minmax`, definido no perfil), para que os pesos sejam
  comparáveis entre componentes de escalas diferentes.
- **O sinal fica no peso**: o próprio experimento descobre se um componente
  aumenta ou diminui a força.
- **Posição na tabela geral**: ranking 1…N das dezenas pelo total, no
  concurso. Desempate: menor dezena à frente.
- **Variação de posição**: posições ganhas/perdidas em relação ao concurso
  anterior.

### Perfil de pesos

```toml
# perfis/padrao_v1.toml
nome = "padrao_v1"
loteria = "lotofacil"
janela = 50
normalizacao = "zscore"        # ou "minmax"

[pesos]
frequencia                = 1.0
maior_espaco              = 1.0
maior_espaco_ocorrencias  = 0.3
menor_espaco              = 0.5
menor_espaco_ocorrencias  = 0.3
soma_variacoes            = -0.8
espaco_final              = 1.2
repeticoes_total          = 0.7
repeticao_atual           = 0.4
```

Perfis são versionados no repositório; o banco registra o hash do `.toml`
usado para gerar cada índice.

## Esquema das tabelas derivadas (em cada `resultados_<loteria>.db`)

```sql
features_dezena(
    concurso, dezena, janela,
    frequencia, maior_espaco, maior_espaco_ocorrencias,
    menor_espaco, menor_espaco_ocorrencias, soma_variacoes,
    espaco_final, repeticoes_total, repeticao_atual,
    saiu, feature_version,
    PRIMARY KEY (concurso, dezena, janela)
)

indice(
    concurso, dezena, perfil,
    total, posicao, variacao_posicao,
    PRIMARY KEY (concurso, dezena, perfil)
)

perfis(nome, versao, hash_toml, criado_em)

motor_estado(...)   -- estado incremental para retomar do último concurso
```

Formato longo (janela como coluna): adicionar uma janela não muda o schema.
Volume estimado: Quina ≈ 2,3 milhões de linhas em `features_dezena`.

## Fases

### Fase 0 — Consolidar a base

| # | Tarefa |
|---|---|
| 0.1 | `LoteriaConfig` ganha faixa de dezenas (`dezena_min`/`dezena_max`) e layout do volante (grade linhas × colunas: Lotofácil 5×5, Mega-Sena 6×10, Quina 8×10; dezenas em ordem crescente, preenchendo linha por linha) |
| 0.2 | Comando `validar <loteria>`: quantidade de dezenas, faixa, repetidas no concurso, buracos na numeração, datas fora de ordem (esta última é **aviso**, não erro). Erros conhecidos da planilha da Caixa ficam numa lista de correções por loteria + concurso (`CORRECOES_DATA`), aplicada antes de gravar |
| 0.3 | Mesma validação aplicada ao gravar (`importar`, `baixar`, `atualizar`). Buraco na numeração é só aviso na gravação (decisão de 2026-10-03); no `validar`, continua sendo erro |
| 0.4 | Testes do parser com fixtures pequenas de cada loteria (`tests/fixtures/`) |
| 0.5 | `todas` aceito no lugar do slug |
| 0.6 | Corrigir textos da CLI que ainda falam em ".htm" |
| 0.7 | Fechar buracos à mão: `sync buracos` lista os concursos que faltam (com as datas dos vizinhos) e `sync inserir` grava um concurso por vez, com a validação normal e data fora de ordem tratada como erro. Cada inserção fica em `cache/manuais_<loteria>.json` (versionado no git, ordenado por data de sorteio), que `importar`/`baixar` reaplicam e conciliam com a fonte oficial: igual → sai do cache; diferente → aviso |

**Pronto quando:** `sync validar todas` passa nos três bancos e `cargo test` está verde.

### Fase 1 — Motor estatístico

**1a — Features** (`src/motor/`)

- Estado incremental por dezena e janela; comando `sync features <loteria>`.
- Testes obrigatórios:
  - exemplo golden acima;
  - **vazamento**: alterar o concurso *t* não altera nenhuma feature de *t*;
  - **incremental = completo**: calcular do zero e adicionar concurso a
    concurso produzem o mesmo resultado.

**1b — Índice de força**

- Leitura de perfis `.toml`, normalização, índice, posição e variação de posição.
- Comando `sync indice <loteria> --perfil X`.

**1c — Avaliação**

- Comando `sync avaliar <loteria> --perfil X`.
- Baseline do acaso: probabilidade de cada dezena sair = 15/25 (Lotofácil),
  6/60 (Mega-Sena), 5/80 (Quina). Métricas: log loss e Brier.
- Ranking: acertos das top-k dezenas vs. esperado pela hipergeométrica
  (ex.: Lotofácil, 15 dezenas ao acaso → 9 acertos em média).
- Validação **walk-forward** (treina até *t*, testa em *t+1…*; nunca embaralha o tempo).
- **Conjunto final trancado** (sugestão: últimos ~10% dos concursos), usado
  uma única vez para validar o perfil campeão — protege contra sobreajuste na
  busca de pesos.
- Testes de aleatoriedade do histórico (qui-quadrado de frequência, runs test).

### Fase 1.5 — Ponte com o Python

- Comando `sync dataset <loteria> --formato parquet|csv`.
- Pasta `ml/`: leitura do dataset, baseline, busca de pesos com walk-forward,
  escrita de novos perfis `.toml`.
- **Vetores de conformidade** (`testes/conformidade/*.json`): entrada →
  features/índice esperados, verificados por `cargo test` **e** `pytest`.

### Fase 2 — Exportação para o app

- Comando `sync exportar [loteria|todas] --saida publico/`:

  ```
  publico/v1/
  ├── index.json              # loterias, último concurso, data de geração, hash de cada arquivo
  └── {loteria}/
      ├── ultimo.json         # último concurso
      ├── tabela_geral.json   # último snapshot: componentes, índice e posição por dezena (perfil ativo)
      └── historico.json      # compacto: [[concurso, "AAAA-MM-DD", [dezenas]], ...]
  ```

- `schema_version` em cada arquivo, hash SHA-256 por arquivo, geração
  determinística (mesmos dados → mesmos bytes).
- `docs/contrato-v1.md`: significado de cada campo + exemplo real. É a
  entrega para o planejamento do front-end.
- Comando `sync publicar` atrás de uma interface simples (destino trocável).

### Fase 3 — Automação e operação

| # | Tarefa |
|---|---|
| 3.1 | `sync ciclo`: atualizar → backfill automático se houver buraco (buscando os concursos que faltam e gravando pelo mesmo caminho do `inserir`, no cache de 0.7) → validar → features (incremental) → índice → exportar → publicar (só se mudou) |
| 3.2 | Calendário de sorteios por loteria no `config.rs` |
| 3.3 | Retentativas com backoff (sorteio ~20h, tentativas até ~23h, nova tentativa no dia seguinte) |
| 3.4 | Arquivo de trava contra execução simultânea |
| 3.5 | Log em arquivo (`logs/sync_AAAA-MM-DD.log`) |
| 3.6 | Alerta após N falhas seguidas ou mudança aparente no site |
| 3.7 | Agendamento via Agendador de Tarefas do Windows (script `.ps1` versionado) |
| 3.8 | Runbook no README: bloqueio, seletor mudou, reprocessar do zero |

**Pronto quando:** uma semana rodando agendado sem intervenção, cada concurso
novo publicado no mesmo dia, e uma falha simulada dispara o alerta.

## Ordem e dependências

0 → 1a → 1b → 1c → 1.5 → 2 → 3. O contrato da Fase 2 é o ponto de encontro
com o planejamento do front-end.

## Decisões em aberto

- Destino da publicação (Cloudflare R2/Pages, GitHub Pages, S3, …) — Fase 2.
- Canal de alerta (Telegram, Discord, e-mail) — Fase 3.
- Tamanho do conjunto trancado (sugestão: ~10% do histórico) — Fase 1c.

## Backlog

- Premiação (ganhadores, rateio, arrecadação, cidades/UF).
- Outras loterias (Dupla Sena, Timemania, Dia de Sorte, +Milionária, Lotomania).
- API própria (axum), se o front-end precisar de algo dinâmico.
- Modelos de ML rodando em Rust (`burn`, `candle` ou ONNX).
- Robustez da raspagem (`downloader.rs`): avaliar XPath em vez de seletores
  CSS e técnicas anti-detecção, caso o site volte a bloquear o acesso
  automatizado.
