# Contrato de dados v1

Formatos que o `rust_spider` grava para serem lidos fora do Rust. Este
arquivo é a fonte do contrato: **mudar, acrescentar ou remover um campo é
documentar aqui primeiro**, no mesmo commit da mudança no código.

Ele cobre dois formatos: o **JSON público do app** (`publico/v1/`, gerado
por `sync exportar`; a primeira seção, escrita para quem planeja o
front-end) e o **dataset de ML** (`sync dataset`, lido pelo Python; a
segunda seção). Decisões e motivos: `docs/PLANEJAMENTO.md`, Fase 2.

## JSON público do app (`sync exportar`)

Quatro arquivos JSON por execução (um `index.json` e, por loteria, três
arquivos de dados), gerados só a partir dos bancos: `sync exportar` não
calcula nada, lê o último snapshot do motor (`features_dezena` e `indice`) e
recusa gravar se features ou índice não estiverem em dia.

```bash
cargo run --bin sync -- exportar <loteria|todas> [--saida publico] [--db caminho] [--perfis perfis]
```

### Onde o app lê (URL base)

```
https://raw.githubusercontent.com/edxgar48/rust_spider_dados/main/v1/
```

Os arquivos ficam em `<base>index.json`, `<base>quina/ultimo.json` etc. A
pasta `publico/v1/` do repositório do código vira a raiz `v1/` do repositório
de dados `edxgar48/rust_spider_dados` (público, branch `main`).

- O GitHub guarda cache de alguns minutos nas URLs `raw`: o app não deve
  esperar ver um concurso novo no mesmo minuto em que ele é publicado.
- A URL base é a **única** coisa que muda se o destino trocar no futuro
  (R2, Pages, S3...): caminhos e conteúdo dos arquivos são os mesmos. Mantenha
  a URL base numa configuração do app, não espalhada pelo código.
- **Estado atual**: `sync publicar` (tarefa 2.4) copia o `publico/v1/` já
  exportado para o clone do repositório de dados e faz o push; a URL acima
  responde depois da primeira publicação. A publicação não muda nenhum campo
  ou caminho do contrato.

### Estrutura de pastas

```
v1/
├── index.json                  # ponto de entrada: loterias, último concurso, hash de cada arquivo
├── lotofacil/
│   ├── ultimo.json             # último concurso sorteado
│   ├── tabela_geral.json       # snapshot do próximo concurso: componentes, índice e posição por dezena
│   └── historico.json          # todos os concursos, compacto
├── megasena/   (mesmos três arquivos)
└── quina/      (mesmos três arquivos)
```

Slugs: `lotofacil`, `megasena`, `quina` (os de `LoteriaConfig::slug`). O
caminho de cada arquivo está no `index.json` (campo `caminho`, relativo a
`v1/`); o app não precisa montar caminhos por conta própria.

### Como o app deve usar o `index.json`

1. Baixar `v1/index.json` (sempre; é pequeno).
2. Conferir `schema_version` = `1`. Se for outro número, o app não entende
   este formato (ver "Compatibilidade").
3. Para cada arquivo que o app usa, comparar o `sha256` do índice com o
   guardado da última vez: igual → o arquivo local vale; diferente → baixar de
   novo `<base><caminho>`.
4. Depois de baixar, conferir o SHA-256 dos bytes recebidos contra o `sha256`
   do índice (e `bytes` contra o tamanho). Se não bater (cache do GitHub
   entre o índice e o arquivo, ou gravação interrompida, ver "Garantias"),
   descartar e tentar de novo mais tarde, sem trocar os dados locais.
5. `ultimo_concurso` e `data` de cada loteria permitem mostrar "último
   concurso" sem baixar nada além do índice.

O hash é calculado sobre os bytes exatos do arquivo: o app deve calculá-lo
sobre os bytes recebidos, antes de interpretar o JSON.

### Convenções comuns

- Todo arquivo é um objeto JSON com `schema_version` (inteiro, hoje `1`) como
  primeira chave. Os de loteria têm `loteria` (slug) logo depois.
