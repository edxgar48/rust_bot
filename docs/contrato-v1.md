# Contrato de dados v1

Formatos que o `rust_spider` grava para serem lidos fora do Rust. Este
arquivo é a fonte do contrato: **mudar, acrescentar ou remover um campo é
documentar aqui primeiro**, no mesmo commit da mudança no código.

A Fase 2 completará este contrato com o JSON público do app
(`publico/v1/`: `index.json`, `ultimo.json`, `tabela_geral.json`,
`historico.json`; ver `docs/PLANEJAMENTO.md`, Fase 2). Por enquanto ele
cobre só o dataset de ML.

## Dataset para o Python (`sync dataset`)

Gerado pelo Rust (`src/dataset.rs`, fluxo em `orquestracao::dataset_de_features`,
CLI em `src/bin/sync.rs`) e lido pelo pacote `ml/rust_spider_ml/dataset.py`.
Uma loteria por execução; o banco é aberto só para leitura; nada é
recalculado (os componentes são os gravados em `features_dezena` pelo
`sync features`).

```bash
cargo run --bin sync -- dataset <loteria> [--formato parquet|csv] [--trancado] [--saida datasets] [--db caminho]
```

### Arquivos

Na pasta `<saida>/<loteria>/` (padrão `datasets/<loteria>/`), um par por
modo e formato:

| Modo | Arquivo de dados | Manifesto |
|---|---|---|
| busca (padrão) | `features_v<fv>.<ext>` | `manifesto_<ext>.json` |
| `--trancado` | `features_v<fv>_trancado.<ext>` | `manifesto_trancado_<ext>.json` |

- `<fv>` é a `feature_version` (hoje `1`, `motor::FEATURE_VERSION`);
  `<ext>` é `parquet` (padrão) ou `csv`.
- Os nomes distintos garantem que o modo `--trancado` nunca sobrescreve os
  arquivos da busca e que a busca de pesos não lê o trancado por engano.
- Gravação: o manifesto antigo do mesmo modo e formato é apagado **antes**
  de o arquivo de dados ser regravado; o manifesto novo é gravado por
  último. Se a gravação falhar no meio, não sobra manifesto descrevendo
  outro arquivo. O par do outro modo/formato não é tocado.

### Manifesto

JSON (UTF-8, indentado com 2 espaços, chaves na ordem abaixo, `\n` no fim,
sem data de geração). Origem: `dataset::Manifesto` / `dataset::manifesto`.

| Campo | Tipo | Significado |
|---|---|---|
| `loteria` | texto | Slug da loteria (`lotofacil`, `megasena`, `quina`), igual a `LoteriaConfig::slug`. |
| `feature_version` | inteiro | Versão das features do arquivo (`motor::FEATURE_VERSION`). |
| `formato` | texto | `parquet` ou `csv`. |
| `arquivo` | texto | Nome do arquivo de dados, na mesma pasta do manifesto (sem caminho). |
| `colunas` | lista de texto | As 16 colunas do arquivo, na ordem (ver abaixo). |
| `primeiro_concurso` | inteiro | Primeiro concurso incluído (o Python exige `1`, porque a impressão dos incluídos cobre desde o concurso 1). |
| `ultimo_concurso` | inteiro | Último concurso incluído. Sem `--trancado`, ≤ `ultimo_concurso_busca`. |
| `ultimo_concurso_busca` | inteiro | Corte fixo da busca: `LoteriaConfig::ultimo_concurso_busca` (Lotofácil 3416, Mega-Sena 2759, Quina 6421). Concursos acima dele são o conjunto trancado. |
| `trecho` | texto | Trecho incluído: `busca` (padrão) ou `busca+trancado` (com `--trancado`). |
| `concursos` | inteiro | Número de concursos incluídos. |
| `linhas` | inteiro | Número de linhas de dados (sem o cabeçalho do CSV) = concursos × dezenas × janelas. |
| `sha256` | texto | SHA-256 do arquivo de dados, 64 dígitos hex minúsculos. |
| `impressao_resultados_banco` | texto | Impressão de `resultados` do **banco inteiro** (a mesma de `motor_estado`); muda a cada sorteio, mesmo os que ficam fora do arquivo. |
| `impressao_resultados_incluidos` | texto | Impressão de `resultados` só dos concursos `1 … ultimo_concurso` (os do arquivo); não muda quando entra um concurso acima do que o arquivo traz. |

