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
  A janela conta pelo **número do concurso**; por isso o motor exige
  numeração contínua e devolve erro se o histórico tiver buraco (e
  `sync features` recusa rodar enquanto o `validar` apontar buraco)
  (decisão de 2026-10-04).
- A janela "total" é gravada como `janela = 0` (constante no código).
- Além dos concursos já sorteados, o motor calcula o snapshot de
  *t* = último concurso + 1 (estado atual, antes do próximo sorteio), com
  `saiu` = `NULL`. É ele que alimenta a tabela geral do app. Esse snapshot
  é **gravado** em `features_dezena` e substituído (com `saiu` preenchido)
  quando o resultado chega; quem monta datasets filtra `saiu IS NOT NULL`
  (decisão de 2026-10-04).
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
| `repeticao_atual` | sequência corrente de concursos seguidos em que saiu (0 se não saiu em *t−1*); limitada pela janela (no máximo *J*) |
| `saiu` | alvo: 1 se saiu no concurso *t*, senão 0; `NULL` no snapshot do próximo concurso, ainda não sorteado |

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
- Dezena que nunca saiu antes de *t* → `espaco_final` = `NULL` (só nos
  primeiros concursos de cada loteria).
- Sequência maior que a janela → `repeticao_atual` = *J* (ex.: saiu nos
  últimos 12 concursos, janela de 10 → 10).
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

Regras do cálculo (decisões de 2026-10-04):

- **z-score** com desvio padrão **populacional** (÷N): as dezenas do
  concurso são o conjunto inteiro, não uma amostra.
- **`NULL`**: média, desvio, mínimo e máximo usam só as dezenas com valor;
  depois o `NULL` recebe o neutro — **0** no z-score (a média) e **0,5** no
  minmax (o meio da escala).
- **Componente sem variação** no concurso (desvio 0, máximo = mínimo, ou
  `NULL` em todas as dezenas): todas recebem o neutro, sem divisão por zero;
  o componente não pesa naquele concurso.
- **Soma em ordem fixa**: a dos 9 componentes do perfil (`Pesos::componentes`).
- **Empate**: o total é arredondado (1e-9) antes de ordenar, para diferenças
  de arredondamento do `f64` não trocarem posições; depois, menor dezena à
  frente.
- **Variação de posição** = posição anterior − posição atual (positivo =
  subiu). No primeiro concurso do índice, sem anterior, é `NULL`.
- **Concursos cobertos**: todos os de `features_dezena`, inclusive o
  snapshot do último + 1 (o da tabela geral do app), com a variação medida
  contra o último concurso.

### Perfil de pesos

```toml
# perfis/lotofacil/padrao_v1.toml
nome = "padrao_v1"
versao = 1
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

Regras dos perfis (decisões de 2026-10-04):

- **Local:** `perfis/<loteria>/<nome>.toml`. A `loteria` do arquivo tem de
  bater com a pasta, e o `nome` com o nome do arquivo.
- **Versão:** campo `versao` (inteiro) obrigatório, conferido com o sufixo do
  nome (`padrao_v1` → 1). Mudou o conteúdo → novo arquivo com a versão
  seguinte.
- **Validação rígida:** os 9 pesos são obrigatórios (peso 0 escrito por
  extenso); chave desconhecida é erro; `janela` só em {0, 10, 50, 100}
  (0 = total); `normalizacao` só `zscore` ou `minmax`.
- **Hash:** SHA-256 do `.toml` depois de tirar o BOM UTF-8 (se houver) e
  normalizar o fim de linha para LF
  (`perfis/**/*.toml text eol=lf` no `.gitattributes`), para o hash ser o
  mesmo em qualquer máquina.
- **Imutabilidade:** perfil já registrado com o mesmo `nome` e conteúdo
  diferente é erro, pedindo para subir a versão. Um índice gravado nunca
  fica com um perfil diferente do que o gerou.
- **Pesos iniciais:** Mega-Sena e Quina começam com os mesmos pesos do
  exemplo, até a busca de pesos da Fase 1.5.
- **Dependências:** crates `toml` (leitura) e `sha2` (hash; reaproveitado
  nos hashes da Fase 2).
- **Quem grava:** a 1b.1 entrega leitura, validação e as funções de banco
  (testadas com banco em memória); o registro na tabela `perfis` é feito
  pelo `sync indice`, na 1b.3.

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
    concurso, dezena,
    perfil,             -- = perfis.nome (sem FK)
    total,              -- REAL NOT NULL
    posicao,            -- INTEGER NOT NULL
    variacao_posicao,   -- INTEGER; NULL só no primeiro concurso
    PRIMARY KEY (concurso, dezena, perfil)
) WITHOUT ROWID

perfis(
    nome, versao, loteria, janela, normalizacao,
    hash_toml,       -- SHA-256 do .toml com fim de linha em LF
    toml,            -- texto do .toml, para o banco bastar sozinho
    criado_em,       -- ISO-8601 UTC com segundos (2026-10-04T18:30:00Z),
                     -- gravado só no primeiro registro
    PRIMARY KEY (nome)
)

motor_estado(        -- uma linha só (id = 1): estado para retomar do último concurso
    id, loteria, feature_version, ultimo_concurso,
    impressao,       -- hash dos resultados já incorporados (concurso, data e dezenas)
    estado           -- EstadoMotor em JSON
)
```

