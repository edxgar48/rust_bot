use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use calamine::{open_workbook_auto, Data, DataType, Reader};
use std::path::Path;

/// Lê a planilha de resultados (.xlsx, apesar do link às vezes sugerir .htm)
/// e extrai os concursos, de acordo com o mapeamento de colunas em `config`.
pub fn parsear_arquivo(caminho: &Path, config: &LoteriaConfig) -> Result<Vec<Resultado>> {
    let erro_planilha = |detalhe: String| SpiderError::Planilha {
        caminho: caminho.to_path_buf(),
        detalhe,
    };

    let mut planilha = open_workbook_auto(caminho).map_err(|e| erro_planilha(e.to_string()))?;
    let intervalo = planilha
        .worksheet_range_at(0)
        .ok_or_else(|| erro_planilha("a planilha não tem nenhuma aba".to_string()))?
        .map_err(|e| erro_planilha(e.to_string()))?;

    let mut resultados = Vec::new();
    let mut linhas_com_colunas_suficientes = 0usize;
    let mut exemplo_nao_reconhecido: Option<Vec<String>> = None;

    for linha in intervalo.rows().skip(1) {
        // pula a linha de cabeçalho
        if linha.len() <= config.primeira_coluna_dezena {
            continue;
        }
        linhas_com_colunas_suficientes += 1;

        match interpretar_linha(linha, config) {
            Some(r) => resultados.push(r),
            None => {
                exemplo_nao_reconhecido
                    .get_or_insert_with(|| linha.iter().map(celula_para_texto).collect());
            }
        }
    }

    if resultados.is_empty() {
        if let Some(exemplo) = exemplo_nao_reconhecido {
            eprintln!(
                "[{}] encontrei {} linha(s) na planilha, mas nenhuma bateu com o layout \
                 configurado (coluna_concurso={}, coluna_data={}, primeira_coluna_dezena={}).\n\
                 Exemplo de linha crua, célula por índice: {:?}",
                config.slug,
                linhas_com_colunas_suficientes,
                config.coluna_concurso,
                config.coluna_data,
                config.primeira_coluna_dezena,
                exemplo
            );
        }
        return Err(SpiderError::TabelaNaoEncontrada(caminho.to_path_buf()));
    }

    Ok(resultados)
}

fn interpretar_linha(celulas: &[Data], config: &LoteriaConfig) -> Option<Resultado> {
    let concurso = celulas.get(config.coluna_concurso)?.as_i64()? as u32;
    let data_bruta = celula_para_texto(celulas.get(config.coluna_data)?);
    let data_sorteio = normalizar_data(&data_bruta)?;

    let mut dezenas = Vec::with_capacity(config.qtd_dezenas);
    for i in 0..config.qtd_dezenas {
        let valor = celulas.get(config.primeira_coluna_dezena + i)?.as_i64()? as u32;
        dezenas.push(valor);
    }

    Some(Resultado {
        loteria: config.slug.to_string(),
        concurso,
        data_sorteio,
        dezenas,
    })
}

fn celula_para_texto(celula: &Data) -> String {
    match celula {
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Empty | Data::DateTime(_) | Data::Error(_) => String::new(),
    }
}

/// Converte `DD/MM/AAAA` (formato usado pela Caixa) para ISO `AAAA-MM-DD`.
pub(crate) fn normalizar_data(bruto: &str) -> Option<String> {
    let partes: Vec<&str> = bruto.trim().split('/').collect();
    if partes.len() != 3 {
        return None;
    }
    let (dia, mes, ano) = (partes[0], partes[1], partes[2]);
    if dia.is_empty() || dia.len() > 2 || mes.is_empty() || mes.len() > 2 || ano.len() != 4 {
        return None;
    }
    Some(format!("{ano}-{mes:0>2}-{dia:0>2}"))
}
