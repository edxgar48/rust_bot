---
name: avaliacao-estatistica
description: Como avaliar o índice de força e modelos de ML contra o acaso — baselines por loteria, log loss, Brier, top-k vs hipergeométrica, walk-forward, conjunto final trancado e testes de aleatoriedade. Use ao implementar a Fase 1c, ao rodar experimentos em ml/ ou ao interpretar resultados.
user-invocable: false
---

# Avaliação estatística

Premissa: sorteios são independentes. O resultado esperado de qualquer
perfil ou modelo é **empatar com o acaso**. Ganho aparente = suspeita de
vazamento ou sobreajuste até prova em contrário.

## Baselines (probabilidade de cada dezena sair)
| Loteria | k/N | p |
|---|---|---|
| Lotofácil | 15/25 | 0,60 |
| Mega-Sena | 6/60 | 0,10 |
| Quina | 5/80 | 0,0625 |

Derivar sempre de `LoteriaConfig` (`qtd_dezenas`, faixa), nunca hardcode.

## Métricas
- Probabilísticas: log loss e Brier do modelo vs. baseline constante *p*.
- Ranking: acertos das top-k dezenas por concurso vs. hipergeométrica
  (média esperada = k·K/N; reportar também intervalo e p-valor).
- Sempre reportar: média, desvio, nº de concursos avaliados, diferença vs. acaso.

## Protocolo
1. **Walk-forward**: ajusta até *t*, avalia em *t+1…t+h*, avança. Nunca embaralha o tempo.
2. **Conjunto trancado**: últimos concursos (tamanho em `docs/PROGRESSO.md`,
   decisão em aberto; sugestão ~10%). Usado **uma única vez**, só para o
   perfil campeão. Registrar a data do uso em `docs/PROGRESSO.md` → Notas.
3. **Múltiplas comparações**: ao testar muitos perfis, reportar quantos foram
   testados; aplicar correção (Bonferroni ou Holm) antes de afirmar significância.

## Testes de aleatoriedade do histórico
- Qui-quadrado de frequência por dezena (uniformidade).
- Runs test na série `saiu` de cada dezena.
- Desvios fortes geralmente indicam **dado corrompido** — verifique `validar` antes.

## Implementação
- Funções puras em `src/avaliacao/`, sem SQL nem I/O.
- Funções estatísticas (hipergeométrica, qui-quadrado) com testes contra
  valores tabelados conhecidos.
