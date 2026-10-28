//! Geração e gravação do `sync exportar`, movidas da CLI (tarefa 3.1a): gerar
//! os arquivos de uma loteria ([`gerar_exportacao`]) e o tudo ou nada de
//! várias ([`exportar_tudo_ou_nada`]). O `sync ciclo` exporta uma loteria por
//! vez com a mesma função, passando uma lista de um item.
//!
//! # `ranking_taxa.json` (opcional)
//!
//! O ranking por taxa (tabela derivada `ranking_taxa`, `sync ranking`) é o
//! único arquivo opcional do contrato. Aqui só se **lê** a tabela e se decide
//! se ela está em dia ([`ler_ranking_para_exportar`]); ausente ou atrasada
//! não é erro: a loteria sai sem o arquivo e o motivo volta em
//! [`RankingParaExportar::Omitido`], em [`ArquivosLoteria::ranking_omitido`]
//! e, para quem exporta, em [`RelatorioExportacao`] (veja
//! [`exportar_com_relatorio`], a API que o `sync ciclo` deve usar para saber
//! se o ranking entrou).

use crate::avaliacao;
use crate::config::{self, LoteriaConfig};
use crate::db;
use crate::error::{Result, SpiderError};
use crate::exportacao::{self, disco::ArquivoGravado, ArquivosLoteria};
use crate::perfil;
use rusqlite::Connection;
use std::fmt;
use std::path::{Path, PathBuf};

/// Nome da tabela derivada que alimenta o `ranking_taxa.json`.
const TABELA_RANKING: &str = "ranking_taxa";

/// O ranking por taxa do próximo concurso, lido de `ranking_taxa` e conferido
/// contra o índice: uma linha por dezena, em ordem crescente de dezena.
#[derive(Debug, Clone, PartialEq)]
pub struct RankingProximo {
    /// K: dezenas sorteadas por concurso (a chance de acaso é K/N).
    pub qtd_sorteadas: u32,
    /// N: dezenas possíveis da loteria.
    pub qtd_dezenas: u32,
    pub linhas: Vec<db::LinhaRankingTaxa>,
}

/// Por que o `ranking_taxa.json` não vai nesta exportação. Nunca é erro: o
/// resto é publicado e a chave `ranking_taxa` fica fora do `index.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoRankingOmitido {
    /// O banco não tem a tabela `ranking_taxa` (o `sync ranking` nunca rodou).
    SemTabela,
    /// A tabela não tem nenhuma linha do perfil ativo.
    SemLinhas { perfil: String },
    /// O ranking do perfil termina em outro concurso que o do snapshot (em
    /// geral: saiu concurso novo e o `sync ranking` não rodou depois).
    Atrasado { gravado: u32, esperado: u32 },
    /// O concurso certo não tem as N dezenas, uma vez cada.
    Incompleto { linhas: usize, esperadas: usize },
    /// A posição do índice gravada no ranking difere da do índice atual (o
    /// índice foi regerado depois do ranking).
    DivergeDoIndice {
        dezena: u32,
        no_ranking: u32,
        no_indice: u32,
    },
    /// As posições batem, mas as notas ou posições do ranking gravado não são
    /// as que o `sync ranking` gravaria hoje com os resultados, as features e
    /// o índice atuais (ex.: um resultado antigo foi corrigido e só
    /// `sync features` e `sync indice` rodaram depois).
    GeradoDeOutroEstado { dezena: u32 },
    /// Não deu para refazer o cálculo de conferência (as funções puras
    /// recusaram os dados). Não derruba a exportação: o ranking sai omitido,
    /// com a causa.
    RecalculoFalhou(String),
}

impl fmt::Display for MotivoRankingOmitido {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemTabela => write!(
                f,
                "o banco não tem a tabela `{TABELA_RANKING}` (rode `sync ranking`)"
            ),
            Self::SemLinhas { perfil } => write!(
                f,
                "`{TABELA_RANKING}` não tem linhas do perfil `{perfil}` (rode `sync ranking`)"
            ),
            Self::Atrasado { gravado, esperado } => write!(
                f,
                "o ranking vai até o concurso {gravado}, mas o snapshot é do {esperado} \
                 (rode `sync ranking`)"
            ),
            Self::Incompleto { linhas, esperadas } => write!(
                f,
                "o ranking do snapshot tem {linhas} linha(s), esperadas {esperadas} \
                 (rode `sync ranking`)"
            ),
            Self::DivergeDoIndice {
                dezena,
                no_ranking,
                no_indice,
            } => write!(
                f,
                "a dezena {dezena} está na posição {no_ranking} do índice no ranking e \
                 {no_indice} no índice atual (rode `sync ranking`)"
            ),
            Self::GeradoDeOutroEstado { dezena } => write!(
                f,
                "o ranking gravado (dezena {dezena}) não bate com o recalculado dos \
                 resultados, features e índice atuais; foi gerado de outro estado \
                 (rode `sync ranking`)"
            ),
            Self::RecalculoFalhou(causa) => write!(
                f,
                "não foi possível conferir o ranking gravado recalculando-o ({causa}); \
                 rode `sync ranking` e confira features e índice"
            ),
        }
    }
}