`sync features <loteria>` (só uma loteria; não aceita `todas`) retoma de
`motor_estado` e calcula só os concursos novos. Regenera tudo do zero se não
houver estado, se a `feature_version` mudou, se o estado não confere com a
loteria ou com `features_dezena`, ou se um resultado já incorporado mudou —
inclusive só a data, porque a impressão inclui a data. Resultado apagado
segue a mesma lógica: se for o último concurso, regenera tudo sem ele; se
estiver abaixo do último, vira buraco e `sync features` recusa rodar sem
alterar nada. Tudo é gravado numa única transação (decisões de 2026-10-04).

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
| 0.5 | `todas` aceito no lugar do slug em `baixar`, `atualizar`, `validar` e `buracos` (e, na Fase 2, `exportar`): roda o comando em cada banco, um por vez, sem juntar bancos. `importar`, `inserir` e `listar` aceitam só uma loteria (decisão de 2026-10-04) |
| 0.6 | Corrigir textos da CLI que ainda falam em ".htm" |
| 0.7 | Fechar buracos à mão: `sync buracos` lista os concursos que faltam (com as datas dos vizinhos) e `sync inserir` grava um concurso por vez, com a validação normal e data fora de ordem tratada como erro. Cada inserção fica em `cache/manuais_<loteria>.json` (versionado no git, ordenado por data de sorteio), que `importar`/`baixar` reaplicam e conciliam com a fonte oficial: igual → sai do cache; diferente → aviso |
| 0.8 | `atualizar` com salto (decisão de 2026-10-05): se o concurso lido na página for maior que o último do banco + 1 (concursos perdidos), não grava só o último (o que deixaria buraco). Copia o banco para `backups/resultados_<loteria>_<AAAAMMDD-HHMMSS>.db`, mostra o caminho e segue o caminho do `baixar` (planilha completa + cache de inserções manuais + validação). Se a planilha falhar, termina com erro sem gravar nada (o backup fica). Se ainda sobrar buraco (a planilha não tem o concurso), avisa e indica `buracos`/`inserir`. Sem pergunta interativa: o comando vai rodar agendado na Fase 3. Salto de 1 continua como hoje. Banco inexistente ou vazio conta como último = 0 (segue o caminho do `baixar`, sem backup); planilha mais atrasada que a página: o concurso da página é gravado em seguida, como num salto de 1 |

**Pronto quando:** `sync validar todas` passa nos três bancos e `cargo test` está verde.

### Fases 1 e 1.5 — concluídas

Decisões detalhadas, com a especificação de cada comando, em
[`docs/decisoes/fases-1-e-1.5.md`](decisoes/fases-1-e-1.5.md). Leia a parte
da fase que a tarefa toca. Regras que as fases seguintes usam:

- **1a — Features** (`src/motor/`): estado incremental por dezena e janela,
  `sync features`; testes de vazamento e incremental = completo.
- **1b — Índice**: perfis `.toml`, normalização, posição e variação;
  `sync indice <loteria> --perfil X` (recusa features desatualizadas, de
  outros resultados ou de outra `feature_version`) e `sync tabela` (CSV por
  concurso para o Excel, pasta `tabelas/` fora do git).
- **1c — Avaliação**: `sync avaliar` e `sync aleatoriedade`.
  - **Probabilidade** pela posição no índice, só com o passado:
    `p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)`,
    m = 100, histórico inteiro; acaso = K/N (15/25, 6/60, 5/80).
  - **Métricas**: log loss (ln, *p* em [1e-15, 1 − 1e-15]) e Brier, por par
    (concurso, dezena); incerteza por concurso; top-k contra a
    hipergeométrica com p-valor exato.
  - **Walk-forward**: 5 dobras consecutivas do segundo concurso do índice
    até o corte da busca. **Veredito**: bootstrap circular, blocos de 50,
    B = 10 000, semente fixa, α = 5%.
  - **Trancado**: corte fixo `ultimo_concurso_busca` no `LoteriaConfig` —
    Lotofácil 3416, Mega-Sena 2759, Quina 6421; uso único, só com
    `--trancado`, registrado em `registros/trancado_<loteria>.json`. Nada que
    escolhe perfil lê acima do corte.
  - **Aleatoriedade**: qui-quadrado corrigido para sorteio sem reposição e
    runs por dezena com p-valor exato (dobro da menor cauda), Holm por
    família, α = 5%, histórico inteiro.
