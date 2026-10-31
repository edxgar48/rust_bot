//! `ranking_posicoes.json` (opcional) no `sync exportar`, tarefa 2.6.
//!
//! O G-1 (índice de força das posições, `sync posicoes`, Fase 1g) é o segundo
//! arquivo opcional do contrato, ao lado do `ranking_taxa.json`. Aqui só se
//! **lê** as tabelas `posicao_*` e se decide se elas valem para o snapshot
//! publicado ([`ler_posicoes_para_exportar`]); o exportar não calcula o que
//! publica. O único cálculo é a conferência: refazer, só na memória, o índice
//! das posições a partir de `posicao_features` e exigir igualdade exata com o
//! gravado. Ausente ou atrasado não é erro: a loteria sai sem o arquivo e o
//! motivo volta em [`PosicoesParaExportar::Omitido`].

use super::{impressao_resultados, IndiceConferido, TabelaConcurso, PERFIL_POSICOES};
use crate::config::LoteriaConfig;
use crate::db;
use crate::db_posicoes as dbp;
use crate::error::Result;
use crate::motor::{self, Normalizacao, FEATURE_VERSION, JANELAS};
use crate::perfil;
use rusqlite::Connection;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// As cinco tabelas derivadas do `sync posicoes`; criadas juntas.
const TABELAS_POSICOES: [&str; 5] = [
    "posicao_marcas",
    "posicao_features",
    "posicao_indice",
    "posicao_perfil",
    "posicao_motor_estado",
];

/// Uma dezena no `ranking_posicoes.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaPosicoesProximo {
    pub dezena: u32,
    /// Posição da dezena na tabela de força (igual à de `tabela_geral.json`).
    pub posicao_indice: u32,
    /// `g_1` da posição que a dezena ocupa (1 = a de maior total).
    pub g_1: u32,
    /// `g_1` que a **dezena** tinha no último sorteado (o da posição que ela
    /// ocupava em `indice(ultimo)`) − `g_1` atual; `None` sem concurso anterior.
    pub variacao_g_1: Option<i64>,
}

/// O G-1 do próximo concurso, lido de `posicao_indice` e conferido: uma linha
/// por dezena, em ordem crescente de dezena, mais o perfil das posições.
#[derive(Debug, Clone, PartialEq)]
pub struct PosicoesProximo {
    pub perfil_posicoes: String,
    pub hash_toml_posicoes: String,
    pub janela_posicoes: u32,
    pub normalizacao_posicoes: Normalizacao,
    pub linhas: Vec<LinhaPosicoesProximo>,
}

/// Por que o `ranking_posicoes.json` não vai nesta exportação. Nunca é erro: o
/// resto é publicado e a chave `ranking_posicoes` fica fora do `index.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoPosicoesOmitido {
    /// O banco não tem as tabelas `posicao_*` (o `sync posicoes` nunca rodou).
    SemTabela,
    /// Não há estado das posições do perfil de força.
    SemEstado { perfil: String },
    /// O perfil das posições não está registrado em `posicao_perfil`.
    PerfilNaoRegistrado { perfil_posicoes: String },
    /// O perfil registrado não pôde ser lido ou não é o do banco.
    PerfilInvalido(String),
    /// O estado foi salvo com outra versão das fórmulas das features.
    VersaoDiferente { salva: u32, atual: u32 },
    /// As posições vão até outro concurso que o último sorteado (em geral:
    /// saiu concurso novo e o `sync posicoes` não rodou depois).
    Atrasado { gravado: u32, esperado: u32 },
    /// As marcas incorporadas não são as dos resultados e do índice de força
    /// atuais (resultado corrigido ou índice de força regerado depois).
    MarcasDiferentes,
    /// `posicao_marcas` ou `posicao_features` não batem com o estado salvo.
    TabelasIncoerentes { tabela: &'static str },
    /// O concurso certo não tem as N posições, uma vez cada.
    Incompleto { linhas: usize, esperadas: usize },
    /// O `g_1` gravado difere do que `posicao_features` daria hoje.
    DivergeDoCalculo { posicao: u32 },
    /// Não deu para refazer o cálculo de conferência (as funções puras
    /// recusaram os dados). Não derruba a exportação: o arquivo sai omitido,
    /// com a causa.
    RecalculoFalhou(String),
    /// Erro ao ler as tabelas `posicao_*` (esquema incompatível, coluna
    /// ausente, valor inesperado). Não derruba a exportação.
    ErroDeLeitura(String),
}