/// O que `ranking_taxa` deu para esta exportação.
#[derive(Debug, Clone, PartialEq)]
pub enum RankingParaExportar {
    /// Em dia: vira `ranking_taxa.json`.
    EmDia(RankingProximo),
    /// Ausente ou atrasado: a loteria sai sem o arquivo.
    Omitido(MotivoRankingOmitido),
}

/// Lê `ranking_taxa` do perfil ativo e confere se ela vale para o snapshot
/// `tabela` (o do `tabela_geral.json`). Só lê; nunca falha por ranking
/// ausente ou atrasado (isso é [`RankingParaExportar::Omitido`]), só por erro
/// do banco.
///
/// "Em dia" = o maior concurso do perfil é o do snapshot, ele tem exatamente
/// as N dezenas, e a `posicao_indice` de cada uma é a do índice. O perfil e o
/// hash já foram conferidos por `ler_indice_conferido` (a tabela `ranking_taxa`
/// só guarda o nome do perfil, e o `sync ranking` recusa perfil alterado).
pub(super) fn ler_ranking_para_exportar(
    conn: &Connection,
    cfg: &LoteriaConfig,
    tabela: &crate::orquestracao::TabelaConcurso,
    conferido: &super::IndiceConferido,
) -> Result<RankingParaExportar> {
    use MotivoRankingOmitido::*;
    let omitido = |m| Ok(RankingParaExportar::Omitido(m));
    if !db::tabela_existe(conn, TABELA_RANKING)? {
        return omitido(SemTabela);
    }
    let Some(gravado) = db::ultimo_concurso_ranking_taxa(conn, &tabela.perfil)? else {
        return omitido(SemLinhas {
            perfil: tabela.perfil.clone(),
        });
    };
    if gravado != tabela.concurso {
        return omitido(Atrasado {
            gravado,
            esperado: tabela.concurso,
        });
    }
    let linhas = db::ler_ranking_taxa_do_concurso(conn, &tabela.perfil, tabela.concurso)?;
    let n = cfg.qtd_dezenas_possiveis() as usize;
    let dezenas_certas = linhas.len() == n
        && cfg
            .dezenas()
            .zip(&linhas)
            .all(|(dezena, linha)| linha.dezena == dezena);
    if !dezenas_certas {
        return omitido(Incompleto {
            linhas: linhas.len(),
            esperadas: n,
        });
    }
    for linha in &linhas {
        let no_indice = tabela
            .linhas
            .iter()
            .find(|l| l.dezena == linha.dezena)
            .map(|l| l.posicao);
        if no_indice != Some(linha.posicao_indice) {
            return omitido(DivergeDoIndice {
                dezena: linha.dezena,
                no_ranking: linha.posicao_indice,
                no_indice: no_indice.unwrap_or(0),
            });
        }
    }
    // Mesmas posições não bastam: corrigir um resultado antigo muda o `saiu`
    // do histórico (logo as taxas) sem necessariamente mudar as posições do
    // snapshot. Refaz, só na memória, o cálculo do `sync ranking` com o que
    // `ler_indice_conferido` acabou de conferir e exige igualdade exata
    // (cálculo determinístico; REAL do SQLite volta bit a bit).
    // Problema no cálculo nunca derruba a exportação: só erro de banco (acima).
    let esperado = ranking_do_snapshot(cfg, &tabela.perfil, conferido, tabela.concurso);
    if let Some(motivo) = comparar_com_recalculo(&linhas, esperado) {
        return omitido(motivo);
    }
    Ok(RankingParaExportar::EmDia(RankingProximo {
        qtd_sorteadas: cfg.qtd_dezenas as u32,
        qtd_dezenas: cfg.qtd_dezenas_possiveis(),
        linhas,
    }))
}

