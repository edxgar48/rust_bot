# Planejamento — back-end de estatísticas de loterias (Fases 0 a 3)

Este documento cobre o back-end: coleta, validação, motor estatístico,
experimentos de ML e exportação/automação. O front-end (app PWA, Fases 4 a 8)
tem planejamento próprio em `docs/PLANEJAMENTO-FRONTEND.md`; o ponto de
contato entre os dois é o contrato de dados da Fase 2 (`docs/contrato-v1.md`).

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
| 0.3 | Mesma validação aplicada ao gravar (`importar`, `baixar`, `atualizar`). Buraco na numeração é só aviso na gravação (decisão de 2026-10-03); no `validar`, continua sendo erro. Concurso com data posterior a hoje (relógio de Brasília) é **erro** na gravação e bloqueia o lote, em todo caminho que grava (`importar`, `baixar`, `atualizar`, `inserir`, `ciclo`) (decisão de 2026-10-13) |
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
  concurso para o Excel, pasta `tabelas/` fora do git; UTF-8 com BOM, `;`,
  vírgula decimal — o BOM faz o Excel em português ler os acentos; decisão
  de 2026-10-10, na 1d.4. O CSV do `sync dataset` segue sem BOM).
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
- **Teto da evidência a favor do acaso** (decisão de 2026-10-10, achado da
  1d.2): com H0 pontual dentro de uma priori estreita, o FB a favor de H0 é
  limitado — no máximo √(1 + Var_priori/Var_amostral), atingido em x = n·p.
  Com priori de 5 %, |log10 FB| a favor do acaso não passa de ~0,13 na
  Mega-Sena, ~0,17 na Quina e ~0,59 na Lotofácil no histórico inteiro (na
  janela de 500: ~0,03, ~0,02 e ~0,23): "muito fraca" ou, na Lotofácil
  inteira, "fraca". A favor de viés
  não há teto. "Mesmo destaque" significa mesma apresentação, não mesma
  escala atingível: o relatório imprime o máximo atingível a favor do acaso
  em cada janela, e o GUIA e o app explicam essa assimetria.
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
  - **Conjunto trancado**: A lê o histórico inteiro, trancado incluso,
    porque descreve o sorteio (como o `sync aleatoriedade`), não avalia
    modelo. Por isso o resultado de A **não alimenta** busca de pesos nem
    escolha de perfil.
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
  aceito. O trancado não entra no cálculo.
  - **Índice inerte** (decisão de 2026-10-10, achado da 1d.3): a calibração
    estima uma taxa por posição, e isso custa evidência mesmo quando o
    índice não carrega informação nenhuma — um índice inerte já sai "muito
    forte, a favor do acaso" (ln FB médio ≈ −17, −40 e −60 na Lotofácil,
    Mega-Sena e Quina). O grau de B, sozinho, não diz que o índice atrapalha.
    Por isso o relatório imprime ao lado o esperado de um índice inerte: as
    mesmas posições diante de 200 históricos simulados sob o acaso (semente
    20261010), com média, faixa de 95 % e quantas réplicas igualam ou
    superam o observado. O índice só mostra informação se o observado ficar
    acima da faixa dos inertes. O GUIA e o app levam a mesma ressalva.
- **Sem tabela nova**: tudo de fórmula fechada e barato; calculado sob
  demanda a partir de `resultados` (A) e da avaliação já existente (B).
- **Onde aparece**: comando `sync evidencia <loteria>` (relatório em texto,
  uma loteria por vez, sem `todas`), colunas novas no CSV do `sync tabela`
  (grau e direção de A, janela principal) e, na Fase 2, no contrato do app.

Tarefas:
- **1d.1** — Conferência dos desvios de frequência da 1c.4 contra fonte
  independente. Resultado: sem erro de dado (detalhes em `PROGRESSO.md`);
  não é mais usada no plano.
- **1d.2** — Evidência por dezena (A): módulo `src/avaliacao/evidencia.rs`
  (função pura, sem banco), comando `sync evidencia`, testes (FB contra
  valores calculados à mão, simetria, graus nos limites, ponto no tempo,
  esperado sob o acaso). **Pronto quando:** roda nas três loterias com a
  sensibilidade da priori no relatório.
- **1d.3** — Evidência do índice (B) no mesmo comando, conferida contra a
  diferença de log loss do `sync avaliar` (mesmo número, outra escala).
  O relatório passa a imprimir, por janela, o máximo atingível a favor do
  acaso em A, e, em B, o esperado de um índice inerte.
- **1d.4** — Grau e direção no CSV do `sync tabela`; `docs/GUIA.md`
  atualizado com a leitura dos graus, o teto a favor do acaso, a ressalva
  do índice inerte em B e a correção do trecho que ainda dá a Lotofácil
  como "concentrada em 1899–3796".

### Fase 1e — Régua sensível (calibração linear na posição)

Decisão de 2026-10-10, depois da 1d.3: a calibração pela posição da 1c estima
uma taxa por posição (N parâmetros), e esse aprendizado custa de −17 a −60 em
ln FB mesmo para um índice sem informação. Com isso a régua quase não
distingue "sem sinal" de "sinal fraco". A 1e acrescenta uma calibração de **um
parâmetro**, cujo custo de aprendizado é da ordem de ½·ln T em vez de
(N−1)/2·ln T. Não muda a conclusão esperada (sorteios independentes: nenhum
perfil deve superar o acaso), só a nitidez dela.

- **Modelo**: `p_t(r) = K/N + b_t·z(r)`, com `z(r) = ((N+1) − 2r)/(N−1)`
  (posição 1 = +1, posição N = −1, média zero entre as N dezenas). Como
  `Σ z = 0`, as N probabilidades somam exatamente K.
- **Estimativa, só com o passado**: mínimos quadrados com encolhimento para
  zero, com **m = 100 concursos fictícios com b = 0** (o mesmo m da 1c):
  `b_t = Σ_(s<t) Σ_r z(r)·saiu(s, r) / ((c + m)·Σ_r z(r)²)`, c = concursos
  sorteados antes de t. O primeiro concurso recebe K/N. Contas em inteiros
  (`w(r) = (N+1) − 2r`) e uma divisão no fim, como na 1c. `b_t` é limitado a
  `±min(K/N, 1 − K/N)`, o que mantém todo *p* em [0, 1] sem quebrar a soma K;
  o limite do log loss (1e-15) vale igual.
- **O que se perde**: sinal que não é monotônico na posição (por exemplo, só
  a posição 7 boa). Num índice ordenado por força isso não tem sentido.
