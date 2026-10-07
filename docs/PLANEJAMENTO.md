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
- Comando `sync tabela <loteria|todas> --perfil X [--concurso N] [--saida tabelas]`
  (1b.4, pedido do usuário em 2026-10-05, para comparar a tabela geral ao
  longo da semana; adianta uma parte da Fase 2 sem substituí-la):
  - grava um CSV por concurso em
    `<saida>/<loteria>/tabela_<loteria>_<perfil>_<concurso>.csv`, com uma
    linha por dezena em ordem de posição: loteria, perfil, concurso, data do
    sorteio (vazia no próximo, ainda não sorteado), dezena, posição, total,
    variação de posição, saiu (vazio no próximo), janela do perfil e as
    9 features dessa janela;
  - formato para o Excel em português: separador `;`, vírgula decimal,
    `NULL` como célula vazia; mesmo concurso → mesmo conteúdo (sobrescreve);
  - padrão: o snapshot do próximo concurso; `--concurso N` exporta um
    concurso já sorteado (1 … último + 1), com `saiu` preenchido;
  - mesmas conferências do `sync avaliar` (resultados válidos, perfil
    registrado com o mesmo hash, features em dia, índice até o último + 1 e
    igual ao recalculado); só lê o banco; `--db` e `--concurso` não valem
    com `todas`; a pasta `tabelas/` fica fora do git.
- Comando `sync indice <loteria> --perfil X` (decisões de 2026-10-05):
  - `--perfil` obrigatório; um perfil e uma loteria por execução, sem `todas`;
  - recusa, sem gravar nada, se as features estiverem desatualizadas (o
    snapshot mais recente não é o último concurso de `resultados` + 1), se
    foram calculadas com outros resultados (a impressão de `resultados` não
    bate com a de `motor_estado`, como num resultado antigo corrigido sem
    rodar `sync features`) ou se forem de outra `feature_version`: pede para
    rodar `sync features` antes;
  - a `feature_version` é conferida só nas linhas da janela do perfil, a
    única que o índice usa;
  - recusa banco ou `features_dezena` vazios, sem criar tabelas;
  - usa só as linhas de `features_dezena` da janela do perfil e confere que
    as dezenas de cada concurso são exatamente as da `LoteriaConfig`;
  - regenera o índice inteiro do perfil a cada execução (apaga as linhas do
    perfil e insere de novo), numa única transação junto com o registro em
    `perfis`.

**1c — Avaliação**

- Comando `sync avaliar <loteria> --perfil X` (decisões de 2026-10-05, 1c.5):
  - **veredito "supera o acaso"**: bootstrap circular por blocos sobre as
    diferenças perfil − acaso do log loss por concurso, **blocos fixos de 50
    concursos** (a calibração acumulada tem memória longa), **B = 10 000**
    reamostragens, gerador próprio (SplitMix64/xoshiro, sem crate nova) com
    **semente fixa** no código; p-valor unilateral e IC 95% percentil;
    α = 5%. O Brier usa o mesmo bootstrap só como informação; o top-k usa o
    p-valor exato da 1c.2;
  - **trecho**: na busca, o veredito sai do trecho inteiro (união das 5
    dobras), e as dobras mostram a estabilidade; no trancado, do próprio
    trancado;
  - **trancado** só com `--trancado` (padrão: walk-forward). O registro do
    "uma única vez" fica em **arquivo versionado no git**,
    `registros/trancado_<loteria>.json` (perfil, hash do `.toml`, até qual
    concurso, quando) — não em tabela do banco, que pode ser refeito. Mesmo
    perfil e hash já avaliados → recusa, a menos de `--repetir` (que mostra
    as avaliações anteriores). O p-valor do trancado aparece também com
    Bonferroni pelo número de perfis já registrados na loteria;
  - **vários perfis ou k na busca**: `--k` opcional (padrão K); aviso fixo na
    saída sobre quando o p-valor vale; `--testados N` mostra também o
    p-valor com Bonferroni;
  - **saída** só em texto: cabeçalho (loteria, perfil, hash, trecho, corte),
    tabela por dobra (log loss, Brier e top-k do perfil contra o acaso,
    diferença, EP, IC normal, p exato) e linha final com o veredito. Não grava
    nada no banco;
  - **pré-condições** como no `sync indice`: uma loteria e um perfil, sem
    `todas`; recusa perfil fora de `perfis` ou com hash diferente do `.toml`
    em disco, índice que não cobre até o último concurso + 1 e features
    desatualizadas; concurso faltando é erro. Observações de `indice` +
    `features_dezena` desde o 1º concurso do índice; SQL em `db.rs`, fluxo em
    `orquestracao`, a CLI só imprime;
  - a avaliação de produção passa só pela orquestração (`sync avaliar`), que
    usa `walk_forward` na busca e o veredito do trancado depois do registro de
    uso único; `avaliar`, `avaliar_topk`, variantes `_entre`,
    `p_valor_runs_exato` e as funções do trancado ficam `pub(crate)` (fora do
    crate não há como avaliar o trancado sem o registro); nunca usar
    `u32::MAX` como corte;
  - detalhes confirmados pelo usuário (2026-10-05): no trancado o k é sempre
    K (`--k` e `--testados` são recusados com `--trancado`); o bootstrap
    exige pelo menos 100 concursos (2 blocos) e, abaixo disso, a saída vem
    sem veredito e sem erro; o Bonferroni do trancado conta os pares
    (perfil, hash) distintos registrados, incluindo o atual, vale também
    para o top-k e não cresce com `--repetir`; `--repetir` é aceito mesmo
    sem uso anterior; a pasta `registros/` fica ao lado do banco usado (com
    os bancos na raiz, `registros/` na raiz, versionada no git).
