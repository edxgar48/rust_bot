---
name: verificar
description: Gate de qualidade do projeto — roda fmt, clippy, testes Rust, pytest (se existir ml/) e validação dos bancos, e devolve um veredito aprovado/reprovado. Use antes de marcar uma tarefa como concluída ou antes de commitar.
allowed-tools: Bash(cargo fmt *) Bash(cargo clippy *) Bash(cargo test *) Bash(cargo run --bin sync -- validar *) Bash(python -m pytest *)
---

# Gate de verificação

Execute em ordem e **não pare no primeiro erro** — colete tudo:

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. Se `ml/` existir: `python -m pytest ml/`
5. Se o subcomando `validar` existir e houver `resultados_*.db`:
   `cargo run --bin sync -- validar todas`

## Relatório

```
GATE: APROVADO | REPROVADO
fmt:     ok | N arquivos
clippy:  ok | N avisos (lista curta)
testes:  X passaram, Y falharam (nomes dos que falharam)
pytest:  ... | n/a
validar: ... | n/a
```

Não corrija nada nesta skill — só relate. Correções voltam para o agente
responsável pela tarefa.
