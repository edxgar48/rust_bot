# Guia dos comandos do `sync`

Como usar cada comando, como ler as respostas e um roteiro para conferir à mão
o que o projeto produz. As saídas de exemplo são reais (Mega-Sena, perfil
`padrao_v1`); os números da seção de resultados refletem o estado de
2026-10-05 e mudam a cada sorteio.

Definições completas (features, índice, avaliação): `docs/PLANEJAMENTO.md`.

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
Mega-Sena e 6421 na Quina.

`--trancado` avalia o **conjunto trancado**, isto é, os concursos depois do
corte. **É de uso único por perfil** e fica registrado em
`registros/trancado_<lot>.json`. Só use quando for validar um perfil campeão.

`aleatoriedade <lot|todas>` testa se o próprio histórico parece aleatório.
Também só lê o banco.

### Tabela geral em CSV

`tabela <lot|todas> --perfil padrao_v1 [--concurso N] [--saida tabelas]` grava
a tabela geral de um concurso num CSV para abrir no Excel: uma linha por
dezena, em ordem de posição, com total, variação de posição, `saiu` e as 9
features da janela do perfil. Só lê o banco e faz as mesmas conferências do
`avaliar` (índice e features em dia).

- **Arquivo:** `tabelas/<lot>/tabela_<lot>_<perfil>_<concurso>.csv`. Um
  arquivo por concurso: rodando depois de cada sorteio, a pasta acumula a
  tabela de cada semana para comparar.
- **Padrão:** o próximo concurso, ainda não sorteado (`data_sorteio` e
  `saiu` vazios). Com `--concurso N`, um concurso já sorteado, com `saiu`
  preenchido (1 = saiu): dá para ver onde estavam as dezenas sorteadas.
- **Formato:** separador `;` e vírgula decimal (Excel em português); célula
  vazia = sem valor (por exemplo, `soma_variacoes` com menos de 3 aparições
  na janela).
- **Saída no terminal:** onde gravou e o top 10 (`*` = saiu, quando o
  concurso já foi sorteado).

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

**Resultado em 2026-10-05:**

- **Qui-quadrado:** rejeita a uniformidade nas três loterias. Na Lotofácil,
  o p é 0,00015, com o desvio concentrado nos concursos 1899 a 3796.
- **Runs test:** só a **dezena 25 da Quina** é rejeitada depois do Holm.
- **Próximo passo:** tudo isso está no Backlog do `PLANEJAMENTO.md`, para
  conferir contra uma fonte independente.

## 5. Rotina e problemas

Rotina depois de cada sorteio, para cada loteria:

```bash
./target/release/sync atualizar todas
./target/release/sync validar todas
./target/release/sync features <lot>
./target/release/sync indice <lot> --perfil padrao_v1
./target/release/sync tabela todas --perfil padrao_v1        # CSV do próximo concurso
./target/release/sync avaliar <lot> --perfil padrao_v1      # opcional
./target/release/sync dataset <lot>                          # opcional: dataset do Python
```

| Sintoma | Causa provável | Solução |
|---|---|---|
| `validar` acusa "concurso X faltando" | buraco | `baixar <lot>`; se a planilha não tiver o concurso, `buracos` e depois `inserir` |
| `features` recusa | o `validar` tem erro | resolva o `validar` primeiro |
| `indice` ou `avaliar` dizem "features desatualizadas" ou "resultados mudaram" | entrou ou foi corrigido um resultado | rode `features` e depois `indice` |
| `avaliar` diz "índice desatualizado ou incoerente" | o índice não foi refeito | rode `indice` |
| `indice` diz "perfil alterado" | o `.toml` foi editado | desfaça a edição ou crie `_v2` |

## 6. Roteiro de avaliação manual

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

## 7. Cuidados

- **Conjunto trancado:** é de uso único. Não rode `avaliar --trancado` para
  testar; reserve para o perfil campeão, depois de investigar os dados.
- **Execuções em paralelo:** não rode dois comandos que gravam no mesmo banco
  ao mesmo tempo.
- **Backups:** o `atualizar` guarda em `backups/` a cópia do banco feita
  antes de baixar a planilha completa. A pasta fica fora do git.
- **SQLite:** para abrir o banco fora do programa, use a URL com `?mode=ro`,
  que abre só para leitura, como nos exemplos acima.