- **Onde entra** (alcance decidido em 2026-10-10):
  - **B do `sync evidencia`** passa a usar a calibração linear como régua
    principal (grau e direção saem dela), com o índice inerte também na
    linear. O FB com a calibração pela posição da 1c fica numa linha
    informativa, sem inerte.
  - **`sync avaliar`** ganha uma linha extra: log loss (perfil − acaso) com a
    calibração linear, no total e por dobra, só informativa (o veredito, o
    bootstrap e o top-k continuam como estão).
  - **Não muda**: a calibração pela posição (m = 100), o veredito da 1c, o
    Python (`ml/`, busca de pesos), as fixtures de conformidade e os registros
    do trancado.
- **Conferência**: ln FB de B (linear) = −pares × (log loss linear − log loss
  do acaso) da linha nova do `avaliar`, dobra a dobra, como na 1d.3.

Tarefas:
- **1e.1** — `calibrar_linear` em `src/avaliacao/calibracao.rs` (função pura,
  mesma saída `ProbabilidadeDezena`), com testes: soma K por concurso,
  ponto no tempo, valores à mão na MINI, limite de `b_t`, inerte com custo
  pequeno, índice que acerta sempre detectado. **Pronto quando:** testes
  verdes e referência independente nos testes.
- **1e.2** — B e índice inerte na calibração linear (linha informativa com a
  da 1c); linha extra no `sync avaliar`; conferência B × `avaliar`. **Pronto
  quando:** roda nas três loterias e a conferência passa.
- **1e.3** — `docs/GUIA.md` e README com a leitura nova de B e da linha extra
  do `avaliar`; números das três loterias em `PROGRESSO.md`.

### Fase 1f — Ranking por taxa histórica de posição e componentes

Pedido do usuário (2026-10-27). Para cada concurso passado, a posição que cada
dezena ocupava na tabela de força **antes** do sorteio (e a faixa de cada
componente) é cruzada com o resultado; a taxa de saída de cada posição e faixa
classifica as dezenas do próximo concurso. É descrição do passado: sorteios são
independentes e o resultado esperado da avaliação é "não supera o acaso".

- **Taxa por posição** `p(r)`: a mesma da 1c, `(saídas da posição r antes de t
  + m·K/N) / (concursos antes de t + m)`, m = 100, histórico inteiro. A posição
  vem de `indice` (perfil ativo), medida antes do sorteio.
- **Faixas dos componentes**: em cada concurso, cada um dos 9 componentes
  normalizados é dividido em **tercis** entre as dezenas daquele concurso
  (baixo, médio, alto); `NULL` é uma faixa própria. Taxa por (componente,
  faixa), com o mesmo encolhimento em direção a K/N.
  Decisões de 2026-10-27:
  - **Tercil** = `floor(3 · menores / n)`, com `menores` = quantas dezenas têm
    valor estritamente menor e n = dezenas com valor não `NULL` no concurso.
    Empatados caem na mesma faixa; contas em inteiros, sobre os valores crus
    de `features_dezena` (z-score e minmax preservam a ordem).
  - **Componente constante** no concurso (todas as dezenas com o mesmo valor):
    todas em "médio", como o neutro do índice. Consequências da regra: com
    uma só dezena com valor, ela conta como constante e fica em "médio"; com
    duas, nunca aparece "alto".
  - **Denominador da taxa por faixa**: `(saídas na faixa antes de t + m·K/N) /
    (pares concurso × dezena na faixa antes de t + m)`, m = 100 contado em
    pares.
  - **Janela dos componentes**: a do perfil ativo (50 no `padrao_v1`), a mesma
    do índice que dá a posição.
- **Nota A (principal)**: a taxa da posição atual de cada dezena. Ordena as N
  dezenas; desempate pela menor dezena, com o total arredondado (1e-9) como no
  índice.
- **Nota B (experimental)**: soma, sobre a posição e as 9 faixas, de
  `ln(taxa / (K/N))` (naive Bayes). Os componentes são correlacionados
  (colinearidade já descrita em *Casos de borda*), então B conta a mesma
  informação mais de uma vez: sai rotulada como experimental, nunca como
  principal. Regressão logística fica fora (é ML, vai para `ml/` se um dia
  fizer sentido).
- **Ponto no tempo**: as taxas usadas no concurso t veem só concursos até t−1;
  o snapshot do próximo concurso usa todo o histórico.
- **Evidência**: grau e direção da 1d (fator de Bayes, H0 = K/N) por posição e
  por faixa, mais a contagem de graus esperada sob o acaso (simulação com
  semente fixa), porque com 25 posições e 9 × 3 faixas algumas aparecem por
  acaso.
- **Avaliação**: A e B passam pela régua da 1c (walk-forward, top-k contra a
  hipergeométrica, bootstrap) e, para A, pela calibração linear da 1e. O
  conjunto trancado segue a regra da 1c: nada que escolhe lê acima do corte.
  Descrever as taxas pode ler o histórico inteiro (como a 1d-A); avaliar não.
- **Tabela derivada** `ranking_taxa(concurso, dezena, perfil, posicao_indice,
  taxa_posicao, nota_b, posicao_ranking_a, posicao_ranking_b)` (duas ordens: por
  A e por B; decisão de 2026-10-27), regenerável de `resultados`,
  `features_dezena` e `indice`; mais as taxas agregadas por posição e faixa
  em `taxa_posicao` e `taxa_faixa` (definidas na 1f.3, decisão de 2026-10-27):
  - `taxa_posicao(perfil, posicao, saidas, exposicoes, taxa, grau, direcao)` e
    `taxa_faixa(perfil, componente, faixa, saidas, exposicoes, taxa, grau,
    direcao)` guardam só o retrato do histórico inteiro, por perfil, conjunto
    trancado incluso. Servem só para descrever.
  - `exposicoes` conta concursos na posição e pares concurso × dezena na faixa.
  - Grau e direção vêm da evidência da 1d com priori de 5 %.
  - Faixa que nunca teve dezena vira linha com taxa K/N e grau e direção
    vazios; as tabelas têm sempre N e 36 linhas.
  - O perfil fica preso pelo nome; o hash é conferido em `perfis` (sem coluna
    de hash).
  - `ranking_taxa.taxa_posicao` (nota A em t, só com concursos até t−1) não é
    o mesmo número que `taxa_posicao.taxa` (histórico inteiro).
- **Contrato**: arquivo novo `ranking_taxa.json` (aditivo; `tabela_geral.json`
  e `ultimo.json` não mudam). Entra no `index.json` com hash, em
  `docs/contrato-v1.md`. Decisões de 2026-10-27 (tarefa 2.5):
  - **Conteúdo**: só o retrato do próximo concurso, uma linha por dezena:
    dezena, posição no índice, nota A, diferença para o acaso (A − K/N), nota
    B e as duas posições no ranking (por A e por B). Sem `taxa_posicao`, sem
    `taxa_faixa`, sem grau de evidência e sem a faixa de cada componente.
  - **Exportar não calcula o que publica**: lê `ranking_taxa`; pode
    recalcular só para conferir se a tabela está em dia, como já faz com o
    índice (em igualdade exata; se não bater, omite o arquivo).
  - **Ranking ausente ou atrasado**: o `sync exportar` publica o resto sem o
    `ranking_taxa.json`; a chave fica ausente de `arquivos` no `index.json`.
  - **`sync ciclo`** passa a rodar `sync ranking` antes do exportar; falha do
    ranking não derruba o ciclo (registra alerta e segue sem o arquivo).
  - **Contrato**: `ranking_taxa` é chave opcional de `arquivos`;
    `docs/contrato-v1.md` descreve três chaves obrigatórias e uma opcional.
    Aditivo, `schema_version` inalterado.

