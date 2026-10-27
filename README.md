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
├── avaliacao/     # índice contra o acaso (log loss, Brier, top-k, walk-forward, bootstrap), testes de aleatoriedade e evidência bayesiana (por dezena e do índice), puro
├── registros.rs   # registro de uso do conjunto trancado (`registros/trancado_<loteria>.json`, versionado)
├── perfil.rs      # perfis de pesos (`perfis/<loteria>/<nome>.toml`): leitura, validação e hash
├── orquestracao.rs # `sync features`, `indice`, `avaliar`, `aleatoriedade`, `evidencia`, `tabela`, `exportar` e `dataset`: validação + motor/avaliação + banco
├── tabela.rs      # CSV de planilha da tabela geral (`sync tabela`)
├── ranking.rs     # CSV e relatório em texto do `sync ranking` (taxa histórica de posição e componentes)
├── exportacao/    # JSON do contrato `v1` (`sync exportar`): serialização determinística (puro) e gravação atômica em disco
├── publicacao/    # `sync publicar`: confere o publico/v1/, decide se mudou e publica num `Destino` (clone git)
├── calendario.rs         # calendário de sorteios (3.2): dia/horário de procura, conferência, tolerância (puro)
├── orquestracao/calendario.rs # `sync calendario`: banco (somente leitura) + calendário
├── orquestracao/ciclo.rs # `sync ciclo`: compõe atualizar, backfill, validar, features, índice, exportar e publicar
├── orquestracao/aguardar.rs # `sync ciclo --aguardar` (3.3): rodadas com backoff até 01h00, relógio e espera injetados
├── orquestracao/alertas.rs # alertas (3.6): noites seguidas com falha, rascunhos `.eml` em `alertas/`, relógio injetado
├── dataset.rs     # dataset de ML (Parquet/CSV) e manifesto (`sync dataset`), sem banco
├── trava.rs       # `sync.lock` (3.4): trava do SO contra execução simultânea
├── log_arquivo.rs # `logs/sync_AAAA-MM-DD.log` (3.5): cópia da tela com hora e nível, abertura e fechamento
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

# avalia o índice de um perfil contra o acaso no trecho de busca (rode `indice` antes; só lê o
# banco). Além do log loss/Brier/top-k, imprime uma linha extra, só informativa: o log loss com a
# calibração linear na posição (perfil − acaso, no total e por dobra), que não muda o veredito
cargo run --bin sync -- avaliar quina --perfil padrao_v1
cargo run --bin sync -- avaliar quina --perfil padrao_v1 --k 10 --testados 3   # outro k; Bonferroni por 3 testados
cargo run --bin sync -- avaliar quina --perfil padrao_v1 --trancado            # conjunto TRANCADO: uso único por perfil

# testes de aleatoriedade do histórico inteiro (qui-quadrado e runs por dezena)
cargo run --bin sync -- aleatoriedade todas

# evidência bayesiana (fator de Bayes em cinco graus). Só lê o banco; não grava nada.
#  A) por dezena, para o próximo concurso: histórico inteiro e últimos 500 concursos, priori de 5 %
#     de K/N com sensibilidade de 2,5 % e 10 %, a contagem de dezenas por grau ao lado da esperada
#     sob o acaso (simulação, semente fixa) e, por janela e priori, o máximo atingível a favor do
#     acaso (`log10 √(1 + Var_priori/Var_amostral)`: a favor do acaso a evidência tem teto).
#  B) do índice de força de um perfil: FB prequencial (soma, par a par, de ln p_índice − ln p_acaso)
#     nas dobras de teste 2 a 5 do walk-forward do `avaliar`, com grau, direção ("a favor do índice"
#     se FB > 1, senão "a favor do acaso") e o log10 FB por dobra (com a Δ log loss por par). A
#     régua PRINCIPAL é a calibração LINEAR na posição da 1e (`p = K/N + b·z(posição)`, um parâmetro
#     `b` aprendido só com o passado, m = 100 concursos fictícios): grau e direção saem dela. É a
#     diferença de log loss da linha linear do `avaliar` em outra escala: ln FB = −pares × (log loss
#     do perfil − log loss do acaso), dobra a dobra (pares = concursos avaliados × N). O trancado
#     não entra no cálculo (nada acima do corte da busca). Ao lado, o esperado de um ÍNDICE INERTE
#     na mesma régua linear (as mesmas posições diante de 200 históricos sorteados ao acaso, semente
#     20261010), com média, faixa de 95 % e quantas réplicas deram log10 FB igual ou maior que o
#     observado. ATENÇÃO: mesmo um índice sem informação paga um custo para aprender a calibração
#     (na linear é pequeno, da ordem de ½·ln T), então o grau sozinho não diz se o índice atrapalha:
#     o índice só mostra algo se o observado ficar ACIMA da faixa de 95 % dos inertes. A calibração
#     por posição da 1c (uma taxa por posição) sai numa linha INFORMATIVA, sem inerte: o custo de
#     aprender N taxas (−17 a −60 em ln FB mesmo para um inerte) faz o seu grau "muito forte, a
#     favor do acaso" não decidir nada.
#  `--perfil` escolhe o perfil de B (padrão: o perfil ativo da loteria, `padrao_v1`); `--perfis`
#  muda a pasta dos perfis (padrão `perfis`). B tem as mesmas pré-condições do `avaliar` (perfil
#  registrado com o hash do `.toml`, `features` e `indice` em dia). Se B não puder ser calculada, A
#  sai igual e a seção B traz o motivo e os comandos a rodar, um por linha; o exit é 0 com o perfil padrão e
#  diferente de 0 quando `--perfil` foi passado.
cargo run --bin sync -- evidencia quina
cargo run --bin sync -- evidencia quina --perfil busca_v1 --perfis outra/pasta

# tabela geral em CSV (Excel: `;`, vírgula decimal, UTF-8 com BOM), um arquivo por concurso
# em tabelas/<loteria>/tabela_<loteria>_<perfil>_<concurso>.csv
# 21 colunas; as duas últimas, `evidencia_grau` e `evidencia_direcao`, trazem a evidência por dezena
# (A do `evidencia`: histórico inteiro, priori de 5 %) no ponto no tempo do concurso — só concursos até
# o anterior —, com os mesmos rótulos do `evidencia`; vazias no concurso 1. Como ler: docs/GUIA.md.
cargo run --bin sync -- tabela todas --perfil padrao_v1                  # próximo concurso
cargo run --bin sync -- tabela quina --perfil padrao_v1 --concurso 7134  # concurso já sorteado (com `saiu`)

# ranking por taxa histórica de posição e componentes (Fase 1f): grava ranking_taxa, taxa_posicao e
# taxa_faixa no banco (regenera as do perfil; padrão: perfil ativo), imprime o relatório e grava o CSV do
# próximo concurso em ranking/<loteria>/ranking_<loteria>_<perfil>_<concurso>.csv. Uma loteria por vez.
# É descrição do passado, não previsão; a nota B é experimental. Recusa se features/índice estiverem defasados.
cargo run --bin sync -- ranking quina
cargo run --bin sync -- ranking megasena --perfil padrao_v1 --saida ranking
# --avaliar: não grava nada; avalia o ranking (A e B) contra o acaso nas dobras de teste 2 a 5 da busca (trancado fora)
cargo run --release --bin sync -- ranking quina --avaliar

# JSON do contrato `v1` para o app, em publico/v1/ (fora do git): index.json (SHA-256 de cada arquivo)
# e, por loteria, ultimo.json, tabela_geral.json (próximo concurso, perfil ativo da loteria) e
# historico.json. Só lê o banco; recusa, sem gravar nada, se features ou índice não estiverem em dia.
# `todas` é tudo ou nada; com um slug só os arquivos dele são regravados e o index.json é refeito.
# JSON sem acento: evidencia_grau muito_fraca|fraca|moderada|forte|muito_forte, evidencia_direcao
# vies|acaso, normalizacao zscore|minmax (identificadores do contrato, não os rótulos do CSV).
cargo run --bin sync -- exportar todas
cargo run --bin sync -- exportar quina --saida outra/pasta

