//! Gravação da exportação em disco (`sync exportar`).
//!
//! Sem SQL e sem regra de negócio: pega os bytes que [`super`] gerou, grava
//! cada arquivo num temporário vizinho e o renomeia (nunca fica um arquivo
//! pela metade) e refaz o `index.json` com o que está **no disco** depois da
//! gravação, tenha sido regravado agora ou numa execução anterior.
//!
//! Ordem: confere a saída das loterias que não serão regravadas, grava todos
//! os temporários (uma falha **nesta etapa** apaga os temporários e deixa a
//! saída como estava), renomeia os arquivos das loterias e, por último, o
//! `index.json`. Uma falha **no meio das renomeações** pode deixar arquivos
//! novos e antigos misturados, com o `index.json` ainda antigo; o próximo
//! `exportar` corrige, e o `sync publicar` (2.4) deve recusar se os hashes do
//! `index.json` não baterem com o disco.

use super::{
    index_json, Arquivo, ArquivosLoteria, Impressao, LoteriaNoIndice, ARQUIVO_INDEX, PASTA_VERSAO,
    SCHEMA_VERSION,
};
use crate::error::{Result, SpiderError};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Um arquivo gravado pela execução, para o relatório da CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivoGravado {
    pub caminho: PathBuf,
    pub impressao: Impressao,
}

/// `<saida>/v1`.
pub fn pasta_versao(saida: &Path) -> PathBuf {
    saida.join(PASTA_VERSAO)
}

fn io(caminho: &Path, origem: std::io::Error) -> SpiderError {
    SpiderError::Io {
        caminho: caminho.to_path_buf(),
        origem,
    }
}

fn invalida(caminho: &Path, motivo: &str) -> SpiderError {
    SpiderError::ExportacaoInvalida(format!("`{}`: {motivo}", caminho.display()))
}

/// Grava os `lotes` (um por loteria) em `<saida>/v1/<loteria>/` e refaz o
/// `index.json` com `slugs` (as loterias do config, na ordem do config).
/// Loteria que não está em `lotes` mantém os arquivos que já tinha.
pub fn gravar(
    saida: &Path,
    lotes: &[ArquivosLoteria],
    slugs: &[&str],
    gerado_em: &str,
) -> Result<Vec<ArquivoGravado>> {
    let raiz = pasta_versao(saida);

    // Saída das loterias preservadas: se estiver inconsistente, recusa antes
    // de gravar qualquer coisa.
    for slug in slugs {
        if !lotes.iter().any(|l| l.slug == *slug) {
            ler_loteria(&raiz, slug)?;
        }
    }

    // 1) Temporários de todos os arquivos.
    let mut pares: Vec<(PathBuf, PathBuf)> = Vec::new(); // (temporário, definitivo)
    let escrito = (|| -> Result<()> {
        for lote in lotes {
            let pasta = raiz.join(&lote.slug);
            fs::create_dir_all(&pasta).map_err(|e| io(&pasta, e))?;
            for arquivo in Arquivo::TODOS {
                let definitivo = pasta.join(arquivo.nome());
                let temporario = pasta.join(format!("{}.tmp", arquivo.nome()));
                escrever_e_sincronizar(&temporario, lote.bytes(arquivo))?;
                pares.push((temporario, definitivo));
            }
        }
        Ok(())
    })();
    if let Err(e) = escrito {
        for (temporario, _) in &pares {
            let _ = fs::remove_file(temporario);
        }
        return Err(e);
    }

    // 2) Renomeia (substitui o arquivo antigo de uma vez).
    let mut gravados = Vec::new();
    for ((temporario, definitivo), (lote, arquivo)) in pares.iter().zip(
        lotes
            .iter()
            .flat_map(|l| Arquivo::TODOS.into_iter().map(move |a| (l, a))),
    ) {
        if let Err(e) = fs::rename(temporario, definitivo) {
            for (resto, _) in &pares {
                let _ = fs::remove_file(resto);
            }
            return Err(io(definitivo, e));
        }
        gravados.push(ArquivoGravado {
            caminho: definitivo.clone(),
            impressao: Impressao::de(lote.bytes(arquivo)),
        });
    }

    // 3) O índice, por último, com o que está no disco.
    let mut entradas: Vec<(&str, Option<LoteriaNoIndice>)> = Vec::with_capacity(slugs.len());
    for slug in slugs {
        entradas.push((slug, ler_loteria(&raiz, slug)?));
    }
    let bytes = index_json(gerado_em, &entradas).into_bytes();
    let definitivo = raiz.join(ARQUIVO_INDEX);
    let temporario = raiz.join(format!("{ARQUIVO_INDEX}.tmp"));
    escrever_e_sincronizar(&temporario, &bytes)?;
    if let Err(e) = fs::rename(&temporario, &definitivo) {
        let _ = fs::remove_file(&temporario);
        return Err(io(&definitivo, e));
    }
    gravados.push(ArquivoGravado {
        caminho: definitivo,
        impressao: Impressao::de(&bytes),
    });
    Ok(gravados)
}

