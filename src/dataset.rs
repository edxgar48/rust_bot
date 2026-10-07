//! Dataset de ML para o Python (`sync dataset`, 1.5.1).
//!
//! Monta, sem banco, os bytes do arquivo de dados (CSV de máquina ou Parquet)
//! e do manifesto a partir de um [`Dataset`] já conferido pela orquestração.
//! Regras (`docs/PLANEJAMENTO.md`, Fase 1.5, decisões de 2026-10-06):
//!
//! - formato longo: uma linha por (concurso, dezena, janela), na ordem da
//!   chave de `features_dezena`, com as colunas de [`COLUNAS`];
//! - nada é recalculado: os componentes são o snapshot de antes do concurso
//!   gravado pelo `sync features`, e `saiu` é o rótulo do próprio concurso;
//! - mesmos dados → mesmos bytes, nos dois formatos e no manifesto (sem
//!   carimbo de tempo; metadados do Parquet fixos).

use crate::config::Trecho;
use crate::error::{Result, SpiderError};
use crate::motor::FeaturesDezena;
use parquet::basic::Compression;
use parquet::data_type::{ByteArray, ByteArrayType, Int32Type};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::{WriterProperties, WriterVersion};
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::Arc;

/// Colunas do dataset, na ordem dos dois formatos. Os componentes têm os
/// mesmos nomes de `features_dezena`.
pub const COLUNAS: [&str; 16] = [
    "concurso",
    "data",
    "dezena",
    "janela",
    "trecho",
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
    "saiu",
    "feature_version",
];

/// Esquema Parquet: inteiros com sinal de 32 bits (os valores cabem com
/// folga e o pandas/numpy lida melhor com sinal do que com `uint32`), textos
/// em UTF-8, `OPTIONAL` só nos componentes que podem ser `NULL`. `saiu` é
/// 0/1, como em `features_dezena` e no CSV.
const ESQUEMA_PARQUET: &str = "message features_dezena {
    REQUIRED INT32 concurso (INTEGER(32, true));
    REQUIRED BYTE_ARRAY data (STRING);
    REQUIRED INT32 dezena (INTEGER(32, true));
    REQUIRED INT32 janela (INTEGER(32, true));
    REQUIRED BYTE_ARRAY trecho (STRING);
    REQUIRED INT32 frequencia (INTEGER(32, true));
    OPTIONAL INT32 maior_espaco (INTEGER(32, true));
    OPTIONAL INT32 maior_espaco_ocorrencias (INTEGER(32, true));
    OPTIONAL INT32 menor_espaco (INTEGER(32, true));
    OPTIONAL INT32 menor_espaco_ocorrencias (INTEGER(32, true));
    OPTIONAL INT32 soma_variacoes (INTEGER(32, true));
    OPTIONAL INT32 espaco_final (INTEGER(32, true));
    REQUIRED INT32 repeticoes_total (INTEGER(32, true));
    REQUIRED INT32 repeticao_atual (INTEGER(32, true));
    REQUIRED INT32 saiu (INTEGER(32, true));
    REQUIRED INT32 feature_version (INTEGER(32, true));
}";

/// `created_by` gravado no rodapé do Parquet. O padrão do crate traz a
/// versão dele (`parquet-rs version X`); fixo aqui, o arquivo não muda só
/// porque o crate foi atualizado. (A codificação das páginas ainda depende
/// da versão do crate travada no `Cargo.lock`.)
pub const CRIADO_POR: &str = "rust_spider sync dataset";

/// Linhas por row group do Parquet: limita a memória das colunas montadas de
/// uma vez (a Quina passa de 2 milhões de linhas) sem picotar demais o
/// arquivo. Fixo, para o arquivo ser determinístico.
pub const LINHAS_POR_GRUPO: usize = 1 << 20;

/// Formato do arquivo de dados.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formato {
    Parquet,
    Csv,
}

impl Formato {
    /// Extensão do arquivo (`parquet`/`csv`), também usada no manifesto.
    pub const fn extensao(self) -> &'static str {
        match self {
            Formato::Parquet => "parquet",
            Formato::Csv => "csv",
        }
    }
}

/// Data e trecho de um concurso do dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcursoDataset {
    pub concurso: u32,
    /// Data do sorteio, `AAAA-MM-DD`.
    pub data: String,
    pub trecho: Trecho,
}

/// Impressões de `resultados` (`orquestracao::impressao_resultados`) que o
/// manifesto leva, para o Python conferir o que leu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpressoesResultados {
    /// Do banco inteiro: a mesma de `motor_estado`; muda a cada sorteio.
    pub banco: String,
    /// Só dos concursos 1 … último incluído no arquivo: não muda quando
    /// entra um concurso acima do que o arquivo traz.
    pub incluidos: String,
}

/// Dados do dataset já conferidos (ver [`Dataset::novo`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    loteria: String,
    feature_version: u32,
    ultimo_concurso_busca: u32,
    com_trancado: bool,
    impressoes: ImpressoesResultados,
    concursos: Vec<ConcursoDataset>,
    linhas: Vec<FeaturesDezena>,
}

