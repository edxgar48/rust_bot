//! Gerador das planilhas de `tests/fixtures/`.
//!
//! As fixtures reproduzem o XML que a Caixa publica (conferido nas planilhas
//! baixadas em 2026-10-02), e não o que um Excel "normal" gravaria, porque é
//! justamente esse formato peculiar que o parser precisa aguentar:
//!
//! - prefixo de namespace `x:` em `workbook.xml` e `sheet1.xml`;
//! - `<x:dimension>` mentindo (cobre só a linha de cabeçalho);
//! - só o cabeçalho tem `r="A1"` e shared strings; as linhas de dados não têm
//!   atributo `r` nem nas linhas nem nas células;
//! - concurso e dezenas são `t="n"` com zero à esquerda (`<x:v>02</x:v>`);
//! - a data é texto `t="str"` no formato `DD/MM/AAAA` — nunca célula de data
//!   do Excel.
//!
//! As linhas de dados são cópias fiéis dos primeiros concursos de cada
//! planilha real (todas as colunas). Para regenerar os arquivos:
//!
//! ```text
//! cargo test --test parser -- --ignored gerar_fixtures
//! ```
//!
//! O ZIP é gravado sem compressão e com data fixa para regenerar sem ruído
//! no git; o teste `fixtures_conferem_com_o_gerador` garante que os arquivos
//! versionados não se afastaram deste código. Ele compara as partes
//! descompactadas (nomes e XML), não o contêiner, para que uma atualização
//! do crate `zip` que mude só o formato do ZIP não quebre o teste.

// Cada arquivo de `tests/` compila este módulo à parte; quem usar só parte
// das funções não deve disparar `dead_code` no clippy.
#![allow(dead_code)]

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// Uma célula de linha de dados, com o tipo que a planilha da Caixa usa.
#[derive(Debug, Clone, Copy)]
pub enum Celula {
    /// Numérica (`t="n"`), com o texto exatamente como está no XML.
    N(&'static str),
    /// Texto inline (`t="str"`).
    S(&'static str),
    /// Data de verdade do Excel (número de série com estilo de data). A Caixa
    /// não usa; existe só para fixar o comportamento do parser nesse caso.
    DataExcel(u32),
}

use Celula::{DataExcel, N, S};

pub struct Planilha {
    pub nome_aba: &'static str,
    pub cabecalho: &'static [&'static str],
    pub linhas: Vec<&'static [Celula]>,
}

/// Diretório versionado das fixtures.
pub fn dir_fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Conteúdo de `corrompida.xlsx`: extensão de planilha, mas não é um ZIP.
/// Comparado byte a byte; o `.gitattributes` marca as fixtures como binárias.
pub const BYTES_CORROMPIDA: &[u8] = b"isto nao e uma planilha xlsx";

/// Todas as fixtures `.xlsx` geradas: (nome do arquivo, bytes).
pub fn todas_as_fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("lotofacil.xlsx", bytes_xlsx(&lotofacil())),
        ("megasena.xlsx", bytes_xlsx(&megasena())),
        ("quina.xlsx", bytes_xlsx(&quina())),
        ("bordas_megasena.xlsx", bytes_xlsx(&bordas_megasena())),
        ("sem_resultados.xlsx", bytes_xlsx(&sem_resultados())),
        ("estreita.xlsx", bytes_xlsx(&estreita())),
        ("corrompida.xlsx", BYTES_CORROMPIDA.to_vec()),
    ]
}

/// Partes de um `.xlsx` descompactadas, na ordem do contêiner: (nome, bytes).
/// `None` se os bytes não forem um ZIP legível.
pub fn partes_xlsx(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut partes = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let mut parte = zip.by_index(i).ok()?;
        let mut conteudo = Vec::new();
        parte.read_to_end(&mut conteudo).ok()?;
        partes.push((parte.name().to_string(), conteudo));
    }
    Some(partes)
}

// ---------------------------------------------------------------------------
// Conteúdo das planilhas
// ---------------------------------------------------------------------------

