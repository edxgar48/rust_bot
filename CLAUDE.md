# rust_spider

Coleta resultados de loterias da Caixa (Lotofácil, Mega-Sena, Quina), grava
em SQLite e — conforme o plano — calcula estatísticas, índice de força e
datasets de ML, exportando JSON para um app mobile.

## Fontes da verdade

- `docs/PLANEJAMENTO.md` — **o quê** construir (fases, especificação das
  features, índice de força, esquema, critérios de pronto).
- `docs/PROGRESSO.md` — **estado** de cada tarefa do plano.
- Mudança de escopo ou de definição vai primeiro para `PLANEJAMENTO.md`,
  nunca só para o código.

## Comandos

```bash
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo run --bin sync -- <comando> <loteria>   # ver README.md
python -m pytest ml/                           # a partir da Fase 1.5
```

## Regras que valem sempre

- Um banco por loteria (`resultados_<loteria>.db`). A tabela `resultados`
  não muda; o que é novo entra em tabelas derivadas, regeneráveis.
- Produção é Rust. Python (`ml/`) só experimenta e propõe perfis de pesos.
- Features do concurso *t* usam apenas concursos até *t−1*.
- Código, nomes e mensagens em português, seguindo o estilo existente.
- `estudos/` contém exemplos antigos fora do build — não mexer.
- Não commitar nem fazer push sem pedido explícito do usuário.

## Agentes

Orquestração: `claude --agent orquestrador` (ou `/proxima-tarefa`). Mapa de
agentes, sub-agentes e skills em `.claude/README.md`.