/// `None` se o ranking gravado é igual ao recalculado; senão o motivo da
/// omissão. Falha do recálculo e tamanho diferente também omitem (nunca erro).
fn comparar_com_recalculo(
    gravadas: &[db::LinhaRankingTaxa],
    esperado: Result<Vec<db::LinhaRankingTaxa>>,
) -> Option<MotivoRankingOmitido> {
    let esperado = match esperado {
        Ok(e) => e,
        Err(e) => return Some(MotivoRankingOmitido::RecalculoFalhou(e.to_string())),
    };
    if esperado.len() != gravadas.len() {
        return Some(MotivoRankingOmitido::Incompleto {
            linhas: gravadas.len(),
            esperadas: esperado.len(),
        });
    }
    gravadas
        .iter()
        .zip(&esperado)
        .find(|(g, n)| g != n)
        .map(|(g, _)| MotivoRankingOmitido::GeradoDeOutroEstado { dezena: g.dezena })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linha(dezena: u32, nota: f64) -> db::LinhaRankingTaxa {
        db::LinhaRankingTaxa {
            concurso: 10,
            dezena,
            posicao_indice: dezena,
            taxa_posicao: nota,
            nota_b: nota,
            posicao_ranking_a: dezena,
            posicao_ranking_b: dezena,
        }
    }

    #[test]
    fn falha_do_recalculo_omite_com_a_causa_em_vez_de_derrubar() {
        let erro = SpiderError::AvaliacaoSemConcursos("x".into());
        let m = comparar_com_recalculo(&[linha(1, 0.5)], Err(erro)).expect("omite");
        let MotivoRankingOmitido::RecalculoFalhou(causa) = &m else {
            panic!("esperava RecalculoFalhou, veio {m:?}");
        };
        assert!(!causa.is_empty());
        assert!(m.to_string().contains(causa.as_str()));
    }

    #[test]
    fn tamanho_diferente_omite_como_incompleto() {
        let m = comparar_com_recalculo(&[linha(1, 0.5)], Ok(vec![linha(1, 0.5), linha(2, 0.5)]));
        assert_eq!(
            m,
            Some(MotivoRankingOmitido::Incompleto {
                linhas: 1,
                esperadas: 2
            })
        );
    }

    #[test]
    fn igual_nao_omite_e_diferente_aponta_a_dezena() {
        let a = vec![linha(1, 0.5), linha(2, 0.6)];
        assert_eq!(comparar_com_recalculo(&a, Ok(a.clone())), None);
        let b = vec![linha(1, 0.5), linha(2, 0.7)];
        assert_eq!(
            comparar_com_recalculo(&a, Ok(b)),
            Some(MotivoRankingOmitido::GeradoDeOutroEstado { dezena: 2 })
        );
    }
}

/// As linhas de `ranking_taxa` do `concurso` (o snapshot) que o `sync ranking`
/// gravaria hoje, ordenadas por dezena. Só as taxas, as notas e as ordens (sem
/// a evidência nem o esperado, que não vão para o arquivo).
fn ranking_do_snapshot(
    cfg: &LoteriaConfig,
    perfil: &str,
    conferido: &super::IndiceConferido,
    concurso: u32,
) -> Result<Vec<db::LinhaRankingTaxa>> {
    let observacoes = super::ranking_taxa::montar_observacoes(
        cfg,
        perfil,
        &conferido.indice,
        &conferido.features,
    )?;
    let serie = avaliacao::calcular_taxas(cfg, &observacoes)?;
    drop(observacoes);
    // `ranquear` só compara dezenas do próprio concurso (nota e posição de `t`
    // não dependem de outras linhas), então basta passar o do snapshot.
    let do_snapshot: Vec<_> = serie
        .linhas
        .iter()
        .filter(|l| l.concurso == concurso)
        .cloned()
        .collect();
    drop(serie);
    let mut linhas: Vec<db::LinhaRankingTaxa> = avaliacao::ranquear(cfg, &do_snapshot)?
        .iter()
        .map(super::ranking_taxa::linha_do_ranking)
        .collect();
    linhas.sort_by_key(|l| l.dezena);
    Ok(linhas)
}

/// Falha de uma loteria na geração; a CLI imprime uma linha por falha.
#[derive(Debug)]
pub struct FalhaExportacao {
    pub slug: &'static str,
    pub erro: SpiderError,
}

/// Como terminou [`exportar_tudo_ou_nada`].
#[derive(Debug)]
pub enum ResultadoExportacao {
    /// Todas as loterias passaram e os arquivos foram gravados.
    Gravado(Vec<ArquivoGravado>),
    /// Alguma loteria falhou na geração: nada foi gravado.
    Cancelado(Vec<FalhaExportacao>),
}

/// O que aconteceu com o `ranking_taxa.json` de uma loteria exportada.
#[derive(Debug, Clone, PartialEq)]
pub enum SituacaoRanking {
    /// Gravado, e com a chave `ranking_taxa` no `index.json`.
    Publicado,
    /// Não gravado, sem a chave no `index.json`; um arquivo antigo da pasta
    /// foi removido. Não é erro.
    Omitido(MotivoRankingOmitido),
}

/// Uma loteria exportada e a situação do ranking dela.
#[derive(Debug, Clone, PartialEq)]
pub struct RankingDaLoteria {
    pub slug: &'static str,
    pub situacao: SituacaoRanking,
}