fn escrever_e_sincronizar(caminho: &Path, bytes: &[u8]) -> Result<()> {
    let mut arquivo = fs::File::create(caminho).map_err(|e| io(caminho, e))?;
    arquivo.write_all(bytes).map_err(|e| io(caminho, e))?;
    arquivo.sync_all().map_err(|e| io(caminho, e))
}

/// O que o disco tem de uma loteria em `raiz` (`<saida>/v1`): `None` se não
/// há nenhum dos três arquivos; erro se faltar algum, se o `ultimo.json` não
/// for o esperado ou se for de outro `schema_version`.
pub fn ler_loteria(raiz: &Path, slug: &str) -> Result<Option<LoteriaNoIndice>> {
    let pasta = raiz.join(slug);
    let mut lidos: Vec<Option<Vec<u8>>> = Vec::with_capacity(3);
    for arquivo in Arquivo::TODOS {
        let caminho = pasta.join(arquivo.nome());
        match fs::read(&caminho) {
            Ok(bytes) => lidos.push(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => lidos.push(None),
            Err(e) => return Err(io(&caminho, e)),
        }
    }
    if lidos.iter().all(Option::is_none) {
        return Ok(None);
    }
    if let Some(i) = lidos.iter().position(Option::is_none) {
        return Err(invalida(
            &pasta.join(Arquivo::TODOS[i].nome()),
            "arquivo ausente, mas a loteria tem outros arquivos na saída — exporte a loteria de novo ou apague a pasta dela",
        ));
    }
    let [Some(ultimo), Some(tabela), Some(historico)] = &lidos[..] else {
        unreachable!("os três arquivos foram lidos");
    };

    let caminho_ultimo = pasta.join(Arquivo::Ultimo.nome());
    let json: serde_json::Value = serde_json::from_slice(ultimo)
        .map_err(|e| invalida(&caminho_ultimo, &format!("JSON inválido ({e})")))?;
    if json["schema_version"].as_u64() != Some(u64::from(SCHEMA_VERSION)) {
        return Err(invalida(
            &caminho_ultimo,
            &format!("schema_version diferente de {SCHEMA_VERSION}"),
        ));
    }
    if json["loteria"].as_str() != Some(slug) {
        return Err(invalida(
            &caminho_ultimo,
            &format!("o campo `loteria` não é `{slug}`"),
        ));
    }
    let ultimo_concurso = json["concurso"]
        .as_u64()
        .and_then(|c| u32::try_from(c).ok())
        .ok_or_else(|| invalida(&caminho_ultimo, "campo `concurso` ausente ou inválido"))?;
    let data = json["data"]
        .as_str()
        .ok_or_else(|| invalida(&caminho_ultimo, "campo `data` ausente ou inválido"))?
        .to_string();

    Ok(Some(LoteriaNoIndice {
        ultimo_concurso,
        data,
        arquivos: [
            Impressao::de(ultimo),
            Impressao::de(tabela),
            Impressao::de(historico),
        ],
    }))
}