Tarefas:
- **1f.1** — `src/avaliacao/posicoes.rs` (função pura, sem banco): taxas por
  posição e por (componente, faixa), só com o passado. Testes: valores à mão
  na MINI, ponto no tempo, incremental = completo, `NULL` como faixa, tercis
  com empate. **Pronto quando:** testes verdes e referência independente.
- **1f.2** — Notas A e B, ordenação e desempate; graus da 1d por posição e
  faixa; esperado sob o acaso. **Pronto quando:** A reproduz a taxa da 1c.
- **1f.3** — Tabelas derivadas, `sync ranking <loteria>` (relatório em texto
  e CSV; uma loteria por vez) e registro do perfil/hash. **Pronto quando:**
  roda nas três loterias e regenera igual.
- **1f.4** — Avaliação de A e B na régua da 1c/1e; números das três loterias
  em `PROGRESSO.md`; `docs/GUIA.md` com a leitura e o aviso de não-previsão.
  Decisões de 2026-10-27:
  - **Onde roda**: opção `--avaliar` do `sync ranking`; só lê o banco e não
    altera o `sync avaliar`.
  - **A**: log loss e Brier de A são idênticos aos do perfil no `sync
    avaliar` (A = `calibrar`), então não se repetem; mostra-se só o que é
    novo — top-k pela ordem de A e calibração linear (1e) sobre a ordem de A —,
    com teste que confirma a igualdade com a 1c.
  - **B**: probabilidade por `calibrar` (m = 100) sobre `posicao_ranking_b`,
    mais a calibração linear, para A e B terem a mesma régua; log loss,
    Brier, top-k e bootstrap como na 1c.
  - **Trancado**: fechado — só as dobras 2 a 5, sem `--trancado` e sem
    registro de uso.
  - **Só código novo**: em arquivos existentes entram só acréscimos.
  - **Interpretações confirmadas na entrega**: a dobra 1 só treina (o total
    de B não é comparável ao do `sync avaliar`, que usa as 5 dobras);
    `--avaliar` recalcula em memória e não lê `ranking_taxa`; k = K, sem
    `--k`; `--avaliar` toma a trava `ranking`; sem bootstrap para A nem para
    as linhas da calibração linear. Perfil que não é o ativo (ex.: `busca_v1`)
    leva aviso de p-valores otimistas.
- **2.x (a numerar)** — `ranking_taxa.json` no `sync exportar` e no contrato,
  com auditoria do `auditor-contrato`.
- **7.7** — tela no app (ver `PLANEJAMENTO-FRONTEND.md`).

### Fase 1g — Índice de força das posições

Pedido do usuário (2026-10-31). A 1f mede só *quanto* cada posição da tabela
de força saiu (uma contagem). A 1g trata a **posição como se fosse uma dezena**:
marca, em cada concurso passado, quais posições a tabela de força ocupava nas
dezenas sorteadas, e roda sobre essas séries o mesmo motor de features e índice.
O resultado é um índice de força das posições 1…N, traduzido para dezenas pela
tabela de força do próximo concurso. Como a 1f, é descrição do passado: sorteios
são independentes e o resultado esperado da avaliação é "não supera o acaso".
Além disso, a posição de uma dezena já é função do histórico dela, então a série
de posições é uma recodificação do mesmo histórico, não informação nova.

Decisões de 2026-10-31:

- **Armazenamento**: tabelas derivadas novas no mesmo `resultados_<loteria>.db`
  (regra "um banco por loteria"). Nada existente muda; tudo regenerável.
- **Motor reaproveitado**: as 9 features e as janelas 10/50/100/total sobre a
  série de cada posição, com perfil próprio `perfis/<loteria>/posicoes_v1.toml`
  (mesmo formato dos perfis atuais; pesos iniciais iguais aos do `padrao_v1`
  até haver busca de pesos).
- **Saída para o app**: ranking de dezenas via posição, em arquivo opcional e
  aditivo do contrato, como o `ranking_taxa.json`.

Definições:

- **Marca** `posicao_marcas(concurso, posicao, perfil, saiu)`: para o concurso
  *t* e o perfil de força ativo, `saiu = 1` se a dezena que ocupava a posição
  *r* em `indice(t)` (posição medida **antes** do sorteio) foi sorteada em *t*.
  Cada concurso marca exatamente K posições. O snapshot do próximo concurso não
  tem marca (`saiu = NULL`).
- **Posição como dezena**: um "resultado" sintético por concurso, com as K
  posições marcadas no lugar das dezenas, numa `LoteriaConfig` sintética com
  `dezena_min = 1`, `dezena_max = N` e o mesmo K. O motor roda sem alteração
  (código só reutilizado, nada existente muda), gerando `posicao_features` e
  `posicao_indice` (mesmos campos de `features_dezena` e `indice`, com `posicao`
  no lugar de `dezena`).
- **Ponto no tempo**: a marca de *t* usa `indice(t)` (só concursos até *t−1*) e o
  resultado de *t*; as features da posição em *t* usam só as marcas até *t−1*.
  Nenhuma etapa lê o futuro.
- **Ranking de dezenas**: no próximo concurso, as posições são ordenadas por
  `posicao_indice`; a dezena que ocupa a posição *r* em `indice(próximo)` herda
  a ordem da posição *r*. Resultado: uma ordem 1…N das dezenas.
- **Aquecimento**: os primeiros concursos têm `indice` sem features (todas as
  dezenas empatadas, posição = número da dezena); as marcas ali são só ruído.
  A numeração do motor exige começar no concurso 1, então a série começa lá e
  a avaliação ignora o aquecimento (ver 1g.3).
- **Perfil preso**: as tabelas ficam por perfil de força (`perfil` na chave),
  porque a marca depende da posição que aquele perfil dava.

Esquema (regenerável de `resultados`, `features_dezena` e `indice`; definido na
1g.3, decisões de 2026-10-31). `perfil` é sempre o perfil de **força** que deu
as posições. A chave começa por `perfil` (e `janela`) para apagar e ler um
perfil sem varrer os outros — única diferença de ordem em relação às tabelas que
já existiam:

