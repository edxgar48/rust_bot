//! Conferência da saída do `sync exportar` (`publico/v1/`) antes de publicar.
//!
//! O `exportar` renomeia os arquivos um a um e o `index.json` por último; uma
//! falha no meio pode deixar arquivos novos e antigos misturados. Aqui nada é
//! confiado: cada `sha256` e `bytes` do índice é refeito a partir do disco, e
//! arquivo que o índice não cita (sobra de `.tmp`, loteria `null` com pasta
//! cheia) também reprova, para que o espelho seja idêntico à saída.

use super::indice::{caminho_seguro, ler_indice, IndiceLido};
use crate::error::{Result, SpiderError};
use crate::exportacao::disco::pasta_versao;
use crate::exportacao::{Impressao, ARQUIVO_INDEX};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Saída conferida: o que pode ser copiado.
#[derive(Debug, Clone)]
pub struct SaidaConferida {
    /// `<saida>/v1`.
    pub raiz: PathBuf,
    pub indice: IndiceLido,
    /// Caminhos relativos a `raiz` (separador `/`) a copiar: o `index.json`
    /// e todo arquivo que ele cita.
    pub arquivos: Vec<String>,
}

fn reprovada(raiz: &Path, motivo: impl Into<String>) -> SpiderError {
    SpiderError::PublicacaoSaidaInvalida {
        caminho: raiz.to_path_buf(),
        motivo: motivo.into(),
    }
}

/// Confere `<saida>/v1` contra o próprio `index.json`. Só lê.
pub fn conferir(saida: &Path) -> Result<SaidaConferida> {
    let raiz = pasta_versao(saida);
    let caminho_indice = raiz.join(ARQUIVO_INDEX);
    let bytes = match fs::read(&caminho_indice) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(reprovada(&raiz, format!("não há `{ARQUIVO_INDEX}`")))
        }
        Err(origem) => {
            return Err(SpiderError::Io {
                caminho: caminho_indice,
                origem,
            })
        }
    };
    let indice = ler_indice(&bytes)
        .map_err(|m| reprovada(&raiz, format!("`{ARQUIVO_INDEX}` inválido: {m}")))?;

    let mut esperados: BTreeSet<String> = BTreeSet::new();
    esperados.insert(ARQUIVO_INDEX.to_string());
    for (slug, entrada) in &indice.loterias {
        let Some(entrada) = entrada else { continue };
        for a in &entrada.arquivos {
            if !caminho_seguro(&a.caminho) {
                return Err(reprovada(
                    &raiz,
                    format!("`{slug}`: caminho `{}` não é relativo a `v1/`", a.caminho),
                ));
            }
            if !esperados.insert(a.caminho.clone()) {
                return Err(reprovada(
                    &raiz,
                    format!("o caminho `{}` aparece duas vezes no índice", a.caminho),
                ));
            }
            let arquivo = raiz.join(&a.caminho);
            let conteudo = match fs::read(&arquivo) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(reprovada(
                        &raiz,
                        format!("`{}` está no índice mas não existe no disco", a.caminho),
                    ))
                }
                Err(origem) => {
                    return Err(SpiderError::Io {
                        caminho: arquivo,
                        origem,
                    })
                }
            };
            let real = Impressao::de(&conteudo);
            if real.bytes != a.bytes {
                return Err(reprovada(
                    &raiz,
                    format!(
                        "`{}` tem {} bytes no disco, o índice diz {}",
                        a.caminho, real.bytes, a.bytes
                    ),
                ));
            }
            if real.sha256 != a.sha256 {
                return Err(reprovada(
                    &raiz,
                    format!(
                        "`{}` tem sha256 {} no disco, o índice diz {}",
                        a.caminho, real.sha256, a.sha256
                    ),
                ));
            }
        }
    }

    let mut presentes = Vec::new();
    listar(&raiz, "", &mut presentes)?;
    if let Some(extra) = presentes.iter().find(|p| !esperados.contains(*p)) {
        return Err(reprovada(
            &raiz,
            format!(
                "`{extra}` está na pasta mas não no índice (sobra de uma exportação interrompida?)"
            ),
        ));
    }
    Ok(SaidaConferida {
        raiz,
        indice,
        arquivos: esperados.into_iter().collect(),
    })
}

/// Arquivos (não pastas) sob `pasta`, com caminho relativo e separador `/`.
fn listar(pasta: &Path, prefixo: &str, saida: &mut Vec<String>) -> Result<()> {
    let io = |origem| SpiderError::Io {
        caminho: pasta.to_path_buf(),
        origem,
    };
    for item in fs::read_dir(pasta).map_err(io)? {
        let item = item.map_err(io)?;
        let nome = item.file_name().to_string_lossy().into_owned();
        let relativo = if prefixo.is_empty() {
            nome
        } else {
            format!("{prefixo}/{nome}")
        };
        if item.file_type().map_err(io)?.is_dir() {
            listar(&item.path(), &relativo, saida)?;
        } else {
            saida.push(relativo);
        }
    }
    Ok(())
}