impl Dataset {
    /// Junta as partes conferindo o que a montagem dos arquivos pressupõe:
    /// pelo menos um concurso; concursos em ordem estritamente crescente;
    /// linhas em ordem estritamente crescente de (concurso, dezena, janela),
    /// cada uma de um concurso da lista, com `saiu` preenchido (só concursos
    /// sorteados); todo concurso com linhas; e, sem `com_trancado`, nenhum
    /// concurso rotulado como trancado nem acima de `ultimo_concurso_busca`
    /// (as duas checagens: o rótulo vem de fora e pode estar errado). Qualquer
    /// falha é [`SpiderError::DatasetIncoerente`].
    pub fn novo(
        loteria: String,
        feature_version: u32,
        ultimo_concurso_busca: u32,
        com_trancado: bool,
        impressoes: ImpressoesResultados,
        concursos: Vec<ConcursoDataset>,
        linhas: Vec<FeaturesDezena>,
    ) -> Result<Self> {
        let incoerente = |d: String| Err(SpiderError::DatasetIncoerente(d));
        if concursos.is_empty() {
            return incoerente("nenhum concurso".to_string());
        }
        if let Some(par) = concursos
            .windows(2)
            .find(|p| p[0].concurso >= p[1].concurso)
        {
            return incoerente(format!(
                "concursos fora de ordem: {} antes de {}",
                par[0].concurso, par[1].concurso
            ));
        }
        if !com_trancado {
            if let Some(c) = concursos.iter().find(|c| c.trecho != Trecho::Busca) {
                return incoerente(format!(
                    "concurso {} do trecho {} sem `--trancado`",
                    c.concurso,
                    c.trecho.nome()
                ));
            }
            if let Some(c) = concursos
                .iter()
                .find(|c| c.concurso > ultimo_concurso_busca)
            {
                return incoerente(format!(
                    "concurso {} acima do corte da busca ({ultimo_concurso_busca}) sem `--trancado`",
                    c.concurso
                ));
            }
        }
        let chave = |f: &FeaturesDezena| (f.concurso, f.dezena, f.janela);
        if let Some(par) = linhas.windows(2).find(|p| chave(&p[0]) >= chave(&p[1])) {
            return incoerente(format!(
                "linhas fora de ordem: {:?} antes de {:?}",
                chave(&par[0]),
                chave(&par[1])
            ));
        }
        if let Some(f) = linhas.iter().find(|f| f.saiu.is_none()) {
            return incoerente(format!(
                "concurso {} sem `saiu` (ainda não sorteado)",
                f.concurso
            ));
        }
        // Com as duas listas ordenadas, os concursos distintos das linhas têm
        // de ser exatamente os da lista de concursos.
        let mut distintos: Vec<u32> = linhas.iter().map(|f| f.concurso).collect();
        distintos.dedup();
        if !distintos
            .iter()
            .copied()
            .eq(concursos.iter().map(|c| c.concurso))
        {
            return incoerente(format!(
                "{} concurso(s) com linhas, {} concurso(s) na lista",
                distintos.len(),
                concursos.len()
            ));
        }
        Ok(Self {
            loteria,
            feature_version,
            ultimo_concurso_busca,
            com_trancado,
            impressoes,
            concursos,
            linhas,
        })
    }

    pub fn loteria(&self) -> &str {
        &self.loteria
    }

    pub fn feature_version(&self) -> u32 {
        self.feature_version
    }

    pub fn com_trancado(&self) -> bool {
        self.com_trancado
    }

    pub fn concursos(&self) -> &[ConcursoDataset] {
        &self.concursos
    }

    pub fn linhas(&self) -> &[FeaturesDezena] {
        &self.linhas
    }

    /// Primeiro concurso incluído.
    pub fn primeiro_concurso(&self) -> u32 {
        // `novo` garante pelo menos um concurso.
        self.concursos.first().map_or(0, |c| c.concurso)
    }

    /// Último concurso incluído.
    pub fn ultimo_concurso(&self) -> u32 {
        self.concursos.last().map_or(0, |c| c.concurso)
    }

    /// Trecho incluído, como sai no manifesto.
    pub fn trecho_incluido(&self) -> &'static str {
        if self.com_trancado {
            "busca+trancado"
        } else {
            "busca"
        }
    }

    /// Cada linha com a data e o trecho do seu concurso. As duas listas
    /// estão ordenadas e todo concurso das linhas está na lista (`novo`),
    /// então basta avançar um cursor.
    fn linhas_com_concurso(&self) -> impl Iterator<Item = (&ConcursoDataset, &FeaturesDezena)> {
        let mut i = 0;
        self.linhas.iter().map(move |f| {
            while self.concursos[i].concurso != f.concurso {
                i += 1;
            }
            (&self.concursos[i], f)
        })
    }
}

