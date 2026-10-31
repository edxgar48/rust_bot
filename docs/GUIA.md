# Guia dos comandos do `sync`

Como usar cada comando, como ler as respostas e um roteiro para conferir à mão
o que o projeto produz. As saídas de exemplo são reais (Mega-Sena, perfil
`padrao_v1`); os números das seções de resultados têm a data no próprio
subtítulo e mudam a cada sorteio.

Definições completas (features, índice, avaliação): `docs/PLANEJAMENTO.md`
e, para as Fases 1 e 1.5, `docs/decisoes/fases-1-e-1.5.md`.

## 1. O caminho dos dados

```
 Site da Caixa
      │  baixar / atualizar / importar      (coleta)
      ▼
 resultados            ← validar, buracos, inserir, listar   (integridade)
      │  features
      ▼
 features_dezena + motor_estado            (estatísticas por dezena e janela)
      │  indice --perfil X
      ▼
 indice + perfis                           (índice de força: total, posição, variação)
      │  avaliar --perfil X
      ▼
 relatório no terminal (não grava nada; só --trancado grava em registros/)

 aleatoriedade  → lê só resultados, independente de perfil
 evidencia      → lê resultados (parte A) e, para a parte B, o índice do perfil
 tabela         → CSV do concurso, com grau e direção da evidência por dezena
 ranking        → taxa histórica por posição e faixa; ordena as dezenas (descrição do passado)
 posicoes       → índice de força das POSIÇÕES (G-1), em tabelas posicao_* próprias (descrição do passado)
```

Cada loteria tem o próprio banco (`resultados_<loteria>.db`). As tabelas
`features_dezena`, `motor_estado`, `indice` e `perfis` são derivadas e podem
ser apagadas e refeitas a partir de `resultados`.

**Como rodar.** Rode sempre a partir da pasta do projeto. O jeito mais rápido
é compilar uma vez e chamar o executável:

```bash
cargo build --release --bin sync
./target/release/sync <comando> <loteria> [opções]
```

`<loteria>` é `lotofacil`, `megasena` ou `quina`. Alguns comandos aceitam
também `todas`. `--help` em qualquer comando mostra as opções.

## 2. Comandos por etapa

### Coleta

| Comando | Para quê | `todas`? |
|---|---|---|
| `atualizar <lot>` | **Uso diário.** Lê o último resultado na página e grava se for novo. Se houver salto (concursos perdidos), faz backup do banco em `backups/` e baixa a planilha completa | sim |
| `baixar <lot>` | Baixa a planilha completa da Caixa e grava. Use para refazer o histórico ou quando o `atualizar` não resolver | sim |
| `importar <lot> <arquivo.xlsx>` | O mesmo que o `baixar`, mas com uma planilha que você já baixou à mão. Não abre navegador | não |

Como ler a saída:

- `Concurso 3066 (2026-10-03) gravado...`: deu certo.
- `Já está atualizado...`: não havia novidade.
- `Salto: ... Backup do banco em ...`: faltavam concursos. Ele copiou o banco
  e baixou a planilha.
- `Correção conhecida: concurso 380...`: é normal. É um erro da planilha da
  Caixa que o programa corrige sozinho.
- `aviso: inserção manual ... difere da fonte oficial`: uma inserção feita à
  mão estava errada. O banco fica com o resultado oficial, e é preciso tirar
  a entrada de `cache/manuais_<lot>.json` à mão.

### Integridade

| Comando | Para quê |
|---|---|
| `validar <lot\|todas>` | Confere quantidade de dezenas, faixa, repetidas, datas e buracos |
| `buracos <lot>` | Lista os concursos que faltam, com as datas dos vizinhos |
| `inserir <lot> <concurso> --data AAAA-MM-DD --dezenas 1,2,...` | Fecha um buraco à mão e registra em `cache/manuais_<lot>.json` |
| `listar <lot>` | Mostra o banco inteiro em JSON. A saída é longa |

Como ler a saída:

- **Tudo certo:** `3066 concurso(s) salvo(s), do 1 ao 3066. Nenhum problema encontrado.`
- **Problema:** aparece `N erro(s): concurso X faltando` e o comando termina
  com erro. Antes de qualquer outra etapa, resolva com `baixar` ou com
  `buracos` + `inserir`, porque o `features` recusa rodar com buraco.

### Features

`features <lot> [--completo]` calcula, para cada dezena, as estatísticas
antes de cada concurso: frequência, espaços, repetições e assim por diante.
Calcula nas janelas de 10, 50 e 100 concursos e no histórico total
(janela = 0). Usa só o passado de cada concurso.

```
Modo: incremental.
1 concurso(s) incorporado(s); 480 linha(s) gravada(s) em features_dezena.
Último concurso: 3066; snapshot do próximo (3067) em features_dezena com saiu = NULL.
```

- **`incremental`:** é o normal, e ele calcula só o que é novo.
- **`completo (motivo)`:** ele recalculou do zero. Os motivos possíveis são:
  primeira vez, versão das fórmulas mudou ou um resultado antigo foi
  corrigido.
- **Snapshot do próximo:** é o estado de cada dezena antes do próximo
  sorteio. É o que a tabela geral do app vai mostrar.

### Índice de força

`indice <lot> --perfil padrao_v1` combina as features com os pesos do perfil
(`perfis/<lot>/padrao_v1.toml`) e gera, por concurso e dezena, o **total**, a
**posição** (1 = mais forte) e a **variação de posição** (positivo = subiu).
A cada execução, o índice inteiro do perfil é refeito.

```
Perfil padrao_v1: já registrado em perfis, sem alteração (hash 95a2...).
3067 concurso(s) indexado(s), do 1 ao 3067; 184020 linha(s) gravada(s) em indice.
```

Recusas comuns:

- **Features desatualizadas ou resultados alterados:** rode `features` antes.
- **Perfil alterado sob o mesmo nome:** perfis são imutáveis. Crie
  `padrao_v2.toml` com `versao = 2`.

### Avaliação contra o acaso