const CABECALHO_LOTOFACIL: &[&str] = &[
    "Concurso",
    "Data Sorteio",
    "Bola1",
    "Bola2",
    "Bola3",
    "Bola4",
    "Bola5",
    "Bola6",
    "Bola7",
    "Bola8",
    "Bola9",
    "Bola10",
    "Bola11",
    "Bola12",
    "Bola13",
    "Bola14",
    "Bola15",
    "Ganhadores 15 acertos",
    "Cidade / UF",
    "Rateio 15 acertos",
    "Ganhadores 14 acertos",
    "Rateio 14 acertos",
    "Ganhadores 13 acertos",
    "Rateio 13 acertos",
    "Ganhadores 12 acertos",
    "Rateio 12 acertos",
    "Ganhadores 11 acertos",
    "Rateio 11 acertos",
    "Acumulado 15 acertos",
    "Arrecadacao Total",
    "Estimativa Prêmio",
    "Acumulado sorteio especial Lotofácil da Independência",
    "Observação",
];

const CABECALHO_MEGASENA: &[&str] = &[
    "Concurso",
    "Data do Sorteio",
    "Bola1",
    "Bola2",
    "Bola3",
    "Bola4",
    "Bola5",
    "Bola6",
    "Ganhadores 6 acertos",
    "Cidade / UF",
    "Rateio 6 acertos",
    "Ganhadores 5 acertos",
    "Rateio 5 acertos",
    "Ganhadores 4 acertos",
    "Rateio 4 acertos",
    "Acumulado 6 acertos",
    "Arrecadação Total",
    "Estimativa prêmio",
    "Acumulado Sorteio Especial Mega da Virada",
    "Observação",
];

const CABECALHO_QUINA: &[&str] = &[
    "Concurso",
    "Data Sorteio",
    "Bola1",
    "Bola2",
    "Bola3",
    "Bola4",
    "Bola5",
    "Ganhadores 5 acertos",
    "Cidade / UF",
    "Rateio 5 acertos",
    "Ganhadores 4 acertos",
    "Rateio 4 acertos",
    "Ganhadores 3 acertos",
    "Rateio 3 acertos",
    "Ganhadores 2 acertos",
    "Rateio 2 acertos",
    "Acumulado 5 acertos",
    "Arrecadacao Total",
    "Estimativa Premio",
    "Acumulado Sorteio Especial Quina de São João",
    "observação",
];

/// Linhas copiadas das planilhas reais. Sem rustfmt para manter uma linha
/// de planilha por bloco compacto, em vez de uma célula por linha.
#[rustfmt::skip]
mod reais {
    use super::Celula::{self, N, S};

    pub const LOTOFACIL_1: &[Celula] = &[
        N("1"), S("29/09/2003"), N("02"), N("03"), N("05"), N("06"), N("09"), N("10"), N("11"),
        N("13"), N("14"), N("16"), N("18"), N("20"), N("23"), N("24"), N("25"), N("5"),
        S("BA; PR; SP"), S("R$49.765,82"), N("154"), S("R$689,84"), N("4645"), S("R$10,00"),
        N("48807"), S("R$4,00"), N("257593"), S("R$2,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"),
        S("R$0,00"), S("Estimativa de prêmio (15 ACERTOS) próximo concurso: R$250.000,00."),
    ];
    pub const LOTOFACIL_2: &[Celula] = &[
        N("2"), S("06/10/2003"), N("01"), N("04"), N("05"), N("06"), N("07"), N("09"), N("11"),
        N("12"), N("13"), N("15"), N("16"), N("19"), N("20"), N("23"), N("24"), N("1"), S("SP"),
        S("R$596.323,70"), N("184"), S("R$1.388,95"), N("6232"), S("R$10,00"), N("81252"),
        S("R$4,00"), N("478188"), S("R$2,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"),
        S("ESTIMATIVA DE PRÊMIO PARA O PRÓXIMO CONCURSO (15 ACERTOS): R$500.000,00"),
    ];
    pub const LOTOFACIL_3: &[Celula] = &[
        N("3"), S("13/10/2003"), N("01"), N("04"), N("06"), N("07"), N("08"), N("09"), N("10"),
        N("11"), N("12"), N("14"), N("16"), N("17"), N("20"), N("23"), N("24"), N("2"), S("SP"),
        S("R$400.623,70"), N("158"), S("R$2.173,36"), N("6897"), S("R$10,00"), N("96244"),
        S("R$4,00"), N("608211"), S("R$2,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"),
        S("Estimativa de prêmio (15 ACERTOS) próximo concurso: R$700.000,00."),
    ];
    pub const LOTOFACIL_4: &[Celula] = &[
        N("4"), S("20/10/2003"), N("01"), N("02"), N("04"), N("05"), N("08"), N("10"), N("12"),
        N("13"), N("16"), N("17"), N("18"), N("19"), N("23"), N("24"), N("25"), N("1"), S("MT"),
        S("R$902.226,02"), N("258"), S("R$1.498,72"), N("9569"), S("R$10,00"), N("123912"),
        S("R$4,00"), N("706657"), S("R$2,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"),
        S("Estimativa de prêmio (15 ACERTOS) próximo concurso: R$800.000,00."),
    ];
    pub const LOTOFACIL_5: &[Celula] = &[
        N("5"), S("27/10/2003"), N("01"), N("02"), N("04"), N("08"), N("09"), N("11"), N("12"),
        N("13"), N("15"), N("16"), N("19"), N("20"), N("23"), N("24"), N("25"), N("2"),
        S("BA; MS"), S("R$380.017,55"), N("472"), S("R$687,49"), N("14736"), S("R$10,00"),
        N("195636"), S("R$4,00"), N("860992"), S("R$2,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"),
        S("R$0,00"), S("Estimativa de prêmio para o próximo concurso (15 ACERTOS): R$800.000,00."),
    ];