/// Nome do arquivo de dados: `features_v<feature_version>.<ext>`, ou
/// `features_v<feature_version>_trancado.<ext>` com o trancado. Nomes
/// distintos: uma execução com `--trancado` nunca sobrescreve o arquivo da
/// busca, e a busca de pesos não lê o trancado por engano.
pub fn nome_arquivo(dataset: &Dataset, formato: Formato) -> String {
    format!(
        "features_v{}{}.{}",
        dataset.feature_version,
        if dataset.com_trancado {
            "_trancado"
        } else {
            ""
        },
        formato.extensao()
    )
}

/// Nome do manifesto: `manifesto_<ext>.json`, ou
/// `manifesto_trancado_<ext>.json` com o trancado. Um por modo e formato,
/// para os arquivos poderem morar na mesma pasta sem um manifesto descrever
/// o arquivo de outro modo ou formato.
pub fn nome_manifesto(dataset: &Dataset, formato: Formato) -> String {
    format!(
        "manifesto_{}{}.json",
        if dataset.com_trancado {
            "trancado_"
        } else {
            ""
        },
        formato.extensao()
    )
}

/// Bytes do arquivo de dados no formato pedido.
pub fn montar(dataset: &Dataset, formato: Formato) -> Result<Vec<u8>> {
    match formato {
        Formato::Csv => Ok(formatar_csv(dataset).into_bytes()),
        Formato::Parquet => montar_parquet(dataset),
    }
}

/// CSV de máquina: separador `,`, cabeçalho, campo vazio para `NULL`, fim de
/// linha `\n`, UTF-8 sem BOM. Só há inteiros e textos sem vírgula (datas
/// ISO, `busca`/`trancado`), então nenhum campo precisa de aspas nem de
/// formatação decimal.
pub fn formatar_csv(dataset: &Dataset) -> String {
    let opcional = |v: Option<u32>| v.map_or_else(String::new, |v| v.to_string());
    // ~45 bytes por linha nos dados reais: evita realocações em milhões de linhas.
    let mut saida = String::with_capacity(64 * (dataset.linhas.len() + 1));
    saida.push_str(&COLUNAS.join(","));
    saida.push('\n');
    for (c, f) in dataset.linhas_com_concurso() {
        // `write!` num `String` não falha.
        let _ = writeln!(
            saida,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            f.concurso,
            c.data,
            f.dezena,
            f.janela,
            c.trecho.nome(),
            f.frequencia,
            opcional(f.maior_espaco),
            opcional(f.maior_espaco_ocorrencias),
            opcional(f.menor_espaco),
            opcional(f.menor_espaco_ocorrencias),
            opcional(f.soma_variacoes),
            opcional(f.espaco_final),
            f.repeticoes_total,
            f.repeticao_atual,
            u8::from(f.saiu == Some(true)),
            dataset.feature_version,
        );
    }
    saida
}

/// Propriedades fixas do escritor: Parquet 1.0, sem compressão, `created_by`
/// fixo e metadados chave-valor só com o que vem do dataset. O resto fica no
/// padrão do crate (dicionário e estatísticas por página), que é
/// determinístico: o dicionário guarda os valores na ordem de chegada.
fn propriedades(dataset: &Dataset) -> WriterProperties {
    let kv = |k: &str, v: String| KeyValue::new(k.to_string(), v);
    WriterProperties::builder()
        .set_writer_version(WriterVersion::PARQUET_1_0)
        .set_compression(Compression::UNCOMPRESSED)
        .set_created_by(CRIADO_POR.to_string())
        .set_key_value_metadata(Some(vec![
            kv("loteria", dataset.loteria.clone()),
            kv("feature_version", dataset.feature_version.to_string()),
            kv("trecho", dataset.trecho_incluido().to_string()),
        ]))
        .build()
}

/// Parquet pela API de baixo nível do crate `parquet` (sem arrow): um row
/// group a cada [`LINHAS_POR_GRUPO`] linhas, colunas na ordem de [`COLUNAS`].
pub fn montar_parquet(dataset: &Dataset) -> Result<Vec<u8>> {
    montar_parquet_em_grupos(dataset, LINHAS_POR_GRUPO)
}

fn montar_parquet_em_grupos(dataset: &Dataset, linhas_por_grupo: usize) -> Result<Vec<u8>> {
    let esquema = Arc::new(parse_message_type(ESQUEMA_PARQUET)?);
    let mut escritor =
        SerializedFileWriter::new(Vec::new(), esquema, Arc::new(propriedades(dataset)))?;
    let pares: Vec<(&ConcursoDataset, &FeaturesDezena)> = dataset.linhas_com_concurso().collect();
    for grupo in pares.chunks(linhas_por_grupo.max(1)) {
        let colunas = ColunasGrupo::montar(grupo, dataset.feature_version)?;
        let mut rg = escritor.next_row_group()?;
        let mut indice = 0;
        while let Some(mut coluna) = rg.next_column()? {
            match colunas.valores(indice) {
                ValoresColuna::Inteiros(valores, definicao) => {
                    coluna
                        .typed::<Int32Type>()
                        .write_batch(valores, definicao, None)?;
                }
                ValoresColuna::Textos(valores) => {
                    coluna
                        .typed::<ByteArrayType>()
                        .write_batch(valores, None, None)?;
                }
            }
            coluna.close()?;
            indice += 1;
        }
        rg.close()?;
    }
    Ok(escritor.into_inner()?)
}

