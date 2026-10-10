//! Vetores de conformidade Rust ↔ Python (1.5.4): os JSON de
//! `testes/conformidade/` têm de ser exatamente o que o motor calcula agora.
//! Nada é regravado aqui; para atualizar depois de uma mudança deliberada,
//! `cargo run --example gerar_conformidade` e conferir o `git diff`.

use rust_spider::conformidade;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn pasta() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testes/conformidade")
}

const AJUDA: &str = "rode `cargo run --example gerar_conformidade` só se a mudança nas fórmulas \
                     ou nos casos for deliberada (e a FEATURE_VERSION tiver subido) e confira o git diff";

#[test]
fn json_versionado_e_igual_ao_que_o_rust_calcula() {
    let gerados = conformidade::gerar_todos().expect("gera os casos");
    for (arquivo, esperado) in &gerados {
        let caminho = pasta().join(arquivo);
        let lido = std::fs::read(&caminho)
            .unwrap_or_else(|e| panic!("{}: {e}; {AJUDA}", caminho.display()));
        assert!(
            lido == esperado.as_bytes(),
            "{arquivo} diverge do que o motor calcula; {AJUDA}"
        );
    }
}

#[test]
fn nao_sobra_nem_falta_arquivo_na_pasta() {
    let esperados: BTreeSet<String> = conformidade::casos()
        .iter()
        .map(|c| c.arquivo.to_string())
        .collect();
    let existentes: BTreeSet<String> = std::fs::read_dir(pasta())
        .expect("pasta testes/conformidade")
        .map(|e| {
            e.expect("entrada")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(existentes, esperados, "{AJUDA}");
}

#[test]
fn geracao_e_deterministica() {
    let a = conformidade::gerar_todos().expect("a");
    let b = conformidade::gerar_todos().expect("b");
    assert_eq!(a, b);
}

#[test]
fn arquivos_sao_json_valido_com_lf_e_tamanho_pequeno() {
    let mut total = 0;
    for (arquivo, texto) in conformidade::gerar_todos().expect("gera") {
        assert!(texto.ends_with("}\n"), "{arquivo}: termina com LF");
        assert!(!texto.contains('\r'), "{arquivo}: sem CR");
        let v: serde_json::Value = serde_json::from_str(&texto).expect("JSON válido");
        assert_eq!(
            v["versao_formato"],
            conformidade::VERSAO_FORMATO,
            "{arquivo}"
        );
        assert_eq!(
            v["feature_version"],
            rust_spider::motor::FEATURE_VERSION,
            "{arquivo}"
        );
        assert!(
            v["esperado"]["indice"]
                .as_array()
                .is_some_and(|l| !l.is_empty()),
            "{arquivo}: índice vazio"
        );
        total += texto.len();
    }
    // "Dezenas de KB no total": o limite pega um caso que cresça sem querer.
    assert!(total < 150_000, "vetores com {total} bytes");
}