- **Datas**: texto `AAAA-MM-DD` (data do sorteio). A única exceção é
  `gerado_em` do `index.json` (ISO-8601 em UTC, `AAAA-MM-DDTHH:MM:SSZ`).
- **Dezenas**: inteiros (nunca texto), sempre em **ordem crescente**; o
  contrato não guarda a ordem em que as bolas saíram.
- **Ausência**: `null` explícito. As chaves documentadas no `v1` inicial estão
  sempre presentes (nunca somem). Campos acrescentados depois são opcionais
  (ver "Compatibilidade dentro de `v1`"): podem faltar ou ser `null`, e o app
  trata ausência como `null`. O app ignora campos desconhecidos.
- Na coluna "Origem" das tabelas, os nomes são do código Rust
  (`src/exportacao/mod.rs`, salvo indicação).

### `index.json`

Ordem das chaves e significado. Origem: `exportacao::index_json`.

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `schema_version` | inteiro | não | Versão do esquema, `1` (`exportacao::SCHEMA_VERSION`). |
| `gerado_em` | texto | não | Quando o índice foi gravado: ISO-8601 UTC com segundos, `Z` no fim. **Único campo não determinístico de todo o contrato**: muda a cada `sync exportar`, mesmo sem dados novos. Não use para decidir se há novidade; use `sha256`. |
| `loterias` | objeto | não | Uma chave por loteria do config, **na ordem do config** (`lotofacil`, `megasena`, `quina`). |
| `loterias.<slug>` | objeto | **sim** | `null` se a loteria ainda não tem arquivos na saída (por exemplo, só uma loteria foi exportada até agora). O app deve tolerar. |

Campos de `loterias.<slug>` (quando não é `null`), na ordem:

| Campo | Tipo | `null`? | Significado | Origem |
|---|---|---|---|---|
| `ultimo_concurso` | inteiro | não | Número do último concurso sorteado; igual ao `concurso` de `ultimo.json`. | `LoteriaNoIndice::ultimo_concurso` (lido do `ultimo.json` no disco) |
| `data` | texto | não | Data desse concurso, `AAAA-MM-DD`; igual à `data` de `ultimo.json`. | `LoteriaNoIndice::data` |
| `arquivos` | objeto | não | Três chaves, nesta ordem: `ultimo`, `tabela_geral`, `historico`. | `Arquivo::chave` |

Chaves de `arquivos`, nesta ordem:

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `ultimo` | objeto | não | Descritor de `ultimo.json`. |
| `tabela_geral` | objeto | não | Descritor de `tabela_geral.json`. |
| `historico` | objeto | não | Descritor de `historico.json`. |

Cada descritor tem os campos:

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `caminho` | texto | não | Caminho relativo a `v1/`, separador `/` (`quina/ultimo.json`). A URL do arquivo é a URL base + `caminho`. |
| `sha256` | texto | não | SHA-256 dos bytes do arquivo, 64 dígitos hexadecimais minúsculos. |
| `bytes` | inteiro | não | Tamanho do arquivo em bytes. |

O índice descreve o que está **no disco** da saída no momento em que foi
gravado: `sync exportar quina` regrava só os arquivos da Quina, mas o índice
refeito traz os hashes de todas as loterias presentes.

### `ultimo.json`

Último concurso sorteado. Origem: `exportacao::ultimo_json`.

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `schema_version` | inteiro | não | `1`. |
| `loteria` | texto | não | Slug. |
| `concurso` | inteiro | não | Número do último concurso sorteado. |
| `data` | texto | não | Data do sorteio, `AAAA-MM-DD`. |
| `dezenas` | lista de inteiros | não | Dezenas sorteadas, crescentes, numa linha só: 15 (Lotofácil), 6 (Mega-Sena) ou 5 (Quina). |

### `historico.json`