/// Valores de uma coluna de um row group, prontos para o `write_batch`.
enum ValoresColuna<'a> {
    /// Valores presentes e, nas colunas `OPTIONAL`, o nível de definição de
    /// cada linha (1 = presente, 0 = `NULL`).
    Inteiros(&'a [i32], Option<&'a [i16]>),
    Textos(&'a [ByteArray]),
}

/// Uma coluna opcional: só os valores presentes, mais a definição por linha.
#[derive(Default)]
struct Opcional {
    valores: Vec<i32>,
    definicao: Vec<i16>,
}

impl Opcional {
    fn empurrar(&mut self, v: Option<u32>) -> Result<()> {
        match v {
            Some(v) => {
                self.valores.push(inteiro(v)?);
                self.definicao.push(1);
            }
            None => self.definicao.push(0),
        }
        Ok(())
    }
}

/// As 16 colunas de um row group.
#[derive(Default)]
struct ColunasGrupo {
    concurso: Vec<i32>,
    data: Vec<ByteArray>,
    dezena: Vec<i32>,
    janela: Vec<i32>,
    trecho: Vec<ByteArray>,
    frequencia: Vec<i32>,
    opcionais: [Opcional; 6],
    repeticoes_total: Vec<i32>,
    repeticao_atual: Vec<i32>,
    saiu: Vec<i32>,
    feature_version: Vec<i32>,
}

impl ColunasGrupo {
    fn montar(grupo: &[(&ConcursoDataset, &FeaturesDezena)], feature_version: u32) -> Result<Self> {
        let mut c = ColunasGrupo::default();
        let versao = inteiro(feature_version)?;
        for (concurso, f) in grupo {
            c.concurso.push(inteiro(f.concurso)?);
            c.data
                .push(ByteArray::from(concurso.data.as_bytes().to_vec()));
            c.dezena.push(inteiro(f.dezena)?);
            c.janela.push(inteiro(f.janela)?);
            c.trecho
                .push(ByteArray::from(concurso.trecho.nome().as_bytes().to_vec()));
            c.frequencia.push(inteiro(f.frequencia)?);
            let opcionais = [
                f.maior_espaco,
                f.maior_espaco_ocorrencias,
                f.menor_espaco,
                f.menor_espaco_ocorrencias,
                f.soma_variacoes,
                f.espaco_final,
            ];
            for (coluna, v) in c.opcionais.iter_mut().zip(opcionais) {
                coluna.empurrar(v)?;
            }
            c.repeticoes_total.push(inteiro(f.repeticoes_total)?);
            c.repeticao_atual.push(inteiro(f.repeticao_atual)?);
            c.saiu.push(i32::from(f.saiu == Some(true)));
            c.feature_version.push(versao);
        }
        Ok(c)
    }

    /// Valores da coluna `indice` de [`COLUNAS`].
    fn valores(&self, indice: usize) -> ValoresColuna<'_> {
        fn obrigatoria(v: &[i32]) -> ValoresColuna<'_> {
            ValoresColuna::Inteiros(v, None)
        }
        match indice {
            0 => obrigatoria(&self.concurso),
            1 => ValoresColuna::Textos(&self.data),
            2 => obrigatoria(&self.dezena),
            3 => obrigatoria(&self.janela),
            4 => ValoresColuna::Textos(&self.trecho),
            5 => obrigatoria(&self.frequencia),
            6..=11 => {
                let o = &self.opcionais[indice - 6];
                ValoresColuna::Inteiros(&o.valores, Some(&o.definicao))
            }
            12 => obrigatoria(&self.repeticoes_total),
            13 => obrigatoria(&self.repeticao_atual),
            14 => obrigatoria(&self.saiu),
            // 15: o esquema tem exatamente as 16 colunas de `COLUNAS`.
            _ => obrigatoria(&self.feature_version),
        }
    }
}

fn inteiro(v: u32) -> Result<i32> {
    i32::try_from(v).map_err(|_| SpiderError::ValorForaDoInt32(v))
}

