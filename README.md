# rust_spider — resultados de loterias da Caixa

Baixa o arquivo de resultados publicado pela Caixa, extrai os concursos e
grava em um banco SQLite (e exporta em JSON), para alimentar outro sistema.

**Estado: funcionando ponta a ponta para Lotofácil, Mega-Sena e Quina**,
validado contra o site real em 2026-09-29 (3791, 3063 e 7129 concursos
extraídos e gravados corretamente, respectivamente).

## Por que baixar um arquivo em vez de raspar a página renderizada

Tentativas anteriores neste projeto tentavam ler os números direto do HTML
renderizado por JavaScript na página da loteria (`page.eval` procurando
seletores como `li.dezena`). Isso se mostrou frágil: os seletores mudavam, e
o site passou a bloquear o acesso automatizado (ver histórico de commits).

A página tem um botão (`#btnResultados`) que dispara o download do arquivo
oficial de resultados. **Apesar do link parecer apontar para um `.htm`, o
arquivo que a Caixa realmente serve é uma planilha Excel (`.xlsx`)** — um
ZIP OOXML de verdade, não HTML. Baixamos com a extensão que o próprio
navegador reconhece (`download.suggested_filename()`) e lemos com um parser
de planilha de verdade (`calamine`), em vez de tentar interpretar como HTML.

O layout de colunas da planilha (colunas oficiais, na ordem em que a Caixa
publica): `Concurso, Data Sorteio, Bola1..Bola15, Observação, Estimativa
Prêmio, ...`. Isso já está refletido em `config.rs`.

Baixar essa planilha inteira sempre que só se quer o concurso mais recente é
caro (milhares de linhas). Por isso existe também `atualizar`, que lê o
último resultado direto do próprio corpo da página (o mesmo painel que
mostra "Concurso NNNN (DD/MM/AAAA)" e as dezenas sorteadas) e só grava se for
um concurso novo — sem tocar na planilha. Curiosamente, Lotofácil ainda usa
o template AngularJS antigo da Caixa (`ng-binding`/`ng-repeat`) enquanto
Mega-Sena e Quina já usam uma versão mais nova com IDs fixos
(`#tituloResultadoConcurso`, `#ulDezenas`) — por isso cada loteria tem seus
próprios `seletor_concurso_data`/`seletor_dezenas` em `config.rs`.

## Arquitetura

```
src/
├── config.rs      # por loteria: URL da página, seletor do botão, layout das colunas
├── downloader.rs  # Playwright: abre a página, clica no botão, captura o evento de download
├── parser.rs      # calamine: lê a planilha (.xlsx) e extrai os concursos
├── db.rs          # rusqlite: schema, upsert por (loteria, concurso) e tabelas derivadas
├── motor/         # features por dezena e janela (série temporal) e índice de força, puro: sem I/O nem SQL
├── perfil.rs      # perfis de pesos (`perfis/<loteria>/<nome>.toml`): leitura, validação e hash
├── orquestracao.rs # `sync features` e `sync indice`: validação + motor + gravação numa transação
├── tempo.rs       # data/hora ISO-8601 UTC sem crate de calendário (`perfis.criado_em`)
├── model.rs       # struct Resultado
├── error.rs       # erros específicos do domínio (thiserror)
└── bin/sync.rs    # CLI (clap) que orquestra tudo
```

## Uso

