//! CSV da tabela geral de um concurso (`sync tabela`, 1b.4).
//!
//! Formato pensado para abrir no Excel em português: separador `;`, vírgula
//! decimal e `NULL` como célula vazia. Uma linha por dezena, em ordem de
//! posição. Mesma tabela → mesmos bytes (o arquivo de um concurso pode ser
//! regravado sem mudar).
//!
//! O arquivo é UTF-8 **com BOM** ([`BOM`]): o Excel pt-BR, ao abrir com duplo
//! clique, assume Windows-1252 quando não há BOM e mostraria "viÃ©s" no rótulo
//! "a favor de viés". (O CSV de máquina do `sync dataset` segue sem BOM, por
//! decisão do plano.)
//!
//! As duas últimas colunas (`evidencia_grau`, `evidencia_direcao`) trazem a
//! evidência bayesiana por dezena (Fase 1d, A) no ponto no tempo do concurso,
//! com os mesmos rótulos do `sync evidencia`; vazias no concurso 1.

use crate::orquestracao::TabelaConcurso;
use std::fmt::Write;

/// Cabeçalho do CSV, na ordem das colunas.
pub const CABECALHO: &str = "loteria;perfil;concurso;data_sorteio;dezena;posicao;total;\
variacao_posicao;saiu;janela;frequencia;maior_espaco;maior_espaco_ocorrencias;menor_espaco;\
menor_espaco_ocorrencias;soma_variacoes;espaco_final;repeticoes_total;repeticao_atual;\
evidencia_grau;evidencia_direcao";

/// Marca de ordem de bytes UTF-8 (`EF BB BF`) que abre o arquivo, para o Excel
/// reconhecer os acentos.
pub const BOM: &str = "\u{FEFF}";

/// Casas decimais do `total` no CSV (o índice guarda o total arredondado a
/// 1e-9; 6 casas bastam para comparar e ordenar à mão).
pub const CASAS_TOTAL: usize = 6;

/// Nome do arquivo: `tabela_<loteria>_<perfil>_<concurso>.csv`.
pub fn nome_arquivo(tabela: &TabelaConcurso) -> String {
    format!(
        "tabela_{}_{}_{}.csv",
        tabela.loteria, tabela.perfil, tabela.concurso
    )
}

/// O CSV inteiro: [`BOM`], cabeçalho e fim de linha `\n`.
pub fn formatar_csv(tabela: &TabelaConcurso) -> String {
    let opcional = |v: Option<u32>| v.map_or_else(String::new, |v| v.to_string());
    let mut saida = String::from(BOM);
    saida.push_str(CABECALHO);
    saida.push('\n');
    for l in &tabela.linhas {
        let f = &l.features;
        // `write!` num `String` não falha.
        let _ = writeln!(
            saida,
            "{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{}",
            tabela.loteria,
            tabela.perfil,
            tabela.concurso,
            tabela.data_sorteio.as_deref().unwrap_or(""),
            l.dezena,
            l.posicao,
            decimal(l.total),
            l.variacao_posicao
                .map_or_else(String::new, |v| v.to_string()),
            l.saiu.map_or("", |s| if s { "1" } else { "0" }),
            tabela.janela,
            f.frequencia,
            opcional(f.maior_espaco),
            opcional(f.maior_espaco_ocorrencias),
            opcional(f.menor_espaco),
            opcional(f.menor_espaco_ocorrencias),
            opcional(f.soma_variacoes),
            opcional(f.espaco_final),
            f.repeticoes_total,
            f.repeticao_atual,
            l.evidencia.map_or("", |e| e.grau.nome()),
            l.evidencia.map_or("", |e| e.direcao.nome()),
        );
    }
    saida
}

