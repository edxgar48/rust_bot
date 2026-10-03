---
name: status-plano
description: Mostra o andamento do plano — tarefas concluídas, em andamento e bloqueadas por fase, decisões em aberto e a próxima tarefa elegível. Use quando o usuário perguntar em que pé está o projeto ou o que vem a seguir.
---

# Status do plano

## Estado atual
!`cat docs/PROGRESSO.md`

## Últimos commits
!`git log --oneline -8`

## Instruções
Responda em poucas linhas:
1. Por fase: concluídas / total, e o que está `[~]` ou `[!]`.
2. Decisões em aberto que bloqueiam algo próximo.
3. **Próxima tarefa elegível**: a primeira `[ ]` cuja fase anterior esteja
   concluída (ordem 0 → 1a → 1b → 1c → 1.5 → 2 → 3) e que não dependa de
   decisão em aberto. Diga qual agente a executa.

Não altere nenhum arquivo.