Impressão (`orquestracao::impressao_resultados`): `fnv1a64:<n>:<hash>`, em
que `<n>` é o número de concursos e `<hash>` são 16 dígitos hex minúsculos
do FNV-1a de 64 bits (base `0xcbf29ce484222325`, primo `0x100000001b3`)
sobre os bytes UTF-8 das linhas `loteria|concurso|AAAA-MM-DD|d1,d2,…\n`,
uma por concurso em ordem crescente, dezenas em ordem crescente.

Exemplo (fixture `ml/tests/fixtures/lotofacil/manifesto_csv.json`, banco
sintético de 40 concursos da Lotofácil):

```json
{
  "loteria": "lotofacil",
  "feature_version": 1,
  "formato": "csv",
  "arquivo": "features_v1.csv",
  "colunas": [
    "concurso",
    "data",
    "dezena",
    "janela",
    "trecho",
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
    "saiu",
    "feature_version"
  ],
  "primeiro_concurso": 1,
  "ultimo_concurso": 40,
  "ultimo_concurso_busca": 3416,
  "trecho": "busca",
  "concursos": 40,
  "linhas": 4000,
  "sha256": "18b645403cfae13f235d861a2e62d4aa5fd0671d8a7e540b740e1082e480e6d1",
  "impressao_resultados_banco": "fnv1a64:40:7c353eaf6ed79bd2",
  "impressao_resultados_incluidos": "fnv1a64:40:7c353eaf6ed79bd2"
}
```

### Colunas do arquivo de dados

Formato longo: uma linha por (concurso, dezena, janela). Os componentes têm
os mesmos nomes de `features_dezena`; a definição de cada um está em
`docs/PLANEJAMENTO.md`, "Especificação das features". Todos são o snapshot
de **antes** do concurso (só concursos ≤ *t−1*); `saiu` é o rótulo do
próprio concurso *t*. Não há coluna `loteria`: ela fica no manifesto, nos
metadados do Parquet e no nome da pasta.

| # | Coluna | Tipo (Parquet) | `NULL`? | Significado |
|---|---|---|---|---|
| 1 | `concurso` | INT32 com sinal | não | Número do concurso *t*. |
| 2 | `data` | texto UTF-8 | não | Data do sorteio, `AAAA-MM-DD`. |
| 3 | `dezena` | INT32 com sinal | não | Dezena. |
| 4 | `janela` | INT32 com sinal | não | Janela em concursos: `0` (todo o histórico), `10`, `50`, `100`. |
| 5 | `trecho` | texto UTF-8 | não | `busca` (concurso ≤ corte) ou `trancado` (acima). |
| 6 | `frequencia` | INT32 com sinal | não | Componente. |
| 7 | `maior_espaco` | INT32 com sinal | sim | Componente. |
| 8 | `maior_espaco_ocorrencias` | INT32 com sinal | sim | Componente. |
| 9 | `menor_espaco` | INT32 com sinal | sim | Componente. |
| 10 | `menor_espaco_ocorrencias` | INT32 com sinal | sim | Componente. |
| 11 | `soma_variacoes` | INT32 com sinal | sim | Componente. |
| 12 | `espaco_final` | INT32 com sinal | sim | Componente. |
| 13 | `repeticoes_total` | INT32 com sinal | não | Componente. |
| 14 | `repeticao_atual` | INT32 com sinal | não | Componente. |
| 15 | `saiu` | INT32 com sinal | não | `1` se a dezena saiu no concurso *t*, `0` se não. |
| 16 | `feature_version` | INT32 com sinal | não | Igual ao `feature_version` do manifesto em todas as linhas. |

`NULL` = sem espaços suficientes no histórico (nunca 0 nem −1).

**Ordem das linhas**: estritamente crescente de (`concurso`, `dezena`,
`janela`), a chave de `features_dezena`, sem repetição. Todo concurso
incluído tem todas as dezenas e todas as janelas; os concursos são
seguidos, desde o 1. Só concursos já sorteados (o snapshot do próximo
concurso, com `saiu` = `NULL`, nunca entra).

### CSV de máquina

- UTF-8 sem BOM, separador `,`, fim de linha `\n` (inclusive na última);
- primeira linha = cabeçalho com as 16 colunas na ordem acima;
- `NULL` = campo vazio; sem aspas (só há inteiros, datas ISO e
  `busca`/`trancado`); sem casas decimais.

### Parquet

- Parquet 1.0, sem compressão, `created_by` fixo
  `rust_spider sync dataset` (independente da versão do crate);
