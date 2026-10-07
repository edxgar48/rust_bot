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
├── avaliacao/     # índice contra o acaso (log loss, Brier, top-k, walk-forward, bootstrap) e testes de aleatoriedade, puro
├── registros.rs   # registro de uso do conjunto trancado (`registros/trancado_<loteria>.json`, versionado)
├── perfil.rs      # perfis de pesos (`perfis/<loteria>/<nome>.toml`): leitura, validação e hash
├── orquestracao.rs # `sync features`, `indice`, `avaliar`, `aleatoriedade`, `tabela` e `dataset`: validação + motor/avaliação + banco
├── tabela.rs      # CSV de planilha da tabela geral (`sync tabela`)
├── dataset.rs     # dataset de ML (Parquet/CSV) e manifesto (`sync dataset`), sem banco
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

# raspa só o último resultado da página e atualiza o banco (sem baixar a planilha
# inteira, a não ser que falte concurso no meio: ver "salto" abaixo)
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

# avalia o índice de um perfil contra o acaso no trecho de busca (rode `indice` antes; só lê o banco)
cargo run --bin sync -- avaliar quina --perfil padrao_v1
cargo run --bin sync -- avaliar quina --perfil padrao_v1 --k 10 --testados 3   # outro k; Bonferroni por 3 testados
cargo run --bin sync -- avaliar quina --perfil padrao_v1 --trancado            # conjunto TRANCADO: uso único por perfil

# testes de aleatoriedade do histórico inteiro (qui-quadrado e runs por dezena)
cargo run --bin sync -- aleatoriedade todas

# tabela geral em CSV (Excel: `;` e vírgula decimal), um arquivo por concurso
# em tabelas/<loteria>/tabela_<loteria>_<perfil>_<concurso>.csv
cargo run --bin sync -- tabela todas --perfil padrao_v1                  # próximo concurso
cargo run --bin sync -- tabela quina --perfil padrao_v1 --concurso 7134  # concurso já sorteado (com `saiu`)

# dataset de ML para o Python (`ml/`): features por (concurso, dezena, janela) dos
# concursos sorteados, em datasets/<loteria>/features_v<versão>.<parquet|csv>
cargo run --bin sync -- dataset quina                     # Parquet, só o trecho de busca
cargo run --bin sync -- dataset quina --formato csv       # CSV de máquina
cargo run --bin sync -- dataset quina --trancado          # inclui o TRANCADO: só na validação final;
                                                          # grava features_v<versão>_trancado.<ext> à parte