- Comando `sync aleatoriedade <loteria|todas>` (decisão de 2026-10-05): roda os
  testes da 1c.4 (qui-quadrado e runs com Holm) e imprime o relatório, sem
  depender de perfil; entra junto com a 1c.5.
- Baseline do acaso: probabilidade de cada dezena sair = 15/25 (Lotofácil),
  6/60 (Mega-Sena), 5/80 (Quina). Métricas: log loss e Brier.
- **Índice → probabilidade** (decisões de 2026-10-05): calibração pela
  posição, só com o passado. A probabilidade da dezena na posição *r* no
  concurso *t* é a taxa com que a posição *r* saiu nos concursos anteriores a
  *t*, puxada para o acaso (*K/N*):
  `p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)`,
  com **m = 100** concursos fictícios e o histórico inteiro (sem janela). As
  *N* probabilidades somam *K*; o primeiro concurso recebe a taxa do acaso.
- **Concursos avaliados**: só os já sorteados, do segundo concurso do índice
  até o corte da busca; o conjunto trancado fica de fora por padrão.
- **Métricas**: log loss com logaritmo natural, *p* limitado a
  [1e-15, 1 − 1e-15], média por par (concurso, dezena); Brier = média de
  (*p* − saiu)².
- **Incerteza**: erro padrão e intervalo de 95% da diferença perfil − acaso,
  calculada **por concurso** (as dezenas de um mesmo concurso não são
  independentes). Erro padrão = desvio amostral (÷(T−1)) / √T; intervalo
  normal (±1,96·EP); com menos de 2 concursos, ambos vazios. O Brier usa a
  probabilidade sem o limite de [1e-15, 1 − 1e-15], que vale só no log loss
  (decisões de 2026-10-05).
- **Ressalva do intervalo** (decisão de 2026-10-05): como a calibração é
  acumulada, as diferenças de concursos vizinhos são correlacionadas e o
  intervalo normal tende a sair **estreito demais** (otimista). Ele serve
  como resumo; o veredito "supera o acaso" da 1c.5 usa um teste robusto à
  correlação serial (bootstrap por blocos de concursos seguidos ou teste de
  permutação).
- **Escopo da 1c.1**: só funções puras em `src/avaliacao/` (taxa do acaso do
  `LoteriaConfig`, calibração, log loss, Brier, resumo), com testes contra
  valores calculados à mão. Leitura do banco e comando ficam na 1c.5; o
  corte do trancado entra no `LoteriaConfig` na 1c.3.