`avaliar <lot> --perfil padrao_v1 [--k K] [--testados N]` só lê o banco. Ela
usa só a **parte de busca**: até o concurso 3416 na Lotofácil, 2759 na
Mega-Sena e 6421 na Quina. Além do log loss, do Brier e do top-k, imprime uma
**linha extra e só informativa**, o log loss com a calibração linear (seção 3).

`--trancado` avalia o **conjunto trancado**, isto é, os concursos depois do
corte. **É de uso único por perfil** e fica registrado em
`registros/trancado_<lot>.json`. Só use quando for validar um perfil campeão.

`aleatoriedade <lot|todas>` testa se o próprio histórico parece aleatório.
Também só lê o banco.

`evidencia <lot> [--perfil padrao_v1] [--perfis pasta]` mostra **quanta
evidência os dados dão**, em cinco graus, de que cada dezena tem uma chance
diferente da esperada e de que o índice de força prevê melhor que o acaso. O
índice é medido com a calibração linear (um parâmetro), ao lado de um índice
inerte de comparação; a calibração por posição da 1c sai numa linha só
informativa. Também só lê o banco, e vale para uma loteria por vez (não aceita
`todas`). Como ler a saída: seção 5.

### Tabela geral em CSV

`tabela <lot|todas> --perfil padrao_v1 [--concurso N] [--saida tabelas]` grava
a tabela geral de um concurso num CSV para abrir no Excel: uma linha por
dezena, em ordem de posição, com total, variação de posição, `saiu`, as 9
features da janela do perfil e, nas duas últimas colunas, o grau e a direção
da evidência da dezena (ver abaixo). Só lê o banco e faz as mesmas
conferências do `avaliar` (índice e features em dia).

- **Arquivo:** `tabelas/<lot>/tabela_<lot>_<perfil>_<concurso>.csv`. Um
  arquivo por concurso: rodando depois de cada sorteio, a pasta acumula a
  tabela de cada semana para comparar.
- **Padrão:** o próximo concurso, ainda não sorteado (`data_sorteio` e
  `saiu` vazios). Com `--concurso N`, um concurso já sorteado, com `saiu`
  preenchido (1 = saiu): dá para ver onde estavam as dezenas sorteadas.
- **Formato:** separador `;`, vírgula decimal e UTF-8 com BOM, para o Excel
  em português reconhecer os acentos (sem o BOM, "a favor de viés" apareceria
  como "viÃ©s"); célula vazia = sem valor (por exemplo, `soma_variacoes`
  com menos de 3 aparições na janela).
- **Saída no terminal:** onde gravou e o top 10 (`*` = saiu, quando o
  concurso já foi sorteado).
- **Colunas `evidencia_grau` e `evidencia_direcao`** (as duas últimas, depois
  de `repeticao_atual`): a evidência bayesiana da dezena, com os mesmos
  rótulos do comando `evidencia`. Grau: `muito fraca`, `fraca`, `moderada`,
  `forte` ou `muito forte`. Direção: `a favor de viés` ou `a favor do acaso`.
  Como ler: seção 5, **antes de tirar conclusão de uma linha só**.
  - Usa o **histórico inteiro** (não a `janela` do perfil) e a priori
    principal de 5 %.
  - **Ponto no tempo:** a tabela do concurso N usa só os concursos até N−1.
    Em `--concurso 30`, a evidência é a que se tinha antes do sorteio 30, não
    a de hoje. No padrão (o próximo concurso), é a mesma da primeira janela do
    `evidencia`. No concurso 1 não há histórico e as duas colunas ficam vazias.
  - Como o `evidencia`, lê todos os concursos já sorteados, inclusive os do
    conjunto trancado: descreve o sorteio, não avalia perfil.

  Exemplo (Lotofácil, concurso 3800, o próximo; colunas `dezena`, `posicao` e
  as duas últimas):

  ```
  16;3;...;forte;a favor de viés
  20;25;...;moderada;a favor de viés
  21;12;...;fraca;a favor do acaso
  ```

### Ranking por taxa histórica

`ranking <lot> [--perfil padrao_v1] [--saida ranking] [--perfis pasta]` cruza,
para cada concurso passado, a posição que cada dezena tinha na tabela de força
**antes** do sorteio (e a faixa de cada um dos 9 componentes) com o resultado, e
ordena as dezenas do próximo concurso pela taxa de saída dessas posições e
faixas. Grava as tabelas derivadas `ranking_taxa`, `taxa_posicao` e
`taxa_faixa` (regeneradas a cada execução) e o CSV do próximo concurso em
`ranking/<lot>/`. Uma loteria por vez (não aceita `todas`) e as mesmas
conferências do `avaliar` (perfil registrado, features e índice em dia).

`ranking <lot> --avaliar` **não grava nada**: avalia esse ranking contra o
acaso, só nas dobras de teste 2 a 5 do walk-forward da busca (o trancado fica
fora e não há `--trancado`). Não aceita `--saida`. Como ler: seção 9.

### Índice de força das posições (G-1)

`posicoes <lot> [--perfil padrao_v1] [--perfil-posicoes posicoes_v1] [--saida posicoes]
[--perfis pasta]` marca, em cada concurso passado, as posições que a tabela de
força ocupava nas dezenas sorteadas, trata cada posição como se fosse uma dezena
e calcula sobre elas as mesmas features e o mesmo índice de força. O resultado é
o **G-1**: a ordem das posições, que o comando traduz em ordem de dezenas (a
dezena herda o G-1 da posição que ocupa no próximo concurso). Grava só as
tabelas `posicao_marcas`, `posicao_features`, `posicao_indice`, `posicao_perfil`
e `posicao_motor_estado` (nenhuma tabela existente muda, e o perfil das posições
não entra em `perfis`) e o CSV do próximo concurso em `posicoes/<lot>/`. Retoma do
estado salvo e calcula só os concursos novos. Uma loteria por vez e as mesmas
conferências do `ranking` (índice de força e features em dia).

`posicoes <lot> --avaliar` **não grava nada**: avalia o G-1 contra o acaso, com a
nota A do `ranking` como referência no mesmo recorte, só nas dobras de teste 2 a
5 do walk-forward da busca e **sem os 100 primeiros concursos** (aquecimento, em
que a tabela de força ainda não tem histórico). O trancado fica fora e não há
`--trancado`. Não aceita `--saida`. Como ler: seção 10.