- esquema `features_dezena`: inteiros `INT32` com anotação
  `INTEGER(32, true)`, textos `BYTE_ARRAY` `STRING`, `OPTIONAL` só nas 6
  colunas que podem ser `NULL`, o resto `REQUIRED`;
- metadados chave-valor do arquivo: `loteria`, `feature_version` (texto) e
  `trecho` (o mesmo valor do manifesto);
- row groups de 1 048 576 (2²⁰) linhas, colunas na ordem acima; estatísticas
  (mínimo/máximo) por coluna gravadas pelo escritor (o Python exige as de
  `trecho` e `concurso`).

### Determinismo

Mesmo banco (mesmos `resultados` e `features_dezena`) e mesmos binários →
mesmos bytes no arquivo de dados e no manifesto, nos dois formatos: sem
carimbo de tempo, metadados fixos, chaves do JSON em ordem fixa. A
codificação das páginas do Parquet depende da versão do crate `parquet`
travada no `Cargo.lock`. A fixture `ml/tests/fixtures/` é regenerada pelo
próprio Rust com `ml/ferramentas/gerar_fixture.py` (bytes iguais em duas
gerações).

### O que o Python confere ao ler (`ler_dataset`)

O ponto de entrada é sempre o manifesto. Em ordem:

1. **Antes de abrir o arquivo de dados** (só o manifesto):
   - recusa manifesto com `trancado` no nome (`manifesto_trancado_<ext>.json`);
   - todos os campos acima presentes e com o tipo certo;
   - `formato` conhecido e manifesto com o nome `manifesto_<formato>.json`;
   - `arquivo` sem `trancado` no nome, só nome (sem pasta), extensão do formato;
   - `trecho` = `busca` (recusa `busca+trancado`);
   - `colunas` exatamente as 16 da tabela acima;
   - **corte contra a tabela do Python** (`CORTES_BUSCA` em
     `ml/rust_spider_ml/dataset.py`, igual a `ultimo_concurso_busca` de
     `src/config.rs`; o pytest confere a igualdade): loteria fora da tabela
     é recusada, e `ultimo_concurso_busca` do manifesto tem de ser igual ao
     da tabela;
   - `ultimo_concurso` ≤ corte da tabela;
   - `feature_version` igual à esperada, se o chamador informar uma.
2. **SHA-256** dos bytes do arquivo igual a `sha256`.
3. **Antes de decodificar** os dados: no Parquet, pelo rodapé, metadados
   `loteria`/`feature_version`/`trecho` iguais aos do manifesto, nomes das
   colunas, e estatísticas de cada row group com `trecho` só `busca` e
   `concurso` ≤ corte (sem estatísticas → recusa); no CSV, nenhum campo
   `,trancado,` nos bytes. É recusa precoce do arquivo do Rust, não
   garantia contra arquivo forjado (outra grafia, estatísticas falsas): essa
   garantia é a conferência linha a linha do passo 4.
4. **Depois de decodificar**: colunas e número de linhas iguais ao
   manifesto; nenhuma linha com `trecho` ≠ `busca`; `feature_version` de
   todas as linhas igual à do manifesto; `saiu` só 0/1; ordem estrita de
   (concurso, dezena, janela); grade completa concursos × dezenas × janelas;
   concursos seguidos e iguais a `primeiro_concurso`…`ultimo_concurso`
   (`concursos` deles); último concurso ≤ corte da tabela; `saiu` igual
   entre as janelas de uma dezena; *K* (soma de `saiu`) igual em todos os
   concursos; uma data por concurso; dataset começando no concurso 1;
   impressão recalculada das dezenas com `saiu` = 1 igual a
   `impressao_resultados_incluidos`.

Qualquer falha levanta `ErroDataset` com a mensagem em português; a
solução é regenerar com `sync dataset`. **A leitura do trancado não existe
no pacote Python**: o par `_trancado` só é gerado para a validação final do
perfil campeão, que roda no Rust (`sync avaliar --trancado`).

### Compatibilidade

- Mudança de campo do manifesto, de coluna, de tipo, de ordem ou de regra
  do formato: **documentar aqui primeiro**, depois o Rust e o Python no
  mesmo commit.
- Mudança na definição de um componente sobe a `feature_version` (o nome do
  arquivo muda junto) e é especificada antes em `docs/PLANEJAMENTO.md`.
- Mudança do corte da busca: `src/config.rs` e `CORTES_BUSCA` juntos (o
  pytest falha se divergirem).
