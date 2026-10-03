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
├── db.rs          # rusqlite: schema e upsert por (loteria, concurso)
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
```

`atualizar` compara o concurso lido na página com `MAX(concurso)` já salvo no
banco daquela loteria: se não houver novidade, não grava nada; se o salto for
maior que 1 (ex.: ficou alguns dias sem rodar e perdeu um concurso no meio),
avisa e sugere rodar `baixar` para preencher o histórico completo — raspagem
só traz o mais recente, não faz backfill de concursos intermediários.

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