/// SHA-256, em hex minúsculo.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Conteúdo do manifesto. A ordem dos campos é a do JSON gravado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Manifesto {
    pub loteria: String,
    pub feature_version: u32,
    pub formato: &'static str,
    /// Nome do arquivo de dados, na mesma pasta do manifesto.
    pub arquivo: String,
    pub colunas: Vec<&'static str>,
    pub primeiro_concurso: u32,
    /// Último concurso incluído.
    pub ultimo_concurso: u32,
    /// Corte fixo da busca (`ultimo_concurso_busca` da loteria).
    pub ultimo_concurso_busca: u32,
    /// `busca` ou `busca+trancado`.
    pub trecho: &'static str,
    pub concursos: usize,
    pub linhas: usize,
    /// SHA-256 (hex minúsculo) do arquivo de dados.
    pub sha256: String,
    /// Impressão de `resultados` do banco inteiro, a mesma de `motor_estado`
    /// (muda a cada sorteio, mesmo os que ficam fora do arquivo).
    pub impressao_resultados_banco: String,
    /// Impressão de `resultados` só dos concursos 1 … `ultimo_concurso`
    /// (os incluídos no arquivo).
    pub impressao_resultados_incluidos: String,
}

/// Manifesto do arquivo de dados `bytes` (já montado no `formato`).
pub fn manifesto(dataset: &Dataset, formato: Formato, bytes: &[u8]) -> Manifesto {
    Manifesto {
        loteria: dataset.loteria.clone(),
        feature_version: dataset.feature_version,
        formato: formato.extensao(),
        arquivo: nome_arquivo(dataset, formato),
        colunas: COLUNAS.to_vec(),
        primeiro_concurso: dataset.primeiro_concurso(),
        ultimo_concurso: dataset.ultimo_concurso(),
        ultimo_concurso_busca: dataset.ultimo_concurso_busca,
        trecho: dataset.trecho_incluido(),
        concursos: dataset.concursos.len(),
        linhas: dataset.linhas.len(),
        sha256: sha256_hex(bytes),
        impressao_resultados_banco: dataset.impressoes.banco.clone(),
        impressao_resultados_incluidos: dataset.impressoes.incluidos.clone(),
    }
}