- **1.5 — Ponte com o Python**: `sync dataset` (Parquet/CSV + manifesto,
  trancado fora por padrão; contrato em `docs/contrato-v1.md`); pasta `ml/`
  (venv, só pyarrow/numpy/pytest, leitura sempre pelo manifesto); busca de
  pesos com walk-forward gerando `perfis/<loteria>/busca_v1.toml` (nenhum
  supera o acaso; o perfil ativo não mudou).
- **1.5.4, concluída — Vetores de conformidade**
  (`testes/conformidade/*.json`): entrada → features/índice esperados,
  verificados por `cargo test` **e** `pytest`. Regras (decisões de
  2026-10-09):
  - **alcance**: o Python ganha uma implementação de referência das
    features (só para conformidade, fora da produção) e a
    `variacao_posicao` no índice; o pytest confere resultados → features e
    features → índice (total, posição, variação);
  - **formato**: um JSON por caso — `versao_formato`, `descricao`, loteria
    (K, N, faixa), `feature_version`, entrada (resultados ou features),
    perfil (janela, normalização, pesos) e esperado (features e índice);
    `NULL` como `null`, floats com 17 dígitos significativos;
  - **casos à mão, esperado gerado pelo Rust**: golden do plano, bordas de
    `NULL`, janelas 10/50/100 e fronteiras, componente constante, todo
    `NULL`, empate exato e quase empate a 1e-9, zscore e minmax, variação no
    primeiro concurso. Ferramenta `examples/gerar_conformidade.rs`; casos
    pequenos conferidos à mão; o `cargo test` falha se o JSON versionado
    divergir do que o Rust calcula (nada é regerado em silêncio);
  - **comparação**: inteiros e posição exatos; `total` igual bit a bit;
  - **local**: `testes/conformidade/` na raiz, com
    `testes/conformidade/** -text` no `.gitattributes`;
  - **tamanho**: só dados sintéticos, até ~150 concursos por caso, dezenas
    de KB no total;
  - **fora do escopo**: calibração, log loss e top-k (já conferidos pela
    fixture da 1.5.2). **Entra de carona** o achado baixo da 1.5.3: fixar
    `set_statistics_enabled` em `src/dataset.rs`, com teste.

### Fase 1d — Índice de evidência bayesiano

Motivo (decisão de 2026-10-08): a 1.5.3 mostrou que nenhum perfil de pesos
supera o acaso, mas a 1c.4 rejeitou a uniformidade nas três loterias. Em vez
de "prever", o app passa a mostrar **quanta evidência os dados dão** — de
viés em cada dezena e de que o índice de força prevê — em cinco graus.

- **Fator de Bayes (FB)**: quantas vezes os dados são mais prováveis sob uma
  hipótese do que sob a outra. Sempre guardado como `log10_fb`.
- **Escala (Jeffreys, adaptada aos cinco graus)**, aplicada a
  `F = max(FB, 1/FB)`: **muito fraca** F < 3; **fraca** 3 ≤ F < 10;
  **moderada** 10 ≤ F < 30; **forte** 30 ≤ F < 100; **muito forte** F ≥ 100.
  Limites fechados à esquerda, comparados em `log10` (0,477…, 1, 1,477…, 2).
- **Direção**: "a favor de viés" (FB > 1), "a favor do acaso" (FB < 1),
  sempre junto do grau; FB = 1 exato fica "a favor do acaso". Nada é
  escondido: a evidência contra o modelo aparece com o mesmo destaque.
- **A) Evidência por dezena** — H0: a chance de a dezena sair é exatamente
  K/N; H1: chance desconhecida com priori Beta centrada em K/N.
  - Cada dezena, sozinha, é Bernoulli(K/N) por concurso, independente entre
    concursos, mesmo com sorteio sem reposição → x saídas em n concursos
    seguem binomial exata; FB de fórmula fechada (beta-binomial ÷ binomial),
    calculada com `ln_gamma`, sem simulação.
  - **Priori**: desvio padrão da chance sob H1 = **5 % de K/N** (viés físico
    plausível é pequeno), o que fixa a concentração da Beta por loteria. O
    relatório traz a sensibilidade com 2,5 % e 10 %, porque o FB depende da
    largura da priori.
  - **Janelas**: histórico inteiro (principal) e os **últimos 500
    concursos** (secundária, cobre viés que muda com o equipamento). FB exato
    nas duas; sem pesos de esquecimento.
  - **Ponto no tempo**: a evidência do concurso t usa só concursos até t−1;
    o snapshot vale para o próximo concurso, como as features.
  - **Muitas dezenas**: com 25, 60 ou 80 dezenas, algumas saem "moderada" por
    acaso. O relatório mostra, ao lado da contagem de dezenas em cada grau, a
    contagem **esperada sob o acaso**, por simulação com semente fixa
    (20261008), e o app leva o mesmo aviso.