/// `total` com [`CASAS_TOTAL`] casas e vírgula decimal, sem `-0,000000`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avaliacao::{Classificacao, Direcao, Grau};
    use crate::motor::FeaturesDezena;
    use crate::orquestracao::LinhaTabela;

    fn features(dezena: u32, saiu: Option<bool>) -> FeaturesDezena {
        FeaturesDezena {
            concurso: 21,
            dezena,
            janela: 10,
            frequencia: 4,
            maior_espaco: Some(3),
            maior_espaco_ocorrencias: Some(1),
            menor_espaco: Some(0),
            menor_espaco_ocorrencias: Some(2),
            soma_variacoes: None,
            espaco_final: Some(0),
            repeticoes_total: 2,
            repeticao_atual: 1,
            saiu,
        }
    }

    fn tabela(data: Option<&str>, saiu: Option<bool>) -> TabelaConcurso {
        TabelaConcurso {
            loteria: "quina".to_string(),
            perfil: "padrao_v1".to_string(),
            janela: 10,
            concurso: 21,
            ultimo_sorteado: 20,
            data_sorteio: data.map(str::to_string),
            linhas: vec![
                LinhaTabela {
                    dezena: 7,
                    posicao: 1,
                    total: 2.5,
                    variacao_posicao: Some(3),
                    saiu,
                    features: features(7, saiu),
                    evidencia: Some(Classificacao {
                        grau: Grau::Moderada,
                        direcao: Direcao::Vies,
                    }),
                },
                LinhaTabela {
                    dezena: 3,
                    posicao: 2,
                    total: -0.000_000_000_4,
                    variacao_posicao: None,
                    saiu: saiu.map(|s| !s),
                    features: features(3, saiu),
                    evidencia: Some(Classificacao {
                        grau: Grau::MuitoFraca,
                        direcao: Direcao::Acaso,
                    }),
                },
            ],
        }
    }

    #[test]
    fn csv_do_proximo_concurso_confere_byte_a_byte() {
        let esperado = format!(
            "{BOM}{CABECALHO}\n\
             quina;padrao_v1;21;;7;1;2,500000;3;;10;4;3;1;0;2;;0;2;1;moderada;a favor de viés\n\
             quina;padrao_v1;21;;3;2;0,000000;;;10;4;3;1;0;2;;0;2;1;muito fraca;a favor do acaso\n"
        );
        assert_eq!(formatar_csv(&tabela(None, None)), esperado);
    }

    #[test]
    fn csv_comeca_com_bom_utf8_e_os_acentos_vao_em_utf8() {
        let csv = formatar_csv(&tabela(None, None));
        let bytes = csv.as_bytes();
        assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
        // O BOM aparece uma só vez, só no começo.
        assert_eq!(csv.matches('\u{FEFF}').count(), 1);
        assert_eq!(&bytes[3..3 + CABECALHO.len()], CABECALHO.as_bytes());
        // "é" de "viés" em UTF-8 (C3 A9), não em Windows-1252.
        assert!(bytes.windows(2).any(|w| w == [0xC3, 0xA9]));
    }

    #[test]
    fn csv_de_concurso_sorteado_tem_data_e_saiu() {
        let csv = formatar_csv(&tabela(Some("2026-10-03"), Some(true)));
        let linhas: Vec<&str> = csv.lines().collect();
        assert_eq!(linhas.len(), 3);
        assert!(linhas[1].starts_with("quina;padrao_v1;21;2026-10-03;7;1;2,500000;3;1;"));
        assert!(linhas[2].starts_with("quina;padrao_v1;21;2026-10-03;3;2;0,000000;;0;"));
    }

    #[test]
    fn cabecalho_e_linhas_tem_o_mesmo_numero_de_colunas() {
        let csv = formatar_csv(&tabela(Some("2026-10-03"), Some(false)));
        let csv = csv.strip_prefix(BOM).expect("BOM");
        let colunas = CABECALHO.split(';').count();
        assert_eq!(colunas, 21);
        assert!(CABECALHO.ends_with(";repeticao_atual;evidencia_grau;evidencia_direcao"));
        for linha in csv.lines() {
            assert_eq!(linha.split(';').count(), colunas, "{linha}");
        }
    }

    #[test]
    fn sem_evidencia_as_duas_colunas_ficam_vazias() {
        let mut t = tabela(None, None);
        for l in &mut t.linhas {
            l.evidencia = None;
        }
        let csv = formatar_csv(&t);
        let colunas = CABECALHO.split(';').count();
        for linha in csv.lines().skip(1) {
            let campos: Vec<&str> = linha.split(';').collect();
            assert_eq!(campos.len(), colunas, "{linha}");
            assert_eq!(&campos[colunas - 2..], ["", ""], "{linha}");
        }
    }

    #[test]
    fn rotulos_de_grau_e_direcao_sao_os_do_relatorio() {
        let mut t = tabela(None, None);
        t.linhas.truncate(1);
        let esperados = [
            (Grau::MuitoFraca, "muito fraca"),
            (Grau::Fraca, "fraca"),
            (Grau::Moderada, "moderada"),
            (Grau::Forte, "forte"),
            (Grau::MuitoForte, "muito forte"),
        ];
        for (grau, nome) in esperados {
            for (direcao, rotulo) in [
                (Direcao::Vies, "a favor de viés"),
                (Direcao::Acaso, "a favor do acaso"),
            ] {
                t.linhas[0].evidencia = Some(Classificacao { grau, direcao });
                let csv = formatar_csv(&t);
                let linha = csv.lines().nth(1).expect("linha da dezena");
                assert!(linha.ends_with(&format!(";{nome};{rotulo}")), "{linha}");
            }
        }
    }

    #[test]
    fn decimal_usa_virgula_e_nao_escreve_menos_zero() {
        assert_eq!(decimal(1.0), "1,000000");
        assert_eq!(decimal(-1.234_567_8), "-1,234568");
        assert_eq!(decimal(-0.0), "0,000000");
        assert_eq!(decimal(-0.000_000_1), "0,000000");
    }

    #[test]
    fn nome_do_arquivo_tem_loteria_perfil_e_concurso() {
        assert_eq!(
            nome_arquivo(&tabela(None, None)),
            "tabela_quina_padrao_v1_21.csv"
        );
    }
}