```sql
posicao_marcas(perfil, concurso, posicao, saiu,            -- só concursos sorteados
               PRIMARY KEY (perfil, concurso, posicao)) WITHOUT ROWID
posicao_features(perfil, janela, concurso, posicao, <9 componentes>, saiu,
                 feature_version,                          -- inclui o snapshot do próximo (saiu NULL)
                 PRIMARY KEY (perfil, janela, concurso, posicao)) WITHOUT ROWID
posicao_indice(perfil, perfil_posicoes, concurso, posicao, total, g_1,
               variacao_g_1,                               -- g_1 anterior − g_1 atual
               PRIMARY KEY (perfil, perfil_posicoes, concurso, posicao)) WITHOUT ROWID
posicao_perfil(nome, versao, loteria, janela, normalizacao, hash_toml, toml,
               criado_em, PRIMARY KEY (nome))              -- registro próprio; `perfis` não é tocada
posicao_motor_estado(perfil, loteria, feature_version, ultimo_concurso,
                     impressao, estado, PRIMARY KEY (perfil))
```

A marca do próximo concurso não existe (nem como linha com `saiu` nulo): só
`posicao_features` e `posicao_indice` cobrem o snapshot do último + 1.

Decisões fechadas na 1g.1 (2026-10-31):

- **Coluna do ranking das posições: G-1**, grafada `g_1` no banco e no código
  (hífen não é identificador SQL válido sem aspas e o projeto usa snake_case;
  o rótulo "G-1" fica para relatórios e app). `g_1` = posição 1…N da posição
  no índice das posições (1 = a de maior total).
- **Aquecimento**: as tabelas gravam todos os concursos; o corte vale **só na
  avaliação** (1g.4). Valor aprovado pelo usuário em 2026-10-31: ignorar os
  concursos 1 a 100 (a maior janela finita, `JANELA_MAX`), constante
  `avaliacao::AQUECIMENTO`.
- **Estado de motor próprio**: tabela `posicao_motor_estado`, uma linha por
  perfil de força (a marca depende da posição que o perfil dava), com
  `perfil, loteria, feature_version, ultimo_concurso, impressao, estado`.
  `impressao` = hash dos resultados **e** das posições do `indice` do perfil já
  incorporados; mudou qualquer um, regenera tudo (regra do `sync features`).
  `sync posicoes` retoma dela e calcula só os concursos novos.

### Golden da 1g (MINI: N = 5, K = 2, 3 concursos)

Entrada fixa (a posição do `indice` é dada, para o golden não depender de
pesos; `d` = dezena):

| t | sorteio | posição de d1 d2 d3 d4 d5 em `indice(t)` |
|---|---|---|
| 1 | {1, 2} | 1 2 3 4 5 (todas empatadas: ordem da dezena) |
| 2 | {2, 3} | 2 1 3 4 5 |
| 3 | {1, 5} | 3 2 1 5 4 |
| 4 (próximo) | — | 2 4 3 5 1 |

Marcas (`posicao_marcas`): posição ocupada pelas dezenas sorteadas.

- t=1: d1→1, d2→2 ⇒ posições {1, 2}
- t=2: d2→1, d3→3 ⇒ posições {1, 3}
- t=3: d1→3, d5→4 ⇒ posições {3, 4}

Resultados sintéticos entregues ao motor: `1:{1,2}`, `2:{1,3}`, `3:{3,4}`.

Features das posições em t=4, janela total (a janela é o histórico inteiro):

| posição | frequencia | maior_espaco (oc.) | menor_espaco (oc.) | soma_variacoes | espaco_final | repeticoes_total | repeticao_atual |
|---|---|---|---|---|---|---|---|
| 1 (saiu em 1, 2) | 2 | 0 (1) | 0 (1) | NULL | 1 | 1 | 0 |
| 2 (saiu em 1) | 1 | NULL | NULL | NULL | 2 | 0 | 0 |
| 3 (saiu em 2, 3) | 2 | 0 (1) | 0 (1) | NULL | 0 | 1 | 2 |
| 4 (saiu em 3) | 1 | NULL | NULL | NULL | 0 | 0 | 1 |
| 5 (nunca) | 0 | NULL | NULL | NULL | NULL | 0 | 0 |

Tradução para dezenas: se a ordem das posições pelo índice for
`g_1`: posição 3 = 1, 1 = 2, 4 = 3, 2 = 4, 5 = 5, então, por `indice(4)`
(posição 3 = d3, 1 = d5, 4 = d2, 2 = d1, 5 = d4), a ordem das dezenas é
d3, d5, d2, d1, d4 (a dezena herda o `g_1` da posição que ocupa). A `g_1` desse
exemplo é entrada, não saída do motor: testa só a tradução.

Ponto no tempo: as features de t=3 não podem ver a marca de t=3 (posição 3 em
t=3 tem `frequencia` 1, não 2).

Tarefas:
- **1g.1** — Especificação fechada: responder as dúvidas acima, golden à mão
  numa MINI (3 concursos, N = 5, K = 2) com as marcas esperadas. **Pronto
  quando:** golden escrito neste documento e aprovado (escrito em 2026-10-31;
  falta a aprovação do usuário).
- **1g.2** — `src/motor/posicoes.rs` (função pura, sem banco): `indice` + resultado
  → marcas → resultado sintético → features e índice das posições. Testes: golden
  da 1g.1, ponto no tempo, incremental = completo, cada concurso com K marcas.
  **Pronto quando:** testes verdes e verificador-vazamento sem achados.
- **1g.3** — Tabelas derivadas e `sync posicoes <loteria>` (texto e CSV; uma
  loteria por vez; registro do perfil/hash). **Pronto quando:** roda nas três
  loterias e regenera igual. Decisões de 2026-10-31 (independência do que já
  existe):
  - **Só lê** `resultados`, `features_dezena`, `indice` e `perfis` (para
    conferir, como o `sync ranking`, que o índice de força está em dia); **só
    grava** nas cinco tabelas `posicao_*`. O perfil das posições é registrado em
    `posicao_perfil` com a mesma imutabilidade (`PerfilAlterado`), nunca em
    `perfis`. Código em arquivos novos (`db_posicoes.rs`, `orquestracao/posicoes.rs`,
    `posicoes.rs`); nos existentes entram só acréscimos (`mod`, `pub use`,
    subcomando da CLI).
  - **Comando**: `sync posicoes <loteria> [--perfil <força>] [--perfil-posicoes
    posicoes_v1] [--saida posicoes] [--db] [--perfis]`; não aceita `todas`; toma
    a trava `posicoes`. Perfil `perfis/<loteria>/posicoes_v1.toml` com os pesos,
    a janela e a normalização do `padrao_v1` até haver busca de pesos.
  - **Incremental**: marcas, features e estado retomam de
    `posicao_motor_estado` e calculam só os concursos novos; o índice das
    posições é refeito inteiro a cada execução (depende só das features
    gravadas). Recalcula tudo do zero se não houver estado, se ele for ilegível
    ou de outra versão, se a impressão das marcas já incorporadas mudou
    (resultado corrigido ou posição do índice de força diferente) ou se
    `posicao_marcas` ou `posicao_features` não baterem com o estado. Com
    concursos novos, o índice de outro perfil das posições do mesmo perfil de
    força é apagado (ficaria velho, como no recálculo do zero) e volta quando
    aquele perfil rodar de novo. Tudo numa transação.
  - **Saída**: relatório em texto e CSV do índice das posições do próximo
    concurso (`g_1`, posição, dezena que a ocupa, total, variação), em
    `<saida>/<loteria>/posicoes_<loteria>_<força>_<posições>_<concurso>.csv`.
    A ordem das dezenas pelo `g_1` e a avaliação ficam para a 1g.4.
