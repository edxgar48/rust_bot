use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::log_arquivo::{diagnosticar, Nivel};
use crate::model::Resultado;
use calamine::{open_workbook_auto, Data, DataType, Range, Reader, Xlsx};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Lê a planilha de resultados (.xlsx, apesar do link às vezes sugerir .htm)
/// e extrai os concursos, de acordo com o mapeamento de colunas em `config`.
pub fn parsear_arquivo(caminho: &Path, config: &LoteriaConfig) -> Result<Vec<Resultado>> {
    let erro_planilha = |detalhe: String| SpiderError::Planilha {
        caminho: caminho.to_path_buf(),
        detalhe,
    };

    let intervalo = abrir_aba(caminho).map_err(erro_planilha)?;

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
            diagnosticar(
                Nivel::Erro,
                &format!(
                    "[{}] encontrei {} linha(s) na planilha, mas nenhuma bateu com o layout \
                 configurado (coluna_concurso={}, coluna_data={}, primeira_coluna_dezena={}).\n\
                 Exemplo de linha crua, célula por índice: {:?}",
                    config.slug,
                    linhas_com_colunas_suficientes,
                    config.coluna_concurso,
                    config.coluna_data,
                    config.primeira_coluna_dezena,
                    exemplo
                ),
            );
        }
        return Err(SpiderError::TabelaNaoEncontrada(caminho.to_path_buf()));
    }

    Ok(resultados)
}

/// Abre a primeira aba. Se o leitor recusar a planilha, tenta de novo depois de
/// corrigir as células numéricas com "-": a planilha da Lotomania marca assim
/// (`t="n"`) a coluna de ganhadores sem prêmio e o calamine aborta com
/// "invalid float literal". O erro devolvido é o da leitura original.
fn abrir_aba(caminho: &Path) -> std::result::Result<Range<Data>, String> {
    let primeiro = match ler_primeira_aba(open_workbook_auto(caminho).map_err(|e| e.to_string())?) {
        Ok(intervalo) => return Ok(intervalo),
        Err(erro) => erro,
    };
    let corrigida = corrigir_celulas_com_traco(caminho).ok_or_else(|| primeiro.clone())?;
    let planilha = Xlsx::new(Cursor::new(corrigida)).map_err(|_| primeiro.clone())?;
    ler_primeira_aba(planilha).map_err(|_| primeiro)
}

fn ler_primeira_aba<RS: Read + std::io::Seek, R: Reader<RS>>(
    mut planilha: R,
) -> std::result::Result<Range<Data>, String>
where
    R::Error: std::fmt::Display,
{
    planilha
        .worksheet_range_at(0)
        .ok_or_else(|| "a planilha não tem nenhuma aba".to_string())?
        .map_err(|e| e.to_string())
}

/// Reescreve o `.xlsx` em memória trocando `t="n"` por `t="str"` nas células
/// cujo valor é só "-". `None` se não for um zip legível ou se nada mudou.
fn corrigir_celulas_com_traco(caminho: &Path) -> Option<Vec<u8>> {
    let mut origem = ZipArchive::new(std::fs::File::open(caminho).ok()?).ok()?;
    let mut destino = ZipWriter::new(Cursor::new(Vec::new()));
    let mut mudou = false;
    for i in 0..origem.len() {
        let mut parte = origem.by_index(i).ok()?;
        let nome = parte.name().to_string();
        let mut bytes = Vec::new();
        parte.read_to_end(&mut bytes).ok()?;
        if nome.starts_with("xl/worksheets/") && nome.ends_with(".xml") {
            for (de, para) in [
                (&b"t=\"n\"><x:v>-</x:v>"[..], &b"t=\"str\"><x:v>-</x:v>"[..]),
                (&b"t=\"n\"><v>-</v>"[..], &b"t=\"str\"><v>-</v>"[..]),
            ] {
                if let Some(trocado) = substituir_bytes(&bytes, de, para) {
                    bytes = trocado;
                    mudou = true;
                }
            }
        }
        destino
            .start_file(nome, SimpleFileOptions::default())
            .ok()?;
        destino.write_all(&bytes).ok()?;
    }
    mudou.then(|| destino.finish().ok().map(Cursor::into_inner))?
}

/// `entrada` com todas as ocorrências de `de` trocadas por `para`; `None` se
/// `de` não aparece. Trabalha em bytes porque a planilha da Caixa não é UTF-8.
fn substituir_bytes(entrada: &[u8], de: &[u8], para: &[u8]) -> Option<Vec<u8>> {
    let mut saida = Vec::with_capacity(entrada.len());
    let mut resto = entrada;
    let mut achou = false;
    while let Some(pos) = resto.windows(de.len()).position(|janela| janela == de) {
        saida.extend_from_slice(&resto[..pos]);
        saida.extend_from_slice(para);
        resto = &resto[pos + de.len()..];
        achou = true;
    }
    achou.then(|| {
        saida.extend_from_slice(resto);
        saida
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_formato_da_caixa() {
        assert_eq!(normalizar_data("29/09/2003").as_deref(), Some("2003-09-29"));
        assert_eq!(normalizar_data("01/10/2026").as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn completa_dia_e_mes_com_zero() {
        assert_eq!(normalizar_data("1/3/1996").as_deref(), Some("1996-03-01"));
        assert_eq!(normalizar_data("9/12/1996").as_deref(), Some("1996-12-09"));
    }

    #[test]
    fn ignora_espacos_nas_pontas() {
        assert_eq!(
            normalizar_data("  11/03/1996 \t").as_deref(),
            Some("1996-03-11")
        );
    }

    #[test]
    fn rejeita_formatos_fora_do_padrao() {
        for bruto in [
            "",
            "11/03",
            "11/03/1996/1",
            "1996-03-11",
            "11/03/96",
            "11/03/19960",
            "/03/1996",
            "11//1996",
            "111/03/1996",
            "11/003/1996",
            "11/03/1996 00:00",
        ] {
            assert_eq!(normalizar_data(bruto), None, "{bruto:?}");
        }
    }
}
