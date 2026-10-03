---
name: padroes-rust
description: Convenções de código Rust do rust_spider — erros com thiserror/anyhow, nomes em português, organização de módulos, testes, clippy e fmt. Use ao escrever ou revisar código Rust neste repositório.
user-invocable: false
paths: ["src/**/*.rs", "tests/**/*.rs", "Cargo.toml"]
---

# Padrões Rust do projeto

## Estilo
- Identificadores, mensagens de erro e comentários em **português**, como no código existente.
- Comentários explicam **por quê** (decisões, armadilhas do site da Caixa), não o quê.
- Edition 2021. `cargo fmt` sem configuração customizada.

## Erros
- Biblioteca (`src/*.rs`): `SpiderError` (thiserror) + alias `crate::Result<T>`.
  Nova categoria de falha → nova variante com mensagem acionável.
- Binário (`src/bin/`): `anyhow::Result`.
- Sem `unwrap()`/`expect()` na biblioteca fora de testes, salvo invariante
  provada localmente (com comentário).

## Organização
- Módulo novo grande → diretório (`src/motor/mod.rs`, `src/motor/features.rs`, …)
  e reexport do essencial em `lib.rs`.
- Structs de saída derivam `Debug, Clone, PartialEq, Serialize, Deserialize`.
- Funções puras recebem `&[Resultado]` e `&LoteriaConfig`; nada de estado global.
- Ordenação explícita (`BTreeMap`, `sort`) onde a saída precisa ser determinística —
  nunca dependa da ordem de `HashMap`.
- Novas dependências: justificar no relatório; preferir crates maduras.

## Testes
- Unitários em `#[cfg(test)] mod tests` no próprio módulo.
- Integração e fixtures em `tests/` e `tests/fixtures/`.
- Banco em testes: `Connection::open_in_memory()`.
- Nenhum teste acessa a rede ou o site da Caixa.

## Gate antes de entregar
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