- **1g.4** — Ranking de dezenas via posição e avaliação na régua da 1c/1e
  (walk-forward, top-k contra a hipergeométrica, bootstrap), ignorando o
  aquecimento; comparação com a nota A da 1f; números das três loterias em
  `PROGRESSO.md`; `docs/GUIA.md` com a leitura e o aviso de não-previsão.
  **Pronto quando:** números publicados e conjunto trancado intacto. Decisões
  de 2026-10-31:
  - **Onde roda**: opção `--avaliar` do `sync posicoes` (conflita com `--saida`),
    só lendo o banco: recalcula as marcas, o índice das posições e as taxas da
    1f em memória, sem ler `posicao_*` nem `ranking_taxa`; não grava tabelas nem
    CSV. Código novo em `avaliacao/avaliacao_g1.rs` (função pura),
    `orquestracao/posicoes/avaliar_g1.rs` e `posicoes/avaliar.rs` (relatório);
    nada existente foi alterado.
  - **Ordem G-1**: no concurso *t*, cada dezena herda o `g_1` da posição que
    ocupava em `indice(t)`; passa pela régua da 1f.4 (`avaliar_ranking`) como
    "ordem B": log loss e Brier com a calibração da 1c (m = 100), top-k contra a
    hipergeométrica, bootstrap por blocos e a linha linear da 1e.
  - **Referência A**: a nota A da 1f é **reavaliada** pela mesma régua, nas
    mesmas linhas e dobras, porque o corte do aquecimento muda a calibração e as
    dobras; os números **não** são comparáveis com os do `sync ranking
    --avaliar`, que parte do concurso 1.
  - **Aquecimento**: os concursos 1 a 100 saem da avaliação inteira (calibração
    inclusive); a série avaliada começa no 101 (primeiro da entrada, sem
    avaliação) e as dobras se dividem a partir do 102,
    daí. As tabelas `posicao_*` continuam gravando todos os concursos.
  - **Trancado fechado**: só as dobras 2 a 5, sem `--trancado` e sem registro de
    uso; resultados, índice e observações são cortados em
    `ultimo_concurso_busca` antes de marcas e taxas.
  - **Ordem das dezenas pelo G-1**: é a ordem das linhas do CSV do `sync
    posicoes` (`g_1` crescente, com a dezena que ocupa a posição).
- **2.6** — `ranking_posicoes.json` opcional no `sync exportar`, no `sync ciclo`
  e em `docs/contrato-v1.md`, com auditoria do `auditor-contrato`. Decisões de
  2026-10-31 (pedido do usuário: o G-1 aparece na tela da Força, junto das
  dezenas, como um dado extra; ver 7.8 em `PLANEJAMENTO-FRONTEND.md`):
  - **Arquivo novo e opcional** `ranking_posicoes.json`, por loteria
    (`<loteria>/ranking_posicoes.json`), chave opcional `ranking_posicoes` de
    `arquivos` no `index.json`, descrita em `docs/contrato-v1.md`. Aditivo:
    `schema_version` inalterado; `ultimo.json`, `tabela_geral.json`,
    `historico.json` e `ranking_taxa.json` não mudam. O contrato passa a ter três
    chaves obrigatórias e duas opcionais.
  - **Conteúdo**: só o retrato do próximo concurso. Cabeçalho como o de
    `ranking_taxa.json` (`schema_version`, `loteria`, `perfil`, `hash_toml`,
    `janela`, `normalizacao`, `feature_version`, `concurso`, `ultimo_sorteado`)
    mais `perfil_posicoes`, `hash_toml_posicoes`, `janela_posicoes` e
    `normalizacao_posicoes` (os do perfil das posições). Uma linha por dezena, em
    ordem crescente de dezena: `dezena`, `posicao_indice` (a posição na tabela de
    força, igual à de `tabela_geral.json`), `g_1` (1…N, sem repetição; é o `g_1`
    da posição que a dezena ocupa) e `variacao_g_1` (`g_1` do concurso anterior −
    `g_1` atual, **por dezena**, ver abaixo; `null` sem concurso anterior). Sem totais, sem features e sem
    marcas: o app não recebe o que não usa.
  - **`variacao_g_1` é da dezena** (decisão de 2026-10-31, antes de qualquer
    publicação): `g_1` que a dezena tinha em `ultimo` (o `g_1` da posição que ela
    ocupava em `indice(ultimo)`, lido de `posicao_indice` do concurso `ultimo`) −
    `g_1` atual dela; positivo = subiu, como `variacao_posicao` de
    `tabela_geral.json`. Não é o `variacao_g_1` da posição gravado em
    `posicao_indice`. A conferência "em dia" cobre os dois concursos (`ultimo` e
    `ultimo + 1`) por igualdade exata.
  - **Independência**: qualquer erro de leitura das tabelas `posicao_*` (esquema
    antigo, coluna ausente, valor inesperado) vira omissão do arquivo, nunca
    impede o `exportar` nem o ciclo de publicar os obrigatórios.
  - **Perfil das posições**: constante `posicoes_v1` (a mesma do
    `sync posicoes`); não entra em `LoteriaConfig`.
  - **Exportar não calcula o que publica**: lê `posicao_indice` e só publica se
    as tabelas `posicao_*` estiverem em dia: o estado salvo vai até o último
    sorteado, a impressão das marcas confere com a recalculada dos resultados e do
    índice de força, `posicao_indice` do par (perfil ativo, `posicoes_v1`) cobre o
    concurso `ultimo + 1` com as N posições, e o `g_1` gravado é exatamente o que
    o cálculo daria com `posicao_features` (igualdade exata). Ausente ou
    atrasado: o `sync exportar` publica o resto, **não grava** o arquivo (apaga um
    antigo) e **omite a chave**, como no `ranking_taxa.json`.
  - **`sync ciclo`** roda `sync posicoes` depois do índice e antes do exportar;
    falha das posições não derruba o ciclo (registra no log e segue sem o
    arquivo), com o mesmo tratamento de alerta do ranking se for um acréscimo
    simples.
  - **Linguagem**: o contrato descreve `g_1` como descrição do passado, sem
    sugerir previsão (a avaliação da 1g.4 não encontrou vantagem sobre o acaso).
