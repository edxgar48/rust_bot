---
name: arquitetura-projeto
description: Princípios de arquitetura do rust_spider — bancos por loteria, tabelas derivadas regeneráveis, motor como série temporal, Rust em produção e Python só para experimentos, limites entre módulos. Use ao projetar, implementar ou revisar qualquer mudança estrutural no projeto.
user-invocable: false
---

# Arquitetura do rust_spider

Especificação completa: `docs/PLANEJAMENTO.md`. Em conflito, o plano vence;
se o plano estiver errado, proponha a mudança no plano antes do código.

## Princípios inegociáveis

1. **Um banco por loteria** (`resultados_<loteria>.db`). Nunca banco único.
2. **`resultados` é imutável no schema.** Tudo novo é tabela derivada
   (`features_dezena`, `indice`, `perfis`, `motor_estado`), que pode ser
   apagada e regenerada a partir de `resultados`. Sem migrações destrutivas.
3. **Motor = série temporal.** Snapshot de cada dezena *antes* de cada
   concurso. O app usa o último snapshot; o ML usa todos. Um único cálculo.
4. **Ponto no tempo.** Features de *t* usam só concursos ≤ *t−1*.
5. **Determinismo.** Mesma entrada → mesmos bytes (features, índice, JSON).
6. **Versionamento.** `feature_version` nas features, hash do `.toml` nos
   perfis, `schema_version` + `/v1/` nos JSON exportados.
7. **Rust é dono da lógica de produção.** Python lê datasets e propõe
   perfis `.toml`; nunca implementa algo de que a produção dependa.
   Paridade garantida por vetores de conformidade.

## Camadas e dependências permitidas

```
bin/sync.rs (CLI, anyhow)        ← orquestra; sem regra de negócio
   │
   ├─ downloader, parser          ← coleta (I/O externo)
   ├─ validacao                   ← regras de integridade dos resultados
   ├─ motor/                      ← features e índice: funções puras sobre &[Resultado]
   ├─ avaliacao/                  ← métricas e baselines: funções puras
   ├─ exportacao/                 ← serialização determinística
   └─ db                          ← única camada que fala SQL
config, model, error             ← compartilhados, sem dependências internas
```

- `motor/` e `avaliacao/` **não** fazem I/O nem SQL: recebem dados, devolvem structs.
- Só `db.rs` (ou `db/`) contém SQL.
- `config.rs` concentra o que é específico de cada loteria; nada de `match slug` espalhado.
- Adicionar loteria = adicionar um `LoteriaConfig`, sem tocar no motor.

## Quando parar e perguntar

- A tarefa exige mudar `resultados`, juntar bancos, ou mover lógica para Python.
- A definição no plano é ambígua ou contradiz um teste golden.
- Surge uma "decisão em aberto" listada em `docs/PROGRESSO.md`.