- **B) Evidência do índice de força** — FB prequencial: soma, concurso a
  concurso, de log(probabilidade do modelo) − log(probabilidade do acaso),
  com as probabilidades calibradas pela posição (m = 100) já usadas na 1c,
  **só nas dobras de teste** do walk-forward (2 a 5). É a mesma diferença de
  log loss da 1c, traduzida em graus. Ressalva registrada: as dezenas do
  concurso são tratadas como Bernoullis independentes (pseudo-verossimilhança),
  igual para o modelo e para o acaso. Perfil ativo por padrão; `--perfil`
  aceito. Não toca o trancado.
- **Sem tabela nova**: tudo de fórmula fechada e barato; calculado sob
  demanda a partir de `resultados` (A) e da avaliação já existente (B).
- **Onde aparece**: comando `sync evidencia <loteria>` (relatório em texto,
  uma loteria por vez, sem `todas`), colunas novas no CSV do `sync tabela`
  (grau e direção de A, janela principal) e, na Fase 2, no contrato do app.

Tarefas:
- **1d.1** — Conferir os desvios de frequência da 1c.4 contra uma fonte
  independente e levantar mudanças de regra/equipamento nos trechos
  apontados (sai do Backlog): uniformidade rejeitada na Lotofácil,
  p ≈ 1,5e-4, concentrada nos concursos 1899–3796; Mega-Sena, p ≈ 6,4e-3;
  Quina, p ≈ 0,042; runs da dezena 25 da Quina. Se o desvio for erro de dado, corrigir a base
  antes da 1d.2. **Pronto quando:** cada trecho apontado tem conclusão
  escrita em `PROGRESSO.md`.
- **1d.2** — Evidência por dezena (A): módulo `src/avaliacao/evidencia.rs`
  (função pura, sem banco), comando `sync evidencia`, testes (FB contra
  valores calculados à mão, simetria, graus nos limites, ponto no tempo,
  esperado sob o acaso). **Pronto quando:** roda nas três loterias com a
  sensibilidade da priori no relatório.
- **1d.3** — Evidência do índice (B) no mesmo comando, conferida contra a
  diferença de log loss do `sync avaliar` (mesmo número, outra escala).
- **1d.4** — Grau e direção no CSV do `sync tabela`; `docs/GUIA.md`
  atualizado com a leitura dos graus.

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
| 3.1 | `sync ciclo`: atualizar → backfill automático se houver buraco (buscando os concursos que faltam e gravando pelo mesmo caminho do `inserir`, no cache de 0.7) → validar → features (incremental) → índice → exportar → publicar (só se mudou). Antes, mover para a biblioteca (`orquestracao`) a lógica de gravação que ainda está na CLI (`processar_e_salvar`, `validar_lote`, `aplicar_atualizacao`), para o ciclo reaproveitá-la |
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

0 → 1a → 1b → 1c → 1.5 → 1d → 2 → 3. O contrato da Fase 2 é o ponto de encontro
com o planejamento do front-end.

## Decisões em aberto

- Destino da publicação (Cloudflare R2/Pages, GitHub Pages, S3, …) — Fase 2.
- Canal de alerta (Telegram, Discord, e-mail) — Fase 3.

## Backlog

- Premiação (ganhadores, rateio, arrecadação, cidades/UF). Base para uma
  fase futura de **popularidade e rateio** (decisão de 2026-10-08): estimar
  quais dezenas e combinações o público mais joga, a partir dos ganhadores
  por faixa, para indicar jogos que dividem menos o prêmio — não muda a
  chance de acertar, muda quanto se recebe. Vem depois da 1d.
- Outras loterias (Dupla Sena, Timemania, Dia de Sorte, +Milionária, Lotomania).
- API própria (axum), se o front-end precisar de algo dinâmico.
- Modelos de ML rodando em Rust (`burn`, `candle` ou ONNX).
- Robustez da raspagem (`downloader.rs`): avaliar XPath em vez de seletores
  CSS e técnicas anti-detecção, caso o site volte a bloquear o acesso
  automatizado.