```bash
# baixa do site, parseia e grava no banco
cargo run --bin sync -- baixar lotofacil

# usa um seletor diferente do configurado, sem recompilar
cargo run --bin sync -- baixar lotofacil --seletor "#btnResultados"

# roda com navegador visível, para depurar bloqueio/seletor
cargo run --bin sync -- baixar lotofacil --visivel

# processa um arquivo já baixado manualmente (sem abrir navegador)
cargo run --bin sync -- importar lotofacil ./downloads/lotofacil_123.xlsx

# lista o que já está no banco, em JSON
cargo run --bin sync -- listar lotofacil

# raspa só o último resultado da página e atualiza o banco (sem baixar a planilha inteira)
cargo run --bin sync -- atualizar lotofacil

# confere a integridade do banco
cargo run --bin sync -- validar lotofacil

# lista os concursos que faltam e fecha um deles à mão
cargo run --bin sync -- buracos lotofacil
cargo run --bin sync -- inserir lotofacil 3793 --data 2026-09-30 --dezenas 1,2,3,...

# calcula as features por dezena e janela e grava em `features_dezena`
cargo run --bin sync -- features quina
cargo run --bin sync -- features quina --completo   # ignora o estado salvo e recalcula do zero

# calcula o índice de força com um perfil de pesos e grava em `indice` (rode `features` antes)
cargo run --bin sync -- indice quina --perfil padrao_v1
cargo run --bin sync -- indice quina --perfil padrao_v1 --perfis outra/pasta   # perfis fora de `perfis/`

# `todas` no lugar do slug percorre todas as loterias configuradas
cargo run --bin sync -- validar todas
cargo run --bin sync -- atualizar todas
```

`todas` vale em `baixar`, `atualizar`, `validar` e `buracos`, na ordem de
`LOTERIAS` em `config.rs`. Os bancos não são juntados: o comando roda em cada
banco padrão, um por vez (por isso `--db` e `--seletor` não podem ser
combinados com `todas`). Uma loteria que falha não interrompe as demais: no
fim sai um resumo (ok/falhou por loteria) e o comando termina com erro se
alguma falhou. O cabeçalho de cada loteria e o resumo vão para a saída
padrão; o erro de cada loteria vai sempre para a saída de erro. `importar`,
`inserir`, `listar`, `features` e `indice` aceitam só uma loteria. `listar`,
`validar`, `buracos`, `features` e `indice` falham com "banco não encontrado"
em vez de criar um banco vazio.

`atualizar` compara o concurso lido na página com `MAX(concurso)` já salvo no
banco daquela loteria: se não houver novidade, não grava nada; se o salto for
maior que 1 (ex.: ficou alguns dias sem rodar e perdeu um concurso no meio),
avisa e sugere rodar `baixar` para preencher o histórico completo — raspagem
só traz o mais recente, não faz backfill de concursos intermediários.

Buracos na numeração nunca impedem uma gravação (só geram aviso), mas
reprovam o `validar` até serem fechados. Para fechá-los um a um, `buracos`
lista os concursos que faltam com as datas dos vizinhos, e `inserir` grava um
concurso digitado à mão. Cada inserção também vai para
`cache/manuais_<loteria>.json` (versionado no git, em ordem de data de
sorteio): `importar`/`baixar` reaplicam essas entradas num banco recriado e,
quando a planilha oficial passa a trazer o concurso, removem a entrada se for
igual ou avisam se for diferente.