Histórico inteiro, em ordem crescente de concurso, **um concurso por linha**
(para o arquivo ser legível e para diffs em git mostrarem praticamente só os
concursos novos: a linha do antigo último ganha a vírgula). Origem: `exportacao::historico_json`.

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `schema_version` | inteiro | não | `1`. |
| `loteria` | texto | não | Slug. |
| `concursos` | lista de listas | não | Um item por concurso, do primeiro ao último, sem buraco e sem repetição. |

Cada item de `concursos` é uma lista de **3 posições fixas** (formato
compacto, sem nomes de chave):

| Posição | Tipo | Significado |
|---|---|---|
| `[0]` | inteiro | Número do concurso. |
| `[1]` | texto | Data do sorteio, `AAAA-MM-DD`. |
| `[2]` | lista de inteiros | Dezenas sorteadas, crescentes. |

O último item é sempre o concurso de `ultimo.json`. É o maior arquivo (cerca
de 150 a 340 KB hoje) e só cresce por acréscimo no fim.

### `tabela_geral.json`

Ranking das dezenas para o **próximo** concurso (o ainda não sorteado), pelo
índice de força do perfil ativo da loteria (`LoteriaConfig::perfil_ativo`,
hoje `padrao_v1` nas três). É o snapshot de `features_dezena` e `indice` do
concurso `último + 1`, o mesmo do `sync tabela`; usa só concursos até o
`ultimo_sorteado`. Origem: `exportacao::tabela_geral_json`.

Cabeçalho, na ordem:

| Campo | Tipo | `null`? | Significado | Origem |
|---|---|---|---|---|
| `schema_version` | inteiro | não | `1`. | `SCHEMA_VERSION` |
| `loteria` | texto | não | Slug. | `TabelaConcurso::loteria` |
| `perfil` | texto | não | Nome do perfil de pesos usado (`padrao_v1`). | `TabelaConcurso::perfil` |
| `hash_toml` | texto | não | SHA-256 (64 hex minúsculos) do `.toml` do perfil, com fim de linha LF e sem BOM. Identifica **exatamente** quais pesos geraram o ranking: se mudar, o ranking pode mudar. | `DadosExportacao::hash_toml` |
| `janela` | inteiro | não | Janela de concursos das features que o perfil usa: `10`, `50` ou `100`; `0` significa **histórico total**. Hoje `50` nas três loterias. | `TabelaConcurso::janela` (`Perfil::janela`) |
| `normalizacao` | texto | não | Como o índice normaliza cada componente entre as dezenas do concurso: `zscore` ou `minmax` (ver "Valores enumerados"). | `normalizacao_no_contrato` |
| `feature_version` | inteiro | não | Versão da definição dos componentes (`motor::FEATURE_VERSION`, hoje `1`). Mudar a definição de um componente sobe este número. | `DadosExportacao::feature_version` |
| `concurso` | inteiro | não | **Próximo concurso** (ainda não sorteado) a que a tabela se refere = `ultimo_sorteado + 1`. | `TabelaConcurso::concurso` |
| `ultimo_sorteado` | inteiro | não | Último concurso já sorteado; é o último que entrou nos cálculos e o `concurso` de `ultimo.json`. | `TabelaConcurso::ultimo_sorteado` |
| `dezenas` | lista de objetos | não | Uma entrada por dezena da loteria (25, 60 ou 80), **em ordem de posição** (1 = mais forte). | |

Entrada de cada dezena, na ordem das chaves (origem: `entrada_da_dezena`):