# `todas` no lugar do slug percorre todas as loterias configuradas
cargo run --bin sync -- validar todas
cargo run --bin sync -- atualizar todas
```

`todas` vale em `baixar`, `atualizar`, `validar`, `buracos`, `aleatoriedade` e `tabela`, na ordem de
`LOTERIAS` em `config.rs`. Os bancos não são juntados: o comando roda em cada
banco padrão, um por vez (por isso `--db` e `--seletor` não podem ser
combinados com `todas`). Uma loteria que falha não interrompe as demais: no
fim sai um resumo (ok/falhou por loteria) e o comando termina com erro se
alguma falhou. O cabeçalho de cada loteria e o resumo vão para a saída
padrão; o erro de cada loteria vai sempre para a saída de erro. `importar`,
`inserir`, `listar`, `features`, `indice`, `avaliar` e `dataset` aceitam só
uma loteria.
`listar`, `validar`, `buracos`, `features`, `indice`, `avaliar`,
`aleatoriedade`, `tabela` e `dataset` falham com "banco não encontrado" em vez de criar um banco
vazio.

`atualizar` compara o concurso lido na página com `MAX(concurso)` já salvo no
banco daquela loteria:

- página igual (ou anterior) ao último salvo: não grava nada ("Já está
  atualizado");
- página = último + 1: valida e grava só esse concurso;
- **salto** (página > último + 1, ex.: ficou alguns dias sem rodar e perdeu
  um concurso no meio): gravar só o da página deixaria buraco, então o
  comando, sem perguntar nada (vai rodar agendado):
  1. copia o banco para `backups/resultados_<loteria>_<AAAAMMDD-HHMMSS>.db`
     (pasta criada se não existir, relativa ao diretório de trabalho; outra
     pasta com `--backups`) e mostra o caminho. **O horário do nome é UTC**,
     não o horário de Brasília. A cópia é feita pelo SQLite (`VACUUM INTO`),
     consistente mesmo com outro processo gravando. Dois backups da mesma
     loteria no mesmo segundo não se sobrescrevem: o segundo ganha `_2`, o
     terceiro `_3`, e assim por diante;
  2. segue o caminho do `baixar`: baixa a planilha completa para `downloads/`
     (ou `--dir`) e grava com o cache de inserções manuais, as correções
     conhecidas e a validação de sempre (tudo ou nada);
  3. se a planilha ainda não traz o concurso da página (a planilha costuma
     atualizar depois da página), grava-o como no salto de 1;
  4. se ainda sobrar buraco, avisa e indica `buracos`/`inserir`, terminando
     sem erro (buraco é só aviso na gravação).

  Se o download, a leitura ou a validação da planilha falhar, o comando
  termina com erro sem gravar nada da planilha, e a mensagem diz onde ficou o
  backup. Se a planilha for gravada e só o concurso da página for recusado
  no passo 3, a planilha fica gravada e o comando termina com erro (com o
  caminho do backup). Com o banco inexistente ou sem concursos da loteria,
  o último conta como 0: página > 1 é salto, mas não há backup (não há o
  que copiar).

```bash
# salto: backup em backups/, planilha em downloads/ (ambos configuráveis)
cargo run --bin sync -- atualizar quina --dir planilhas --backups copias
```

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

`avaliar <loteria> --perfil <nome>` mede o índice gravado do perfil contra o
acaso (regras em `docs/PLANEJAMENTO.md`, 1c). A posição de cada dezena vira
probabilidade pela calibração acumulada (só o passado), e o comando imprime
log loss, Brier e acertos do top-k (k = K da loteria, ou `--k`) do perfil e do
acaso, com diferença, erro padrão, IC 95% normal e o p-valor exato do top-k,
em 5 dobras de walk-forward no trecho de busca (até o corte fixo da loteria,
`ultimo_concurso_busca` em `config.rs`) e no trecho inteiro. O veredito
"supera o acaso" vem do **bootstrap circular por blocos** de 50 concursos
(10 000 reamostragens, semente fixa) sobre as diferenças perfil − acaso do log
loss por concurso: p-valor unilateral e IC 95% percentil, α = 5% (o Brier
passa pelo mesmo bootstrap só como informação). Trecho com menos de 100
concursos fica sem veredito. Os p-valores só valem se o perfil e o k foram
escolhidos sem ver o trecho: quem testou vários informa quantos com
`--testados N` (Bonferroni, p·N limitado a 1). Só lê o banco (aberto só para
leitura) e recusa, sem avaliar: `todas`; perfil de outra loteria, fora de
`perfis` ou com hash diferente do `.toml` em disco; resultados com buraco ou
erro de integridade; features desatualizadas (como no `indice`); índice que
não vai até o último concurso + 1 ou que difere do índice recalculado em
memória com as features atuais (por exemplo, `features` regenerado depois de
corrigir um resultado antigo) — nos dois casos, rode `indice` de novo.

`--trancado` avalia o **conjunto trancado** (do corte + 1 até o último
sorteado), que é de uso único por perfil: cada uso fica registrado em
`registros/trancado_<loteria>.json` **ao lado do banco** (no diretório do
arquivo passado em `--db`, ou do banco padrão), com perfil, hash do `.toml`,
até qual concurso e quando (ISO-8601 UTC). Os bancos reais ficam na raiz do
repositório, então o registro é `registros/` na raiz, de onde quer que o
comando seja rodado. Esse arquivo **vai para o git** — não fica no banco, que
é derivado e pode ser refeito. O mesmo perfil e hash já registrados são
recusados, a menos de `--repetir` (que mostra as avaliações anteriores e
registra a nova). O trancado usa sempre k = K (`--k` e `--testados` valem só
na busca). No trancado o p-valor sai também com Bonferroni pelo número de
perfis distintos já registrados na loteria.

Cuidados com o registro do trancado:

- **Não troque a pasta.** Existe uma opção `--registros`, escondida da
  ajuda, só para os testes: uso gravado em outra pasta não conta para o uso
  único nem para o Bonferroni do registro versionado. Quando a pasta não é a
  padrão, ou quando o arquivo de registro ainda não existe (primeiro uso da
  loteria), o comando avisa na saída de erro com o caminho absoluto onde o
  uso vai ficar; se não for o arquivo do git, interrompa. Além disso, todo
  relatório do trancado traz no cabeçalho a linha `Registro do trancado
  lido:` com o caminho absoluto do arquivo lido e quantos usos ele tinha
  (`0 (arquivo inexistente; ...)` no primeiro uso); a recusa do uso único
  também cita o arquivo e o total de usos.
- **Ordem:** a avaliação roda, o relatório é montado em memória, o uso é
  gravado e só então o relatório é impresso. Se a saída padrão estiver
  fechada (`| head`, por exemplo), o comando não dá panic: sai com erro e
  manda o relatório para a saída de erro, avisando que o uso já foi
  registrado. Se a saída fechar antes da avaliação (no aviso inicial), nada é
  avaliado nem registrado.
- **Uma execução por vez.** Não há trava de arquivo: o registro é recarregado
  logo antes de gravar e o uso novo é juntado ao conteúdo recarregado (o uso
  gravado por outra execução não se perde; se ela registrou o mesmo perfil, a
  segunda é recusada; se o registro mudar em 3 recargas seguidas, nada é
  gravado). Ainda assim sobra uma janela mínima entre a última recarga e a
  gravação: **não rode `--trancado` em paralelo**.

`dataset <loteria> [--formato parquet|csv] [--trancado] [--saida datasets]`
grava o dataset que o Python (`ml/`) lê: as linhas de `features_dezena` —
uma por (concurso, dezena, janela), o snapshot de antes do concurso, nada
recalculado — dos concursos já sorteados (o snapshot do próximo concurso não
entra). Colunas, nesta ordem: `concurso, data, dezena, janela, trecho`, os 9
componentes com os nomes de `features_dezena`, `saiu` (0/1, o rótulo do
próprio concurso) e `feature_version`. `trecho` é `busca` (até o corte fixo
da loteria) ou `trancado` (acima dele). **Por padrão o trancado fica fora**:
só entram concursos até o corte; `--trancado` inclui o resto e avisa na saída
de erro — é só para a validação final do perfil campeão, nunca para a busca
de pesos.

- **Arquivos:** `<saida>/<loteria>/features_v<feature_version>.<parquet|csv>`
  e, ao lado, `manifesto_<parquet|csv>.json` (um por formato, para os dois
  conviverem na pasta). Com `--trancado`, nomes próprios:
  `features_v<feature_version>_trancado.<parquet|csv>` e
  `manifesto_trancado_<parquet|csv>.json`; uma execução com `--trancado`
  nunca sobrescreve nem apaga os arquivos da busca, e a busca de pesos não lê
  o trancado por engano. `datasets/` fica fora do git.
- **Manifesto** (JSON indentado, chaves nesta ordem): `loteria`,
  `feature_version`, `formato`, `arquivo` (nome do arquivo de dados ao lado),
  `colunas`, `primeiro_concurso` e `ultimo_concurso` (incluídos), corte
  `ultimo_concurso_busca`, `trecho` (`busca` ou `busca+trancado`),
  `concursos` e `linhas` (contagens), `sha256` do arquivo de dados,
  `impressao_resultados_banco` (impressão de `resultados` do banco inteiro, a
  mesma de `motor_estado`; muda a cada sorteio, mesmo fora do arquivo) e
  `impressao_resultados_incluidos` (a mesma função só sobre os concursos 1 …
  `ultimo_concurso`; no dataset da busca não muda quando entra um concurso
  acima do corte), para o Python conferir o que leu.
- **Parquet:** crate `parquet` sem arrow, sem compressão, um row group a cada
  1 048 576 linhas; inteiros `INT32` com sinal, `data` e `trecho` em texto
  UTF-8, `NULL` nos componentes sem valor; metadados `loteria`,
  `feature_version` e `trecho`; `created_by` fixo (`rust_spider sync
  dataset`).
- **CSV de máquina** (diferente do CSV de planilha do `tabela`): separador
  `,`, cabeçalho, campo vazio = `NULL`, fim de linha LF, UTF-8 sem BOM. Só há
  inteiros e textos sem vírgula, então não há aspas nem casas decimais.
- **Determinismo:** mesmos dados → mesmos bytes no arquivo e no manifesto
  (ordem (concurso, dezena, janela), sem data de geração). Os bytes do
  Parquet dependem da versão do crate travada no `Cargo.lock`.
- **Pré-condições:** uma loteria por vez, sem `todas`; banco aberto só para
  leitura; recusa, sem gravar nada, as mesmas situações do `indice`: banco
  sem resultados ou sem features, features desatualizadas (snapshot diferente
  do último + 1), resultados alterados desde o último `features`, outra
  `feature_version` ou série incompleta em alguma janela. O manifesto antigo
  do mesmo modo e formato é apagado antes de regravar o arquivo de dados: se
  faltar o manifesto, o arquivo não é confiável. O par do outro modo (busca
  ou trancado) não é tocado.

`aleatoriedade <loteria|todas>` testa o histórico inteiro de `resultados`
(inclusive o trancado; não depende de perfil nem de features): qui-quadrado
de frequência (corrigido para sorteio sem reposição) e runs test por dezena
com p-valor exato e correção de Holm, α = 5%. Rejeição costuma indicar dado
errado antes de sorteio viciado: confira com `validar`. Só lê o banco.

Por padrão cada loteria grava em um banco SQLite separado —
`resultados_<loteria>.db` (ex.: `resultados_lotofacil.db`,
`resultados_megasena.db`) — para não misturar dados de loterias diferentes
no mesmo arquivo. Os downloads vão para `downloads/` e os backups do
`atualizar` com salto para `backups/`. Configuráveis via `--db`, `--dir` e
`--backups`; os três ficam fora do git (`*.db` e `/downloads` no
`.gitignore`). As saídas de `tabela` e `dataset` (`tabelas/`, `datasets/`)
também ficam fora do git.

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