### Dataset para o Python

`dataset <lot> [--formato parquet|csv] [--trancado] [--saida datasets]` grava
as features de todos os concursos já sorteados (uma linha por concurso,
dezena e janela, com `data`, `trecho` e `saiu`) para a pasta `ml/` ler. Só lê
o banco e recusa se as features não estiverem em dia (rode `features` antes).

- **Arquivos:** `datasets/<lot>/features_v1.parquet` (padrão) ou
  `features_v1.csv`, com `manifesto_parquet.json` / `manifesto_csv.json` ao
  lado (corte, linhas, SHA-256 do arquivo e duas impressões dos resultados:
  `impressao_resultados_banco`, do banco inteiro, que muda a cada sorteio, e
  `impressao_resultados_incluidos`, só dos concursos que estão no arquivo).
- **Trancado fora por padrão:** só entram concursos até o corte (3416 na
  Lotofácil, 2759 na Mega-Sena, 6421 na Quina). `--trancado` inclui o resto e
  só deve ser usado na validação final do perfil campeão. Ele grava em
  arquivos próprios, `features_v1_trancado.<ext>` e
  `manifesto_trancado_<ext>.json`, sem tocar nos arquivos da busca. A coluna
  `trecho` e o campo `trecho` do manifesto dizem o que entrou.
- **Mesmos dados, mesmos bytes:** rodar duas vezes dá o mesmo SHA-256.

## 3. Como interpretar o `avaliar`

**Cabeçalho.** Mostra o perfil, o hash, o trecho (`concursos 2–2759`) e o
corte. Confira se o trecho termina no corte, porque isso garante que o
trancado ficou fora.

**Tabela de log loss.** Mede quão boas são as probabilidades que o índice
atribui. Menor é melhor.

```
     dobra    concursos      T      perfil       acaso    diferença          EP  IC 95% (normal)
         1        2–552    551    0.326257    0.325083    +1.174e-3    2.249e-4  [+7.327e-4, +1.614e-3]
     total       2–2759   2758    0.325506    0.325083    +4.235e-4    7.070e-5  [+2.849e-4, +5.621e-4]
```

- **diferença = perfil − acaso.** Se for **negativa**, o perfil foi melhor
  que o acaso. Se for **positiva**, foi pior.
- **As 5 dobras** mostram se o resultado é estável ao longo do tempo. Um
  perfil bom de verdade teria a diferença negativa na maioria das dobras.
- **EP e IC normal** tendem a sair estreitos demais. Servem só como
  orientação, e quem decide é o veredito.

**Brier.** Lê-se do mesmo jeito que o log loss e serve só como informação.

**Log loss com a calibração linear (linha extra, só informativa).** Vem logo
depois do Brier, com a mesma tabela (cinco dobras e total) e a mesma leitura:
diferença = perfil − acaso, **negativa = perfil melhor**.

```
Log loss com a calibração linear na posição (só informativo: ...):
     dobra    concursos      T      perfil       acaso    diferença   (EP e IC 95% ...)
         2     685–1367    683    0.673067    0.673012    +5.523e-5   ...
     total       2–3416   3415    0.672961    0.673012    -5.022e-5   ...
```

- **Por que existe.** A calibração das tabelas acima (a da 1c) estima uma taxa
  de acerto por posição: 25, 60 ou 80 números aprendidos só com o passado.
  Esse aprendizado custa log loss mesmo para um índice sem informação, e é
  um dos motivos de a diferença dela sair positiva nas três loterias. A calibração
  linear usa **um parâmetro só**, `p = K/N + b·z(posição)`, com a posição 1
  valendo +1 e a posição N valendo −1. O custo de aprender é bem menor e a
  linha enxerga um sinal fraco que a régua por posição afogaria.
- **Não muda o veredito.** O bootstrap, o "SUPERA/NÃO supera", as tabelas de
  log loss e Brier, o top-k e o trancado seguem exatamente como eram, com a
  calibração por posição. A linha linear não tem bootstrap, e o trancado não
  ganha linha linear.
- **Como ler.** Uma diferença negativa aqui não é "supera o acaso". O IC normal
  é otimista (a calibração acumulada gera correlação serial), e o total desta
  linha inclui a dobra 1, que o `evidencia` não usa. Quem decide se o índice
  carrega informação é o `evidencia` (seção 5), que põe essa mesma diferença
  ao lado do que um índice inerte produziria.
- **Como casa com a parte B do `evidencia`.** Nas dobras 2 a 5, dobra a dobra,
  `ln FB de B = −pares × diferença desta linha`, com `pares` = concursos
  avaliados × N. Na Lotofácil, dobra 2: 17 075 pares × +5,523e-5 = +0,943, ou
  seja, ln FB = −0,943 e log10 FB = −0,410, o número da tabela por dobra do
  `evidencia`. O total desta linha (dobras 1 a 5) não se compara com o B
  inteiro, que cobre só as dobras 2 a 5.

**Top-k.** Quantas das k dezenas mais bem posicionadas saíram, em média por
concurso. Maior é melhor.

```
Top-6 (...; ao acaso k·K/N = 0.6000):
     total       2–2759   2758   0.5986   0.6000    -0.0014   0.0134   [-0.0276, +0.0248]  0.5452
```

- **perfil contra acaso:** 0,5986 acerto por concurso contra 0,6 ao acaso.
- **p exato:** é a chance de o acaso acertar pelo menos isso. Se for **menor
  que 0,05**, o top-k superou o acaso. Aqui deu 0,55, ou seja, nada.
- **Distribuição de acertos:** quantos concursos tiveram 0, 1, 2… acertos, ao
  lado de quanto o acaso daria. Se os números estiverem parecidos, o
  comportamento é de acaso.

**Veredito.**

```
=> NÃO supera o acaso no log loss: p = 1.0000 > α = 0.05.
```

- Vem de um bootstrap por blocos de 50 concursos, que é robusto à correlação
  entre concursos vizinhos.
- **Só se aparecer "SUPERA o acaso" com p ≤ 0,05** há algum sinal. E esse
  sinal ainda precisa se confirmar no trancado.