| Campo | Tipo | `null`? | Significado |
|---|---|---|---|
| `dezena` | inteiro | não | Número da dezena. |
| `posicao` | inteiro | não | Lugar no ranking, `1` a N (N = quantidade de dezenas da loteria), pelo `total` decrescente. Desempate: menor dezena à frente. Sem repetição. |
| `total` | número | não | Índice de força: soma dos 9 componentes normalizados vezes os pesos do perfil. **Sempre com 9 casas decimais fixas** (`3.518532681`, `0.000000000`; nunca expoente, nunca `-0`). Só compara dezenas do mesmo arquivo; não é probabilidade e a escala muda entre normalizações e perfis. |
| `variacao_posicao` | inteiro | **sim** | Posições ganhas em relação ao concurso anterior: `posição anterior − posição atual`. **Positivo = subiu** no ranking, negativo = caiu, `0` = ficou. `null` só quando não há concurso anterior para comparar (primeiro concurso do histórico); na prática não aparece nos arquivos reais. |
| `frequencia` | inteiro | não | Componente: quantas vezes a dezena saiu nos concursos da janela (`0` se não saiu). |
| `maior_espaco` | inteiro | **sim** | Componente: maior espaço entre duas aparições seguidas dentro da janela. Espaço = `b − a − 1` (concursos que ficaram sem sair entre elas; `0` se saiu em concursos seguidos). `null` com menos de 2 aparições na janela. |
| `maior_espaco_ocorrencias` | inteiro | **sim** | Componente: quantas vezes esse maior espaço ocorreu. `null` quando `maior_espaco` é `null`. |
| `menor_espaco` | inteiro | **sim** | Componente: menor espaço entre aparições seguidas na janela. `null` com menos de 2 aparições. |
| `menor_espaco_ocorrencias` | inteiro | **sim** | Componente: quantas vezes o menor espaço ocorreu. `null` quando `menor_espaco` é `null`. |
| `soma_variacoes` | inteiro | **sim** | Componente: soma de \|espaço atual − espaço anterior\| entre espaços seguidos (mede o quanto o ritmo de aparição oscila). `null` com menos de 3 aparições na janela (menos de 2 espaços). |
| `espaco_final` | inteiro | **sim** | Componente: concursos desde a última aparição (`(último sorteado) − (concurso da última aparição)`). **Não é limitado pela janela** (conta até a última vez que saiu em qualquer época). `null` só se a dezena nunca saiu. |
| `repeticoes_total` | inteiro | não | Componente: vezes em que a dezena saiu em dois concursos seguidos dentro da janela. |
| `repeticao_atual` | inteiro | não | Componente: sequência corrente de concursos seguidos em que a dezena saiu, contada para trás a partir do último sorteado (`0` se não saiu nele); no máximo o tamanho da janela. |
| `evidencia_grau` | texto | **sim** | Quanta evidência os dados dão de que a chance dessa dezena difere do acaso (K/N). Um de cinco graus, ver abaixo. `null` só no concurso 1 (sem histórico); na prática não aparece. |
| `evidencia_direcao` | texto | **sim** | Para que lado a evidência aponta: `vies` ou `acaso`. `null` junto com `evidencia_grau`. |

Os 9 componentes (`frequencia` ... `repeticao_atual`) são os valores brutos,
na janela do perfil, **antes** de normalizar; são os mesmos de
`features_dezena`, e a definição completa está em `docs/PLANEJAMENTO.md`,
"Especificação das features". Os 6 que podem ser `null` significam "o
histórico da janela não tem aparições suficientes para calcular", **nunca**
zero. No índice, o `null` entra como valor neutro (`0` = a média no
`zscore`, `0.5` no `minmax`). Componente sem variação no concurso (todos
`null`, ou máximo = mínimo entre as dezenas) dá o neutro a todas as dezenas.

A evidência da dezena (A) usa o histórico inteiro e a priori principal (5 %)
**no ponto no tempo**: só concursos até `ultimo_sorteado`. Ela é independente
do índice de força (não usa o `total`). A evidência do próprio índice (B)
**não** está nesta versão (ver "Compatibilidade").

### Valores enumerados

Identificadores do contrato: ASCII, minúsculos, sem acento nem espaço. Não são
os rótulos de exibição do CSV (`muito fraca`, `a favor de viés`...), para que
mudar um rótulo de tela não mude o contrato. O app mapeia cada valor para o
texto que quiser mostrar; um teste fixa a tabela.

