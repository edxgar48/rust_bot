//! Testes do parser contra recortes das planilhas reais da Caixa
//! (`tests/fixtures/`, geradas por `tests/comum/mod.rs`).

mod comum;

use calamine::{open_workbook_auto, Data, Reader};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::parser::parsear_arquivo;
use rust_spider::{Resultado, SpiderError};
use std::path::PathBuf;

fn fixture(nome: &str) -> PathBuf {
    comum::dir_fixtures().join(nome)
}

fn loteria(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn res(slug: &str, concurso: u32, data: &str, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: slug.to_string(),
        concurso,
        data_sorteio: data.to_string(),
        dezenas: dezenas.to_vec(),
    }
}

/// Regrava `tests/fixtures/` a partir do gerador. Rode só ao mudar o gerador:
/// `cargo test --test parser -- --ignored gerar_fixtures`.
#[test]
#[ignore = "regrava as fixtures versionadas; rode só ao mudar tests/comum/mod.rs"]
fn gerar_fixtures() {
    let dir = comum::dir_fixtures();
    std::fs::create_dir_all(&dir).expect("criar tests/fixtures");
    for (nome, bytes) in comum::todas_as_fixtures() {
        std::fs::write(dir.join(nome), bytes).expect("gravar fixture");
    }
}

/// Compara o conteúdo descompactado (nomes e bytes de cada parte), não o
/// contêiner ZIP: assim um `cargo update` do crate `zip` que só mude o formato
/// do arquivo não quebra o teste. O que não é ZIP (`corrompida.xlsx`) é
/// comparado byte a byte.
#[test]
fn fixtures_conferem_com_o_gerador() {
    const REGENERAR: &str = "rode `cargo test --test parser -- --ignored gerar_fixtures`";
    for (nome, esperado) in comum::todas_as_fixtures() {
        let gravado = std::fs::read(fixture(nome))
            .unwrap_or_else(|e| panic!("fixture {nome} ausente ({e}); {REGENERAR}"));
        match comum::partes_xlsx(&esperado) {
            Some(partes_esperadas) => {
                let partes_gravadas = comum::partes_xlsx(&gravado)
                    .unwrap_or_else(|| panic!("fixture {nome} não é um ZIP legível; {REGENERAR}"));
                let nomes = |p: &[(String, Vec<u8>)]| -> Vec<String> {
                    p.iter().map(|(n, _)| n.clone()).collect()
                };
                assert_eq!(
                    nomes(&partes_gravadas),
                    nomes(&partes_esperadas),
                    "fixture {nome}: partes diferentes do gerador; {REGENERAR}"
                );
                for ((parte, gravada), (_, esperada)) in
                    partes_gravadas.iter().zip(&partes_esperadas)
                {
                    assert!(
                        gravada == esperada,
                        "fixture {nome}: parte {parte} difere do gerador; {REGENERAR}"
                    );
                }
            }
            None => assert!(
                gravado == esperado,
                "fixture {nome} difere do gerador; {REGENERAR}"
            ),
        }
    }
}

/// Garante que a fixture tem os mesmos tipos de célula da planilha real:
/// se isto mudar, os testes abaixo deixam de cobrir o formato da Caixa.
#[test]
fn fixtures_usam_os_tipos_de_celula_da_planilha_real() {
    for nome in ["lotofacil.xlsx", "megasena.xlsx", "quina.xlsx"] {
        let mut planilha = open_workbook_auto(fixture(nome)).expect("abrir fixture");
        let intervalo = planilha
            .worksheet_range_at(0)
            .expect("primeira aba")
            .expect("ler aba");
        let primeira = intervalo.rows().nth(1).expect("linha do concurso 1");
        assert!(
            matches!(&primeira[0], Data::Float(f) if *f == 1.0),
            "{nome}: concurso {:?}",
            primeira[0]
        );
        assert!(
            matches!(&primeira[1], Data::String(s) if s.len() == 10 && s.as_bytes()[2] == b'/'),
            "{nome}: data deveria ser texto DD/MM/AAAA, veio {:?}",
            primeira[1]
        );
        assert!(
            matches!(&primeira[2], Data::Float(_)),
            "{nome}: dezena {:?}",
            primeira[2]
        );
    }
}

#[test]
fn lotofacil_primeiros_concursos() {
    let config = loteria("lotofacil");
    let resultados = parsear_arquivo(&fixture("lotofacil.xlsx"), config).expect("parsear");
    let s = "lotofacil";
    assert_eq!(
        resultados,
        vec![
            res(
                s,
                1,
                "2003-09-29",
                &[2, 3, 5, 6, 9, 10, 11, 13, 14, 16, 18, 20, 23, 24, 25]
            ),
            res(
                s,
                2,
                "2003-10-06",
                &[1, 4, 5, 6, 7, 9, 11, 12, 13, 15, 16, 19, 20, 23, 24]
            ),
            res(
                s,
                3,
                "2003-10-13",
                &[1, 4, 6, 7, 8, 9, 10, 11, 12, 14, 16, 17, 20, 23, 24]
            ),
            res(
                s,
                4,
                "2003-10-20",
                &[1, 2, 4, 5, 8, 10, 12, 13, 16, 17, 18, 19, 23, 24, 25]
            ),
            res(
                s,
                5,
                "2003-10-27",
                &[1, 2, 4, 8, 9, 11, 12, 13, 15, 16, 19, 20, 23, 24, 25]
            ),
        ]
    );
    assert!(resultados
        .iter()
        .all(|r| r.dezenas.len() == config.qtd_dezenas));
}