- **7.8** — tela da Força com o G-1 (ver `PLANEJAMENTO-FRONTEND.md`).

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
- **Regras do `sync exportar`** (decisões de 2026-10-12; os campos são o
  contrato `v1`: depois só se acrescenta campo opcional):
  - **pré-condições**: só lê o banco, não calcula o que publica (pode
    recalcular só para conferir se a tabela está em dia); recusa e não grava
    nada se features ou índice do perfil não estiverem em dia (como o
    `sync tabela` e o `sync avaliar`);
  - **perfil**: sempre o perfil ativo da loteria (`perfil_ativo` do
    `LoteriaConfig`), sem `--perfil`;
  - **`tabela_geral.json`**: snapshot do próximo concurso (último + 1), pela
    mesma função do `sync tabela`. Cabeçalho `schema_version`, `loteria`,
    `perfil`, `hash_toml`, `janela`, `normalizacao`, `feature_version`,
    `concurso` (o próximo), `ultimo_sorteado`. Uma entrada por dezena, em
    ordem de posição: `dezena`, `posicao`, `total`, `variacao_posicao`, os 9
    componentes da janela do perfil (`null` explícito quando faltar),
    `evidencia_grau` e `evidencia_direcao` (evidência por dezena, A). A
    evidência do índice (B) fica fora por ora (custa 1 a 4 s) e entra depois
    como campo opcional;
  - **textos sem acento**: os JSON `v1` são ASCII puro. Valores de texto do
    contrato com identificadores próprios, que não seguem os rótulos de
    exibição do CSV: `evidencia_grau` ∈ {`muito_fraca`, `fraca`,
    `moderada`, `forte`, `muito_forte`}; `evidencia_direcao` ∈ {`vies`,
    `acaso`}; `normalizacao` ∈ {`zscore`, `minmax`}. Um teste fixa a tabela
    (decisão de 2026-10-12);
  - **números**: `total` como número JSON com 9 casas fixas;
  - **`ultimo.json`**: `schema_version`, `loteria`, `concurso`, `data`,
    `dezenas`;
  - **`historico.json`**: `{schema_version, loteria, concursos: [[concurso,
    "AAAA-MM-DD", [dezenas]], ...]}`, histórico inteiro em ordem crescente;
  - **`index.json`**: `schema_version`, `gerado_em` (ISO-8601 UTC, o único
    campo que muda a cada execução) e uma entrada por loteria do config com
    `ultimo_concurso`, `data` e, por arquivo, `caminho`, `sha256`, `bytes`;
    loteria sem arquivos na saída → `null`. `exportar <loteria>` regrava só
    os arquivos dela e refaz o `index.json` com os hashes dos arquivos
    presentes no disco; `exportar todas` é tudo ou nada (se uma loteria
    falhar, nada é gravado);
  - **formatação**: UTF-8 sem BOM, fim de linha `\n`, indentação de 2
    espaços, exceto `historico.json` (um concurso por linha); cada arquivo é
    gravado num temporário e renomeado (nunca fica pela metade);
  - **`.gitignore`**: `publico/` fica fora do repositório do código (os dados
    vão para o `rust_spider_dados`);
  - **entrega**: 2.1 e 2.2 numa entrega só (hashes e `schema_version` já
    fazem parte dos arquivos); a 2.3 (`docs/contrato-v1.md`) vem logo depois.
- `docs/contrato-v1.md`: significado de cada campo + exemplo real. É a
  entrega para o planejamento do front-end.
- Comando `sync publicar` atrás de uma interface simples (destino trocável).
  **Destino, por enquanto, um repositório git separado** (decisão de
  2026-10-12):
  - repositório próprio só para os dados, **público**, fora do repositório
    do código: `edxgar48/rust_spider_dados` (criado em 2026-10-12, branch
    `main`, SSH `git@github.com:edxgar48/rust_spider_dados.git`);
    `publico/v1/` vai para a raiz dele;
  - `sync publicar` usa um clone local do repositório (caminho configurável),
    copia `publico/v1/`, e só faz commit e push se algum hash do
    `index.json` mudou — sem commit vazio; push via SSH, com as credenciais
    git já configuradas na máquina (nenhum segredo no projeto);
  - o app lê pelas URLs
    `https://raw.githubusercontent.com/edxgar48/rust_spider_dados/main/v1/...`
    (cache de alguns minutos no GitHub); a URL base entra no
    `docs/contrato-v1.md`;
  - "por enquanto": o destino fica atrás da interface, para trocar por
    R2/Pages/S3 sem mudar o `sync exportar` nem o contrato (só a URL base).