**Avisos no fim.**

- **Quando o p-valor vale:** os p-valores só valem se você não escolheu o
  perfil olhando este mesmo trecho. Se testou vários perfis, use
  `--testados N`.
- **Sobre o IC normal:** ele sai otimista, como explicado acima.

**Resultado com `padrao_v1` em 2026-10-05:**

| Loteria | Log loss | Top-k: p exato | Leitura |
|---|---|---|---|
| Lotofácil | não supera (p 0,9999) | 0,0014 | top-k acima do acaso: coincide com o desvio de frequência da 1c.4, então investigar os dados antes de concluir |
| Mega-Sena | não supera (p 1,0) | 0,55 | comportamento de acaso |
| Quina | não supera (p 1,0) | 0,010 | igual à Lotofácil |

**Linha linear com `padrao_v1` em 2026-10-10** (diferença perfil − acaso do log
loss; o veredito acima não mudou nas três):

| Loteria | Total (dobras 1–5) | Dobras 1 / 2 / 3 / 4 / 5 |
|---|---|---|
| Lotofácil | −5,02e-5 | −2,67e-4 / +5,5e-5 / +6,1e-5 / −2,7e-5 / −7,3e-5 |
| Mega-Sena | +9,86e-6 | +2,4e-6 / +3,1e-5 / −5,7e-6 / +1,3e-5 / +8,4e-6 |
| Quina | +3,63e-6 | +1,2e-5 / +3,1e-6 / +2,2e-6 / +1,1e-6 / −4,4e-7 |

Os sinais se misturam nas dobras (Lotofácil), o zero cabe no IC (Mega-Sena) ou a
diferença é minúscula (Quina, em que o IC normal, otimista, mal exclui o zero,
do lado positivo: perfil um pouco pior que o acaso). Em vez de +3,5e-4
(Lotofácil) ou +2,5e-4 (Quina) da régua por posição, as
diferenças ficam pequenas e, em boa parte, de sinal misto: o custo de
aprendizado quase sumiu e o que sobra está perto de zero.

## 4. Como interpretar o `aleatoriedade`

```
Qui-quadrado de frequência (...): estatística 89.4686 com 59 graus de liberdade, p = 0.0064 → REJEITA a uniformidade (p ≤ α).
  Esperado por dezena: 306.60 saídas
```

- **Qui-quadrado:** pergunta se todas as dezenas saem com a mesma frequência.
  Com **p ≤ 0,05**, a resposta é "não parece uniforme".

```
  dezena   saídas  sequências      média           p      p Holm
      26      250         481      460.2      0.0100      0.6002
  60 dezena(s) testada(s), 0 fixa(s); 2 com p ≤ α sem correção; 0 rejeitada(s) depois do Holm.
```

- **Runs test:** olha, para cada dezena, a sequência de "saiu / não saiu" ao
  longo dos concursos. **sequências** é o número de blocos seguidos de saiu
  ou não saiu, e **média** é o que o acaso daria. Muito abaixo da média quer
  dizer que a dezena aparece em rajadas. Muito acima quer dizer que ela
  alterna demais.
- **Olhe a coluna `p Holm`.** Ela já corrige o fato de testar 60 ou 80
  dezenas de uma vez. Um p sem correção abaixo de 0,05 é esperado só por
  acaso: 3 em 60 dezenas, em média.
- **Se houver rejeição,** confira primeiro os dados (erro de planilha), e só
  depois desconfie do sorteio.

**Resultado em 2026-10-05, com a conferência da tarefa 1d.1 (2026-10-09):**

- **Qui-quadrado:** rejeita a uniformidade nas três loterias. Na Lotofácil,
  o p é 0,00015. **Não é erro de dado:** os concursos foram conferidos contra
  duas fontes externas ao nosso pipeline. A API da Caixa cobriu todos os
  concursos da Mega-Sena e da Quina, mas só 1912 dos 3799 da Lotofácil; um
  espelho com o histórico completo cobriu todos nas três loterias. Só a data
  do concurso 380 da Quina diverge, o que já está corrigido no programa. As
  fontes são independentes do nosso pipeline, **não da Caixa**: se o erro
  estivesse na origem, as duas repetiriam o mesmo erro.
- **Não é um desvio concentrado em um trecho.** Uma leitura anterior dizia que
  o desvio da Lotofácil estava nos concursos 1899 a 3796. Isso foi refutado: o
  corte em 1899 foi escolhido depois de olhar os dados, não coincide com
  nenhuma mudança de regra, de local ou de equipamento, e as duas metades do
  histórico têm o **mesmo padrão** (a diferença entre elas não é
  significativa, p ≈ 0,55). Quem puxa o desvio são poucas dezenas: a 16 sai
  menos que o esperado, e a 20, a 25 e a 10 saem mais.
- **Mega-Sena e Quina:** também sem erro de dado. O desvio da Mega-Sena
  continua significativo depois de corrigir por testar três loterias
  (p ≈ 0,018); o da Quina (p ≈ 0,044) vira p ≈ 0,13 e é compatível com o
  acaso.
- **Runs test:** só a **dezena 25 da Quina** é rejeitada depois do Holm. Não é
  erro de dado e não coincide com mudança de local ou equipamento; depois de
  corrigir por testar 80 dezenas (ou as 165 das três loterias) fica marginal
  (p entre 0,02 e 0,09).
- **Conclusão:** sem explicação conhecida. Pode ser acaso, ou um viés pequeno
  e persistente. Para medir *quanta* evidência existe, em vez de só rejeitar
  ou não, use o `evidencia` (seção 5).

## 5. Como interpretar o `evidencia`

```bash
./target/release/sync evidencia <lot> [--perfil padrao_v1] [--perfis pasta] [--db arquivo]
```

Em vez de "prever", o `evidencia` diz **quanta evidência os dados dão**. São
duas partes:

- **A. Por dezena:** a dezena sai com chance diferente de K/N (15/25 na
  Lotofácil, 6/60 na Mega-Sena, 5/80 na Quina)? Usa só `resultados` e vale
  para o próximo concurso.