/// JSON do manifesto: indentado, chaves na ordem de [`Manifesto`], `\n` no
/// fim. Sem data de geração: mesmos dados → mesmos bytes.
pub fn formatar_manifesto(manifesto: &Manifesto) -> Result<String> {
    let mut texto = serde_json::to_string_pretty(manifesto).map_err(SpiderError::ManifestoJson)?;
    texto.push('\n');
    Ok(texto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use parquet::record::Field;

    fn concurso(concurso: u32, data: &str, trecho: Trecho) -> ConcursoDataset {
        ConcursoDataset {
            concurso,
            data: data.to_string(),
            trecho,
        }
    }

    fn linha(concurso: u32, dezena: u32, janela: u32, saiu: Option<bool>) -> FeaturesDezena {
        FeaturesDezena {
            concurso,
            dezena,
            janela,
            frequencia: concurso + dezena,
            maior_espaco: (concurso > 1).then_some(3),
            maior_espaco_ocorrencias: (concurso > 1).then_some(1),
            menor_espaco: Some(0),
            menor_espaco_ocorrencias: None,
            soma_variacoes: (dezena == 2).then_some(7),
            espaco_final: Some(janela),
            repeticoes_total: 2,
            repeticao_atual: 1,
            saiu,
        }
    }

    fn impressoes() -> ImpressoesResultados {
        ImpressoesResultados {
            banco: "fnv1a64:3:0123456789abcdef".to_string(),
            incluidos: "fnv1a64:2:fedcba9876543210".to_string(),
        }
    }

    /// Concursos 1 (busca) e 2 (trancado, se `corte` = 1), dezenas 1 e 2,
    /// janelas 0 e 10.
    fn exemplo(com_trancado: bool) -> Dataset {
        let trecho2 = if com_trancado {
            Trecho::Trancado
        } else {
            Trecho::Busca
        };
        let mut linhas = Vec::new();
        for c in 1..=2 {
            for d in 1..=2 {
                for j in [0, 10] {
                    linhas.push(linha(c, d, j, Some(d == c)));
                }
            }
        }
        Dataset::novo(
            "quina".to_string(),
            1,
            if com_trancado { 1 } else { 2 },
            com_trancado,
            impressoes(),
            vec![
                concurso(1, "2020-01-02", Trecho::Busca),
                concurso(2, "2020-01-05", trecho2),
            ],
            linhas,
        )
        .expect("dataset")
    }

    // -----------------------------------------------------------------------
    // CSV.
    // -----------------------------------------------------------------------

    #[test]
    fn csv_confere_byte_a_byte() {
        let esperado = "concurso,data,dezena,janela,trecho,frequencia,maior_espaco,\
maior_espaco_ocorrencias,menor_espaco,menor_espaco_ocorrencias,soma_variacoes,espaco_final,\
repeticoes_total,repeticao_atual,saiu,feature_version\n\
1,2020-01-02,1,0,busca,2,,,0,,,0,2,1,1,1\n\
1,2020-01-02,1,10,busca,2,,,0,,,10,2,1,1,1\n\
1,2020-01-02,2,0,busca,3,,,0,,7,0,2,1,0,1\n\
1,2020-01-02,2,10,busca,3,,,0,,7,10,2,1,0,1\n\
2,2020-01-05,1,0,trancado,3,3,1,0,,,0,2,1,0,1\n\
2,2020-01-05,1,10,trancado,3,3,1,0,,,10,2,1,0,1\n\
2,2020-01-05,2,0,trancado,4,3,1,0,,7,0,2,1,1,1\n\
2,2020-01-05,2,10,trancado,4,3,1,0,,7,10,2,1,1,1\n";
        assert_eq!(formatar_csv(&exemplo(true)), esperado);
    }

    #[test]
    fn csv_tem_cabecalho_lf_sem_bom_e_16_colunas_em_toda_linha() {
        let csv = formatar_csv(&exemplo(false));
        assert!(!csv.starts_with('\u{feff}'));
        assert!(!csv.contains('\r'));
        assert!(csv.ends_with('\n'));
        let linhas: Vec<&str> = csv.lines().collect();
        assert_eq!(linhas[0], COLUNAS.join(","));
        assert_eq!(linhas.len(), 9);
        for l in &linhas {
            assert_eq!(l.split(',').count(), COLUNAS.len(), "{l}");
            assert!(!l.contains(';') && !l.contains('"'), "{l}");
        }
        assert!(linhas[1..].iter().all(|l| l.contains(",busca,")));
    }

    #[test]
    fn csv_null_e_campo_vazio_e_zero_e_zero() {
        let csv = formatar_csv(&exemplo(false));
        let campos: Vec<&str> = csv.lines().nth(1).expect("linha").split(',').collect();
        // maior_espaco NULL (concurso 1), menor_espaco 0, menor_espaco_ocorrencias NULL.
        assert_eq!((campos[6], campos[8], campos[9]), ("", "0", ""));
    }

    #[test]
    fn mesmo_dataset_mesmos_bytes_nos_dois_formatos() {
        let (a, b) = (exemplo(true), exemplo(true));
        for formato in [Formato::Csv, Formato::Parquet] {
            assert_eq!(
                montar(&a, formato).expect("a"),
                montar(&b, formato).expect("b"),
                "{formato:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Parquet.
    // -----------------------------------------------------------------------

    fn ler_parquet(bytes: Vec<u8>) -> SerializedFileReader<bytes::Bytes> {
        SerializedFileReader::new(bytes::Bytes::from(bytes)).expect("Parquet legível")
    }

    #[test]
    fn parquet_lido_de_volta_tem_esquema_linhas_e_valores() {
        let ds = exemplo(true);
        let leitor = ler_parquet(montar_parquet(&ds).expect("parquet"));
        let meta = leitor.metadata().file_metadata();
        assert_eq!(meta.num_rows(), 8);
        assert_eq!(meta.created_by(), Some(CRIADO_POR));
        let nomes: Vec<&str> = meta
            .schema_descr()
            .columns()
            .iter()
            .map(|c| c.name())
            .collect();
        assert_eq!(nomes, COLUNAS);
        let kv: Vec<(String, Option<String>)> = meta
            .key_value_metadata()
            .expect("metadados")
            .iter()
            .map(|k| (k.key.clone(), k.value.clone()))
            .collect();
        assert_eq!(
            kv,
            [
                ("loteria".to_string(), Some("quina".to_string())),
                ("feature_version".to_string(), Some("1".to_string())),
                ("trecho".to_string(), Some("busca+trancado".to_string())),
            ]
        );

        let linhas: Vec<parquet::record::Row> = leitor
            .get_row_iter(None)
            .expect("linhas")
            .collect::<std::result::Result<_, _>>()
            .expect("linhas válidas");
        assert_eq!(linhas.len(), 8);
        let campos = |i: usize| -> Vec<(String, Field)> {
            linhas[i]
                .get_column_iter()
                .map(|(n, f)| (n.clone(), f.clone()))
                .collect()
        };
        // Primeira linha: concurso 1, dezena 1, janela 0, NULLs onde o CSV tem vazio.
        let l0 = campos(0);
        assert_eq!(l0[0], ("concurso".to_string(), Field::Int(1)));
        assert_eq!(l0[1], ("data".to_string(), Field::Str("2020-01-02".into())));
        assert_eq!(l0[4], ("trecho".to_string(), Field::Str("busca".into())));
        assert_eq!(l0[6], ("maior_espaco".to_string(), Field::Null));
        assert_eq!(l0[8], ("menor_espaco".to_string(), Field::Int(0)));
        assert_eq!(l0[9], ("menor_espaco_ocorrencias".to_string(), Field::Null));
        assert_eq!(l0[14], ("saiu".to_string(), Field::Int(1)));
        assert_eq!(l0[15], ("feature_version".to_string(), Field::Int(1)));
        // Última linha: concurso 2 (trancado), dezena 2, janela 10.
        let l7 = campos(7);
        let valores: Vec<Field> = l7.into_iter().map(|(_, f)| f).collect();
        assert_eq!(
            valores,
            [
                Field::Int(2),
                Field::Str("2020-01-05".into()),
                Field::Int(2),
                Field::Int(10),
                Field::Str("trancado".into()),
                Field::Int(4),
                Field::Int(3),
                Field::Int(1),
                Field::Int(0),
                Field::Null,
                Field::Int(7),
                Field::Int(10),
                Field::Int(2),
                Field::Int(1),
                Field::Int(1),
                Field::Int(1),
            ]
        );
    }

    #[test]
    fn parquet_em_varios_grupos_tem_as_mesmas_linhas_e_e_deterministico() {
        let ds = exemplo(false);
        let um = ler_parquet(montar_parquet(&ds).expect("um grupo"));
        let bytes = montar_parquet_em_grupos(&ds, 3).expect("grupos de 3");
        assert_eq!(bytes, montar_parquet_em_grupos(&ds, 3).expect("de novo"));
        let varios = ler_parquet(bytes);
        assert_eq!(um.metadata().num_row_groups(), 1);
        assert_eq!(varios.metadata().num_row_groups(), 3);
        let linhas = |l: &SerializedFileReader<bytes::Bytes>| -> Vec<String> {
            l.get_row_iter(None)
                .expect("linhas")
                .map(|r| r.expect("linha").to_string())
                .collect()
        };
        assert_eq!(linhas(&um), linhas(&varios));
    }

    #[test]
    fn parquet_comeca_e_termina_com_par1_e_nao_tem_versao_do_crate() {
        let bytes = montar_parquet(&exemplo(false)).expect("parquet");
        assert_eq!(&bytes[..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
        let texto = String::from_utf8_lossy(&bytes);
        assert!(!texto.contains("parquet-rs version"));
    }

    // -----------------------------------------------------------------------
    // Dataset::novo.
    // -----------------------------------------------------------------------

    fn novo(
        com_trancado: bool,
        concursos: Vec<ConcursoDataset>,
        linhas: Vec<FeaturesDezena>,
    ) -> Result<Dataset> {
        Dataset::novo(
            "quina".to_string(),
            1,
            1,
            com_trancado,
            impressoes(),
            concursos,
            linhas,
        )
    }

    fn incoerente(r: Result<Dataset>, trecho: &str) {
        match r {
            Err(SpiderError::DatasetIncoerente(d)) => assert!(d.contains(trecho), "{d}"),
            outro => panic!("esperava DatasetIncoerente ({trecho}), veio {outro:?}"),
        }
    }

    #[test]
    fn novo_recusa_o_que_a_montagem_nao_aceita() {
        let c1 = || concurso(1, "2020-01-02", Trecho::Busca);
        let c2 = || concurso(2, "2020-01-05", Trecho::Busca);
        let l = |c, d| linha(c, d, 0, Some(false));
        incoerente(novo(false, vec![], vec![]), "nenhum concurso");
        incoerente(
            novo(false, vec![c2(), c1()], vec![l(1, 1), l(2, 1)]),
            "concursos fora de ordem",
        );
        incoerente(
            novo(false, vec![c1()], vec![l(1, 2), l(1, 1)]),
            "linhas fora de ordem",
        );
        incoerente(
            novo(false, vec![c1()], vec![l(1, 1), l(1, 1)]),
            "linhas fora de ordem",
        );
        incoerente(
            novo(false, vec![c1()], vec![linha(1, 1, 0, None)]),
            "sem `saiu`",
        );
        incoerente(
            novo(false, vec![c1()], vec![l(1, 1), l(2, 1)]),
            "concurso(s)",
        );
        // Com a flag: sem ela o 2 já cairia no corte (1) antes desta checagem.
        incoerente(novo(true, vec![c1(), c2()], vec![l(1, 1)]), "concurso(s)");
        incoerente(
            novo(
                false,
                vec![c1(), concurso(2, "2020-01-05", Trecho::Trancado)],
                vec![l(1, 1), l(2, 1)],
            ),
            "trancado sem `--trancado`",
        );
        // Com a flag, o mesmo trancado passa.
        novo(
            true,
            vec![c1(), concurso(2, "2020-01-05", Trecho::Trancado)],
            vec![l(1, 1), l(2, 1)],
        )
        .expect("com trancado");
    }

    #[test]
    fn novo_sem_trancado_recusa_concurso_acima_do_corte_mesmo_rotulado_busca() {
        // Corte em 1: o concurso 2 rotulado `busca` (rótulo errado) é recusado
        // sem `--trancado`, pelo número, independentemente do rótulo.
        incoerente(
            novo(
                false,
                vec![
                    concurso(1, "2020-01-02", Trecho::Busca),
                    concurso(2, "2020-01-05", Trecho::Busca),
                ],
                vec![linha(1, 1, 0, Some(false)), linha(2, 1, 0, Some(true))],
            ),
            "concurso 2 acima do corte da busca (1) sem `--trancado`",
        );
        // Só o concurso do corte passa.
        novo(
            false,
            vec![concurso(1, "2020-01-02", Trecho::Busca)],
            vec![linha(1, 1, 0, Some(false))],
        )
        .expect("até o corte");
    }

    #[test]
    fn valor_maior_que_int32_tem_erro_proprio() {
        match inteiro(u32::MAX) {
            Err(SpiderError::ValorForaDoInt32(v)) => assert_eq!(v, u32::MAX),
            outro => panic!("esperava ValorForaDoInt32, veio {outro:?}"),
        }
        assert_eq!(inteiro(i32::MAX as u32).expect("cabe"), i32::MAX);
    }

    #[test]
    fn primeiro_ultimo_e_trecho_incluido() {
        let ds = exemplo(false);
        assert_eq!((ds.primeiro_concurso(), ds.ultimo_concurso()), (1, 2));
        assert_eq!(ds.trecho_incluido(), "busca");
        assert_eq!(exemplo(true).trecho_incluido(), "busca+trancado");
    }

    // -----------------------------------------------------------------------
    // Nomes e manifesto.
    // -----------------------------------------------------------------------

    #[test]
    fn nomes_dos_arquivos() {
        let ds = exemplo(false);
        assert_eq!(nome_arquivo(&ds, Formato::Parquet), "features_v1.parquet");
        assert_eq!(nome_arquivo(&ds, Formato::Csv), "features_v1.csv");
        assert_eq!(nome_manifesto(&ds, Formato::Csv), "manifesto_csv.json");
        assert_eq!(
            nome_manifesto(&ds, Formato::Parquet),
            "manifesto_parquet.json"
        );
        let tr = exemplo(true);
        assert_eq!(
            nome_arquivo(&tr, Formato::Parquet),
            "features_v1_trancado.parquet"
        );
        assert_eq!(nome_arquivo(&tr, Formato::Csv), "features_v1_trancado.csv");
        assert_eq!(
            nome_manifesto(&tr, Formato::Csv),
            "manifesto_trancado_csv.json"
        );
        assert_eq!(
            nome_manifesto(&tr, Formato::Parquet),
            "manifesto_trancado_parquet.json"
        );
    }

    #[test]
    fn nenhum_nome_do_trancado_coincide_com_um_da_busca() {
        let (busca, tr) = (exemplo(false), exemplo(true));
        let nomes = |ds: &Dataset| -> Vec<String> {
            [Formato::Csv, Formato::Parquet]
                .into_iter()
                .flat_map(|f| [nome_arquivo(ds, f), nome_manifesto(ds, f)])
                .collect()
        };
        let (nb, nt) = (nomes(&busca), nomes(&tr));
        assert!(nb.iter().all(|n| !nt.contains(n)), "{nb:?} × {nt:?}");
    }

    #[test]
    fn sha256_confere_com_vetor_conhecido() {
        // FIPS 180-2, "abc".
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn manifesto_confere_byte_a_byte() {
        let ds = exemplo(true);
        let bytes = montar(&ds, Formato::Csv).expect("csv");
        let m = manifesto(&ds, Formato::Csv, &bytes);
        assert_eq!(m.sha256, sha256_hex(&bytes));
        let colunas = COLUNAS
            .iter()
            .map(|c| format!("    \"{c}\""))
            .collect::<Vec<_>>()
            .join(",\n");
        let esperado = format!(
            "{{\n  \"loteria\": \"quina\",\n  \"feature_version\": 1,\n  \"formato\": \"csv\",\n  \
             \"arquivo\": \"features_v1_trancado.csv\",\n  \"colunas\": [\n{colunas}\n  ],\n  \
             \"primeiro_concurso\": 1,\n  \"ultimo_concurso\": 2,\n  \
             \"ultimo_concurso_busca\": 1,\n  \"trecho\": \"busca+trancado\",\n  \
             \"concursos\": 2,\n  \"linhas\": 8,\n  \"sha256\": \"{}\",\n  \
             \"impressao_resultados_banco\": \"fnv1a64:3:0123456789abcdef\",\n  \
             \"impressao_resultados_incluidos\": \"fnv1a64:2:fedcba9876543210\"\n}}\n",
            m.sha256
        );
        let texto = formatar_manifesto(&m).expect("manifesto");
        assert_eq!(texto, esperado);
        assert_eq!(texto, formatar_manifesto(&m).expect("de novo"));
    }

    #[test]
    fn manifesto_de_cada_formato_aponta_o_proprio_arquivo() {
        let ds = exemplo(false);
        for formato in [Formato::Csv, Formato::Parquet] {
            let bytes = montar(&ds, formato).expect("bytes");
            let m = manifesto(&ds, formato, &bytes);
            assert_eq!(m.formato, formato.extensao());
            assert_eq!(m.arquivo, nome_arquivo(&ds, formato));
            assert_eq!((m.linhas, m.concursos), (8, 2));
            assert_eq!(m.trecho, "busca");
        }
    }
}