- **Regras do `sync publicar`** (decisões de 2026-10-12):
  - **clone**: padrão `../rust_spider_dados`, trocável com `--repo <pasta>`;
    se não existir, erro com o `git clone` a rodar (não clona sozinho); recusa
    se a URL de fetch ou a de push do `origin` não terminar em
    `rust_spider_dados` (com ou sem `.git`), para um `--repo` errado nunca
    commitar `v1/` no repositório do código; a recusa mostra só o nome do
    repositório, nunca a URL inteira (pode trazer credencial) (decisão de
    2026-10-13);
  - **antes de copiar**: `git fetch` e confere que o clone está limpo, em
    `main` e sem divergir de `origin/main`; se não estiver, recusa sem
    tocar em nada (nunca `reset --hard`);
  - **o que entra**: só copia o `publico/v1/` já exportado, sem chamar o
    `exportar` (quem encadeia é o `sync ciclo`, 3.1); publica o `v1/`
    inteiro, sem argumento de loteria; aceita `--saida publico`;
  - **integridade**: confere `sha256` e `bytes` do `index.json` contra os
    arquivos no disco e recusa se não baterem;
  - **o que conta como mudança**: algum hash, ou a lista de loterias e
    arquivos, diferente do `index.json` do clone; se só `gerado_em` mudou,
    "nada a publicar" e sai com sucesso, sem copiar nem commitar;
  - **espelho**: `v1/` do clone fica idêntico a `publico/v1/`, apagando o
    que sumiu da saída; fora de `v1/` nada é tocado;
  - **commit**: um por publicação, mensagem fixa com o último concurso de
    cada loteria (ex.: "Dados: lotofacil 3802, megasena 3069, quina
    7140"), autor = git configurado na máquina;
  - **push que falha**: o commit fica local e o comando termina com erro; a
    próxima execução tenta o push de novo, mesmo sem mudança;
  - **implementação**: chama o `git` da máquina (sem crate nova, SSH já
    configurado); destino atrás de uma interface `Destino` em
    `src/publicacao/`; testes com um repositório "remoto" local, sem rede, e
    só quando o git estiver instalado.

### Fase 3 — Automação e operação

| # | Tarefa |
|---|---|
| 3.1a | Mover para a biblioteca (`orquestracao`), sem mudar comportamento, a lógica de gravação que ainda está na CLI (`processar_e_salvar`, `validar_lote`, `aplicar_atualizacao`) e o tudo ou nada do `exportar todas`, para o ciclo reaproveitá-los |
| 3.1b | `sync ciclo <loteria\|todas>`: atualizar → backfill se houver buraco → validar → features (incremental) → índice → exportar → publicar. Regras (decisões de 2026-10-13): **backfill** = baixar a planilha inteira pelo caminho do `baixar`; se o buraco persistir, só aviso e o `validar` reprova a loteria (busca concurso a concurso, com fonte nova, fica no backlog); **falha isolada** = uma loteria com falha não interrompe as outras e o comando sai com erro no fim; **publicação parcial** = exporta só as loterias que passaram (`exportar <loteria>` uma a uma, `index.json` com o que há no disco) e roda um único `publicar` no fim; **sem concurso novo** = segue até o `publicar` mesmo assim (todos os passos são idempotentes; o `publicar` reenvia push pendente e diz "nada a publicar" se nada mudou); **índice** só do `perfil_ativo` da loteria, features incrementais (a orquestração já decide quando regenerar); retentativas, trava, log e calendário ficam para 3.2–3.5 |
| 3.2 | Calendário de sorteios por loteria no `config.rs` (decisões de 2026-10-13): **dias e horário** — dias da semana de sorteio e horário a partir do qual vale procurar o resultado, por loteria; **conferência contra os dados** — os dias da Caixa mudam com o tempo, então o calendário é comparado com os dias da semana dos últimos concursos do banco e a divergência vira aviso (calendário desatualizado); **feriados e especiais** — concurso esperado que não aparece no dia (feriado, adiamento, Mega da Virada) é aviso, não erro, até passar a tolerância (o próximo dia de sorteio); **saída** — para (loteria, data) diz se é dia de sorteio e desde que horário procurar, e é o que a 3.3 (retentar só quando um concurso era esperado) e a 3.6 (dia sem sorteio não conta como falha) consomem. O `ciclo` da 3.1 não depende dele: sem concurso novo, segue sem efeito |
| 3.3 | Retentativas com backoff. Regras (decisões de 2026-10-13): **quem repete** — um processo só, `sync ciclo --aguardar`, disparado uma vez por dia às 23h00 de Brasília (3.7) e coberto pela trava da 3.4; sem `--aguardar`, o `ciclo` faz uma passada só; **o que provoca retentativa** — só o concurso esperado ainda não saiu (dia de sorteio pelo calendário da 3.2 e a página ainda mostra o anterior) ou falha transitória de navegador/rede; erro de validação ou de gravação não é repetido; **janela e intervalos** — da primeira passada às 23h00 até 01h00 do dia seguinte, com intervalos crescentes (5, 10, 20, 30, 30… min), só para as loterias que ainda faltam; depois da meia-noite o dia de sorteio esperado continua sendo o da véspera; **publicação** — cada loteria que chega segue para features, índice e exportação e é publicada logo (`publicar` a cada loteria que chega; até um commit por loteria por noite no repositório de dados); **dia seguinte** — é a próxima execução agendada, nenhum processo espera até o outro dia; fora de dia de sorteio ou com a loteria `EmDia`, não há espera; **tentativas curtas** — as 3 do `downloader.rs` (2 s e 4 s) continuam valendo dentro de cada tentativa; **testes** — relógio e espera injetados na biblioteca, sem dormir de verdade; **buraco persistente** (pendência da 3.1b) — backup e download da planilha no máximo uma vez por execução do `ciclo`, mesmo com várias rodadas |
| 3.4 | Arquivo de trava contra execução simultânea. Regras (decisões de 2026-10-13): **quem trava** — os comandos que gravam (`baixar`, `importar`, `atualizar`, `inserir`, `features`, `indice`, `exportar`, `publicar`, `ciclo`, `avaliar --trancado`, `dataset`); os só de leitura ficam livres (`listar`, `validar`, `buracos`, `calendario`, `aleatoriedade`, `evidencia`), e o `tabela` também (lê o banco só para leitura e grava só em `tabelas/`, onde nenhum outro comando escreve); resolve a corrida `exportar` × `publicar` (2.4) e a do uso único do conjunto trancado (1c.5); **escopo** — uma trava só, `sync.lock` na pasta de trabalho de onde o `sync` roda (onde ficam os bancos; vale mesmo com `--db` em outro lugar — a 3.7 fixa essa pasta) (`publico/` e o clone de dados são comuns às três loterias); **trava esquecida** — trava do sistema operacional sobre o arquivo (`File::try_lock`, `rust-version` 1.88 → 1.89), que some se o processo morrer ou a máquina reiniciar; PID, comando e horário de início ficam num arquivo ao lado, `sync.lock.dono`, só para a mensagem (no Windows o processo barrado não lê o arquivo travado; sem o `.dono`, a mensagem sai sem detalhes e o bloqueio vale igual); **trava ocupada** — falha na hora, sem esperar, com "outra execução em andamento: PID, comando, desde HH:MM" e saída com erro (a execução agendada que a encontrar vira alerta na 3.6); **junto** — a espera do `--aguardar` (3.3) passa de `std::thread::sleep` para `tokio::time::sleep` |
| 3.5 | Log em arquivo (`logs/sync_AAAA-MM-DD.log`). Regras (decisões de 2026-10-13): **quem grava** — os mesmos comandos que pegam a trava da 3.4; os só de leitura ficam só no console; **conteúdo** — cópia de tudo o que sai na tela (saída normal e erros, com a cadeia completa de causas), cada linha com hora HH:MM:SS de Brasília e nível `INFO`/`AVISO`/`ERRO`, mais uma linha de abertura (PID, comando completo, versão) e uma de fechamento (duração, sucesso ou falha); **implementação** — sem crate nova: um escritor de log na biblioteca (testável com pasta temporária) recebe as linhas dos `Evento`/`EventoCiclo`, e a CLI mostra e grava pelo mesmo caminho; **data do arquivo** — a do início da execução em Brasília (a noite inteira do `ciclo --aguardar` cai num arquivo só), aberto em modo de acréscimo; **pasta e retenção** — `logs/` na pasta de trabalho (a mesma do `sync.lock`), no `.gitignore`; ao abrir o arquivo do dia, apaga logs com mais de 90 dias; **log que não grava** — aviso no console e o comando segue sem log, nunca derruba o `ciclo`; **junto** (pendência da 3.1b) — a separação erros/avisos do validar fica num lugar só (hoje duplicada entre `orquestracao/ciclo.rs` e a CLI) e o validar do `ciclo` passa a mostrar a dica de buracos (`DicaBuracos`) |
| 3.6 | Alerta após N falhas seguidas ou mudança aparente no site. **Canal** (decisão de 2026-10-13): e-mail, sem disparo automático — o sistema grava o e-mail pronto (rascunho) e o usuário o abre e envia quando quiser; sem SMTP nem credencial no projeto. Paradas longas previstas (decisão de 2026-10-13) vêm dos sorteios especiais de datas comemorativas — a loteria fica sem concursos regulares antes do especial (ex.: Mega da Virada, Lotofácil da Independência, Quina de São João) — e não podem contar como falha: o `Atrasado` do calendário (3.2) dentro dessas janelas vira aviso, não alerta Regras (decisões de 2026-10-13): **falha e N** — falha é uma noite em que a loteria termina em erro no `ciclo` ou fica `Atrasado` fora de janela de sorteio especial; alerta após N = 2 noites seguidas, um sucesso zera; concurso que só não saiu até 01h00 não conta; trava ocupada, pasta/binário ausente e processo morto contam num contador geral; **mudança aparente no site** — alerta já na primeira noite quando todas as rodadas terminam com erro de forma da página (elemento/botão não encontrado, `PaginaIncompleta`, `LinhaInvalida`, e também planilha ilegível ou com layout inesperado — `Planilha`, `TabelaNaoEncontrada`); rede/navegador seguem o N; a classificação fica numa função só da biblioteca; **histórico** — `alertas/estado.json` na pasta de trabalho, no `.gitignore`, nada no banco; ilegível = recomeça do zero com aviso; **rascunho** — `.eml` com `X-Unsent: 1` em `alertas/AAAA-MM-DD_HHMMSS_<loteria>-<motivo>.eml` (a loteria no nome evita colisão de dois alvos no mesmo segundo), assunto `[rust_spider] <loteria>: <motivo>`, corpo com o erro e as causas, o caminho do log do dia e os passos do runbook; destinatário lido de `alertas/destinatario.txt` (local, fora do git), sem ele o `.eml` sai sem `To:`; **quando gerar** — no fim do `sync ciclo` (módulo `orquestracao/alertas.rs`); trava ocupada sai com código 3 e o `ciclo_agendado.ps1` chama `sync alerta --geral` (não pega a trava) para trava ocupada e processo morto; binário ausente fica só no `agendador.log` e no "Último resultado" do Agendador; **aviso de rascunho novo** — linha AVISO no console, no log do dia e no `agendador.log`; `sync alertas` lista os rascunhos ainda não movidos para `alertas/enviados/`; **janelas dos especiais** — lista fixa por loteria no `config.rs` (data do especial, dias de parada antes e tolerância depois, porque nos bancos o especial não cai exatamente na data: Mega da Virada 31/12 com 20 antes e 2 depois, Lotofácil da Independência 7/9 com 15 e 8, Quina de São João 24/6 com 15 e 4), conferida por teste contra todas as paradas de 2021 a 2026 nos bancos; **noite** — a data do início da execução; duas execuções na mesma noite valem uma; **falha do `publicar`** — na passada única conta para cada loteria exportada (até 3 rascunhos iguais); no `--aguardar`, a falha do `publicar` de reenvio conta no contador geral; **repetição** — um rascunho por alvo e motivo enquanto durar, lembrete a cada 7 noites ainda em falha; volta ao normal só gera linha no log; **testes** — integração com loteria falsa falhando N vezes conferindo o `.eml`, depois execução manual autorizada (pasta inexistente ou trava presa) |
| 3.7 | Agendamento via Agendador de Tarefas do Windows (script `.ps1` versionado), uma vez por dia às 23h00 com `sync ciclo todas --aguardar` (3.3); o `publicar` sem console roda o SSH com `BatchMode=yes` (falha em vez de travar pedindo senha ou confirmação de host). Regras (decisões de 2026-10-13): **scripts** — dois em `scripts/`: `ciclo_agendado.ps1` (o que a tarefa executa: entra na pasta de trabalho, liga o SSH sem perguntas e devolve o código de saída do `sync`) e `registrar_tarefa.ps1` (cria ou atualiza a tarefa sem duplicar, com `-Remover` e `-Mostrar`); quem registra é o usuário, rodando o script à mão; **conta** — o usuário da máquina, "executar estando conectado ou não", sem guardar senha (S4U), sem console; alternativa `-SoConectado` (LogonType Interactive, sem elevação, janela oculta, só roda com o usuário conectado) para quando o registro com S4U der "Acesso negado" (decisão de 2026-10-13); **máquina desligada** — `StartWhenAvailable` (roda ao ligar; fora da janela 23h–01h o `--aguardar` faz uma passada só), sem acordar o PC, sem segunda instância, limite de 3 h, roda na bateria; **binário** — `target\release\sync.exe` direto, sem `cargo run` (um `cargo build --release` durante a janela da noite falha com o `.exe` em uso); **pasta de trabalho** — parâmetro `-Pasta`, padrão a raiz do repositório (fixa `sync.lock` e `logs/` da 3.4/3.5); **SSH** — o `.ps1` define `GIT_SSH_COMMAND="ssh -o BatchMode=yes -o ConnectTimeout=30"` só para o processo agendado, sem mudar o Rust; **rastro** — o `.ps1` grava uma linha por execução em `logs/agendador.log` (cobre binário ou pasta ausentes); **navegador** — sem código novo (a 3.3 já repete falhas passageiras); conferir numa primeira execução manual autorizada que o Playwright abre sob essa conta; **compatibilidade e testes** — Windows PowerShell 5.1, sem Pester; testes de integração em Rust (sintaxe dos `.ps1`, pulado sem PowerShell; `ciclo_agendado.ps1` com binário e pasta de teste conferindo pasta, SSH e código de saída; `registrar_tarefa.ps1 -Mostrar` sem registrar nada) |
| 3.8 | Runbook no README: bloqueio, seletor mudou, reprocessar do zero |

**Pronto quando:** uma semana rodando agendado sem intervenção, cada concurso
novo publicado no mesmo dia, e uma falha simulada dispara o alerta.

## Ordem e dependências

0 → 1a → 1b → 1c → 1.5 → 1d → 1e → 1f → 1g → 2 → 3. O contrato da Fase 2 é o ponto de encontro
com o planejamento do front-end.

## Decisões em aberto

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
- Aviso de inserção manual divergente (`src/manuais.rs`, `Conciliacao::Divergente`):
  diz "o banco fica com o oficial", o que é falso quando vem do `atualizar`,
  que nesse caso não grava nada (o banco fica com a inserção manual). Ajustar
  o texto conforme o caminho, ou fazer o `atualizar` gravar o oficial
  (registrado em 2026-10-13, achado da revisão da 3.8).