- **B. Do índice de força:** o índice de um perfil prevê melhor que o acaso?
  É a diferença de log loss da **linha linear** do `avaliar` (seção 3),
  traduzida em graus. Se o índice ou as features não estiverem em dia, a
  parte A sai do mesmo jeito e a B mostra o motivo e os comandos a rodar.

### O fator de Bayes e os cinco graus

O **fator de Bayes (FB)** diz quantas vezes os dados são mais prováveis sob uma
hipótese do que sob a outra. Na parte A, as hipóteses são "a chance é
exatamente K/N" (acaso) e "a chance é um pouco diferente" (viés). O FB aparece
em `log10`: +1 quer dizer FB = 10 (dados 10 vezes mais prováveis com viés),
−1 quer dizer FB = 1/10 (10 vezes mais prováveis com o acaso).

O grau vem do tamanho de F = o maior entre FB e 1/FB, sem olhar o lado:

| Grau | F | módulo de log10 FB |
|---|---|---|
| muito fraca | menor que 3 | menor que 0,48 |
| fraca | de 3 a 10 | de 0,48 a 1 |
| moderada | de 10 a 30 | de 1 a 1,48 |
| forte | de 30 a 100 | de 1,48 a 2 |
| muito forte | 100 ou mais | 2 ou mais |

- **Direção:** vem junto do grau, sempre. `a favor de viés` quando FB > 1 e
  `a favor do acaso` quando FB < 1 (FB = 1 exato também conta como acaso).
  Leia os dois: "forte, a favor de viés" é bem diferente de "forte, a favor do
  acaso". A evidência contra o viés aparece com o mesmo destaque que a a favor.
- **Grau não é tamanho do viés.** Mede o quanto os dados convencem. Com
  milhares de concursos, até um viés pequeno pode ser "forte".
- **Priori.** O FB depende de quão grande se imagina o viés possível. O
  principal usa um desvio de 5 % de K/N; o relatório mostra também 2,5 % e
  10 %. Uma conclusão que só aparece com uma das três merece desconfiança.
- **Janelas.** A principal usa o histórico inteiro; a secundária, os últimos
  500 concursos (pega um viés que mude com o equipamento).

### A favor do acaso, a evidência tem teto

"Mesmo destaque" quer dizer a mesma *apresentação*, não a mesma *escala
atingível*. Como o acaso é uma chance exata dentro de uma priori estreita, os
dados nunca conseguem dar muita evidência a favor dele: o FB a favor do acaso
tem um máximo, que ocorre quando a frequência é exatamente K/N. A favor de
viés não há máximo. O relatório imprime esse máximo em cada janela. Com a
priori de 5 % (módulo de log10 FB máximo a favor do acaso):

| Loteria | Histórico inteiro | Últimos 500 |
|---|---|---|
| Lotofácil | 0,59 (fraca) | 0,23 (muito fraca) |
| Mega-Sena | 0,13 (muito fraca) | 0,03 (muito fraca) |
| Quina | 0,17 (muito fraca) | 0,02 (muito fraca) |

Como ler: **"muito fraca, a favor do acaso" é o máximo que os dados podem dizer
a favor do acaso** na Mega-Sena e na Quina (e nos últimos 500 da Lotofácil).
Não significa "faltam dados" nem "evidência inconclusiva". Por isso, não
compare as duas direções só pela etiqueta: na Lotofácil inteira, "fraca, a
favor do acaso" já é o melhor resultado que o acaso consegue.

### Muitas dezenas: algumas saem destacadas por acaso

Com 25, 60 ou 80 dezenas, algumas saem "moderada" ou mais só por sorte. Por
isso o relatório mostra, ao lado da contagem observada em cada grau, a
contagem **esperada sob o acaso** (média de 1000 históricos simulados,
semente 20261008). Só um excesso sobre o esperado indica algo além do acaso.

Resultado em 2026-10-10 (priori de 5 %, histórico inteiro; dezenas "moderada"
ou mais, observadas contra esperadas sob o acaso):

| Loteria | Observadas | Esperadas | Quais |
|---|---|---|---|
| Lotofácil | 2 | 0,14 | dezena 16 (forte, log10 FB +1,9) e dezena 20 (moderada, +1,2), as duas a favor de viés |
| Mega-Sena | 1 | 0,04 | uma dezena moderada |
| Quina | 0 | 0,14 | nenhuma |

Nos últimos 500 concursos, nenhuma das três tem dezena "moderada" ou mais. O
quadro combina com o qui-quadrado da seção 4: o desvio da Lotofácil vem de
poucas dezenas, e nada indica erro de dado.

### Parte B: o índice de força e o "índice inerte"

O relatório da parte B traz o log10 FB do índice contra o acaso, somado
concurso a concurso só nas **dobras de teste 2 a 5** do mesmo walk-forward do
`avaliar`. O conjunto trancado não entra. Aqui a direção é `a favor do índice`
(FB > 1) ou `a favor do acaso`, e **não há teto** a favor do acaso, porque o
índice é um modelo concreto, que pode perder muito para o acaso.

**A régua principal é a calibração linear.** Ela transforma a posição do
índice em probabilidade com um parâmetro só, `p = K/N + b·z(posição)`, e o
`b` é aprendido só com o passado. **O grau e a direção da parte B saem dela.**
É a mesma calibração da linha extra do `avaliar` (seção 3), e o relatório
confere os dois: `ln FB = −pares × diferença de log loss` da linha linear,
dobra a dobra. Como aprender um `b` custa pouco (da ordem de ½·ln T, contra
(N−1)/2·ln T de uma taxa por posição), a régua distingue melhor "sem sinal" de
"sinal fraco". O que ela não enxerga é sinal que não seja monotônico na
posição (por exemplo, só a posição 7 boa), o que num índice ordenado por força
não faz sentido.

