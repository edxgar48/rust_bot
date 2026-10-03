---
name: proxima-tarefa
description: Executa a próxima tarefa do plano (ou a tarefa indicada, ex. "0.2") através do agente orquestrador, com delegação, gate de qualidade e atualização do progresso.
argument-hint: "[id-da-tarefa]"
disable-model-invocation: true
context: fork
agent: orquestrador
background: false
---

Execute o ciclo de orquestração para a tarefa `$ARGUMENTS`.
Se nenhum id foi informado, escolha a próxima tarefa elegível em `docs/PROGRESSO.md`.

Você está rodando como subagente: não é possível perguntar ao usuário no
meio do caminho. Se precisar de uma decisão (decisão em aberto, ambiguidade
no plano, commit), pare e devolva a pergunta no relatório final.