    pub const MEGASENA_1: &[Celula] = &[
        N("1"), S("11/03/1996"), N("04"), N("05"), N("30"), N("33"), N("41"), N("52"), N("0"),
        S(""), S("R$0,00"), N("17"), S("R$39.158,92"), N("2016"), S("R$330,21"),
        S("R$1.714.650,23"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const MEGASENA_2: &[Celula] = &[
        N("2"), S("18/03/1996"), N("09"), N("37"), N("39"), N("41"), N("43"), N("49"), N("1"),
        S("PR"), S("R$2.307.162,23"), N("65"), S("R$14.424,02"), N("4488"), S("R$208,91"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const MEGASENA_3: &[Celula] = &[
        N("3"), S("25/03/1996"), N("10"), N("11"), N("29"), N("30"), N("36"), N("47"), N("2"),
        S("RN; SP"), S("R$391.192,51"), N("62"), S("R$10.515,93"), N("4261"), S("R$153,01"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const MEGASENA_4: &[Celula] = &[
        N("4"), S("01/04/1996"), N("01"), N("05"), N("06"), N("27"), N("42"), N("59"), N("0"),
        S(""), S("R$0,00"), N("39"), S("R$15.322,24"), N("3311"), S("R$180,48"),
        S("R$717.080,75"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const MEGASENA_5: &[Celula] = &[
        N("5"), S("08/04/1996"), N("01"), N("02"), N("06"), N("16"), N("19"), N("46"), N("0"),
        S(""), S("R$0,00"), N("98"), S("R$5.318,10"), N("5399"), S("R$96,53"),
        S("R$1.342.488,85"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];

    pub const QUINA_1: &[Celula] = &[
        N("1"), S("13/03/1994"), N("25"), N("45"), N("60"), N("76"), N("79"), N("3"), S(""),
        S("R$75.731.225,00"), N("127"), S("R$1.788.927,00"), N("7030"), S("R$42.982,00"), N("0"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const QUINA_2: &[Celula] = &[
        N("2"), S("17/03/1994"), N("13"), N("30"), N("58"), N("63"), N("64"), N("1"), S(""),
        S("R$118.499.397,00"), N("105"), S("R$1.128.565,00"), N("4861"), S("R$32.422,00"), N("0"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const QUINA_3: &[Celula] = &[
        N("3"), S("20/03/1994"), N("05"), N("15"), N("32"), N("33"), N("48"), N("1"), S(""),
        S("R$154.244.270,00"), N("232"), S("R$664.846,00"), N("10196"), S("R$20.120,00"), N("0"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const QUINA_4: &[Celula] = &[
        N("4"), S("24/03/1994"), N("27"), N("57"), N("60"), N("61"), N("77"), N("1"), S(""),
        S("R$124.185.246,00"), N("78"), S("R$1.592.119,00"), N("4033"), S("R$40.954,00"), N("0"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
    pub const QUINA_5: &[Celula] = &[
        N("5"), S("27/03/1994"), N("19"), N("44"), N("53"), N("54"), N("71"), N("1"), S(""),
        S("R$147.595.715,00"), N("137"), S("R$1.077.341,00"), N("5338"), S("R$36.774,00"), N("0"),
        S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S("R$0,00"), S(""),
    ];
}

use reais::*;

pub fn lotofacil() -> Planilha {
    Planilha {
        nome_aba: "LOTOFÁCIL",
        cabecalho: CABECALHO_LOTOFACIL,
        linhas: vec![
            LOTOFACIL_1,
            LOTOFACIL_2,
            LOTOFACIL_3,
            LOTOFACIL_4,
            LOTOFACIL_5,
        ],
    }
}

pub fn megasena() -> Planilha {
    Planilha {
        nome_aba: "MEGA SENA",
        cabecalho: CABECALHO_MEGASENA,
        linhas: vec![MEGASENA_1, MEGASENA_2, MEGASENA_3, MEGASENA_4, MEGASENA_5],
    }
}

pub fn quina() -> Planilha {
    Planilha {
        nome_aba: "QUINA",
        cabecalho: CABECALHO_QUINA,
        linhas: vec![QUINA_1, QUINA_2, QUINA_3, QUINA_4, QUINA_5],
    }
}

/// Layout da Mega-Sena com linhas que o parser deve ignorar entre dois
/// concursos válidos (1 e 2). Cada linha ruim falha por um motivo só.
pub fn bordas_megasena() -> Planilha {
    Planilha {
        nome_aba: "MEGA SENA",
        cabecalho: CABECALHO_MEGASENA,
        linhas: vec![
            MEGASENA_1,
            // Linha sem nenhuma célula.
            &[],
            // Linha só com textos vazios.
            &[S(""), S(""), S(""), S(""), S(""), S(""), S(""), S("")],
            // Linha curta: concurso e data, sem dezenas.
            &[N("90"), S("01/01/2000")],
            // Cabeçalho repetido no meio da planilha.
            &[
                S("Concurso"),
                S("Data do Sorteio"),
                S("Bola1"),
                S("Bola2"),
                S("Bola3"),
                S("Bola4"),
                S("Bola5"),
                S("Bola6"),
            ],
            // Data já em ISO (a planilha usa DD/MM/AAAA).
            &[
                N("91"),
                S("2000-01-01"),
                N("01"),
                N("02"),
                N("03"),
                N("04"),
                N("05"),
                N("06"),
            ],
            // Data como célula de data do Excel (série de 2000-01-01).
            &[
                N("92"),
                DataExcel(36526),
                N("01"),
                N("02"),
                N("03"),
                N("04"),
                N("05"),
                N("06"),
            ],
            // Falta a última dezena.
            &[
                N("93"),
                S("01/01/2000"),
                N("01"),
                N("02"),
                N("03"),
                N("04"),
                N("05"),
                S(""),
            ],
            // Dezena com texto não numérico.
            &[
                N("94"),
                S("01/01/2000"),
                N("01"),
                N("02"),
                N("03"),
                S("x"),
                N("05"),
                N("06"),
            ],
            MEGASENA_2,
        ],
    }
}

/// Layout da Mega-Sena com várias linhas, nenhuma reconhecível.
pub fn sem_resultados() -> Planilha {
    Planilha {
        nome_aba: "MEGA SENA",
        cabecalho: CABECALHO_MEGASENA,
        linhas: vec![
            &[S("Total"), S(""), S(""), S(""), S(""), S(""), S(""), S("")],
            &[
                N("1"),
                S("sem data"),
                N("04"),
                N("05"),
                N("30"),
                N("33"),
                N("41"),
                N("52"),
            ],
        ],
    }
}

/// Planilha com só duas colunas: nenhuma linha alcança `primeira_coluna_dezena`.
pub fn estreita() -> Planilha {
    Planilha {
        nome_aba: "Plan1",
        cabecalho: &["Concurso", "Data Sorteio"],
        linhas: vec![&[N("1"), S("11/03/1996")], &[N("2"), S("18/03/1996")]],
    }
}

// ---------------------------------------------------------------------------
// Escrita do .xlsx
// ---------------------------------------------------------------------------

/// Monta o `.xlsx` em memória, com bytes determinísticos.
pub fn bytes_xlsx(planilha: &Planilha) -> Vec<u8> {
    let opcoes = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default());
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let partes = [
        ("[Content_Types].xml", CONTENT_TYPES.to_string()),
        ("_rels/.rels", RELS.to_string()),
        ("xl/workbook.xml", workbook_xml(planilha.nome_aba)),
        ("xl/_rels/workbook.xml.rels", WORKBOOK_RELS.to_string()),
        ("xl/styles.xml", STYLES.to_string()),
        (
            "xl/sharedStrings.xml",
            shared_strings_xml(planilha.cabecalho),
        ),
        ("xl/worksheets/sheet1.xml", sheet_xml(planilha)),
    ];
    for (nome, conteudo) in partes {
        zip.start_file(nome, opcoes).expect("iniciar parte do zip");
        zip.write_all(conteudo.as_bytes())
            .expect("gravar parte do zip");
    }
    zip.finish().expect("finalizar zip").into_inner()
}

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#;

/// Estilos 0 e 1 como na planilha real (normal e cabeçalho); o 2 é um
/// estilo de data (`numFmtId="14"`), usado só por `Celula::DataExcel`.
const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="10"/><name val="Arial"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#;

const NS_PLANILHA: &str = r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:x="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

fn workbook_xml(nome_aba: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?><x:workbook {NS_PLANILHA}><x:sheets><x:sheet name="{}" sheetId="1" r:id="rId1" /></x:sheets></x:workbook>"#,
        escapar_xml(nome_aba)
    )
}

fn shared_strings_xml(cabecalho: &[&str]) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{n}" uniqueCount="{n}">"#,
        n = cabecalho.len()
    );
    for texto in cabecalho {
        xml.push_str(&format!("<si><t>{}</t></si>", escapar_xml(texto)));
    }
    xml.push_str("</sst>");
    xml
}

fn sheet_xml(planilha: &Planilha) -> String {
    let ultima_coluna = letra_coluna(planilha.cabecalho.len() - 1);
    // Como na planilha da Caixa: a dimensão declara só a linha 1.
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?><x:worksheet {NS_PLANILHA}><x:dimension ref="A1:{ultima_coluna}1" /><x:sheetData><x:row r="1" spans="1:{}">"#,
        planilha.cabecalho.len()
    );
    for i in 0..planilha.cabecalho.len() {
        xml.push_str(&format!(
            r#"<x:c r="{}1" s="1" t="s"><x:v>{i}</x:v></x:c>"#,
            letra_coluna(i)
        ));
    }
    xml.push_str("</x:row>");
    for linha in &planilha.linhas {
        if linha.is_empty() {
            xml.push_str("<x:row />");
            continue;
        }
        xml.push_str("<x:row>");
        for celula in linha.iter() {
            match celula {
                N(v) => xml.push_str(&format!(r#"<x:c t="n"><x:v>{v}</x:v></x:c>"#)),
                S(v) => xml.push_str(&format!(
                    r#"<x:c t="str"><x:v>{}</x:v></x:c>"#,
                    escapar_xml(v)
                )),
                DataExcel(serie) => {
                    xml.push_str(&format!(r#"<x:c s="2" t="n"><x:v>{serie}</x:v></x:c>"#))
                }
            }
        }
        xml.push_str("</x:row>");
    }
    xml.push_str("</x:sheetData></x:worksheet>");
    xml
}

/// Índice de coluna a partir de 0 → letra(s) do Excel (`0` → `A`, `26` → `AA`).
fn letra_coluna(indice: usize) -> String {
    let mut n = indice + 1;
    let mut letras = Vec::new();
    while n > 0 {
        let resto = (n - 1) % 26;
        letras.push(b'A' + resto as u8);
        n = (n - 1) / 26;
    }
    letras.reverse();
    String::from_utf8(letras).expect("letras ASCII")
}

fn escapar_xml(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
