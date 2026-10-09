# Decisões detalhadas — Fases 1 (1a, 1b, 1c) e 1.5

Texto movido de `docs/PLANEJAMENTO.md` em 2026-10-09, sem alteração, para
encurtar o plano (os agentes o leem inteiro a cada tarefa). Continua valendo:
é a especificação do que já está implementado. Mudança de definição continua
indo primeiro para o `PLANEJAMENTO.md`, que aponta para cá.

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
  Regras da 1.5.3, busca de pesos (decisões de 2026-10-06):
  - **o que varia**: os 9 pesos com sinal livre, a janela (0, 10, 50, 100) e a
    normalização (zscore, minmax); pesos normalizados (soma dos valores
    absolutos = 1), perfil com todos os pesos zero proibido;
  - **método**: busca aleatória com semente fixa e número fixo de candidatos
    por loteria (ex.: 2.000 em cada uma das 8 combinações de janela ×
    normalização), só com numpy; refinamento peso a peso opcional;
  - **walk-forward**: para cada dobra i de 2 a 5, o melhor candidato nas
    dobras 1 … i−1 é medido na dobra i; a estimativa honesta é o log loss
    nessas dobras de teste contra o acaso. O perfil final é o melhor no
    trecho de busca inteiro (dobras 1 a 5);
  - **métrica otimizada**: log loss médio por concurso, com a calibração da
    1.5.2; Brier e top-k só como informação;
  - **saída**: grava sempre o melhor em `perfis/<loteria>/busca_v1.toml` (ou
    a versão seguinte, se já existir), com cabeçalho em comentário (semente,
    candidatos, dados usados, resultado contra o acaso); pesos arredondados a
    6 casas e reavaliados já arredondados. A busca não grava no banco, não
    roda o trancado e não troca o perfil ativo (decisão do usuário);
  - **vários candidatos**: o relatório informa quantos foram testados e avisa
    que o p-valor do `sync avaliar` na busca sai otimista para o vencedor; a
    validação que vale é a do trancado;
  - **entregas**: `ml/rust_spider_ml/busca.py`,
    `ml/experimentos/002_busca_pesos.py` (saída em texto), pytest (mesma
    semente → mesmo resultado, a dobra de teste não influencia a escolha,
    multiplicar os pesos não muda o resultado, `.toml` gerado relido sem
    erro) e conferência numa cópia do banco (`sync indice` + `sync avaliar`
    com o perfil gerado batem com o Python dentro de 1e-12);
  - **ordem**: primeiro a tabela de cortes, o contrato do manifesto e as
    correções baixas da 1.5.2; depois a busca.
- **Vetores de conformidade** (`testes/conformidade/*.json`): entrada →
  features/índice esperados, verificados por `cargo test` **e** `pytest`.