- Ranking: acertos das top-k dezenas vs. esperado pela hipergeométrica
  (ex.: Lotofácil, 15 dezenas ao acaso → 9 acertos em média). Regras da
  1c.2 (decisões de 2026-10-05):
  - **k principal = K** da loteria (15, 6, 5); as funções aceitam qualquer
    k de 1 a N−1, e a 1c.5 decide se o comando ganha `--k`;
  - **top-k** = dezenas com `posicao` ≤ k (o empate já é resolvido no índice);
  - **concursos**: o mesmo trecho da 1c.1 (do segundo concurso do índice
    até o corte, só os sorteados, sem o trancado);
  - **saída**: média de acertos por concurso contra o esperado k·K/N e a
    diferença; distribuição observada de 0, 1, 2… acertos ao lado da
    esperada pelo acaso; erro padrão e intervalo de 95% por concurso, como
    na 1c.1;
  - **p-valor exato e unilateral**: chance de o acaso dar pelo menos o total
    de acertos observado (soma de T hipergeométricas independentes). Vale
    mesmo com a correlação serial do ranking, porque sob o acaso o sorteio
    de *t* é independente da posição calculada até *t−1*. Também devolve o
    `log10` da cauda, que continua legível quando a probabilidade some no
    `f64`. **Só vale se o perfil e o k foram escolhidos sem ver os sorteios
    do trecho avaliado** (decisão de 2026-10-05): se a busca de pesos usou o
    mesmo trecho, ou se o melhor foi escolhido entre vários perfis ou k, o
    p-valor sai otimista — avaliar no conjunto trancado ou corrigir por
    Bonferroni/Holm, informando quantos perfis/k foram testados (1c.5);
  - **hipergeométrica em inteiros** (`u128` basta, até na Quina), testada
    contra valores tabelados;
  - **escopo**: só funções puras em `src/avaliacao/`, reaproveitando
    `Observacao`; leitura do banco e comando na 1c.5.
- Validação **walk-forward** (treina até *t*, testa em *t+1…*; nunca embaralha o tempo).
  Regras da 1c.3 (decisões de 2026-10-05):
  - **corte no config**: campo `ultimo_concurso_busca` no `LoteriaConfig`
    (3416, 2759, 6421) e uma função que diz se o concurso é da busca ou do
    trancado; o dataset da 1.5.1 usa a mesma função;
  - **walk-forward em Rust** = avaliar o trecho de busca em **5 dobras
    consecutivas de tamanho igual** (do segundo concurso do índice até o
    corte; a sobra da divisão vai para a última), com log loss / Brier (1c.1)
    e top-k (1c.2) contra o acaso em cada dobra, para ver se o resultado é
    estável. A busca de pesos da 1.5.3 usa as mesmas dobras (ajusta até uma
    dobra, testa nela);
  - **calibração separada da avaliação**: a calibração sempre começa no
    primeiro concurso do índice; um limite inferior só da avaliação define a
    dobra que entra nas métricas;
  - **trancado** só é avaliado quando pedido explicitamente (padrão = busca):
    do corte + 1 até o último sorteado, com a calibração usando todo o
    passado. Garantir o "uma única vez" (registro do que já foi avaliado no
    trancado) fica para a 1c.5;
  - **p-valor por dobra** sem otimização extra; otimizar só se a medição
    passar de 2 s no total;
  - **escopo**: só funções puras em `src/avaliacao/` e o campo novo em
    `config.rs`; leitura do banco, comando e o teste robusto do veredito
    (bootstrap por blocos) ficam na 1c.5.
- **Conjunto final trancado**: os últimos ~10% dos concursos, usados uma
  única vez para validar o perfil campeão — protege contra sobreajuste na
  busca de pesos. O corte é **fixo pelo número do concurso** (decisão de
  2026-10-05, 10% do histórico de então), não uma porcentagem que se move:

  | Loteria | Busca e walk-forward | Trancado | Concursos trancados em 2026-10-05 |
  |---|---|---|---|
  | Lotofácil | 1 … 3416 (até 2025-06-12) | 3417 em diante (desde 2025-06-13) | 380 |
  | Mega-Sena | 1 … 2759 (até 2024-08-08) | 2760 em diante (desde 2024-08-10) | 307 |
  | Quina | 1 … 6421 (até 2024-04-20) | 6422 em diante (desde 2024-04-22) | 713 |

  Concursos sorteados depois também ficam trancados: o trancado cresce com o
  tempo e os sorteios novos servem de teste extra, em dados que nem existiam
  quando o perfil foi escolhido. Nem a busca de pesos (Python) nem a escolha
  de perfil podem ler concursos acima do corte; o corte fica no código, por
  loteria, e o dataset da 1.5 marca a qual parte cada concurso pertence.