**O índice inerte também é medido na régua linear.** Mesmo um índice sem
informação paga um custo para aprender o `b`: o relatório imprime um
`ATENÇÃO` dizendo isso, e o custo, embora pequeno, não é zero (por isso o
`log10 FB` de um inerte fica em torno de −0,3, e não em 0). Então o grau
sozinho não diz se o índice atrapalha ou ajuda. O relatório imprime, ao lado,
o esperado de um índice inerte: as mesmas posições do perfil diante de 200
históricos simulados ao acaso (semente 20261010), com a média, a faixa de 95 %
e quantas réplicas igualaram ou superaram o observado. **O índice só mostra
informação se o observado ficar acima da faixa dos inertes.** Resultado em
2026-10-10 (`padrao_v1`):

| Loteria | log10 FB observado | Média do inerte | Faixa de 95 % do inerte | Réplicas ≥ observado |
|---|---|---|---|---|
| Lotofácil | −0,121 | −0,291 | de −1,019 a +0,554 | 38 de 200 |
| Mega-Sena | −0,674 | −0,295 | de −1,172 a +0,310 | 181 de 200 |
| Quina | −0,268 | −0,330 | de −1,046 a +0,226 | 65 de 200 |

Grau e direção do observado: "muito fraca, a favor do acaso" na Lotofácil,
"fraca, a favor do acaso" na Mega-Sena e "muito fraca, a favor do acaso" na
Quina. Nas três loterias o observado cai dentro da faixa: **o índice é
indistinguível de um índice sem informação**, o mesmo que o `avaliar` já
mostrava na seção 3. Dentro da faixa, a posição não importa: a Lotofácil e a
Quina ficam um pouco acima da média dos inertes (38 e 65 réplicas de 200 foram
iguais ou melhores) e a Mega-Sena um pouco abaixo (181 de 200), mas nenhuma das
três é um resultado que um inerte não produza com frequência. Um "fraca, a
favor do acaso" na Mega-Sena, portanto, não quer dizer que o índice atrapalha.
As 200 réplicas dão uma contagem aproximada, com resolução de 0,5 ponto
percentual.

**Por dobra (só informativo; o grau sai do total).** A tabela do relatório
traz, para cada dobra 2 a 5, o `log10 FB` e a `Δ log loss/par` (perfil − acaso,
negativa = perfil melhor) da régua linear. É a mesma diferença que o `avaliar`
mostra por dobra na linha linear, e a conferência acima vale para cada linha.
Exemplo da Lotofácil:

```
    dobra    concursos  avaliados     pares    log10 FB  Δ log loss/par  FB posição   Δ posição/par
        2     685–1367        683     17075      -0.410       +5.523e-5      -3.238       +4.366e-4
        5    2734–3416        683     17075      +0.539       -7.273e-5      -0.209       +2.818e-5
```

Se o sinal das dobras muda (aqui, dobras 2 e 3 contra 4 e 5), o resultado não
é estável ao longo do tempo, o mesmo critério do `avaliar`.

**A calibração por posição da 1c virou uma linha informativa, sem inerte.**
O relatório ainda mostra o log10 FB com a régua antiga (uma taxa por posição),
nas colunas `FB posição` e `Δ posição/par` e numa linha à parte:

| Loteria | log10 FB, calibração por posição |
|---|---|
| Lotofácil | −7,313 |
| Mega-Sena | −13,587 |
| Quina | −24,998 |

Esse grau é "muito forte, a favor do acaso" nas três, e **não decide nada**:
estimar 25, 60 ou 80 taxas custa de −17 a −60 em ln FB mesmo para um índice
inerte (medido na 1d.3), e por isso não há inerte nesta linha. Ela existe só
para comparar com as tabelas de log loss do `avaliar` (que continuam por
posição) e com a edição anterior do relatório. Quem lê "muito forte, a favor do
acaso" nesta linha não deve concluir que o índice atrapalha.

Ressalvas que o relatório também imprime: as dezenas de um concurso são
tratadas como independentes (na verdade, elas somam exatamente K), então o FB é
uma régua de evidência, não a probabilidade exata do sorteio; e, se o perfil
foi escolhido vendo esses concursos, o resultado sai otimista (o trancado existe
para isso).

### Resumo: o que a evidência não diz

- Um grau **a favor do acaso** em B não prova que o índice atrapalha;
  compare com a faixa do inerte, que a régua linear também imprime. O grau
  "muito forte, a favor do acaso" da linha por posição não decide nada.
- Um observado dentro da faixa do inerte quer dizer "sem sinal detectável",
  não "sinal zero": a régua linear é mais sensível que a por posição, mas ainda
  só vê sinal monotônico na posição.
- Um "muito fraca, a favor do acaso" em A é o melhor que o acaso pode obter,
  não falta de dados.
- Uma dezena destacada em A precisa ser lida contra o esperado sob o acaso para
  o número de dezenas da loteria.
- Nada disso é uma previsão do próximo sorteio.

## 6. Rotina e problemas

Rotina depois de cada sorteio, para cada loteria:

```bash
./target/release/sync atualizar todas
./target/release/sync validar todas
./target/release/sync features <lot>
./target/release/sync indice <lot> --perfil padrao_v1
./target/release/sync tabela todas --perfil padrao_v1        # CSV do próximo concurso
./target/release/sync avaliar <lot> --perfil padrao_v1      # opcional
./target/release/sync evidencia <lot>                        # opcional: graus de evidência
./target/release/sync dataset <lot>                          # opcional: dataset do Python
```

| Sintoma | Causa provável | Solução |
|---|---|---|
| `validar` acusa "concurso X faltando" | buraco | `baixar <lot>`; se a planilha não tiver o concurso, `buracos` e depois `inserir` |
| `features` recusa | o `validar` tem erro | resolva o `validar` primeiro |
| `indice` ou `avaliar` dizem "features desatualizadas" ou "resultados mudaram" | entrou ou foi corrigido um resultado | rode `features` e depois `indice` |
| `avaliar` diz "índice desatualizado ou incoerente" | o índice não foi refeito | rode `indice` |
| `indice` diz "perfil alterado" | o `.toml` foi editado | desfaça a edição ou crie `_v2` |

## 7. Roteiro de avaliação manual

1. **Coleta.** Compare os 3 últimos concursos de cada loteria com o site da
   Caixa:

   ```bash
   python -c "import sqlite3;c=sqlite3.connect('file:resultados_quina.db?mode=ro',uri=True);[print(r) for r in c.execute('select concurso,data_sorteio,dezenas from resultados order by concurso desc limit 3')]"
   ```

