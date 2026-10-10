//! Grava os vetores de conformidade Rust ↔ Python (1.5.4) em
//! `testes/conformidade/`. Não é produção: nada depende dele.
//!
//! Os casos (entradas e perfis) são feitos à mão em `src/conformidade.rs`; o
//! esperado vem do motor. O `cargo test` falha se os arquivos versionados
//! divergirem do que o motor calcula, então rode isto **só** depois de uma
//! mudança deliberada nas fórmulas (com `FEATURE_VERSION` subida) ou nos
//! casos, e confira o `git diff` antes de commitar.
//!
//! ```text
//! cargo run --example gerar_conformidade [pasta]
//! ```
//!
//! Padrão: `testes/conformidade`. Só escreve os arquivos dos casos; não apaga
//! nada (um arquivo sobrando é apontado pelo teste).

use anyhow::Context;
use rust_spider::conformidade;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let pasta = match argumentos.as_slice() {
        [] => PathBuf::from("testes/conformidade"),
        [p] => PathBuf::from(p),
        _ => anyhow::bail!("uso: gerar_conformidade [pasta]"),
    };
    std::fs::create_dir_all(&pasta).with_context(|| format!("criando {}", pasta.display()))?;
    for (arquivo, conteudo) in conformidade::gerar_todos()? {
        let caminho = pasta.join(arquivo);
        // `write` grava os bytes como estão (LF), sem conversão de fim de linha.
        std::fs::write(&caminho, conteudo)
            .with_context(|| format!("gravando {}", caminho.display()))?;
        println!("{}", caminho.display());
    }
    Ok(())
}