- Testes de aleatoriedade do histórico (qui-quadrado de frequência, runs test).
  Decisões de 2026-10-05: o runs test roda **por dezena**, sobre a
  sequência saiu/não saiu ao longo dos concursos, no **histórico inteiro**.
  **Significância** (decisão de 2026-10-05): α = 5% por família de testes.
  Nos testes feitos dezena a dezena (ex.: runs test das 80 dezenas da
  Quina), correção de **Holm** para o conjunto; testes globais (qui-quadrado
  de frequência sobre todas as dezenas) usam α = 5% direto. O relatório
  mostra, por dezena, o p-valor original e o corrigido. Rejeição depois da
  correção: conferir primeiro os dados (erros de planilha), depois o sorteio.
  Detalhes (decisões de 2026-10-05):
  - **qui-quadrado corrigido para sorteio sem reposição** (o de Pearson
    comum é conservador demais: na Lotofácil a média seria 10, não 24);
  - **runs test com p-valor exato**, bilateral pelo **dobro da menor
    cauda**: p = min(1, 2·min(P(R ≤ r), P(R ≥ r))). A aproximação normal
    dava alarme falso acima dos 5% depois do Holm. A regra da soma das
    probabilidades até a observada foi descartada: a distribuição de R é
    "em serrote" (P(2k+1)/P(2k) = (n−2k)/(2k)), e a paridade de R — que só
    diz se o primeiro e o último concurso têm o mesmo estado — passava a
    decidir a rejeição (Quina: ≈ 27% com R par contra ≈ 2% com R ímpar);
  - dezena que **nunca ou sempre saiu** no trecho **entra no Holm com
    p = 1** (conta como um teste);
  - histórico **inteiro**, do concurso 1 ao último, incluindo o trancado
    (estes testes não escolhem perfil).

### Fase 1.5 — Ponte com o Python

- Comando `sync dataset <loteria> --formato parquet|csv` (decisões de 2026-10-06, 1.5.1):
  - **trancado fora por padrão**: só concursos até o corte da busca
    (`trecho_do_concurso` = Busca); coluna `trecho` sempre presente;
    `--trancado` inclui o resto, só para a validação final;
  - **formato longo**: uma linha por (concurso, dezena, janela), igual a
    `features_dezena`; o Python pivota;
  - **colunas**: `concurso, data, dezena, janela, trecho`, os 9 componentes,
    `saiu` e `feature_version`. Sem índice nem perfil (o Python recalcula o
    índice a partir dos pesos; os vetores de conformidade da 1.5.4 garantem
    que bate com o Rust). Só concursos já sorteados (`saiu` preenchido);
  - **Parquet** com o crate `parquet` sem a parte `arrow`, sem compressão ou
    com zstd;
  - **determinismo**: mesmos dados → mesmos bytes nos dois formatos (ordem
    (concurso, dezena, janela), sem carimbo de tempo no arquivo, metadados do
    Parquet fixos), com teste;
  - **CSV de máquina**: separador `,`, ponto decimal, cabeçalho, campo vazio
    para ausente, LF, UTF-8 sem BOM (diferente do CSV de planilha do
    `sync tabela`);
  - **saída**: `datasets/<loteria>/features_v<feature_version>.<ext>` (pasta
    fora do git); com `--trancado`, nome próprio,
    `features_v<feature_version>_trancado.<ext>`, para a busca de pesos
    nunca ler o trancado por engano. Ao lado de cada arquivo, um manifesto
    **por formato** (`manifesto_<ext>.json`, e `manifesto_trancado_<ext>.json`
    com `--trancado`) com loteria, versão das features, último concurso,
    corte, trecho incluído, número de linhas, SHA-256 do arquivo, a
    impressão dos resultados **do banco** (rotulada assim; muda a cada
    sorteio) e a impressão **só dos concursos incluídos**, para o Python
    conferir o que leu (decisões de 2026-10-06);
  - **`--formato`** padrão `parquet`; sem coluna `loteria` (a loteria fica no
    manifesto, nos metadados do Parquet e no nome da pasta);
  - **pré-condições**: uma loteria por execução, sem `todas`; recusa features
    desatualizadas ou resultados alterados (mesma conferência do `sync
    indice`/`sync avaliar`); banco só para leitura; SQL em `db.rs`, fluxo em
    `orquestracao`, montagem do arquivo num módulo próprio sem banco.