2. **Integridade.** O `validar todas` deve dizer "Nenhum problema encontrado"
   nas três loterias.
3. **Features.** Escolha uma dezena e confira à mão o `espaco_final` da
   janela total: deve ser o último concurso menos o último em que ela saiu.

   ```bash
   python -c "import sqlite3;c=sqlite3.connect('file:resultados_quina.db?mode=ro',uri=True);[print(r) for r in c.execute('select janela,frequencia,maior_espaco,menor_espaco,espaco_final,repeticao_atual from features_dezena where dezena=25 and concurso=(select max(concurso) from features_dezena) order by janela')]"
   ```

4. **Tabela geral do próximo concurso,** que é o que o app vai mostrar. O
   jeito mais fácil é `sync tabela todas --perfil padrao_v1` e abrir os CSV de
   `tabelas/` no Excel. Pelo terminal:

   ```bash
   python -c "import sqlite3;c=sqlite3.connect('file:resultados_quina.db?mode=ro',uri=True);[print(r) for r in c.execute(\"select posicao,dezena,round(total,3),variacao_posicao from indice where perfil='padrao_v1' and concurso=(select max(concurso) from indice) order by posicao limit 10\")]"
   ```

   As posições devem ir de 1 a N sem repetir. Se rodar de novo depois do
   próximo sorteio, as linhas antigas não podem mudar.
5. **Avaliação.** Rode `avaliar` nas três loterias e confira se o trecho
   termina no corte e se o veredito bate com a tabela da seção 3.
6. **Aleatoriedade.** Rode `aleatoriedade todas` e confira se o resultado
   bate com o da seção 4.
7. **Evidência.** Rode `evidencia <lot>` nas três loterias. Confira se a
   contagem observada de dezenas "moderada" ou mais bate com a seção 5 (e se
   continua perto do esperado sob o acaso), se o máximo atingível a favor do
   acaso é o da tabela da seção 5 e se, na parte B, o observado ainda está
   dentro da faixa do índice inerte da régua linear (e se a linha por posição,
   informativa, continua em −7,3, −13,6 e −25,0). Confira também, em qualquer
   dobra de B, que `ln FB = −pares × Δ log loss/par` e que o Δ é o da linha
   linear do `avaliar` (a conta da seção 3). Depois abra o CSV do `tabela`: o grau e a
   direção da dezena devem ser os mesmos da primeira janela do `evidencia`.

## 8. Cuidados

- **Conjunto trancado:** é de uso único. Não rode `avaliar --trancado` para
  testar; reserve para o perfil campeão, depois de investigar os dados.
- **Execuções em paralelo:** não rode dois comandos que gravam no mesmo banco
  ao mesmo tempo.
- **Backups:** o `atualizar` guarda em `backups/` a cópia do banco feita
  antes de baixar a planilha completa. A pasta fica fora do git.
- **SQLite:** para abrir o banco fora do programa, use a URL com `?mode=ro`,
  que abre só para leitura, como nos exemplos acima.

## 9. Como interpretar o `ranking` e o `ranking --avaliar`

**Aviso antes de tudo.** O `ranking` é a **descrição do passado**, não uma
previsão. Os sorteios são independentes: a posição de uma dezena na tabela de
força, ou a faixa de um componente, não muda a chance de ela sair no próximo
concurso. O resultado esperado de qualquer ranking feito com essas taxas é
**empatar com o acaso**, e a avaliação existe para conferir isso. Um ranking
que "acerta" acima do acaso é suspeita de vazamento ou de sobreajuste até prova
em contrário. Nada neste comando diz o que vai sair.

### O que são A e B

- **Taxa por posição.** Para cada posição `r` da tabela de força, quantas vezes
  a dezena que estava nela saiu, encolhida em direção a K/N com m = 100
  concursos fictícios: é a mesma probabilidade da calibração da 1c do
  `avaliar`. Cada concurso usa só os concursos anteriores a ele.
- **Faixas dos componentes.** Em cada concurso, cada um dos 9 componentes
  (frequência, maior espaço…) divide as dezenas em tercis (baixo, médio, alto;
  `nulo` quando o componente não tem valor). Há uma taxa de saída por
  (componente, faixa), com o mesmo encolhimento.
- **Nota A (principal).** A taxa da posição atual de cada dezena. Ordena as N
  dezenas; empate vai para a menor dezena.
- **Nota B (EXPERIMENTAL).** Soma de `ln(taxa / (K/N))` sobre a posição e as 9
  faixas (naive Bayes). Os componentes são correlacionados, então B conta a
  mesma informação várias vezes e exagera a distância ao acaso. Nunca é a nota
  principal.

O relatório do `ranking` (sem `--avaliar`) mostra o topo de A e de B, a taxa de
cada posição e faixa com **grau e direção da evidência** (a escala da seção 5)
e a contagem de graus observada contra a esperada sob o acaso. Com 25, 60 ou 80
posições e 9 × 3 faixas, algumas saem destacadas só por acaso: compare sempre
com a contagem esperada. O grau e a direção só descrevem; nenhuma nota nem
ordem os usa. Essa descrição lê o histórico inteiro, trancado incluso.

### O `ranking --avaliar`

Só lê o banco. Recalcula as taxas e as ordens em memória e as avalia no mesmo
trecho do `avaliar`, mas **só nas dobras de teste 2 a 5** (a dobra 1 só treina a
calibração). Nada acima do corte da busca é lido para avaliar.

- **A.** O log loss e o Brier de A são idênticos aos do perfil no `avaliar` (A é
  aquela calibração) e **não se repetem**. Aparece o que é novo: o **top-k pela
  ordem de A** (acertos por concurso contra a hipergeométrica, com p exato, como
  na seção 3) e o **log loss com a calibração linear** sobre a ordem de A (linha
  só informativa, sem bootstrap).
- **B.** Como B é um escore, a probabilidade vem da calibração pela posição da
  1c sobre a ordem de B. Aparecem log loss, Brier, a linha linear, o top-k e o
  **veredito** pelo bootstrap por blocos (50 concursos, 10 000 reamostragens),
  tudo como na seção 3.