# publica o publico/v1/ já exportado no clone git do repositório de dados e faz push (origin/main).
# Não exporta nem lê banco; não clona sozinho (se o clone faltar, o erro traz o `git clone` a rodar).
# Clone padrão ../rust_spider_dados (relativo ao diretório atual), trocável com --repo.
# Antes de copiar: confere sha256 e bytes do index.json contra o disco, faz `git fetch` e recusa, sem
# tocar em nada, se o clone estiver sujo, fora de `main` ou atrás/divergente de origin/main, ou se a
# URL do origin (fetch ou push) não for a de um repositório `rust_spider_dados` (evita publicar no repo do código).
# Só publica se algum hash ou a lista de arquivos mudou (só `gerado_em` diferente = "nada a publicar",
# sucesso, sem commit). Um commit por publicação, "Dados: lotofacil 3802, megasena 3069, quina 7140"
# (loteria null no index fica fora da mensagem), autor = git da máquina; v1/ do clone vira espelho do
# publico/v1/ (apaga o que sumiu; fora de v1/ nada é tocado). Push que falha: o commit fica local, o
# comando termina com erro e a próxima execução reenvia, mesmo sem mudança. Qualquer commit local do
# clone à frente de origin/main também é enviado pelo `publicar` (envio pendente): use o clone só
# para a publicação. Remoto vazio (sem commit em main) é recusado: faça antes um commit inicial
# fora de v1/ (ex.: README) e `git push -u origin main`.
git clone git@github.com:edxgar48/rust_spider_dados.git ../rust_spider_dados   # uma vez
cargo run --bin sync -- publicar
cargo run --bin sync -- publicar --repo outra/pasta --saida publico

# ciclo completo (3.1b): por loteria, atualizar -> backfill (se sobrar buraco) -> validar -> features
# (incremental) -> índice do perfil ativo -> exportar; depois de todas, UM publicar. Só compõe os
# comandos acima, com as mesmas flags e padrões (--dir, --backups, --cache, --perfis, --saida, --repo,
# --visivel; --db só com slug). Comportamento:
#  - falha isolada: uma loteria que falha (seletor mudou, rede fora, validar reprovou...) não interrompe
#    as outras; só as que passaram são exportadas (index.json com o que há no disco, a falhada fica como
#    estava) e o publicar roda mesmo assim. No fim, resumo por loteria e saída com erro (exit != 0) se
#    qualquer loteria ou o publicar falhou (push que falha deixa o commit local; a próxima execução reenvia);
#  - sem concurso novo: segue até o publicar ("Nada a publicar" se nada mudou); rodar duas vezes seguidas
#    não muda nada nem gera commit;
#  - backfill: se depois do atualizar ainda faltar concurso no meio do banco, baixa a planilha inteira
#    (caminho do `baixar`, com backup antes). Se o buraco persistir, só aviso e o validar reprova a loteria
#    (feche com `sync buracos` + `sync inserir`). Após um salto o atualizar já baixou a planilha: não baixa
#    de novo;
#  - sem `--aguardar` o ciclo faz uma passada só (e um único publicar no fim). A trava (3.4), o log em
#    arquivo (3.5), os alertas (3.6) e o agendamento (3.7) existem: ver "Trava", "Log", "Agendamento" e
#    "Alertas e operação". No fim, o ciclo conta a noite de cada loteria e grava o rascunho de e-mail de
#    alerta quando a regra manda (a saída do comando não muda por isso).
cargo run --bin sync -- ciclo todas
cargo run --bin sync -- ciclo quina --repo ../rust_spider_dados

# retentativas (3.3): `ciclo --aguardar` é o modo da execução agendada das 23h00 (3.7); um processo só repete
# o ciclo em rodadas, nenhum processo espera até o outro dia. O que provoca nova rodada:
#  - o concurso esperado ainda não saiu: dia de sorteio pelo calendário (3.2, estado `aguardando`) e a
#    página ainda mostra o concurso anterior. A loteria que espera não exporta nem publica;
#  - falha transitória de navegador/rede ("falha ao iniciar o Playwright", timeout do seletor, download que
#    não começou). Erro de validação, de gravação ou de banco NÃO é repetido: a loteria falha na hora;
#  - janela: da primeira passada até 01h00 de Brasília (a 1ª passada às 23h00 deixa 2 h), com esperas de
#    5, 10, 20, 30, 30... min entre as rodadas e só para as loterias que faltam (23:05, 23:15, 23:35, 00:05,
#    00:35 e a última rodada às 01:00, com o sono cortado no limite). Depois da meia-noite o sorteio
#    esperado continua sendo o da véspera. Só há janela se o próximo 01h00 está a até 4 h do início: uma
#    execução manual às 10h ou às 02h faz uma passada só. Loteria `em_dia`, `atrasado` ou em dia sem
#    sorteio nunca espera;
#  - publicação: cada loteria que chega (features, índice, exportar) é publicada logo, até um commit por
#    loteria por noite. Se nada foi publicado (todas falharam), um publicar final reenvia push pendente;
#  - fim da janela: loteria que ainda aguarda roda o resto do ciclo uma vez, sem novidade, e termina com
#    AVISO (exit 0; o alerta é da 3.6). Falha transitória persistente é falha (exit != 0) na etapa em que
#    parou; isso vale também para a loteria que aguardou a noite toda e cai numa falha transitória justo
#    na rodada das 01h00 (falha, sem aviso de "concurso ausente"). Página que veio incompleta (cabeçalho do
#    concurso novo sem as dezenas ainda) conta como transitória. Backup (do salto ou do backfill) e download
#    da planilha do backfill: um backup por loteria por execução, mesmo com várias rodadas;
#  - as 3 tentativas curtas do downloader (2 s e 4 s) continuam por baixo de cada passada.
cargo run --bin sync -- ciclo todas --aguardar

# alertas (3.6; regras em "Alertas e operação"): `alertas` lista os rascunhos de e-mail ainda não movidos
# para alertas/enviados/ (só lê, sem trava); `alerta --geral` conta uma falha do "geral" (trava ocupada,
# processo morto) e grava o rascunho quando a regra manda; não pega a trava. O `ciclo` já faz o resto sozinho.
# `sync` sai com código 3 quando a trava está ocupada (1 nas demais falhas).
cargo run --bin sync -- alertas
cargo run --bin sync -- alerta --geral "trava ocupada: PID 4120"

# calendário de sorteios (3.2), só leitura: para hoje (relógio de Brasília, UTC-3 fixo) diz se é dia de
# sorteio e desde que horário procurar o resultado (21h00 nas três loterias; o sorteio é por volta das
# 20h), o estado do último concurso do banco e os avisos de conferência. Dias de sorteio e horário ficam
# em config.rs (dias_sorteio, procurar_a_partir_das), conferidos nos bancos reais em 2026-10-13
# (Lotofácil e Quina: domingo a sexta; Mega-Sena: terça, quinta e domingo).
#  - estado do último concurso: em_dia = nenhum concurso esperado faltando; aguardando = um sorteio já
#    passou do horário de procura e o concurso não está no banco, mas o horário de procura do dia de
#    sorteio seguinte ainda não chegou (feriado, adiamento, site atrasado: aviso, não falha); atrasado =
#    passou também desse horário (é o que a 3.6 contará como falha). Exatamente no horário de procura o
#    concurso já é esperado;
#  - conferência: nos 56 dias até a data do último concurso, avisa se um dia do calendário não teve
#    nenhum concurso ou se um dia fora do calendário teve 2 ou mais (calendário possivelmente
#    desatualizado). Um sorteio especial isolado (ex.: Mega da Virada) não avisa;
#  - avisos e estados não mudam o código de saída; só falha se o banco não existe/não tem concursos;
#  - --data AAAA-MM-DD consulta outro dia (só a linha "é dia de sorteio"); o estado segue o relógio de agora.
cargo run --bin sync -- calendario todas
cargo run --bin sync -- calendario megasena --data 2026-10-17

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
`inserir`, `listar`, `features`, `indice`, `avaliar`, `evidencia` e `dataset` aceitam só
uma loteria.
`listar`, `validar`, `buracos`, `features`, `indice`, `avaliar`,
`aleatoriedade`, `evidencia`, `tabela` e `dataset` falham com "banco não encontrado" em vez de criar um banco
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