- Pasta `ml/`: leitura do dataset, baseline, busca de pesos com walk-forward,
  escrita de novos perfis `.toml`. Regras da 1.5.2 (decisões de 2026-10-06):
  - **ambiente**: venv em `ml/.venv` (fora do git), Python 3.12, versões
    fixadas em `ml/pyproject.toml`; o gate roda
    `ml/.venv/Scripts/python -m pytest ml/`;
  - **dependências**: só `pyarrow`, `numpy` e `pytest` (sem pandas, polars
    ou scikit-learn);
  - **leitura sempre pelo manifesto**: confere SHA-256, número de linhas,
    `feature_version` e último concurso ≤ corte; recusa linha com
    `trecho != busca` e qualquer arquivo `_trancado`; lê Parquet e CSV para
    a mesma estrutura; leitura do trancado fora desta tarefa;
  - **baseline** = (a) o acaso *K/N* (log loss, Brier e top-k esperado) nas
    mesmas 5 dobras do Rust + (b) a avaliação de um perfil `.toml` refeita em
    Python (índice, calibração pela posição com m = 100 e métricas),
    conferida com `sync avaliar --perfil padrao_v1` com tolerância ~1e-12 —
    é a métrica que a 1.5.3 otimiza. Nenhum modelo de ML agora;
  - **K e N** tirados do próprio dataset (N dezenas por concurso, K = soma de
    `saiu`), recusando concurso que não bata; sem tabela fixa no Python;
  - **dobras** as mesmas da 1c.3, refeitas em Python e testadas contra os
    limites reais do Rust;
  - **experimento** `ml/experimentos/001_baseline.py`, saída em texto,
    resultado anotado em `docs/PROGRESSO.md`;
  - **testes**: pytest com dados sintéticos pequenos e uma fixture pequena
    (CSV e Parquet) gerada pelo `sync dataset` num banco sintético, em
    `ml/tests/fixtures/`, versionada no git.
  Para o começo da 1.5.3 (decisões de 2026-10-06): o Python ganha uma tabela
  com o corte da busca de cada loteria (3416, 2759, 6421), conferida contra o
  `ultimo_concurso_busca` do manifesto (reforço: o corte fica no código também
  do lado Python); e o formato do manifesto do dataset é documentado como
  contrato em `docs/contrato-v1.md`, porque o Python depende dele.
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

0 → 1a → 1b → 1c → 1.5 → 2 → 3. O contrato da Fase 2 é o ponto de encontro
com o planejamento do front-end.

## Decisões em aberto

- Destino da publicação (Cloudflare R2/Pages, GitHub Pages, S3, …) — Fase 2.
- Canal de alerta (Telegram, Discord, e-mail) — Fase 3.

## Backlog

- Investigar os desvios de frequência apontados pela 1c.4 nos bancos reais
  (uniformidade rejeitada na Lotofácil, p ≈ 1,5e-4, concentrada nos concursos
  1899–3796; Mega-Sena, p ≈ 6,4e-3; Quina, p ≈ 0,042; runs da dezena 25 da
  Quina): conferir esses trechos contra uma fonte independente antes de
  qualquer conclusão sobre o sorteio. Depois da 1c.5.
- Premiação (ganhadores, rateio, arrecadação, cidades/UF).
- Outras loterias (Dupla Sena, Timemania, Dia de Sorte, +Milionária, Lotomania).
- API própria (axum), se o front-end precisar de algo dinâmico.
- Modelos de ML rodando em Rust (`burn`, `candle` ou ONNX).
- Robustez da raspagem (`downloader.rs`): avaliar XPath em vez de seletores
  CSS e técnicas anti-detecção, caso o site volte a bloquear o acesso
  automatizado.