impl fmt::Display for MotivoPosicoesOmitido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemTabela => write!(
                f,
                "o banco não tem as tabelas `posicao_*` (rode `sync posicoes`)"
            ),
            Self::SemEstado { perfil } => write!(
                f,
                "não há estado das posições do perfil `{perfil}` (rode `sync posicoes`)"
            ),
            Self::PerfilNaoRegistrado { perfil_posicoes } => write!(
                f,
                "o perfil das posições `{perfil_posicoes}` não está registrado em \
                 `posicao_perfil` (rode `sync posicoes`)"
            ),
            Self::PerfilInvalido(causa) => write!(
                f,
                "o perfil das posições registrado não pôde ser conferido ({causa})"
            ),
            Self::VersaoDiferente { salva, atual } => write!(
                f,
                "as posições foram calculadas na versão {salva} das features, o código está \
                 na {atual} (rode `sync posicoes`)"
            ),
            Self::Atrasado { gravado, esperado } => write!(
                f,
                "as posições vão até o concurso {gravado}, mas o último sorteado é o \
                 {esperado} (rode `sync posicoes`)"
            ),
            Self::MarcasDiferentes => write!(
                f,
                "as marcas gravadas não batem com os resultados e o índice de força atuais \
                 (rode `sync posicoes`)"
            ),
            Self::TabelasIncoerentes { tabela } => write!(
                f,
                "`{tabela}` não bate com o estado salvo das posições (rode `sync posicoes`)"
            ),
            Self::Incompleto { linhas, esperadas } => write!(
                f,
                "o índice das posições do snapshot tem {linhas} linha(s), esperadas \
                 {esperadas} (rode `sync posicoes`)"
            ),
            Self::DivergeDoCalculo { posicao } => write!(
                f,
                "o g_1 gravado da posição {posicao} não bate com o recalculado de \
                 `posicao_features` (rode `sync posicoes`)"
            ),
            Self::ErroDeLeitura(causa) => write!(
                f,
                "não foi possível ler as tabelas `posicao_*` ({causa}); rode `sync posicoes`"
            ),
            Self::RecalculoFalhou(causa) => write!(
                f,
                "não foi possível conferir o índice das posições recalculando-o ({causa}); \
                 rode `sync posicoes` e confira features e índice"
            ),
        }
    }
}

/// O que `posicao_*` deu para esta exportação.
#[derive(Debug, Clone, PartialEq)]
pub enum PosicoesParaExportar {
    /// Em dia: vira `ranking_posicoes.json`.
    EmDia(PosicoesProximo),
    /// Ausente ou atrasado: a loteria sai sem o arquivo.
    Omitido(MotivoPosicoesOmitido),
}

/// Lê as tabelas `posicao_*` e confere se valem para o snapshot (veja
/// [`ler_posicoes_interno`]). Qualquer erro de leitura das tabelas das posições
/// (esquema antigo, coluna ausente, `NULL` inesperado) vira
/// [`MotivoPosicoesOmitido::ErroDeLeitura`]: nenhum problema nelas impede
/// publicar os arquivos obrigatórios. Por isso nunca devolve `Err`.
pub(super) fn ler_posicoes_para_exportar(
    conn: &Connection,
    cfg: &LoteriaConfig,
    tabela: &TabelaConcurso,
    conferido: &IndiceConferido,
) -> Result<PosicoesParaExportar> {
    Ok(
        ler_posicoes_interno(conn, cfg, tabela, conferido).unwrap_or_else(|e| {
            PosicoesParaExportar::Omitido(MotivoPosicoesOmitido::ErroDeLeitura(e.to_string()))
        }),
    )
}