#[test]
fn megasena_primeiros_concursos() {
    let config = loteria("megasena");
    let resultados = parsear_arquivo(&fixture("megasena.xlsx"), config).expect("parsear");
    let s = "megasena";
    assert_eq!(
        resultados,
        vec![
            res(s, 1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(s, 2, "1996-03-18", &[9, 37, 39, 41, 43, 49]),
            res(s, 3, "1996-03-25", &[10, 11, 29, 30, 36, 47]),
            res(s, 4, "1996-04-01", &[1, 5, 6, 27, 42, 59]),
            res(s, 5, "1996-04-08", &[1, 2, 6, 16, 19, 46]),
        ]
    );
    assert!(resultados
        .iter()
        .all(|r| r.dezenas.len() == config.qtd_dezenas));
}

#[test]
fn quina_primeiros_concursos() {
    let config = loteria("quina");
    let resultados = parsear_arquivo(&fixture("quina.xlsx"), config).expect("parsear");
    let s = "quina";
    assert_eq!(
        resultados,
        vec![
            res(s, 1, "1994-03-13", &[25, 45, 60, 76, 79]),
            res(s, 2, "1994-03-17", &[13, 30, 58, 63, 64]),
            res(s, 3, "1994-03-20", &[5, 15, 32, 33, 48]),
            res(s, 4, "1994-03-24", &[27, 57, 60, 61, 77]),
            res(s, 5, "1994-03-27", &[19, 44, 53, 54, 71]),
        ]
    );
    assert!(resultados
        .iter()
        .all(|r| r.dezenas.len() == config.qtd_dezenas));
}

/// Linhas vazias, curtas, cabeçalho repetido, data fora do formato
/// `DD/MM/AAAA` (inclusive célula de data do Excel) e dezena faltando ou não
/// numérica são puladas sem derrubar a planilha.
#[test]
fn linhas_irreconheciveis_sao_ignoradas() {
    // Confere que a linha do concurso 92 chega mesmo como data do Excel, para
    // o teste não passar só porque a célula virou outra coisa.
    let mut planilha = open_workbook_auto(fixture("bordas_megasena.xlsx")).expect("abrir");
    let intervalo = planilha
        .worksheet_range_at(0)
        .expect("primeira aba")
        .expect("ler aba");
    let linha_92 = intervalo
        .rows()
        .find(|l| matches!(&l[0], Data::Float(f) if *f == 92.0))
        .expect("linha do concurso 92");
    assert!(
        matches!(&linha_92[1], Data::DateTime(_)),
        "{:?}",
        linha_92[1]
    );

    let resultados =
        parsear_arquivo(&fixture("bordas_megasena.xlsx"), loteria("megasena")).expect("parsear");
    let s = "megasena";
    assert_eq!(
        resultados,
        vec![
            res(s, 1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(s, 2, "1996-03-18", &[9, 37, 39, 41, 43, 49]),
        ]
    );
}

#[test]
fn arquivo_inexistente_da_erro_de_planilha() {
    let caminho = fixture("nao_existe.xlsx");
    let erro = parsear_arquivo(&caminho, loteria("megasena")).expect_err("deveria falhar");
    match erro {
        SpiderError::Planilha { caminho: c, .. } => assert_eq!(c, caminho),
        outro => panic!("esperava SpiderError::Planilha, veio {outro:?}"),
    }
}

#[test]
fn arquivo_corrompido_da_erro_de_planilha() {
    let caminho = fixture("corrompida.xlsx");
    let erro = parsear_arquivo(&caminho, loteria("quina")).expect_err("deveria falhar");
    match erro {
        SpiderError::Planilha { caminho: c, .. } => assert_eq!(c, caminho),
        outro => panic!("esperava SpiderError::Planilha, veio {outro:?}"),
    }
}

#[test]
fn planilha_sem_linhas_reconhecidas_da_tabela_nao_encontrada() {
    let caminho = fixture("sem_resultados.xlsx");
    let erro = parsear_arquivo(&caminho, loteria("megasena")).expect_err("deveria falhar");
    match erro {
        SpiderError::TabelaNaoEncontrada(c) => assert_eq!(c, caminho),
        outro => panic!("esperava SpiderError::TabelaNaoEncontrada, veio {outro:?}"),
    }
}

#[test]
fn planilha_sem_colunas_de_dezenas_da_tabela_nao_encontrada() {
    let caminho = fixture("estreita.xlsx");
    let erro = parsear_arquivo(&caminho, loteria("megasena")).expect_err("deveria falhar");
    match erro {
        SpiderError::TabelaNaoEncontrada(c) => assert_eq!(c, caminho),
        outro => panic!("esperava SpiderError::TabelaNaoEncontrada, veio {outro:?}"),
    }
}