- **Como ler as tabelas.** Cada uma traz concursos avaliados, T, a métrica do
  ranking, a do acaso, a diferença e a incerteza (EP e IC normal; o IC normal
  sai estreito demais, quem decide é o bootstrap). Log loss e Brier: diferença
  **negativa** = melhor que o acaso. Top-k: diferença **positiva** = melhor.
- **Múltiplos testes.** A saída traz vários p-valores (top-k de A e de B,
  bootstrap de B, mais os de cada dobra) e **nenhum é corrigido** por múltiplas
  comparações. Um p ≤ 0,05 solto aparece por acaso uma vez em 20.
- **Trancado fechado.** Não há `--trancado` e nenhum uso é registrado. O
  trancado é de uso único por perfil campeão, e este ranking não é um perfil.

**Resultado com `padrao_v1` em 2026-10-27** (dobras de teste 2 a 5; veredito de
B pelo bootstrap; top-k com k = K, p exato do trecho inteiro):

| Loteria | A: top-k (observado × acaso, p) | A: log loss linear, diferença | B: log loss, diferença (p do bootstrap) | B: top-k (p) |
|---|---|---|---|---|
| Lotofácil | 8,9978 × 9,0000, p 0,54 | +1,41e-5 | +1,34e-4 (0,96), não supera | 8,9956 × 9,0000, p 0,58 |
| Mega-Sena | 0,6081 × 0,6000, p 0,30 | +1,28e-5 | +2,32e-4 (0,9986), não supera | 0,6153 × 0,6000, p 0,16 |
| Quina | 0,3154 × 0,3125, p 0,35 | +1,72e-6 | +1,55e-4 (1,00), não supera | 0,3094 × 0,3125, p 0,67 |

Leitura: nas três loterias nada supera o acaso. B fica **pior** que o acaso no
log loss (diferença positiva) porque estima uma taxa por posição de B e esse
aprendizado custa; é o mesmo custo descrito na seção 3, e a linha linear, que
custa pouco, fica perto de zero. Os top-k têm p grandes. É o esperado para
sorteios independentes.

## 10. Como interpretar o `posicoes` e o `posicoes --avaliar`

**Aviso antes de tudo.** O G-1 é a **descrição do passado**, não uma previsão.
Os sorteios são independentes, e a posição que uma dezena ocupa na tabela de
força já é função do histórico dela: o índice das posições é o **mesmo histórico
recodificado**, não informação nova. O resultado esperado é **empatar com o
acaso**, e a avaliação existe para conferir isso. Um G-1 que parecesse superar o
acaso seria, até prova em contrário, vazamento do futuro ou sobreajuste.

### O que é o G-1

- Em cada concurso, as K posições ocupadas pelas dezenas sorteadas ficam
  **marcadas**. A posição vem de `indice` do concurso (medida antes do sorteio).
- Cada posição 1…N passa a ter uma série de aparições, e o motor das features
  roda sobre ela como rodaria sobre uma dezena (frequência, espaços, repetições…).
  As features de uma posição em *t* só usam as marcas até *t−1*.
- **G-1** é a classificação 1…N das posições por esse índice (1 = a de maior
  total). Cada dezena herda o G-1 da posição que ocupa no concurso.
- O perfil das posições (`posicoes_v1`) começa com os pesos do `padrao_v1`, até
  haver uma busca de pesos.

### O `posicoes --avaliar`

- **Mesma régua** do `ranking --avaliar` (seção 9): dobras de teste 2 a 5, log
  loss e Brier com a calibração da 1c, a linha linear da 1e, o top-k contra a
  hipergeométrica e o veredito pelo bootstrap por blocos.
- **Aquecimento.** Os concursos 1 a 100 ficam fora da avaliação inteira,
  calibração inclusive; o 101 é o primeiro concurso da série (recebe o acaso por
  construção e não é avaliado) e as dobras se dividem a partir do 102. Por isso os
  números **não** são comparáveis com os do `ranking --avaliar`, que parte do
  concurso 1. A nota A é reavaliada no mesmo recorte para a comparação valer.
- **Registro.** A avaliação só lê o banco: o perfil das posições não precisa
  estar registrado em `posicao_perfil` (a saída avisa quando não está).
- **Múltiplos testes e trancado fechado.** Valem os avisos da seção 9: vários
  p-valores sem correção, e nenhum uso do conjunto trancado.

**Resultado com `padrao_v1` e `posicoes_v1` em 2026-10-31** (dobras de teste 2 a
5, sem o aquecimento; veredito de G-1 pelo bootstrap; top-k com k = K, p exato do
trecho inteiro; diferenças de log loss em relação ao acaso, positivas = pior):

| Loteria | Trecho avaliado | G-1: log loss, diferença (p do bootstrap) | G-1: top-k (p) | G-1: log loss linear, diferença | A, mesmo recorte: log loss (p) / top-k (p) |
|---|---|---|---|---|---|
| Lotofácil | 765–3416 (2652) | +2,13e-4 (0,96), não supera | 8,9581 × 9,0000, p 0,96 | +6,95e-7 | +2,65e-4 (1,00) / 9,0083, p 0,37 |
| Mega-Sena | 633–2759 (2127) | +3,21e-4 (1,00), não supera | 0,5891 × 0,6000, p 0,77 | +8,19e-6 | +3,13e-4 (1,00) / 0,6060, p 0,35 |
| Quina | 1366–6421 (5056) | +1,74e-4 (1,00), não supera | 0,3095 × 0,3125, p 0,66 | +6,91e-8 | +1,26e-4 (1,00) / 0,3159, p 0,33 |

Leitura: nas três loterias o G-1 **não supera o acaso**, e a referência A também
não. O log loss com a calibração por posição fica **pior** que o acaso (diferença
positiva) porque estimar uma taxa por posição tem custo, o mesmo descrito na
seção 3; a linha linear, que custa pouco, fica praticamente em zero. Os top-k têm
p grandes, com o G-1 até um pouco abaixo do acaso nas três loterias. É o
esperado para sorteios independentes: recodificar o histórico em posições não cria
informação.