/// O que [`exportar_com_relatorio`] gravou.
#[derive(Debug)]
pub struct RelatorioExportacao {
    /// Arquivos gravados (renomeados) nesta execução, com o `index.json` por
    /// último. É o que [`ResultadoExportacao::Gravado`] traz.
    pub gravados: Vec<ArquivoGravado>,
    /// Uma entrada por loteria exportada, na ordem de `alvos`: o ranking
    /// entrou ou foi omitido (e por quê).
    pub rankings: Vec<RankingDaLoteria>,
    /// `ranking_taxa.json` antigos que foram apagados por causa de omissão.
    pub removidos: Vec<PathBuf>,
}

/// Como terminou [`exportar_com_relatorio`].
#[derive(Debug)]
pub enum ResultadoComRelatorio {
    Gravado(RelatorioExportacao),
    Cancelado(Vec<FalhaExportacao>),
}

/// Lê e confere o banco de uma loteria com o perfil ativo e gera os arquivos
/// dela (os três obrigatórios e, se o ranking estiver em dia, o
/// `ranking_taxa.json`). Não grava nada. Banco inexistente é erro, sem criar
/// arquivo.
pub fn gerar_exportacao(
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    raiz_perfis: &Path,
) -> Result<ArquivosLoteria> {
    if !db_caminho.exists() {
        return Err(SpiderError::BancoNaoEncontrado(db_caminho.to_path_buf()));
    }
    let carregado = perfil::carregar(raiz_perfis, cfg, cfg.perfil_ativo)?;
    let mut conn = db::abrir_somente_leitura(db_caminho)?;
    let dados = super::dados_para_exportar(&mut conn, cfg, &carregado)?;
    exportacao::gerar_loteria(&dados)
}

/// Gera os arquivos de todas as `alvos` na memória (só lê os bancos) e só
/// então grava, com o `index.json` de todas as loterias do config. Se uma
/// falhar, nada é gravado e as falhas voltam em
/// [`ResultadoExportacao::Cancelado`]. `gerado_em` (o relógio) só é lido
/// depois de todas passarem. Erros de gravação ou do relógio voltam em `Err`.
///
/// Ranking ausente ou atrasado **não** é falha. Para saber se o ranking entrou,
/// use [`exportar_com_relatorio`] (esta função só descarta essa informação).
pub fn exportar_tudo_ou_nada(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    raiz_perfis: &Path,
    saida: &Path,
    gerado_em: impl FnOnce() -> Result<String>,
) -> Result<ResultadoExportacao> {
    Ok(
        match exportar_com_relatorio(alvos, raiz_perfis, saida, gerado_em)? {
            ResultadoComRelatorio::Gravado(r) => ResultadoExportacao::Gravado(r.gravados),
            ResultadoComRelatorio::Cancelado(f) => ResultadoExportacao::Cancelado(f),
        },
    )
}

/// Igual a [`exportar_tudo_ou_nada`], mas devolve também, por loteria, se o
/// `ranking_taxa.json` entrou ou foi omitido (com o motivo), e os arquivos
/// antigos removidos. É a API para quem precisa reagir à omissão (a CLI
/// imprime; o `sync ciclo` pode registrar alerta).
pub fn exportar_com_relatorio(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    raiz_perfis: &Path,
    saida: &Path,
    gerado_em: impl FnOnce() -> Result<String>,
) -> Result<ResultadoComRelatorio> {
    let mut lotes = Vec::new();
    let mut rankings = Vec::new();
    let mut falhas = Vec::new();
    for (cfg, db_caminho) in alvos {
        match gerar_exportacao(cfg, db_caminho, raiz_perfis) {
            Ok(lote) => {
                rankings.push(RankingDaLoteria {
                    slug: cfg.slug,
                    situacao: match &lote.ranking_omitido {
                        None => SituacaoRanking::Publicado,
                        Some(m) => SituacaoRanking::Omitido(m.clone()),
                    },
                });
                lotes.push(lote);
            }
            Err(erro) => falhas.push(FalhaExportacao {
                slug: cfg.slug,
                erro,
            }),
        }
    }
    if !falhas.is_empty() {
        return Ok(ResultadoComRelatorio::Cancelado(falhas));
    }
    let slugs: Vec<&str> = config::LOTERIAS.iter().map(|l| l.slug).collect();
    let gravacao = exportacao::disco::gravar_com_relatorio(saida, &lotes, &slugs, &gerado_em()?)?;
    Ok(ResultadoComRelatorio::Gravado(RelatorioExportacao {
        gravados: gravacao.gravados,
        rankings,
        removidos: gravacao.removidos,
    }))
}