`features` calcula, para cada concurso *t*, o snapshot de cada dezena em
cada janela (10, 50, 100 e total, gravada como `janela = 0`) usando só os
concursos até *t−1* (definições em `docs/PLANEJAMENTO.md`, "Especificação das
features"), e grava na tabela `features_dezena` do próprio banco da loteria.
Também grava o snapshot do próximo concurso, ainda não sorteado (último + 1,
com `saiu` = `NULL`), que é substituído quando esse concurso entra no banco.
O estado do cálculo fica em `motor_estado`: na execução seguinte só os
concursos novos são processados. O cálculo recomeça do zero (apagando e
recriando as linhas) quando não há estado salvo, quando a versão das fórmulas
(`feature_version`) mudou, quando algum resultado já incorporado foi corrigido,
quando `features_dezena` não bate com o estado ou com `--completo`. Antes de
calcular roda a mesma validação do `validar`: havendo erro (buraco incluso), o
comando recusa sem gravar nada e orienta usar `buracos`/`inserir`; datas fora de
ordem são só avisos. Tudo é gravado numa única transação (tudo ou nada). No
fim, imprime o modo (completo ou incremental, com o motivo), os concursos
incorporados, as linhas gravadas e o próximo concurso. `features_dezena` e
`motor_estado` são derivadas: podem ser apagadas e regeneradas a qualquer
momento a partir de `resultados`.

`indice <loteria> --perfil <nome>` calcula o índice de força de cada dezena
em cada concurso (total ponderado, posição de 1 a N e variação de posição em
relação ao concurso anterior; regras em `docs/PLANEJAMENTO.md`, "Índice de
força") com o perfil `perfis/<loteria>/<nome>.toml` (outra pasta com
`--perfis`), e grava na tabela `indice`, uma linha por (concurso, dezena,
perfil). Usa só as linhas de `features_dezena` da janela do perfil, inclusive
o snapshot do próximo concurso (o da tabela geral do app); `saiu` nunca entra
no cálculo. `--perfil` é obrigatório e cada execução trata um perfil e uma
loteria. A cada execução o índice inteiro do perfil é apagado e calculado de
novo (os outros perfis não são tocados), numa única transação junto com o
registro do perfil na tabela `perfis` — que guarda o `.toml`, o hash SHA-256 e
o momento do primeiro registro (`criado_em`, ISO-8601 UTC). Perfil já
registrado com o mesmo nome e outro conteúdo é erro: perfis são imutáveis,
crie a versão seguinte (`padrao_v2`). O comando recusa, sem gravar nada (nem
criar tabelas):

- perfil de outra loteria (o `loteria` do `.toml` não é a loteria do banco):
  use o perfil de `perfis/<loteria>/`;
- banco sem resultados, `features_dezena` vazia, features desatualizadas (o
  snapshot mais recente não é o último concurso + 1), calculadas com outros
  resultados (um resultado antigo corrigido depois do último `features`: a
  impressão de `resultados` não bate com a de `motor_estado`) ou de outra
  versão das fórmulas na janela do perfil: rode `features` antes;
- `features_dezena` incoerente na janela do perfil (a série `1 … último + 1`
  com concurso faltando, ou com dezenas faltando ou fora do universo da
  loteria — só acontece se a tabela foi mexida por fora): regenere com
  `sync features <loteria> --completo`.

`indice` é derivada: pode ser apagada e regenerada a qualquer momento a partir
de `features_dezena` e dos `.toml`. `perfis` também é recriada pelo próprio
`indice`, mas apagá-la perde o `criado_em` original de cada perfil (o novo
registro passa a valer como o primeiro).

Por padrão cada loteria grava em um banco SQLite separado —
`resultados_<loteria>.db` (ex.: `resultados_lotofacil.db`,
`resultados_megasena.db`) — para não misturar dados de loterias diferentes
no mesmo arquivo. Os downloads vão para `downloads/`. Ambos configuráveis
via `--db` e `--dir`, e ambos ignorados pelo git.

## Estado atual / o que falta confirmar

- **Lotofácil, Mega-Sena e Quina**: seletor de download (`#btnResultados`),
  layout de colunas da planilha e seletores de raspagem da página (`atualizar`)
  confirmados contra o site real.
- Para adicionar outra loteria da Caixa, o caminho mais rápido é assumir o
  mesmo `#btnResultados` e o mesmo layout de colunas, e só ajustar se o
  `importar`/`baixar` acusar layout inesperado (ver item abaixo).
- Se `importar`/`baixar` disser que nenhuma linha bateu com o layout
  configurado, o programa já imprime no stderr um exemplo de linha crua com
  o índice de cada célula — use isso para corrigir `coluna_concurso`,
  `coluna_data` ou `primeira_coluna_dezena` em `config.rs`.

Para adicionar outra loteria, basta um novo `LoteriaConfig` na lista
`LOTERIAS` em `config.rs`.

## Dependências

```bash
cargo build
playwright install   # instala os navegadores usados pelo Playwright
```

`rusqlite` usa a feature `bundled`, então não é preciso instalar SQLite no
sistema — mas é necessário ter um compilador C disponível (neste ambiente já
havia um funcionando).
