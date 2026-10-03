# Agentes e skills do rust_spider

Estrutura de agentes do Claude Code para executar `docs/PLANEJAMENTO.md`,
com o estado em `docs/PROGRESSO.md`.

## Como usar

| Quero… | Comando |
|---|---|
| Sessão dedicada de orquestração (recomendado) | `claude --agent orquestrador` |
| Executar a próxima tarefa a partir de uma sessão normal | `/proxima-tarefa` ou `/proxima-tarefa 1a.2` |
| Ver em que pé está o plano | `/status-plano` |
| Rodar o gate de qualidade | `/verificar` |
| Chamar um agente específico | "use o engenheiro-motor para …" |

Com `claude --agent orquestrador`, o orquestrador pode perguntar suas
decisões no meio do caminho. Via `/proxima-tarefa` ele roda como subagente:
em vez de perguntar, para e devolve as perguntas no relatório.

## Hierarquia

```
orquestrador (opus) ─ coordena, não programa; único que edita docs/PROGRESSO.md
├── engenheiro-dados ──────── Fase 0 ──── └─ escritor-testes
├── engenheiro-motor ──────── Fase 1a/1b ─ ├─ escritor-testes
│                                          └─ verificador-vazamento
├── engenheiro-avaliacao ──── Fase 1c ──── ├─ escritor-testes
│                                          └─ verificador-vazamento
├── cientista-ml ──────────── Fase 1.5 ─── ├─ verificador-conformidade
│                                          └─ verificador-vazamento
├── engenheiro-publicacao ─── Fase 2 ───── ├─ auditor-contrato
│                                          └─ escritor-testes
├── engenheiro-operacao ───── Fase 3 ───── └─ escritor-testes
└── revisor-arquitetura (opus, só leitura) ─ gate final ─ └─ verificador-vazamento
```

Cada agente só pode chamar os sub-agentes listados em `tools: Agent(...)`.
Os sub-agentes de verificação e auditoria são **somente leitura**, e o
`escritor-testes` só mexe em testes. Profundidade máxima: 3 níveis abaixo da
sessão principal (padrão do Claude Code), suficiente para
`/proxima-tarefa` → orquestrador → engenheiro → sub-agente.

## Ciclo de uma tarefa

1. O orquestrador escolhe a tarefa e marca `[~]`.
2. Delega ao engenheiro da fase, com um briefing autossuficiente.
3. O engenheiro implementa, usando sub-agentes para testes e auditorias.
4. Gate: `/verificar` + `revisor-arquitetura`. Se reprovar, volta ao engenheiro (máx. 2 rodadas).
5. Marca `[x]`, reporta e **pergunta antes de commitar** (um commit por tarefa).

## Skills

| Skill | Tipo | Carregada por |
|---|---|---|
| `arquitetura-projeto` | conhecimento | todos os agentes principais |
| `padroes-rust` | conhecimento (auto em `src/**/*.rs`) | engenheiros, revisor, escritor-testes |
| `ponto-no-tempo` | conhecimento | motor, avaliação, ML, revisor, verificador-vazamento |
| `avaliacao-estatistica` | conhecimento | avaliação, ML, verificador-vazamento |
| `contrato-dados` | conhecimento | publicação, ML, revisor, auditor, conformidade |
| `padroes-python-ml` | conhecimento (auto em `ml/**`) | ML, verificador-conformidade |
| `verificar` | fluxo (`/verificar`) | gate, usado por todos |
| `status-plano` | fluxo (`/status-plano`) | usuário |
| `proxima-tarefa` | fluxo (`/proxima-tarefa`) | usuário (só manual) |

## Manutenção

- Mudou uma regra do projeto? Atualize **a skill** correspondente, não os agentes.
- As especificações (features, índice, contrato) moram em `docs/PLANEJAMENTO.md`;
  as skills apontam para lá em vez de duplicar.
- Nova fase ou domínio: crie o agente, adicione-o ao `tools: Agent(...)` do
  orquestrador e ao mapa de delegação no prompt dele.