/// Lê as tabelas `posicao_*` do perfil de força `tabela.perfil` com o perfil das
/// posições [`PERFIL_POSICOES`] e confere se valem para o snapshot `tabela`. Só
/// lê; nunca falha por dado ausente ou atrasado (isso é
/// [`PosicoesParaExportar::Omitido`]), só por erro do banco.
///
/// "Em dia" = o estado salvo vai até o último sorteado; a impressão das marcas
/// é a recalculada dos resultados e do índice de força (`conferido`, já
/// conferido); `posicao_marcas` e `posicao_features` batem com o estado;
/// `posicao_indice` cobre o próximo concurso com as N posições; e o `g_1`, o
/// total e a variação gravados são exatamente os que o cálculo daria com
/// `posicao_features` (igualdade exata, cálculo determinístico; REAL do SQLite
/// volta bit a bit). O perfil das posições vem de `posicao_perfil` (o que foi
/// de fato usado no cálculo), conferido pelo hash.
fn ler_posicoes_interno(
    conn: &Connection,
    cfg: &LoteriaConfig,
    tabela: &TabelaConcurso,
    conferido: &IndiceConferido,
) -> Result<PosicoesParaExportar> {
    use MotivoPosicoesOmitido::*;
    let omitido = |m| Ok(PosicoesParaExportar::Omitido(m));
    for nome in TABELAS_POSICOES {
        if !db::tabela_existe(conn, nome)? {
            return omitido(SemTabela);
        }
    }
    let forca = tabela.perfil.as_str();
    let proximo = tabela.concurso;

    let Some(registrado) = dbp::buscar_perfil(conn, PERFIL_POSICOES)? else {
        return omitido(PerfilNaoRegistrado {
            perfil_posicoes: PERFIL_POSICOES.to_string(),
        });
    };
    let carregado = match perfil::interpretar(&registrado.toml, Path::new("posicao_perfil")) {
        Ok(c) => c,
        Err(e) => return omitido(PerfilInvalido(e.to_string())),
    };
    if carregado.hash_toml != registrado.hash_toml
        || carregado.perfil.nome != PERFIL_POSICOES
        || carregado.perfil.loteria != cfg.slug
    {
        return omitido(PerfilInvalido(format!(
            "o registro de `{PERFIL_POSICOES}` não corresponde ao seu `.toml` guardado"
        )));
    }
    let pp = &carregado.perfil;

    let Some(salvo) = dbp::ler_estado(conn, forca)? else {
        return omitido(SemEstado {
            perfil: forca.to_string(),
        });
    };
    if salvo.feature_version != FEATURE_VERSION {
        return omitido(VersaoDiferente {
            salva: salvo.feature_version,
            atual: FEATURE_VERSION,
        });
    }
    if salvo.ultimo_concurso != tabela.ultimo_sorteado {
        return omitido(Atrasado {
            gravado: salvo.ultimo_concurso,
            esperado: tabela.ultimo_sorteado,
        });
    }

    let marcas = match motor::marcar_historico(cfg, &conferido.resultados, &conferido.indice) {
        Ok(m) => m,
        Err(e) => return omitido(RecalculoFalhou(e.to_string())),
    };
    if impressao_resultados(&marcas) != salvo.impressao {
        return omitido(MarcasDiferentes);
    }
    drop(marcas);

    let n = cfg.qtd_dezenas_possiveis() as usize;
    let ultimo = tabela.ultimo_sorteado;
    let (marcas_gravadas, maior_marca) = dbp::resumo_marcas(conn, forca)?;
    if marcas_gravadas != u64::from(ultimo) * n as u64 || maior_marca != Some(ultimo) {
        return omitido(TabelasIncoerentes {
            tabela: "posicao_marcas",
        });
    }
    let (features_gravadas, maior_feature) = dbp::resumo_features(conn, forca)?;
    if features_gravadas != (u64::from(ultimo) + 1) * n as u64 * JANELAS.len() as u64
        || maior_feature != Some(proximo)
    {
        return omitido(TabelasIncoerentes {
            tabela: "posicao_features",
        });
    }

    // Os dois retratos que o arquivo usa: o do próximo concurso (o `g_1`) e o do
    // último sorteado (o `g_1` antigo, para a variação por dezena).
    let anterior_existe = ultimo >= 1;
    let concursos: Vec<u32> = if anterior_existe {
        vec![ultimo, proximo]
    } else {
        vec![proximo]
    };
    let gravado_ambos: Vec<_> = dbp::ler_indice(conn, forca, PERFIL_POSICOES)?
        .into_iter()
        .filter(|l| concursos.contains(&l.concurso))
        .collect();
    // Cada posição 1…N uma vez por concurso, em ordem (a leitura já vem por
    // concurso e posição).
    let completo = gravado_ambos.len() == n * concursos.len()
        && gravado_ambos.chunks(n).zip(&concursos).all(|(bloco, &c)| {
            bloco
                .iter()
                .zip(1u32..)
                .all(|(linha, posicao)| linha.concurso == c && linha.dezena == posicao)
        });
    if !completo {
        return omitido(Incompleto {
            linhas: gravado_ambos.len(),
            esperadas: n * concursos.len(),
        });
    }

    let features = dbp::ler_features_da_janela(conn, forca, pp.janela)?;
    let recalculado = match motor::calcular_indice(&features, pp.janela, pp.normalizacao, &pp.pesos)
    {
        Ok(r) => r,
        Err(e) => return omitido(RecalculoFalhou(e.to_string())),
    };
    drop(features);
    let esperado: Vec<_> = recalculado
        .into_iter()
        .filter(|l| concursos.contains(&l.concurso))
        .collect();
    if esperado.len() != gravado_ambos.len() {
        return omitido(Incompleto {
            linhas: esperado.len(),
            esperadas: gravado_ambos.len(),
        });
    }
    let diverge = gravado_ambos.iter().zip(&esperado).find(|(g, e)| {
        (g.concurso, g.dezena, g.posicao, g.variacao_posicao)
            != (e.concurso, e.dezena, e.posicao, e.variacao_posicao)
            || g.total.to_bits() != e.total.to_bits()
    });
    if let Some((g, _)) = diverge {
        return omitido(DivergeDoCalculo { posicao: g.dezena });
    }

    let via_de = |concurso: u32| {
        let linhas: Vec<_> = gravado_ambos
            .iter()
            .filter(|l| l.concurso == concurso)
            .cloned()
            .collect();
        motor::dezenas_via_posicao(cfg, concurso, &conferido.indice, &linhas)
    };
    let via = match via_de(proximo) {
        Ok(v) => v,
        Err(e) => return omitido(RecalculoFalhou(e.to_string())),
    };
    // `g_1` que cada dezena tinha em `ultimo` (o da posição que ela ocupava em
    // `indice(ultimo)`): a variação é da dezena, não da posição.
    let g_1_antigo: Option<HashMap<u32, u32>> = if anterior_existe {
        match via_de(ultimo) {
            Ok(v) => Some(v.into_iter().map(|d| (d.dezena, d.g_1)).collect()),
            Err(e) => return omitido(RecalculoFalhou(e.to_string())),
        }
    } else {
        None
    };
    let mut linhas: Vec<LinhaPosicoesProximo> = via
        .iter()
        .map(|v| LinhaPosicoesProximo {
            dezena: v.dezena,
            posicao_indice: v.posicao,
            g_1: v.g_1,
            variacao_g_1: g_1_antigo
                .as_ref()
                .and_then(|m| m.get(&v.dezena))
                .map(|&antigo| i64::from(antigo) - i64::from(v.g_1)),
        })
        .collect();
    linhas.sort_by_key(|l| l.dezena);
    Ok(PosicoesParaExportar::EmDia(PosicoesProximo {
        perfil_posicoes: pp.nome.clone(),
        hash_toml_posicoes: carregado.hash_toml.clone(),
        janela_posicoes: pp.janela,
        normalizacao_posicoes: pp.normalizacao,
        linhas,
    }))
}
