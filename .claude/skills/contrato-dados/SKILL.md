---
name: contrato-dados
description: Regras do contrato de dados com o front-end e com o Python — estrutura publico/v1, schema_version, geração determinística, hashes, compatibilidade e documentação em docs/contrato-v1.md. Use ao criar ou alterar exportação JSON, datasets Parquet/CSV ou qualquer formato consumido fora do Rust.
user-invocable: false
---

# Contrato de dados

Estrutura e arquivos: `docs/PLANEJAMENTO.md`, Fase 2. Documentação do
contrato: `docs/contrato-v1.md` (fonte para o planejamento do front-end).

## Regras
- **Compatibilidade dentro de `v1`**: só adicionar campos opcionais. Remover,
  renomear ou mudar tipo/semântica de campo → `v2` em paralelo.
- `schema_version` em todo arquivo; `index.json` lista arquivos com SHA-256.
- **Determinismo byte a byte**: chaves em ordem estável, sem timestamps de
  geração dentro dos arquivos de dados (a data de geração fica só em
  `index.json`), números com formatação fixa, fim de linha `\n`.
- Datas ISO `AAAA-MM-DD`. Dezenas como inteiros. `null` explícito para ausência.
- Todo campo novo é documentado em `docs/contrato-v1.md` **no mesmo commit**,
  com tipo, significado e exemplo.
- Datasets de ML (`sync dataset`): mesmos nomes de coluna das tabelas
  derivadas; incluir `feature_version` e a loteria.

## Verificação
- Exportar duas vezes do mesmo banco → `diff` vazio.
- Validar cada JSON contra o exemplo documentado no contrato.