Concurso com data posterior a hoje (relógio de Brasília) é erro em qualquer gravação
(`importar`, `baixar`, `atualizar`, `inserir`, `ciclo`) e recusa o lote inteiro; data de
hoje é aceita, e o `validar` não reporta isso.

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
acaso (regras em `docs/decisoes/fases-1-e-1.5.md`, 1c). A posição de cada dezena vira
probabilidade pela calibração acumulada (só o passado), e o comando imprime
log loss, Brier e acertos do top-k (k = K da loteria, ou `--k`) do perfil e do
acaso, com diferença, erro padrão, IC 95% normal e o p-valor exato do top-k,
em 5 dobras de walk-forward no trecho de busca (até o corte fixo da loteria,
`ultimo_concurso_busca` em `config.rs`) e no trecho inteiro. O veredito
"supera o acaso" vem do **bootstrap circular por blocos** de 50 concursos
(10 000 reamostragens, semente fixa) sobre as diferenças perfil − acaso do log
loss por concurso: p-valor unilateral e IC 95% percentil, α = 5% (o Brier
passa pelo mesmo bootstrap só como informação). Trecho com menos de 100
concursos fica sem veredito. Entre o Brier e o top-k vai uma **linha extra,
só informativa**: o log loss (perfil − acaso, no total e nas 5 dobras) com a
**calibração linear na posição** (Fase 1e: `p = K/N + b·z(posição)`, um único
parâmetro `b` estimado só com o passado, com m = 100 concursos fictícios com
b = 0, que custa muito menos para aprender que as N taxas da calibração por
posição). Ela não tem bootstrap nem veredito, o trancado não ganha linha
linear, e o resto do relatório (tabelas, veredito, top-k) sai igual ao de
antes; é a mesma diferença que a parte B do `sync evidencia` converte em fator
de Bayes (ln FB = −pares × diferença, nas dobras 2 a 5). Os p-valores só valem
se o perfil e o k foram escolhidos sem ver o trecho: quem testou vários informa
quantos com `--testados N` (Bonferroni, p·N limitado a 1). Só lê o banco (aberto só para
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
- **Uma execução por vez.** `avaliar --trancado` toma a trava `sync.lock` (ver
  "Trava"), então duas execuções não rodam juntas. Por baixo, o registro ainda
  é recarregado logo antes de gravar e o uso novo é juntado ao conteúdo
  recarregado (se o registro mudar em 3 recargas seguidas, nada é gravado).

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

## Trava contra execução simultânea (`sync.lock`)

Os comandos que gravam tomam, antes de qualquer gravação, uma trava do sistema
operacional sobre `sync.lock` e a seguram até terminar (no `ciclo --aguardar`,
a noite toda): `baixar`, `importar`, `atualizar`, `inserir`, `features`,
`indice`, `exportar`, `publicar`, `ciclo`, `dataset` e `avaliar --trancado`.
Ficam livres `listar`, `validar`, `buracos`, `calendario`, `aleatoriedade`,
`evidencia`, `tabela` e `avaliar` sem `--trancado`.

- Uma trava só, no `sync.lock` da **pasta de trabalho** (a mesma dos bancos
  padrão `resultados_<loteria>.db`), mesmo com `--db` apontando para outro
  lugar: `publico/` e o clone de dados são comuns às três loterias. Rode o
  `sync` sempre da mesma pasta.
- Trava ocupada: o comando falha na hora, sem esperar, com saída diferente de
  zero e a mensagem `outra execução em andamento: PID 1234, comando ciclo,
  desde 23:00` (horário de Brasília). O comando sai com **código 3** (só neste
  caso; qualquer outra falha sai com 1) e o `ciclo_agendado.ps1` transforma o 3
  num alerta do "geral" (ver "Alertas e operação").
- Não existe trava órfã: ela é do sistema operacional e some se o processo
  morrer, for encerrado à força ou a máquina reiniciar. Não apague o
  `sync.lock` à mão para "destravar"; se a mensagem aparece, há mesmo um
  processo com aquele PID (confira no Gerenciador de Tarefas).
- `sync.lock.dono` guarda PID, comando e início só para a mensagem (no Windows
  o processo que perde não consegue ler o próprio `sync.lock` travado). Se
  faltar, a mensagem sai sem detalhes. Os dois arquivos estão no `.gitignore`.
- Exige Rust 1.89 (`File::try_lock`).

## Log em arquivo (`logs/`)

Os mesmos comandos que tomam a trava gravam um log em
`logs/sync_AAAA-MM-DD.log`, na pasta de trabalho (a mesma do `sync.lock`, fora
do git). Os comandos só de leitura ficam só no console e não criam a pasta.

```text
23:00:01 INFO  Início: PID 4120, comando `sync ciclo todas --aguardar`, versão 0.1.0
23:00:03 INFO  === Lotofácil (lotofacil) ===
23:05:10 INFO  Aguardando lotofacil: o concurso do sorteio de 2026-10-13 ainda não saiu ...
00:12:44 ERRO  Error: o ciclo terminou com falha(s); veja o resumo acima
00:12:44 ERRO  Fim: falha, duração 1 h 12 min 44 s
```

- É uma cópia de tudo o que sai na tela (saída normal e de erro, inclusive o
  erro final com a cadeia `Caused by:` e as mensagens do navegador, como
  "tentativa 2/3 falhou"), uma linha do log por linha da tela, cada uma com a
  hora de Brasília (`HH:MM:SS`) e o nível. O texto do console não muda.
- **Níveis**: `INFO` é o andamento normal (inclui "aguardando o concurso" do
  `--aguardar`); `AVISO` é o que o comando seguiu mas merece olhar (avisos da
  validação, buraco e a dica de como fechá-lo, salto, concurso que não saiu,
  falha de navegador/rede que será repetida, divergência de inserção manual ou
  de correção); `ERRO` é falha de etapa, de loteria, da publicação ou do
  comando.
- A primeira linha traz o PID, o comando completo (os argumentos) e a versão; a
  última, a duração e `sucesso` ou `falha`. Um processo morto à força (queda de
  energia, `taskkill`) não escreve o fechamento: um log que termina sem `Fim:`
  indica isso.
- **Arquivo do dia**: a data é a do *início* da execução em Brasília, então a
  noite do `ciclo --aguardar` (23h às 01h) fica toda num arquivo. O arquivo é
  aberto em acréscimo: várias execuções no mesmo dia, inclusive a que a trava
  recusou ("outra execução em andamento"), aparecem no mesmo arquivo. O log
  abre antes da trava de propósito, para o agendado que encontrar a trava
  ocupada deixar rastro.
- **Retenção**: ao abrir o arquivo do dia, apaga os `sync_AAAA-MM-DD.log` com
  mais de 90 dias (pela data do nome). Arquivos com outro nome não são tocados.
- **Log que não grava nunca derruba o comando**: se não abrir (sem permissão,
  `logs` é um arquivo...), sai `Aviso: não foi possível abrir o log em arquivo`
  no console e o comando segue sem log; se a escrita falhar no meio, avisa uma
  vez e o log se desliga.
- Os diagnósticos de `downloader` e `parser` chegam ao log por um hook global
  (`OnceLock` em `log_arquivo`). É a única exceção de estado global do
  projeto, aceita só para a camada de I/O; não deve virar precedente em
  `motor/` nem em `avaliacao/`.

## Agendamento (Windows)

O Agendador de Tarefas roda `sync ciclo todas --aguardar` todo dia às 23h00.
Quem registra a tarefa é você, à mão; nenhum comando do projeto a registra
sozinho. Scripts em `scripts/` (Windows PowerShell 5.1):

```powershell
cargo build --release                       # gera target\release\sync.exe
scripts\registrar_tarefa.ps1 -Mostrar       # só imprime a definição, não registra
scripts\registrar_tarefa.ps1 [-Pasta C:\...]  # cria ou atualiza a tarefa "rust_spider ciclo"
scripts\registrar_tarefa.ps1 -Remover       # apaga a tarefa
scripts\registrar_tarefa.ps1 -SoConectado  # alternativa sem elevação (só roda com você logado)
```

Se a política de execução bloquear os scripts, use
`powershell -ExecutionPolicy Bypass -File scripts\registrar_tarefa.ps1 ...`.

- A tarefa roda `scripts\ciclo_agendado.ps1`, que entra na pasta de trabalho
  (`-Pasta`, padrão a raiz do repositório; é ela que fixa `sync.lock`, `logs/`
  e os bancos), liga o SSH sem perguntas (`GIT_SSH_COMMAND` com
  `BatchMode=yes`, só para esse processo), chama `target\release\sync.exe`
  direto (sem `cargo run`) e devolve o código de saída do `sync`.
- Conta: o usuário atual, "executar estando conectado ou não", sem guardar
  senha (S4U). Se a máquina estava desligada às 23h, roda ao ligar
  (`StartWhenAvailable`; fora da janela 23h–01h o `--aguardar` faz uma passada
  só). Não acorda o PC, não abre segunda instância, limite de 3 h, roda na
  bateria. Repetir `registrar_tarefa.ps1` atualiza a tarefa, não duplica.
- **Rastro**: `logs\agendador.log` (duas linhas por execução: uma de início,
  gravada antes do sync e com o `pid=` do PowerShell, e uma de fim com código
  de saída e o erro do próprio script, como binário ou pasta ausentes; mais
  linhas `AVISO` quando há rascunho de alerta novo ou processo morto),
  `logs\sync_AAAA-MM-DD.log` (o ciclo em si) e "Último resultado" da tarefa no
  Agendador (`Get-ScheduledTaskInfo -TaskName "rust_spider ciclo"`).
- Linha de início sem linha de fim = processo morto (ex.: limite de 3 h do
  Agendador). Na execução seguinte o script percebe isso sozinho (ver
  "Alertas e operação"). Se a noite seguinte acusar "outra execução em
  andamento", confira se ficou um `sync.exe` vivo (`Get-Process sync`).
- `-Pasta` é resolvida para caminho absoluto sem barra final antes de ir para a
  tarefa (pasta inexistente = erro; raiz de unidade como `C:\` é recusada).
- O gatilho das 23h00 usa o fuso da máquina, que precisa ser o de Brasília.
  Registrar com S4U exige PowerShell elevado ("Executar como administrador");
  se mesmo assim der "Acesso negado" (ex.: conta Microsoft, política da
  máquina), use `-SoConectado`: LogonType Interactive, sem elevação, janela
  oculta, mas só roda com o usuário conectado (desconectado às 23h = roda no
  próximo login, pelo `StartWhenAvailable`). Chave SSH protegida por
  ssh-agent ou passphrase pode não estar acessível sob S4U: confira na primeira
  execução manual.
- **Não recompile o release entre 23h e 01h**: com o `sync.exe` em uso o
  `cargo build --release` falha.
- **Primeira execução manual**, para conferir o Playwright e o SSH sob a conta
  S4U: com a tarefa registrada, rode `Start-ScheduledTask -TaskName "rust_spider
  ciclo"` num horário em que o ciclo possa gravar de verdade e leia os dois
  logs.
- Os `.ps1` são só ASCII (o 5.1 lê UTF-8 sem BOM como ANSI); os testes
  (`tests/agendamento.rs`) usam um binário falso e não tocam no Agendador.

## Alertas e operação

Quando algo dá errado, o sistema **não envia nada**: grava um rascunho de
e-mail (`.eml`) e avisa. Você abre o rascunho (Outlook, Thunderbird...),
revisa e envia quando quiser. Não há SMTP nem senha no projeto.

```powershell
cargo run --bin sync -- alertas                                   # rascunhos ainda não enviados (só lê)
cargo run --bin sync -- alerta --geral "trava ocupada: PID 4120"  # conta uma falha do "geral"
```

- **Falha de uma loteria**: a noite em que ela termina em erro no `ciclo`
  (qualquer etapa, inclusive publicar) ou fica `atrasado` pelo calendário
  (`sync calendario`, 3.2) fora da janela de um sorteio especial. O concurso
  que só não saiu até 01h00 é aviso e **não** conta. Sucesso (ou `em_dia`)
  zera o contador. Duas execuções na mesma noite valem uma falha só.
- **Quando alerta**: na **2ª noite seguida** de falha (`N = 2`) da mesma
  loteria. **Mudança aparente no site** alerta já na 1ª noite: todas as
  rodadas da noite terminaram em erro de forma da página (elemento ou botão
  não encontrado, página incompleta, linha inválida, planilha ilegível ou com
  layout inesperado). Erro de rede ou de
  navegador segue o N.
- **Repetição**: um rascunho por (alvo, motivo) enquanto o problema durar;
  lembrete a cada 7 noites seguidas ainda em falha, contadas do último
  rascunho. Voltar ao normal só escreve uma linha no log.
- **O "geral"** é a execução em si: *trava ocupada* (o `sync` sai com código 3
  e o `ciclo_agendado.ps1` chama `sync alerta --geral`) e *processo morto*
  (abaixo). Conta noites com o mesmo N e um rascunho por motivo e incidente; um
  `ciclo` que chega ao fim zera o contador. O motivo é o texto antes do
  primeiro `:` do `--geral` (o resto é detalhe). `sync alerta` **não pega a
  trava**, roda mesmo com outro `sync` vivo e aparece no log do dia.
- **Processo morto** (critério do `ciclo_agendado.ps1`): a última linha de
  início/fim do `logs\agendador.log` é um `inicio` com `pid=` de um processo
  que não existe mais. O script fecha essa linha (`AVISO processo morto`, para
  não repetir), chama `sync alerta --geral "processo morto: ..."` e segue.
  Linha de início sem `pid=` (de antes da 3.6) não é avaliada. Binário ou pasta
  ausentes ficam **só** no `agendador.log` e no "Último resultado" do
  Agendador: sem binário não há como gerar o alerta.
- **Sorteios especiais** (`sorteios_especiais` em `config.rs`): a loteria fica
  sem concursos regulares antes do especial, e nessa janela o `atrasado` é só
  aviso. Mega da Virada: 31/12, 20 dias antes e 2 depois; Lotofácil da
  Independência: 7/9, 15 antes e 8 depois (o de 2026 saiu em 15/9); Quina de
  São João: 24/6, 15 antes e 4 depois (o de 2026 saiu em 28/6). Conferidas
  contra as paradas dos bancos reais; se a Caixa mudar a data ou a duração,
  ajuste a lista.
- **Onde ficam as coisas** (pasta de trabalho, tudo fora do git):
  `alertas/AAAA-MM-DD_HHMMSS_<loteria>-<motivo>.eml` (horário de Brasília),
  `alertas/estado.json` (histórico por alvo: noites seguidas, primeira e última
  falha, último erro, último alerta; ilegível = recomeça do zero com aviso,
  nunca derruba o `ciclo`) e `alertas/destinatario.txt` (opcional: o endereço
  do `To:`, na primeira linha que não seja vazia nem comece com `#`; sem o
  arquivo o rascunho sai sem `To:` e você preenche).
- **Como você é avisado**: linha `AVISO: rascunho de e-mail de alerta gravado
  ...` no console, no log do dia e (linha `AVISO: rascunho de alerta novo`) no
  `logs\agendador.log`, que compara a lista de `alertas\*.eml` antes e depois
  do `sync`. `sync alertas` lista os pendentes; depois de enviar, **mova o
  `.eml` para `alertas\enviados\`**.
- **O rascunho** é RFC 822 em UTF-8 com `X-Unsent: 1`, assunto
  `[rust_spider] <loteria>: <motivo>` e corpo em texto: loteria, motivo, noites
  seguidas, último concurso no banco, etapa e erro com a cadeia de causas, o
  caminho do log do dia e passos curtos. Os passos completos estão na seção
  "Runbook" logo abaixo (a tabela de entrada leva do motivo do rascunho ao procedimento).

## Runbook

O que fazer quando o ciclo não roda sozinho. Cada procedimento tem **sintoma**,
**diagnóstico**, **correção** e **como conferir que voltou ao normal**. Convenções:

- Todos os comandos rodam no PowerShell, **na pasta de trabalho** (a raiz do
  repositório: é ela que fixa `sync.lock`, os bancos, `logs/`, `alertas/`,
  `cache/` e `registros/`). Abaixo, `sync` é `target\release\sync.exe` (o que o
  Agendador usa) ou, sem o release compilado, `cargo run --bin sync --`; os
  dois são equivalentes.
- `<loteria>` é `lotofacil`, `megasena` ou `quina`. O perfil ativo das três é
  `padrao_v1` (`perfil_ativo` em `src/config.rs`).
- Quase tudo o que corrige algo **grava** e por isso toma a trava (`sync.lock`).
  Faça **fora da janela 23h–01h** (a tarefa agendada segura a trava a noite
  toda) ou com a tarefa pausada (ver "Pausar a tarefa agendada").
- O site da Caixa não é nosso: poucas tentativas, nada de laço de repetição. O
  `ciclo` já repete sozinho dentro da janela, com espera de 5 a 30 min.

### Entrada: de onde você veio

| Motivo do rascunho `.eml` / sintoma | Seção |
|---|---|
| `site possivelmente mudou` | 2 (seletor/layout); se a página abre normal no seu navegador mas o robô não acha nada, 1a |
| `falhas seguidas (N noites)`, erro `erro do navegador ...` / timeout | 1a (primeiro separe "rede fora" de "bloqueio") |
| `falhas seguidas (N noites)` com etapa `calendario`: `o concurso do sorteio de ... não está no banco e o prazo do calendário passou` (o ciclo terminou sem erro) | "Atrasado pelo calendário", logo abaixo |
| `elemento não encontrado (seletor ...)` noite após noite | 1a e 2 |
| `trava ocupada` (código 3, "outra execução em andamento") | 1b |
| `processo morto`, `logs\agendador.log` com `inicio` sem linha de fim | 1b |
| `falha ao publicar`, `publicação: ...` | 3c |
| `... erro(s) de integridade` (`validar` reprovou), buraco na numeração | `sync buracos` + `sync inserir` (seção "Uso"); banco todo suspeito: 3b |
| `rode sync features`, `índice incoerente`, `dataset incoerente` | 3a |
| Quer recomeçar do zero | 3 |

#### Atrasado pelo calendário

O ciclo pode terminar em exit 0 e mesmo assim contar a noite como falha: se o
último concurso do banco é anterior ao último sorteio esperado pelo calendário
e o prazo passou (`verificar_atraso`, etapa `calendario`), o motivo é "o
concurso do sorteio de ... não está no banco e o prazo do calendário passou
(último concurso: N de AAAA-MM-DD)". Dentro da janela de um sorteio especial
conhecido só sai um evento de aviso, sem falha. Procedimento:

1. `sync calendario <loteria>`: mostra o estado (`em_dia`, `aguardando`,
   `atrasado`) e o que o calendário espera.
2. Confira no site se esse concurso saiu.
3. Saiu e o robô não o pega: veja 1a (bloqueio) ou feche o buraco à mão com
   `sync inserir` (1a, item b).
4. Não saiu (adiamento, ou sorteio especial que não está na lista): ajuste
   `sorteios_especiais` da loteria em `src/config.rs` e recompile o release
   **fora da janela 23h–01h** (`cargo build --release`, ver seção 2, passo 3).
   Se foi só um atraso pontual, o concurso entra no ciclo seguinte e a
   contagem zera sozinha.

### Pausar a tarefa agendada

Vale para os procedimentos abaixo que gravam. O nome da tarefa é
`rust_spider ciclo`.

```powershell
Disable-ScheduledTask -TaskName "rust_spider ciclo"   # não dispara mais às 23h00
Get-ScheduledTaskInfo -TaskName "rust_spider ciclo"   # LastTaskResult, NextRunTime
Get-Process sync -ErrorAction SilentlyContinue        # tem sync vivo agora?
# ... trabalho manual ...
Enable-ScheduledTask -TaskName "rust_spider ciclo"    # não esqueça de religar
```

Se `Disable-ScheduledTask`/`Enable-ScheduledTask` der acesso negado, abra o
PowerShell como administrador (se a tarefa foi registrada com S4U, sem
`-SoConectado`, pode exigir PowerShell elevado).

`scripts\registrar_tarefa.ps1 -Remover` apaga a tarefa de vez (para voltar,
rode `scripts\registrar_tarefa.ps1`, ver "Agendamento"). Desabilitar não
interrompe uma execução que já começou: se `Get-Process sync` mostrar um `sync`
vivo, espere ou veja 1b.

### 1. Bloqueio

#### 1a. O site da Caixa barrando o acesso automatizado

**Sintoma.** Nos rascunhos e logs: `elemento não encontrado (seletor ...) — a
página pode ter mudado de layout ou bloqueado o acesso`, `elemento de download
não encontrado`, `nenhum download foi iniciado após clicar no seletor ...`,
`página do último concurso incompleta`, ou `erro do navegador ao acessar ...`
com timeout (cada espera por elemento dura 20 s; são 3 tentativas curtas por
passada e o `ciclo --aguardar` repete em rodadas). O que indica bloqueio é ser
**persistente**: todas as rodadas, várias noites. Erro de forma da página gera o
rascunho `site possivelmente mudou` já na 1ª noite; erro de navegador/rede,
`falhas seguidas` na 2ª.

**Diagnóstico.**

1. Separe **rede fora** de **bloqueio**:

   ```powershell
   Test-NetConnection loterias.caixa.gov.br -Port 443    # TcpTestSucceeded : True?
   ```

   Abra também a página da loteria no seu navegador (a URL está em
   `pagina_url`, `src/config.rs`; ex.: `https://loterias.caixa.gov.br/Paginas/Quina.aspx`).
   Sem rede, ou site fora do ar para você também: é rede/site, não bloqueio;
   espere e rode `sync ciclo <loteria>` depois (o N = 2 noites do alerta cobre
   isso).
2. Se o seu navegador abre normal, rode o robô com a janela visível e olhe o
   que ele recebe:

   ```powershell
   sync atualizar quina --visivel
   ```

   Página de "acesso negado", captcha, verificação de robô, página em branco
   ou um layout diferente do que você vê no seu navegador = **bloqueio**. A
   página igual à sua mas o robô não acha o elemento = **seletor mudou**
   (seção 2). Falhou uma vez e na segunda passou = instabilidade pontual.

**Correção (contorno manual, sem depender do robô).** Enquanto o robô está
barrado, alimente os bancos à mão, fora da janela 23h–01h (ou com a tarefa
pausada), e siga o ciclo nos passos que não tocam o site:

```powershell
# a) Planilha: no seu navegador, na página da loteria, use o botão de download
#    dos resultados e salve o .xlsx (ex.: downloads\quina_manual.xlsx). Depois:
sync importar quina downloads\quina_manual.xlsx   # valida e grava (tudo ou nada); reaplica o cache de inserções manuais
sync validar quina

# b) Um concurso só (a planilha da Caixa demora a atualizar): digite-o
sync buracos quina                                # só buracos INTERNOS; para o último, use listar/calendario
sync validar quina                                # imprime "N concurso(s) salvo(s), do X ao Y": Y é o último gravado
sync calendario quina                             # estado do último concurso e o que se espera
sync inserir quina 7141 --data 2026-10-14 --dezenas 5,17,33,41,70

# c) Confira o que digitou contra o site (concurso, data e dezenas);
#    a saída do listar é JSON, um campo por linha (na Lotofácil 15 dezenas
#    ocupam 15 linhas), então pegue o final com folga
sync validar quina                                # o último concurso (Y) deve ser o digitado
sync listar quina | Select-Object -Last 25

# d) Resto do ciclo, sem o site
sync features quina
sync indice quina --perfil padrao_v1
sync exportar quina
Get-Content publico\v1\quina\ultimo.json          # confira contra o site ANTES de publicar
sync publicar
```

**Digitou errado?** `inserir` só fecha buraco: com o concurso já gravado ele
recusa ("já está no banco; `inserir` só fecha buracos"), então não dá para
corrigir por ele. Só a planilha oficial corrige, e **apenas** quando você a
aplica de forma explícita, fora da janela 23h-01h: `sync baixar <loteria>` ou
`sync importar <loteria> <arquivo>` sobrescrevem a linha do banco (upsert em
`db.rs`). `sync atualizar` e `sync ciclo` **não** corrigem concurso já
gravado: se a página traz o mesmo concurso que você digitou, o `atualizar` o
trata como "sem novidade", só concilia o cache (emitindo o aviso "difere") e
não grava nada; o backfill do ciclo só roda quando há buraco. Com a tarefa
habilitada, o ciclo repetiria e publicaria o dado errado noite após noite.

1. Pause a tarefa até corrigir (`Disable-ScheduledTask`, ver "Pausar a tarefa
   agendada"; religue depois).
2. Aplique a planilha oficial com `sync baixar <loteria>` ou
   `sync importar <loteria> <arquivo>`, confira com `sync validar` e
   `sync listar`, e corrija ou apague a entrada errada em
   `cache\manuais_<loteria>.json` (senão o aviso se repete).
3. Refaça `features`, `indice` e `exportar` (obrigatório se já exportou ou
   publicou o dado errado), confira `publico\v1\<loteria>\ultimo.json` contra
   o site e só então `publicar`.

Atenção: o texto do aviso "difere" ("o banco fica com o oficial") só vale
quando vem do `baixar`/`importar`. Vindo do `atualizar`/`ciclo`, o banco **não**
foi alterado e continua com o valor digitado.

`sync atualizar` e `sync ciclo` consultam o site, então não servem enquanto
durar o bloqueio; `importar`, `inserir`, `features`, `indice`, `exportar` e
`publicar` não abrem navegador.

**Tarefa agendada enquanto isso.** Pode ficar habilitada: ela só falha, sem
estragar nada (loteria que falha não é exportada nem publicada) e deixa o
rastro (rascunho na 1ª noite com erro de página ou na 2ª com erro de rede, e
depois só um lembrete a cada 7 noites). Se você está alimentando os bancos à
mão todo dia, pause-a (`Disable-ScheduledTask`) para ela não tomar a trava às
23h nem fazer tentativas contra um site que já recusa você. Em qualquer caso,
**não aumente a frequência**: insistir só piora o bloqueio.

**Correção definitiva.** Não há contorno automático hoje (só `--visivel` e o
User-Agent fixo em `src/downloader.rs`). O item "Robustez da raspagem" do
Backlog em `docs/PLANEJAMENTO.md` (XPath em vez de seletores CSS e técnicas
anti-detecção, "caso o site volte a bloquear o acesso automatizado") é o
caminho: trate-o como tarefa própria.

**Conferir que voltou.** `sync atualizar <loteria>` termina com "Já está
atualizado" ou grava o concurso novo; `sync ciclo <loteria>` termina em exit 0
e `Fim: sucesso` aparece em `logs\sync_<dia>.log`. Na noite seguinte da tarefa,
sem `.eml` novo (`sync alertas` sem pendências): o sucesso zera o contador de
falhas sozinho. Mova os rascunhos tratados para `alertas\enviados\`.

#### 1b. Execução travada (`sync.lock` ocupado, processo morto, `sync.exe` órfão)

**Sintoma.** `outra execução em andamento: PID 4120, comando ciclo, desde
23:00` e saída com **código 3**; rascunho `trava ocupada` (na 2ª noite
seguida); ou rascunho `processo morto` / `logs\agendador.log` cuja última
linha é um `inicio | ... pid=...` sem a linha de fim (`->`).

**Diagnóstico.**

```powershell
Get-Content sync.lock.dono                           # PID, comando e hora de início (só para a mensagem)
Get-Process sync -ErrorAction SilentlyContinue       # há mesmo um sync vivo? compare o Id com o PID acima
Get-Content logs\agendador.log -Tail 6               # início/fim de cada execução da tarefa
Get-Content "logs\sync_$(Get-Date -Format yyyy-MM-dd).log" -Tail 30
Get-ScheduledTaskInfo -TaskName "rust_spider ciclo"  # último resultado no Agendador
```

- **Há um `sync` vivo e é legítimo** (rodada longa do `--aguardar` entre 23h e
  01h, ou um comando seu em outra janela): espere terminar. A trava é do
  sistema operacional e some quando o processo termina.
- **Há um `sync.exe` vivo e preso**: o `--aguardar` não passa da janela (até
  ~01h00), mas o limite de 3 h do Agendador pode deixar um `sync.exe` órfão
  (o PowerShell da tarefa morre e o filho nem sempre). Um `sync` vivo bem
  depois de 01h00 que não escreve no log do dia há muito tempo está preso.
  Confirme que o `Id` é o do `sync.lock.dono` (ou o único `sync`) e encerre:

  ```powershell
  Stop-Process -Id 4120      # troque pelo PID confirmado
  ```

- **Não há `sync` vivo mas a mensagem persiste**: não existe trava órfã (a
  trava é liberada pelo sistema operacional mesmo se o processo morrer à
  força ou a máquina reiniciar). Confira se o PID não é de um `sync` de
  **outra pasta de trabalho** e rode de novo; um `sync.lock.dono` velho sozinho
  não bloqueia nada.
- **Nunca apague o `sync.lock`.** Apagar o arquivo enquanto um processo o
  segura permitiria duas execuções gravando ao mesmo tempo. O `sync.lock.dono`
  é só texto de apoio e pode ser ignorado.
- **`processo morto`**: a execução anterior foi interrompida (limite de 3 h,
  queda de energia, `taskkill`, PC desligado). O log do dia (sem `Fim:`) mostra
  até onde foi. Cada etapa grava tudo ou nada nos bancos e em `publico\v1`,
  então nada fica gravado pela metade. **Exceção:** o clone `..\rust_spider_dados`.
  Um processo morto no meio do `publicar` pode deixá-lo sujo, e o próximo
  `publicar` recusa ("tem alterações locais"); a limpeza está em 3c.

**Correção.** Com a trava livre (nenhum `sync` vivo):

```powershell
sync ciclo todas        # ou uma loteria; idempotente, completa o que faltou
```

**Conferir que voltou.** `Get-Process sync` sem resultado depois de 01h00;
`Fim: sucesso` no log do dia; na noite seguinte, `agendador.log` com a linha de
início seguida da de fim (`exit 0`); `sync alertas` sem pendências (o rascunho
de `trava ocupada` só sai na 2ª noite, e um `ciclo` que chega ao fim zera o
contador do "geral").

### 2. Seletor mudou

**Sintoma.** Rascunho `site possivelmente mudou` na 1ª noite; erros
`elemento não encontrado (seletor ...)`, `elemento de download não encontrado
(seletor ...)`, `página do último concurso incompleta` ou, se foi a
**planilha** que mudou de forma, `tabela de resultados não encontrada ou em
layout inesperado`, `erro ao ler a planilha`, `dado inválido: ...` (todos viram
o mesmo motivo).

**Diagnóstico.**

1. Confirme com o robô visível e, em paralelo, com o seu navegador:

   ```powershell
   sync atualizar quina --visivel
   ```

   Se a página abre normal e o erro cita um seletor, o elemento mudou de lugar
   ou de nome (se nem abre, ou pede verificação, é a seção 1a). No seu
   navegador, abra a página da loteria, F12, inspecione o botão de download
   dos resultados, o título "Concurso NNNN (dd/mm/aaaa)" e uma dezena do
   último resultado, e anote o novo seletor CSS de cada um.
2. Onde ficam os seletores: em `src/config.rs`, um bloco `LoteriaConfig` por
   loteria, com `pagina_url` e:

   | Campo | O que acha | Lotofácil | Mega-Sena e Quina |
   |---|---|---|---|
   | `seletor_botao_download` | botão que baixa a planilha | `#btnResultados` | `#btnResultados` |
   | `seletor_concurso_data` | texto "Concurso 3794 (01/10/2026)" | `.title-bar h2 span.ng-binding` | `#tituloResultadoConcurso` |
   | `seletor_dezenas` | cada `<li>` de dezena do último resultado (na página) | `ul.simple-container.lista-dezenas.lotofacil li.dezena` | `#ulDezenas li` |

   A Lotofácil usa o template AngularJS antigo (classes `ng-*`, mais frágeis);
   Mega-Sena e Quina já usam IDs fixos. A Caixa ainda está migrando os
   templates, então é de se esperar que a Lotofácil mude primeiro.
3. **Testar sem recompilar:** só o seletor do **botão de download** tem a
   opção `--seletor`, e só no `baixar` (`atualizar` e `ciclo` não a têm;
   `seletor_concurso_data` e `seletor_dezenas` só mudam em `src/config.rs`).
   Use um banco e uma pasta de teste para não mexer nos reais (`*.db` e
   `/downloads` estão no `.gitignore`; `downloads_teste` apague depois):

   ```powershell
   sync baixar quina --seletor "#novoBotao" --visivel --db teste_quina.db --dir downloads_teste --cache cache_teste
   ```

   O `--cache cache_teste` é para o `baixar` não reaplicar nem limpar o
   `cache\manuais_*.json` real. `--seletor` não vale com `todas`. Se funcionar, o valor que deu certo vai
   para `seletor_botao_download` da loteria.
4. **Layout da planilha** (as colunas mudaram): o erro vem acompanhado de uma
   mensagem no stderr e no log (nível `ERRO`), como esta:

   ```text
   [quina] encontrei 7140 linha(s) na planilha, mas nenhuma bateu com o layout
   configurado (coluna_concurso=0, coluna_data=1, primeira_coluna_dezena=2).
   Exemplo de linha crua, célula por índice: [...]
   ```

   O exemplo mostra cada célula pelo índice: veja em que posição estão o
   número do concurso, a data e a primeira dezena e ajuste `coluna_concurso`,
   `coluna_data` e `primeira_coluna_dezena` da loteria em `src/config.rs`. Para
   testar sem rede, com uma planilha já baixada (as do robô ficam em
   `downloads\<loteria>_<número>.xlsx`), use um banco **e um cache** de teste
   (sem `--cache`, o `importar` usaria o `cache\` real e o limparia):

   ```powershell
   $planilha = Get-ChildItem downloads\quina_*.xlsx | Sort-Object LastWriteTime | Select-Object -Last 1
   sync importar quina $planilha.FullName --db teste_quina.db --cache cache_teste
   ```

**Correção.**

1. Edite `src/config.rs` (só o campo que mudou, na loteria certa).
2. Teste sem gravar nos bancos reais:

   ```powershell
   cargo test
   Copy-Item resultados_quina.db teste_quina.db     # cópia só para o teste
   cargo run --bin sync -- atualizar quina --db teste_quina.db --cache cache_teste --visivel
   ```

   `atualizar` sobre a cópia confere os seletores da página (sai "Já está
   atualizado" ou grava na cópia). O `--cache cache_teste` evita que o teste
   mexa no `cache\manuais_*.json` real. Se a cópia estiver atrás da página
   por mais de um concurso, o `atualizar` baixa a planilha inteira e faz
   backup da cópia em `backups\`; é só ruído de teste, apague depois. `cargo test` não acessa o site: só garante
   que a mudança não quebrou o resto; se você mexeu em colunas, testes que
   usam o layout antigo precisam acompanhar a mudança.
3. **Recompile o release fora da janela 23h–01h** (com o `sync.exe` em uso o
   build falha). A tarefa agendada roda o `target\release\sync.exe`; sem este
   passo ela continua com o seletor velho:

   ```powershell
   cargo build --release
   ```

4. Rode o ciclo real, à mão, fora da janela:

   ```powershell
   sync ciclo quina
   ```

5. Apague os arquivos de teste e mova o rascunho já tratado:

   ```powershell
   Remove-Item teste_quina.db
   Remove-Item downloads_teste -Recurse -ErrorAction SilentlyContinue
   sync alertas                                          # lista os .eml pendentes
   New-Item -ItemType Directory -Force alertas\enviados | Out-Null
   Move-Item alertas\<arquivo>.eml alertas\enviados\    # o nome que o "sync alertas" mostrou
   ```

**Conferir que voltou.** `sync ciclo <loteria>` com exit 0 e `Fim: sucesso`;
`sync validar <loteria>` sem erros; `publico\v1\<loteria>\ultimo.json` com o
último concurso; a tarefa na noite seguinte sem rascunho novo (o sucesso zera
o contador). Se o site mudou em **todas** as loterias, repita por loteria (ou
`sync ciclo todas`).

### 3. Reprocessar do zero

Em níveis, do mais leve ao mais pesado: **use o mais leve que resolve**. Todos
tomam a trava e devem ser feitos com a tarefa agendada fora da janela ou
pausada (ver "Pausar a tarefa agendada"). As tabelas derivadas
(`features_dezena`, `motor_estado`, `indice`, `perfis`) são regeneráveis a
partir de `resultados`; `resultados` não.

#### 3a. Só as derivadas

Quando: features ou índice desatualizados ou incoerentes (mensagens `rode
sync features`, `regenere com sync features --completo`), `dataset
incoerente`.

```powershell
sync features <loteria> --completo         # apaga e recria features_dezena + motor_estado a partir de resultados
sync indice <loteria> --perfil padrao_v1   # apaga e recalcula o índice do perfil
sync exportar <loteria>                    # ou: sync exportar todas
sync publicar                              # "nada a publicar" também é sucesso
```

Para as três loterias, repita por loteria (`features` e `indice` não aceitam
`todas`). Não existe comando que apague as tabelas derivadas; `features
--completo` e `indice` já as refazem do zero, com o mesmo efeito. `features`
recusa rodar se o `validar` apontar erro (buraco incluso): feche com `sync
buracos` e `sync inserir`. Se você editou o `.toml` de um perfil já registrado,
`indice` recusa (perfis são imutáveis): desfaça a edição.

**Conferir.** `sync ciclo <loteria>` com exit 0; rodado de novo, não muda nada
nem republica ("Nada a publicar").

#### 3b. Banco recriado a partir da planilha

Quando: `resultados` suspeito (dados errados que o `validar` não cobre,
planilha corrigida pela Caixa com datas antigas alteradas, banco corrompido).

```powershell
New-Item -ItemType Directory -Force backups | Out-Null
Copy-Item resultados_quina.db backups\resultados_quina_manual.db   # backup (os bancos não usam WAL: copiar o arquivo basta)
Get-ChildItem resultados_quina.db*                                 # só o .db? Se houver -journal/-wal, mova-os junto (corrompido)
Move-Item resultados_quina.db resultados_quina_velho.db            # tira do caminho; guarde até conferir
sync baixar quina                  # baixa a planilha e cria o banco novo
sync validar quina
```

Se a listagem acima mostrar também `resultados_quina.db-journal` ou
`-wal`, mova-os para `resultados_quina_velho.db-journal` / `-wal` (um resto
deixado ao lado do banco novo seria aplicado a ele).

O `baixar` reaplica sozinho o cache de inserções manuais
`cache\manuais_<loteria>.json` (se existir; a entrada que a planilha já traz é
removida do cache quando igual ou avisada quando diferente) e as correções
conhecidas (`CORRECOES_DATA` em `src/config.rs`; hoje, a data do concurso 380 da
Quina). Sem acesso ao site, use a planilha baixada à mão:
`sync importar quina downloads\quina_manual.xlsx`. O banco novo nasce só com
`resultados`.

**Confira antes de exportar e publicar.** Compare o banco novo com o velho e
com o site: a linha "N concurso(s) salvo(s), do X ao Y" de `sync validar quina --db
resultados_quina_velho.db` contra a de `sync validar quina` (contagem e último
concurso devem bater, salvo o que a planilha corrigiu de propósito),
o último concurso contra o site, e `sync validar quina` sem erros. Se algo não
bater, **não exporte nem publique**: volte atrás com `Move-Item
resultados_quina_velho.db resultados_quina.db -Force`.

Só então faça **o nível 3a** (o `indice` registra o perfil de novo e o
`criado_em` do perfil passa a ser o de agora):

```powershell
sync features quina
sync indice quina --perfil padrao_v1
sync exportar quina
sync publicar
```

Depois de publicado e com `ultimo.json` conferido, apague
`resultados_quina_velho.db` e o backup. Para voltar atrás depois de publicar,
restaure o velho e refaça `features`, `indice`, `exportar` e `publicar`.

#### 3c. `publico/` e o clone de dados

Quando: `publico\v1` apagado ou corrompido, `index.json` que não bate com os
arquivos, `publicação: ...` recusando, clone de dados sujo.

```powershell
sync exportar todas        # regrava publico\v1 a partir dos bancos (precisa de features e índice em dia: 3a)
sync publicar              # copia para ..\rust_spider_dados e envia (push)
```

`publico/` é só saída: pode ser apagado e o `exportar todas` o refaz com os
mesmos bytes (exceto `gerado_em`); `publicar` diz "nada a publicar" se só isso
mudou. Push que falhou deixa o commit local no clone e rodar `sync publicar`
de novo reenvia. **Clone sujo** (um `publicar` morto no meio da cópia deixa
arquivos alterados em `..\rust_spider_dados`; o próximo `publicar` recusa com
"tem alterações locais (git status não está limpo)", sem copiar nada): veja
`git -C ..\rust_spider_dados status`. O clone é espelho de `publico\v1`, então
descarte as alterações locais de `v1/` (`git -C ..\rust_spider_dados restore
--staged --worktree v1` e `git -C ..\rust_spider_dados clean -fd v1`) e rode
`sync publicar`, que refaz a cópia. Se o `status` mostrar algo fora de `v1/`,
olhe antes de descartar. Se o clone `..\rust_spider_dados` sumiu, o `publicar` recusa
e o erro traz o `git clone` a rodar; use esse clone só para a publicação (não
edite à mão). Ver o comentário do `publicar` em "Uso" e `docs/contrato-v1.md`.

**Conferir.** `sync publicar` termina com sucesso; `git -C ..\rust_spider_dados
status` limpo e `git -C ..\rust_spider_dados log -1` com a mensagem
"Dados: ...".

#### 3d. O que NÃO se apaga nem se reseta

- **`registros\trancado_<loteria>.json`**: registra o uso único do conjunto
  trancado (perfil, hash, até qual concurso, quando). Deve ficar **versionado no
  git** (hoje `registros\` ainda não está no repositório: commite depois de
  cada `avaliar --trancado`), não no banco. Apagá-lo ou editá-lo libera um segundo uso do trancado e invalida o
  Bonferroni. Banco recriado não o afeta.
- **`cache\manuais_<loteria>.json`**: concursos digitados à mão que a planilha
  ainda não traz; também deve ficar **versionado no git** (hoje `cache\` ainda
  não está no repositório: commite depois de cada `inserir`). É o que permite recriar um banco sem
  perdê-los. Quem o limpa é o próprio `baixar`/`importar`, quando a planilha
  passa a trazer o concurso igual.
  Uma entrada com data futura (digitada errada) faz toda gravação da
  loteria ser recusada ("lote recusado"), pois o `baixar`/`importar` a
  reaplica: corrija a data no arquivo (ou remova a entrada) e rode de novo.
- **`perfis\`**: os `.toml` de pesos (imutáveis, com hash registrado no banco).
  Não os edite depois de usados; crie a versão seguinte.
- **`alertas\estado.json`**: histórico por alvo (noites seguidas de falha,
  motivos já alertados). É seguro apagar, mas o efeito é: contadores zerados
  (um problema de rede só volta a alertar na 2ª noite seguinte) e a memória de
  "já avisei" perdida, então um problema que ainda dure gera um rascunho de
  novo e o lembrete de 7 noites recomeça. Arquivo ilegível recebe o mesmo
  tratamento (recomeça do zero, com aviso) sem derrubar o ciclo. Não apague no
  meio de um incidente que você quer acompanhar.
- **`resultados_<loteria>.db`**: só pelo procedimento 3b, com backup e com
  `baixar`/`importar` em seguida.

### Referência: comportamento nas falhas simuladas

- **Seletor errado** (`ElementoNaoEncontrado` em todas as rodadas): 1ª noite =
  rascunho `site possivelmente mudou`; nas noites seguintes nada de novo (já
  avisado), só o lembrete a cada 7 noites. Se o erro virar de rede, vale o N e
  sai um rascunho `falhas seguidas` à parte.
- **Rede fora** (erro de navegador em todas as rodadas): 1ª noite só registra
  em `estado.json`; 2ª noite = rascunho `falhas seguidas (2 noites)`.
- **Trava ocupada**: com um `sync` em andamento, outro `sync` sai com 3; o
  agendado conta no "geral" e alerta na 2ª noite.
- **Disco sem espaço ou `alertas/` sem permissão**: aviso no console e no log
  (`não foi possível gravar ...`); o ciclo termina com o código de sempre e o
  alerta é tentado de novo na próxima execução.
- Os testes (`tests/alertas.rs`, `tests/agendamento.rs`) cobrem esses
  cenários com loteria falsa e binário falso, em pastas temporárias.

## Experimentos em Python (`ml/`)

Python só experimenta e propõe perfis; nada da produção depende dele. Lê o
dataset do `sync dataset` **sempre pelo manifesto** (confere SHA-256, linhas,
`feature_version`, corte e a impressão dos resultados; recusa o trancado) e
refaz o índice, a calibração e as métricas do `sync avaliar`.

```bash
# ambiente (Python 3.12; versões fixadas em ml/pyproject.toml; ml/.venv fica fora do git)
py -3.12 -m venv ml/.venv
ml/.venv/Scripts/python -m pip install --group ml/pyproject.toml:ambiente

# testes (a partir da raiz)
ml/.venv/Scripts/python -m pytest ml/

# experimento 001: acaso K/N e perfil padrao_v1 nas 5 dobras do walk-forward
cargo run --release --bin sync -- dataset lotofacil          # gera datasets/lotofacil/
ml/.venv/Scripts/python ml/experimentos/001_baseline.py datasets/lotofacil/manifesto_parquet.json

# opcional: conferir com o Rust em precisão total (o `sync avaliar` imprime só 6 casas)
cargo run --release --example referencia_avaliacao -- lotofacil padrao_v1 resultados_lotofacil.db > ref_lotofacil.json
ml/.venv/Scripts/python ml/experimentos/001_baseline.py datasets/lotofacil/manifesto_parquet.json --referencia ref_lotofacil.json
```

O contrato do JSON público do app (`sync exportar`: campos, URL base, exemplo
real) e do dataset e do manifesto que o Python lê (colunas, tipos, chaves do
manifesto e o que muda entre versões) está em `docs/contrato-v1.md`.

### Busca de pesos (experimento 002)

Regras completas em `docs/decisoes/fases-1-e-1.5.md`, Fase 1.5, "Regras da 1.5.3". Em
resumo:

- **o que varia**: os 9 pesos (sinal livre, soma dos `|w|` = 1, arredondados
  a 6 casas antes de avaliar), a janela (0, 10, 50, 100) e a normalização
  (zscore, minmax);
- **método**: busca aleatória com semente fixa (padrão 20261006), o mesmo
  número de candidatos em cada uma das 8 combinações janela × normalização
  (padrão 2.000 → 16.000 candidatos por loteria);
- **métrica**: log loss médio por concurso, com a calibração pela posição do
  `sync avaliar` (m = 100); Brier e top-k só informativos;
- **walk-forward aninhado** nas 5 dobras do trecho de busca: o melhor nas
  dobras 1…i−1 é medido na dobra i (estimativa honesta do método contra o
  acaso K/N); o perfil gravado é o melhor nas 5 dobras (dentro da amostra,
  otimista);
- **só o trecho de busca**: o Python recusa o manifesto do trancado.

```bash
# 1) datasets da busca numa CÓPIA do banco e numa pasta própria (sem --trancado)
mkdir -p C:/tmp/busca && cp resultados_lotofacil.db C:/tmp/busca/
cargo run --release --bin sync -- dataset lotofacil --db C:/tmp/busca/resultados_lotofacil.db --saida C:/tmp/busca/datasets
# (o mesmo para megasena e quina)

# 2) busca (uma ou mais loterias; --processos só muda o tempo, não o resultado)
ml/.venv/Scripts/python ml/experimentos/002_busca_pesos.py \
    C:/tmp/busca/datasets/lotofacil/manifesto_parquet.json \
    C:/tmp/busca/datasets/megasena/manifesto_parquet.json \
    C:/tmp/busca/datasets/quina/manifesto_parquet.json \
    --processos 4                      # opcionais: --semente 20261006 --orcamento 2000 --perfis perfis
```

- **Saída:** um perfil por loteria em `perfis/<loteria>/busca_v<N>.toml` (N =
  a versão seguinte à maior que já existe na pasta; **nunca sobrescreve**),
  com o experimento no cabeçalho (semente, orçamento, dataset com SHA-256,
  walk-forward), e o relatório no stdout. Tudo ou nada: todos os manifestos
  são validados e todas as buscas rodam antes de gravar o primeiro `.toml`.
  A versão só olha os arquivos da pasta: o nome não pode já estar registrado
  na tabela `perfis` de um banco com outro conteúdo (o `sync indice` recusa;
  perfis são imutáveis).
- **O que não faz:** não grava em banco, não lê nem roda o trancado e não
  troca o perfil ativo. O `.toml` é uma proposta para revisão.
- **Tempo:** ~19 min as 3 loterias com `--processos 4` (busca completa).

Para conferir um perfil gerado com o Rust (na cópia do banco):

```bash
cargo run --release --bin sync -- indice lotofacil --perfil busca_v1 --db C:/tmp/busca/resultados_lotofacil.db
cargo run --release --bin sync -- avaliar lotofacil --perfil busca_v1 --db C:/tmp/busca/resultados_lotofacil.db --testados 16000
```

`--testados 16000` (o número de candidatos testados, impresso pelo 002)
aplica Bonferroni; mesmo assim o p-valor no trecho de busca sai otimista
para o vencedor, escolhido nesses mesmos sorteios. Resultado esperado:
empatar com o acaso. A validação que vale é a do trancado (`sync avaliar
--trancado`), uma única vez e por decisão do usuário.

As fixtures de `ml/tests/fixtures/` (dataset CSV/Parquet de um banco sintético
de 40 concursos, saídas do `sync avaliar` e referências do Rust) são geradas
pelo próprio Rust com `ml/.venv/Scripts/python ml/ferramentas/gerar_fixture.py`
(`--bancos PASTA` com **cópias** dos bancos reais regenera também
`dobras_rust.json`, os limites reais das dobras).

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