`evidencia_grau`, do mais fraco ao mais forte (`docs/GUIA.md`, "O fator de
Bayes e os cinco graus"). O grau mede **o quanto os dados convencem** de uma
das duas hipóteses (a dezena sai com a chance exata K/N, ou com uma chance
um pouco diferente), nunca o tamanho do viés:

| Valor | Significado (F = quantas vezes os dados são mais prováveis sob a hipótese favorecida) |
|---|---|
| `muito_fraca` | F menor que 3. Quase nada a concluir. |
| `fraca` | F de 3 a 10. |
| `moderada` | F de 10 a 30. |
| `forte` | F de 30 a 100. |
| `muito_forte` | F de 100 ou mais. |

`evidencia_direcao`:

| Valor | Significado |
|---|---|
| `vies` | Os dados favorecem "a chance dessa dezena é um pouco diferente de K/N" (fator de Bayes maior que 1). |
| `acaso` | Os dados favorecem "a chance é exatamente K/N" (fator de Bayes menor ou igual a 1). |

Leia sempre os dois juntos: `forte` + `vies` é bem diferente de `forte` +
`acaso`. Dois cuidados de leitura, para o texto do app não prometer demais:

- **`muito_fraca` + `acaso` é o máximo que os dados podem dizer a favor do
  acaso** na Mega-Sena e na Quina, e **`fraca` + `acaso` já é o máximo na
  Lotofácil**, com o histórico atual (o acaso é uma chance exata; a evidência
  a favor dele tem teto baixo). Não significa "faltam dados" nem
  "inconclusivo".
- Grau alto + `vies` num histórico de milhares de concursos pode ser um desvio
  minúsculo. Não é "essa dezena vai sair" e não é previsão.

`normalizacao` (do perfil; igual em todas as dezenas do arquivo):

| Valor | Significado |
|---|---|
| `zscore` | Cada componente é normalizado entre as dezenas do concurso por (valor − média) / desvio padrão populacional; o `null` vira `0` (a média). Hoje o valor de `padrao_v1` nas três loterias. |
| `minmax` | Cada componente é levado à escala 0 a 1 entre as dezenas do concurso; o `null` vira `0.5`. |

O app não precisa recalcular nada com isso: serve para mostrar de onde vem o
`total` e para detectar troca de perfil (`perfil` e `hash_toml`).

### Formatação e garantias

- **Texto**: ASCII puro (nenhum byte acima de 127, nenhum acento); UTF-8 sem
  BOM; fim de linha `\n`, com um `\n` depois do último `}`.
- **Indentação**: 2 espaços, um campo por linha, exceto: listas de dezenas
  (`[2, 7, 50, 61, 79]`, numa linha) e **`historico.json`, com um concurso
  por linha**.
- **Ordem das chaves**: a das tabelas acima, sempre a mesma (não ordenada
  alfabeticamente). O app não deve depender da ordem, mas ela é estável.
- **Determinismo**: o mesmo banco gera os mesmos bytes. A única exceção é
  `gerado_em` no `index.json`; por isso o `sha256` dos arquivos de dados só
  muda quando os dados mudam. Mesmos bytes ⇒ mesmo hash ⇒ o app não baixa de
  novo (e o git do repositório de dados não vê mudança).
- **Gravação atômica por arquivo**: cada arquivo é gravado num temporário e
  renomeado; ninguém lê um arquivo pela metade. O `index.json` é o último a
  ser gravado.
- **Limitação**: a troca de **vários** arquivos não é atômica. Uma falha no
  meio das renomeações pode deixar arquivos novos e antigos misturados
  enquanto o `index.json` ainda é o antigo. O próximo `sync exportar` corrige.
  Por isso o consumidor sempre confere o `sha256` de cada arquivo contra o
  índice (passo 4 de "Como o app deve usar o `index.json`"); conferir os hashes
  do índice contra o disco antes de publicar está previsto para a 2.4.
- **Tudo ou nada**: `exportar todas` não grava nada se uma loteria falhar;
  `exportar <loteria>` regrava só os arquivos dela.

### Compatibilidade dentro de `v1`

- Dentro de `v1` só se **acrescenta campo opcional**. **Campo opcional =
  pode faltar ou ser `null`**: arquivos gerados antes do campo existir não o
  têm, e depois dele pode vir `null` quando não se aplica. O app trata ausência
  como `null`. Não se remove, renomeia, reordena de forma
  incompatível nem muda o tipo ou o significado de um campo existente.
- **O app deve ignorar campos desconhecidos** (em qualquer objeto) e não
  deve depender da ordem das chaves.
- Mudança incompatível ⇒ nova pasta `v2/` publicada **em paralelo** a `v1/`
  (o app antigo continua lendo `v1/`). O `schema_version` de todo arquivo
  acompanha a pasta.
- **Campo previsto para o futuro**: a evidência do próprio índice de força (B
  do `sync evidencia`: o índice do perfil prevê melhor que o acaso?). Fica
  fora por ora porque custa de 1 a 4 s por loteria; se entrar, será como campo
  opcional, com local e formato a decidir; o app deve ignorar campos
  desconhecidos e não assumir que o cabeçalho tem só as chaves de hoje.
- Todo campo novo é documentado aqui, com tipo, significado e exemplo, no
  mesmo commit que o introduz no código.

### Exemplo real

Gerado em 2026-10-12 por `sync exportar`, numa cópia dos bancos, com os
dados que a coleta tinha nessa data: **Lotofácil** até o concurso 3799
(2026-10-07), **Mega-Sena** até o 3067 (2026-10-06), **Quina** até o 7137
(2026-10-07). Os JSON abaixo são trechos literais desses arquivos; os cortes
são marcados fora do JSON.

`index.json` (inteiro; `gerado_em` é o único valor que muda a cada execução):

```json
{
  "schema_version": 1,
  "gerado_em": "2026-10-12T13:57:29Z",
  "loterias": {
    "lotofacil": {
      "ultimo_concurso": 3799,
      "data": "2026-10-07",
      "arquivos": {
        "ultimo": {
          "caminho": "lotofacil/ultimo.json",
          "sha256": "3551f78e1416361271f5f83c399d07796a08d322207a6ecb04ec687d8a134cba",
          "bytes": 165
        },
        "tabela_geral": {
          "caminho": "lotofacil/tabela_geral.json",
          "sha256": "e0f022638ecc13cd0dd2a65fbd3b0b727c4b2e280cb504d2015cc872d307b803",
          "bytes": 11315
        },
        "historico": {
          "caminho": "lotofacil/historico.json",
          "sha256": "cdfba36acfce19e2819c023e26982a588afba7afb2cafb98e6a5d0cae7c4e2e7",
          "bytes": 312870
        }
      }
    },
    "megasena": {
      "ultimo_concurso": 3067,
      "data": "2026-10-06",
      "arquivos": {
        "ultimo": {
          "caminho": "megasena/ultimo.json",
          "sha256": "b4bf843be8a99732c161b88cba4f931bf8b661466b56702651d8d4091647eac7",
          "bytes": 132
        },
        "tabela_geral": {
          "caminho": "megasena/tabela_geral.json",
          "sha256": "cc8a27b86faee006356e2c9e78a1b7ebff92daea68db4e9571f6409ef0b5df71",
          "bytes": 26922
        },
        "historico": {
          "caminho": "megasena/historico.json",
          "sha256": "e06fa4405d49b3e465c9d6afb34d9c74bc91af627cefe69f07814c6f747f3832",
          "bytes": 155705
        }
      }
    },
    "quina": {
      "ultimo_concurso": 7137,
      "data": "2026-10-07",
      "arquivos": {
        "ultimo": {
          "caminho": "quina/ultimo.json",
          "sha256": "bfc89aacfd5cb12e46cb8c18387662ed75b08520c52d80e0df8bd51db64c3147",
          "bytes": 125
        },
        "tabela_geral": {
          "caminho": "quina/tabela_geral.json",
          "sha256": "b66097d78395223668affd7d1af1718a1b532b349b3b4ca09d1f079647caf5de",
          "bytes": 35993
        },
        "historico": {
          "caminho": "quina/historico.json",
          "sha256": "599396da7529ba4e944a1fc1ec8af8469e2ceb30cdf81ce2472cc4cbbc75fceb",
          "bytes": 337525
        }
      }
    }
  }
}
```

`quina/ultimo.json` (inteiro, concurso 7137 de 2026-10-07):

```json
{
  "schema_version": 1,
  "loteria": "quina",
  "concurso": 7137,
  "data": "2026-10-07",
  "dezenas": [2, 7, 50, 61, 79]
}
```

`quina/tabela_geral.json` (cabeçalho e 3 das 80 dezenas: as posições 1, 2 e
5, a última com componentes `null`. **Corte**: no arquivo real as posições 3
e 4 ficam entre a 2ª e a 3ª entradas abaixo, e as posições 6 a 80 vêm depois
da última, no mesmo formato, antes do `]` e do `}` finais; o bloco é um
trecho, não JSON completo):

```json
{
  "schema_version": 1,
  "loteria": "quina",
  "perfil": "padrao_v1",
  "hash_toml": "5bea2957a4aa375ffaf102aab3ca21d39252213f1a133b4f3538589df6d2c873",
  "janela": 50,
  "normalizacao": "zscore",
  "feature_version": 1,
  "concurso": 7138,
  "ultimo_sorteado": 7137,
  "dezenas": [
    {
      "dezena": 43,
      "posicao": 1,
      "total": 3.518532681,
      "variacao_posicao": 0,
      "frequencia": 7,
      "maior_espaco": 11,
      "maior_espaco_ocorrencias": 1,
      "menor_espaco": 0,
      "menor_espaco_ocorrencias": 2,
      "soma_variacoes": 35,
      "espaco_final": 2,
      "repeticoes_total": 2,
      "repeticao_atual": 0,
      "evidencia_grau": "muito_fraca",
      "evidencia_direcao": "acaso"
    },
    {
      "dezena": 26,
      "posicao": 2,
      "total": 2.870609291,
      "variacao_posicao": 1,
      "frequencia": 4,
      "maior_espaco": 8,
      "maior_espaco_ocorrencias": 1,
      "menor_espaco": 0,
      "menor_espaco_ocorrencias": 1,
      "soma_variacoes": 8,
      "espaco_final": 34,
      "repeticoes_total": 1,
      "repeticao_atual": 0,
      "evidencia_grau": "fraca",
      "evidencia_direcao": "vies"
    },
    {
      "dezena": 56,
      "posicao": 5,
      "total": 2.247086713,
      "variacao_posicao": 2,
      "frequencia": 0,
      "maior_espaco": null,
      "maior_espaco_ocorrencias": null,
      "menor_espaco": null,
      "menor_espaco_ocorrencias": null,
      "soma_variacoes": null,
      "espaco_final": 65,
      "repeticoes_total": 0,
      "repeticao_atual": 0,
      "evidencia_grau": "muito_fraca",
      "evidencia_direcao": "vies"
    }
  ]
}
```

Lendo a dezena 56: é a 5ª mais forte, subiu 2 posições, não
saiu nos últimos 50 concursos (`frequencia` 0, então os cinco componentes de
espaço são `null`), mas faz 65 concursos que não sai (`espaco_final` 65,
maior que a janela).

`quina/historico.json` (começo do arquivo e, em seguida, o fim; o arquivo
real tem 7137 linhas em `concursos`):

```json
{
  "schema_version": 1,
  "loteria": "quina",
  "concursos": [
    [1, "1994-03-13", [25, 45, 60, 76, 79]],
    [2, "1994-03-17", [13, 30, 58, 63, 64]],
```

Corte: 7133 linhas (concursos 3 a 7135). O arquivo termina assim:

```
    [7136, "2026-10-06", [13, 14, 30, 53, 60]],
    [7137, "2026-10-07", [2, 7, 50, 61, 79]]
  ]
}
```


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
