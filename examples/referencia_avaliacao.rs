//! Vetor de referência do `sync avaliar` em precisão total, para a
//! conferência Python × Rust da `ml/` (1.5.2).
//!
//! O `sync avaliar` imprime as médias com 6 casas e as diferenças com 4
//! algarismos, pouco para a tolerância de ~1e-12 que o Python precisa
//! atingir. Este exemplo roda **a mesma** avaliação da CLI
//! (`orquestracao::avaliar_perfil`, só o trecho de busca, k = K) e grava o
//! walk-forward e o veredito em JSON (o `serde_json` escreve cada `f64` com
//! o menor texto que volta ao mesmo valor). Não é produção: nada depende
//! dele, e o trancado não é aceito.
//!
//! ```text
//! cargo run --release --example referencia_avaliacao -- <loteria> <perfil> <banco> [pasta_perfis]
//! ```

use anyhow::Context;
use rust_spider::orquestracao::{self, OpcoesAvaliacao, TrechoAvaliado};
use rust_spider::{config, db, perfil};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let (slug, nome_perfil, banco, pasta_perfis) = match argumentos.as_slice() {
        [l, p, b] => (l, p, PathBuf::from(b), PathBuf::from("perfis")),
        [l, p, b, d] => (l, p, PathBuf::from(b), PathBuf::from(d)),
        _ => anyhow::bail!("uso: referencia_avaliacao <loteria> <perfil> <banco> [pasta_perfis]"),
    };
    let cfg = config::buscar(slug).with_context(|| format!("loteria desconhecida: {slug}"))?;
    let carregado = perfil::carregar(&pasta_perfis, cfg, nome_perfil)?;
    let mut conn = db::abrir_somente_leitura(&banco)
        .with_context(|| format!("abrindo {}", banco.display()))?;
    // Só a busca (padrão): a pasta de registros do trancado não é lida nem
    // gravada, e a data de avaliação não entra no relatório.
    let relatorio = orquestracao::avaliar_perfil(
        &mut conn,
        cfg,
        &carregado,
        &OpcoesAvaliacao::default(),
        &PathBuf::from("registros"),
        "1970-01-01T00:00:00Z",
    )?;
    let TrechoAvaliado::Busca(walk_forward) = &relatorio.trecho else {
        anyhow::bail!("a referência cobre só o trecho de busca");
    };
    let saida = serde_json::json!({
        "loteria": relatorio.loteria,
        "perfil": relatorio.perfil,
        "hash_toml": relatorio.hash_toml,
        "janela": relatorio.janela,
        "normalizacao": relatorio.normalizacao,
        "k": relatorio.k,
        "corte": relatorio.corte,
        "primeiro_indice": relatorio.primeiro_indice,
        "walk_forward": walk_forward,
        "veredito": relatorio.veredito,
    });
    println!("{}", serde_json::to_string_pretty(&saida)?);
    Ok(())
}
