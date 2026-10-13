//! Geração e gravação do `sync exportar`, movidas da CLI (tarefa 3.1a): gerar
//! os arquivos de uma loteria ([`gerar_exportacao`]) e o tudo ou nada de
//! várias ([`exportar_tudo_ou_nada`]). O `sync ciclo` exporta uma loteria por
//! vez com a mesma função, passando uma lista de um item.

use crate::config::{self, LoteriaConfig};
use crate::db;
use crate::error::{Result, SpiderError};
use crate::exportacao::{self, disco::ArquivoGravado, ArquivosLoteria};
use crate::perfil;
use std::path::{Path, PathBuf};

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

/// Lê e confere o banco de uma loteria com o perfil ativo e gera os três
/// arquivos dela. Não grava nada. Banco inexistente é erro, sem criar arquivo.
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
pub fn exportar_tudo_ou_nada(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    raiz_perfis: &Path,
    saida: &Path,
    gerado_em: impl FnOnce() -> Result<String>,
) -> Result<ResultadoExportacao> {
    let mut lotes = Vec::new();
    let mut falhas = Vec::new();
    for (cfg, db_caminho) in alvos {
        match gerar_exportacao(cfg, db_caminho, raiz_perfis) {
            Ok(lote) => lotes.push(lote),
            Err(erro) => falhas.push(FalhaExportacao {
                slug: cfg.slug,
                erro,
            }),
        }
    }
    if !falhas.is_empty() {
        return Ok(ResultadoExportacao::Cancelado(falhas));
    }
    let slugs: Vec<&str> = config::LOTERIAS.iter().map(|l| l.slug).collect();
    let gravados = exportacao::disco::gravar(saida, &lotes, &slugs, &gerado_em()?)?;
    Ok(ResultadoExportacao::Gravado(gravados))
}
