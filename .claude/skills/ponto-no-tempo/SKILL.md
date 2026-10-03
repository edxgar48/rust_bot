---
name: ponto-no-tempo
description: Regras anti-vazamento (point-in-time) do motor estatístico e padrões de teste para garanti-las — snapshot antes do concurso, janelas, NULLs, incremental igual a completo. Use ao implementar, testar ou auditar features, índice de força ou datasets de ML.
user-invocable: false
---

# Ponto no tempo (anti-vazamento)

Definições exatas dos componentes, exemplo golden e casos de borda:
`docs/PLANEJAMENTO.md`, seção "Especificação das features". Leia antes de codar.

## Regras
1. O snapshot do concurso *t* é calculado **antes** de incorporar *t* ao estado.
   Ordem do laço: `snapshot(t)` → `registrar alvo saiu(t)` → `atualizar_estado(t)`.
2. Janela *J* em *t* = concursos *t−J* … *t−1*. Janela "total" = 1 … *t−1*.
3. `espaco_final` não é limitado pela janela.
4. Sem espaços suficientes → `NULL` (`Option<_>` no Rust), nunca 0 ou −1.
5. Normalização do índice é **por concurso**, entre as dezenas daquele
   concurso — nunca com estatísticas do histórico inteiro (vaza o futuro).
6. Nenhum componente pode ler `saiu` de *t* ou qualquer dado de concurso ≥ *t*.

## Testes obrigatórios para qualquer feature nova
- **Golden**: o exemplo do plano (aparições 3, 4, 8, 9, 10, 15; *t* = 20)
  reproduzido campo a campo.
- **Vazamento**: alterar as dezenas do concurso *t* (e de qualquer *t' > t*)
  não altera nenhuma feature de *t*.
- **Incremental = completo**: processar N concursos de uma vez e processar
  N−k e depois k (retomando de `motor_estado`) dá resultado idêntico.
- **Bordas**: dezena que nunca saiu; uma única aparição; duas; janela maior
  que o histórico; primeiro concurso.
- **Determinismo**: duas execuções → saída idêntica.

## Sinais de vazamento ao revisar
- Uso de `resultados.len()`, `max`, médias ou z-score sobre o histórico todo.
- Ordenação por data quando concursos podem ter datas iguais (use `concurso`).
- Modelo/perfil com desempenho acima do acaso: trate como bug até provar o contrário.
