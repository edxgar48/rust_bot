//! CSV e relatório em texto do `sync posicoes` (Fase 1g, tarefa 1g.3).
//!
//! O CSV segue o formato de `ranking`: separador `;`, vírgula decimal, UTF-8
//! **com BOM** e fim de linha `\n`. Traz o índice das posições no próximo
//! concurso, uma linha por posição, da mais forte (`g_1` = 1) para a mais
//! fraca, com a dezena que ocupa a posição na tabela de força.
//!
//! Mesmo relatório → mesmos bytes, tanto no CSV quanto no texto: ordem
//! explícita em tudo e nenhuma data nem tempo de execução (ficam por conta da
//! CLI).

use crate::config::LoteriaConfig;
use crate::motor::JANELA_TOTAL;
use crate::orquestracao::{ModoPosicoes, RegistroPerfil, RelatorioPosicoes};
use std::fmt::Write;

/// Cabeçalho do CSV, na ordem das colunas.
pub const CABECALHO: &str =
    "loteria;perfil;perfil_posicoes;concurso;g_1;posicao;dezena;total;variacao_g_1";

/// Marca de ordem de bytes UTF-8 (`EF BB BF`) que abre o arquivo.
pub const BOM: &str = "\u{FEFF}";

/// Casas decimais do total no CSV.
pub const CASAS_TOTAL: usize = 9;

/// Quantas posições o texto mostra (o CSV traz todas).
pub const TOPO_NO_TEXTO: usize = 10;

/// Nome do arquivo: `posicoes_<loteria>_<perfil>_<perfil_posicoes>_<concurso>.csv`,
/// com o concurso do índice (o próximo, ainda não sorteado).
pub fn nome_arquivo(r: &RelatorioPosicoes) -> String {
    format!(
        "posicoes_{}_{}_{}_{}.csv",
        r.loteria, r.perfil, r.perfil_posicoes, r.proximo_concurso
    )
}

/// O CSV inteiro: [`BOM`], cabeçalho e uma linha por posição.
pub fn formatar_csv(r: &RelatorioPosicoes) -> String {
    let mut saida = String::from(BOM);
    saida.push_str(CABECALHO);
    saida.push('\n');
    for l in &r.proximo {
        // `write!` num `String` não falha.
        let _ = writeln!(
            saida,
            "{};{};{};{};{};{};{};{};{}",
            r.loteria,
            r.perfil,
            r.perfil_posicoes,
            r.proximo_concurso,
            l.g_1,
            l.posicao,
            l.dezena,
            decimal(l.total),
            l.variacao_g_1.map_or_else(String::new, |v| v.to_string()),
        );
    }
    saida
}

/// Total com [`CASAS_TOTAL`] casas e vírgula decimal, sem `-0,000000000`.
fn decimal(valor: f64) -> String {
    let texto = format!("{valor:.CASAS_TOTAL$}");
    let texto = if texto
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        texto.trim_start_matches('-').to_string()
    } else {
        texto
    };
    texto.replace('.', ",")
}

fn descrever_janela(janela: u32) -> String {
    if janela == JANELA_TOTAL {
        "total".to_string()
    } else {
        janela.to_string()
    }
}

/// Relatório em texto do `sync posicoes`: o que foi gravado e o topo do índice
/// das posições no próximo concurso, sempre com o aviso de que é descrição do
/// passado.
pub fn formatar_relatorio(cfg: &LoteriaConfig, r: &RelatorioPosicoes) -> String {
    let mut s = String::new();
    let mut linha = |texto: String| {
        s.push_str(&texto);
        s.push('\n');
    };

    linha(format!(
        "Índice de força das posições da {}: concurso {} (o próximo, ainda não sorteado).",
        cfg.nome_exibicao, r.proximo_concurso
    ));
    linha(format!(
        "Posições dadas pelo perfil de força {}; índice das posições com o perfil {} \
         (versão {}, janela {}, {}, hash {}).",
        r.perfil,
        r.perfil_posicoes,
        r.versao,
        descrever_janela(r.janela),
        r.normalizacao,
        r.hash_toml
    ));
    linha(match &r.modo {
        ModoPosicoes::Incremental => format!(
            "Retomou do estado salvo: {} concurso(s) novo(s).",
            r.concursos_novos
        ),
        ModoPosicoes::Completo(motivo) => format!(
            "Recalculou do zero ({motivo}): {} concurso(s).",
            r.concursos_novos
        ),
    });
    linha(match r.registro {
        RegistroPerfil::Novo => format!(
            "Perfil {} registrado agora em posicao_perfil.",
            r.perfil_posicoes
        ),
        RegistroPerfil::JaRegistrado => format!(
            "Perfil {} já registrado em posicao_perfil, sem alteração.",
            r.perfil_posicoes
        ),
    });
    linha(String::new());
    linha(format!(
        "Topo do índice das posições (G-1) no concurso {}; as {} posições estão no CSV:",
        r.proximo_concurso,
        r.proximo.len()
    ));
    linha("  G-1  posição  dezena  variação        total".to_string());
    for l in r.proximo.iter().take(TOPO_NO_TEXTO) {
        linha(format!(
            "  {:>3}  {:>7}  {:>6}  {:>8}  {:>11}",
            l.g_1,
            l.posicao,
            l.dezena,
            l.variacao_g_1
                .map_or_else(|| "-".to_string(), |v| format!("{v:+}")),
            decimal(l.total)
        ));
    }
    linha(String::new());
    linha(
        "Aviso: é descrição do passado, não previsão. Os sorteios são independentes e a posição \
         de uma dezena já vem do histórico dela; espera-se que este índice não supere o acaso \
         (a avaliação vem na tarefa 1g.4)."
            .to_string(),
    );
    s
}

// `sync posicoes --avaliar` (1g.4): relatório em texto da avaliação do G-1.
mod avaliar;
pub use avaliar::formatar_avaliacao;
