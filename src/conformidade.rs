//! Vetores de conformidade Rust ↔ Python (tarefa 1.5.4).
//!
//! Cada caso é um JSON em `testes/conformidade/` com a entrada (resultados
//! ou features), o perfil (janela, normalização, pesos) e o **esperado**
//! calculado pelo motor: features e índice (total, posição, variação). O
//! `cargo test` regenera tudo daqui e exige bytes iguais aos arquivos
//! versionados (nada é regravado em silêncio); o `pytest` da `ml/` confere a
//! implementação de referência em Python contra os mesmos arquivos.
//!
//! Só dados sintéticos e pequenos. Para regravar depois de uma mudança
//! **deliberada** nas fórmulas (e subir a `FEATURE_VERSION`):
//!
//! ```text
//! cargo run --example gerar_conformidade
//! ```
//!
//! Formato (`versao_formato` = 1):
//!
//! - `entrada.tipo = "resultados"`: `resultados` é a lista de dezenas de cada
//!   concurso (o concurso é a posição + 1, a partir de 1); o esperado inclui
//!   o snapshot do concurso seguinte ao último (`saiu = null`);
//! - `entrada.tipo = "features"`: `features` são as linhas da janela do
//!   perfil, nas colunas de `colunas_features`; `esperado.features = null`;
//! - `esperado.concursos_conferidos`: lista de concursos cujas linhas estão
//!   no arquivo (`null` = todas). O índice é sempre calculado com a série
//!   inteira; só o que é escrito é filtrado;
//! - `NULL` é `null`; todo `f64` é escrito com 17 algarismos significativos
//!   (`{:.16e}`), o que volta exatamente ao mesmo `f64`.

use crate::config::{LoteriaConfig, Volante};
use crate::error::Result;
use crate::motor::{
    self, calcular_indice, FeaturesDezena, IndiceDezena, Normalizacao, Pesos, FEATURE_VERSION,
};
use crate::Resultado;
use std::fmt::Write as _;

/// Versão do formato dos arquivos JSON.
pub const VERSAO_FORMATO: u32 = 1;

/// Colunas das linhas de features nos JSON (entrada e esperado).
pub const COLUNAS_FEATURES: [&str; 13] = [
    "concurso",
    "dezena",
    "janela",
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
];

/// Colunas das linhas do índice nos JSON.
pub const COLUNAS_INDICE: [&str; 5] =
    ["concurso", "dezena", "total", "posicao", "variacao_posicao"];

/// Os nove componentes, na ordem de [`Pesos::componentes`] (só os testes
/// conferem a ordem; o JSON usa os nomes de `componentes()`).
#[cfg(test)]
const NOMES_PESOS: [&str; 9] = [
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
];

/// Loteria sintética do caso: K dezenas sorteadas numa faixa.
#[derive(Debug, Clone, Copy)]
pub struct Quantidades {
    pub k: usize,
    pub dezena_min: u32,
    pub dezena_max: u32,
}

/// Entrada de um caso.
#[derive(Debug, Clone)]
pub enum Entrada {
    /// Dezenas sorteadas de cada concurso, do 1 em diante.
    Resultados(Vec<Vec<u32>>),
    /// Linhas de features já prontas (só a janela do perfil).
    Features(Vec<FeaturesDezena>),
}

/// Um caso de conformidade.
#[derive(Debug, Clone)]
pub struct Caso {
    /// Nome do arquivo em `testes/conformidade/`.
    pub arquivo: &'static str,
    pub descricao: &'static str,
    pub loteria: Quantidades,
    pub entrada: Entrada,
    pub janela: u32,
    pub normalizacao: Normalizacao,
    pub pesos: Pesos,
    /// Concursos escritos no esperado (`None` = todos).
    pub conferidos: Option<Vec<u32>>,
}

/// O que o motor calcula para um caso.
#[derive(Debug, Clone)]
pub struct Calculado {
    /// Todas as janelas (resultados) ou a entrada (features).
    pub features: Vec<FeaturesDezena>,
    pub indice: Vec<IndiceDezena>,
}

/// Configuração da loteria sintética do caso. Campos de coleta (site,
/// seletores) não são usados pelo motor.
fn config_do_caso(q: Quantidades) -> LoteriaConfig {
    LoteriaConfig {
        slug: "sintetica",
        nome_exibicao: "Sintética",
        pagina_url: "",
        seletor_botao_download: "",
        seletor_concurso_data: "",
        seletor_dezenas: "",
        qtd_dezenas: q.k,
        dezena_min: q.dezena_min,
        dezena_max: q.dezena_max,
        volante: Volante {
            linhas: 1,
            colunas: q.dezena_max - q.dezena_min + 1,
        },
        coluna_concurso: 0,
        coluna_data: 1,
        primeira_coluna_dezena: 2,
        // O motor não olha o corte da busca.
        ultimo_concurso_busca: 0,
        perfil_ativo: "caso_v1",
    }
}

/// Roda o motor sobre a entrada do caso.
pub fn calcular(caso: &Caso) -> Result<Calculado> {
    let features = match &caso.entrada {
        Entrada::Resultados(sorteios) => {
            let cfg = config_do_caso(caso.loteria);
            let resultados: Vec<Resultado> = (1_u32..)
                .zip(sorteios)
                .map(|(concurso, dezenas)| Resultado {
                    loteria: cfg.slug.to_string(),
                    concurso,
                    data_sorteio: "2000-01-01".to_string(),
                    dezenas: dezenas.clone(),
                })
                .collect();
            let (estado, mut linhas) = motor::calcular(&cfg, &resultados)?;
            // Como no `sync indice`: o índice vai até o próximo concurso.
            linhas.extend(estado.snapshot());
            linhas
        }
        Entrada::Features(linhas) => linhas.clone(),
    };
    let indice = calcular_indice(&features, caso.janela, caso.normalizacao, &caso.pesos)?;
    Ok(Calculado { features, indice })
}

// ---------------------------------------------------------------------------
// Escrita do JSON (à mão: o `serde_json` não escreve `f64` com 17 dígitos).
// ---------------------------------------------------------------------------

/// `f64` com 17 algarismos significativos, sem normalizar `-0.0`: se o motor
/// voltar a produzir zero negativo num total, o arquivo muda e o teste pega.
fn flutuante(x: f64) -> String {
    format!("{x:.16e}")
}

fn texto(s: &str) -> String {
    let mut saida = String::with_capacity(s.len() + 2);
    saida.push('"');
    for c in s.chars() {
        match c {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            '\n' => saida.push_str("\\n"),
            _ => saida.push(c),
        }
    }
    saida.push('"');
    saida
}

fn opcional(v: Option<u32>) -> String {
    v.map_or_else(|| "null".to_string(), |v| v.to_string())
}

fn linha_de_features(f: &FeaturesDezena) -> String {
    let saiu = match f.saiu {
        Some(true) => "1",
        Some(false) => "0",
        None => "null",
    };
    format!(
        "[{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}]",
        f.concurso,
        f.dezena,
        f.janela,
        f.frequencia,
        opcional(f.maior_espaco),
        opcional(f.maior_espaco_ocorrencias),
        opcional(f.menor_espaco),
        opcional(f.menor_espaco_ocorrencias),
        opcional(f.soma_variacoes),
        opcional(f.espaco_final),
        f.repeticoes_total,
        f.repeticao_atual,
        saiu,
    )
}

fn linha_de_indice(i: &IndiceDezena) -> String {
    format!(
        "[{}, {}, {}, {}, {}]",
        i.concurso,
        i.dezena,
        flutuante(i.total),
        i.posicao,
        i.variacao_posicao
            .map_or_else(|| "null".to_string(), |v| v.to_string()),
    )
}

/// Linhas dentro de um array, uma por linha, com a indentação dada.
fn bloco(linhas: &[String], recuo: &str) -> String {
    let corpo: Vec<String> = linhas.iter().map(|l| format!("{recuo}  {l}")).collect();
    format!("[\n{}\n{recuo}]", corpo.join(",\n"))
}

fn lista_de_nomes(nomes: &[&str]) -> String {
    let t: Vec<String> = nomes.iter().map(|n| texto(n)).collect();
    format!("[{}]", t.join(", "))
}

/// Texto JSON do caso (esperado calculado agora pelo motor). Termina em `\n`.
pub fn gerar(caso: &Caso) -> Result<String> {
    let calculado = calcular(caso)?;
    let filtro = |c: u32| caso.conferidos.as_ref().is_none_or(|v| v.contains(&c));
    let q = caso.loteria;

    let mut s = String::new();
    // `write!` num `String` não falha.
    let _ = writeln!(s, "{{");
    let _ = writeln!(s, "  \"versao_formato\": {VERSAO_FORMATO},");
    let _ = writeln!(s, "  \"descricao\": {},", texto(caso.descricao));
    let _ = writeln!(
        s,
        "  \"loteria\": {{\"qtd_dezenas\": {}, \"dezena_min\": {}, \"dezena_max\": {}}},",
        q.k, q.dezena_min, q.dezena_max
    );
    let _ = writeln!(s, "  \"feature_version\": {FEATURE_VERSION},");
    let _ = writeln!(s, "  \"entrada\": {{");
    match &caso.entrada {
        Entrada::Resultados(sorteios) => {
            let linhas: Vec<String> = sorteios
                .iter()
                .map(|d| {
                    let v: Vec<String> = d.iter().map(u32::to_string).collect();
                    format!("[{}]", v.join(", "))
                })
                .collect();
            let _ = writeln!(s, "    \"tipo\": \"resultados\",");
            let _ = writeln!(s, "    \"resultados\": {}", bloco(&linhas, "    "));
        }
        Entrada::Features(linhas) => {
            let _ = writeln!(s, "    \"tipo\": \"features\",");
            let _ = writeln!(
                s,
                "    \"colunas_features\": {},",
                lista_de_nomes(&COLUNAS_FEATURES)
            );
            let l: Vec<String> = linhas.iter().map(linha_de_features).collect();
            let _ = writeln!(s, "    \"features\": {}", bloco(&l, "    "));
        }
    }
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"perfil\": {{");
    let _ = writeln!(s, "    \"janela\": {},", caso.janela);
    let _ = writeln!(
        s,
        "    \"normalizacao\": {},",
        texto(caso.normalizacao.como_str())
    );
    let _ = writeln!(s, "    \"pesos\": {{");
    let pesos = caso.pesos.componentes();
    for (i, (nome, peso)) in pesos.iter().enumerate() {
        let virgula = if i + 1 < pesos.len() { "," } else { "" };
        let _ = writeln!(s, "      {}: {}{virgula}", texto(nome), flutuante(*peso));
    }
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"esperado\": {{");
    match &caso.conferidos {
        None => {
            let _ = writeln!(s, "    \"concursos_conferidos\": null,");
        }
        Some(v) => {
            let t: Vec<String> = v.iter().map(u32::to_string).collect();
            let _ = writeln!(s, "    \"concursos_conferidos\": [{}],", t.join(", "));
        }
    }
    match &caso.entrada {
        Entrada::Resultados(_) => {
            let _ = writeln!(
                s,
                "    \"colunas_features\": {},",
                lista_de_nomes(&COLUNAS_FEATURES)
            );
            let l: Vec<String> = calculado
                .features
                .iter()
                .filter(|f| filtro(f.concurso))
                .map(linha_de_features)
                .collect();
            let _ = writeln!(s, "    \"features\": {},", bloco(&l, "    "));
        }
        Entrada::Features(_) => {
            let _ = writeln!(s, "    \"features\": null,");
        }
    }
    let _ = writeln!(
        s,
        "    \"colunas_indice\": {},",
        lista_de_nomes(&COLUNAS_INDICE)
    );
    let l: Vec<String> = calculado
        .indice
        .iter()
        .filter(|i| filtro(i.concurso))
        .map(linha_de_indice)
        .collect();
    let _ = writeln!(s, "    \"indice\": {}", bloco(&l, "    "));
    let _ = writeln!(s, "  }}");
    let _ = writeln!(s, "}}");
    Ok(s)
}

/// Todos os casos gerados: `(nome do arquivo, conteúdo)`, em ordem de nome.
pub fn gerar_todos() -> Result<Vec<(&'static str, String)>> {
    casos()
        .iter()
        .map(|c| gerar(c).map(|texto| (c.arquivo, texto)))
        .collect()
}

// ---------------------------------------------------------------------------
// Os casos.
// ---------------------------------------------------------------------------

fn pesos(v: [f64; 9]) -> Pesos {
    Pesos {
        frequencia: v[0],
        maior_espaco: v[1],
        maior_espaco_ocorrencias: v[2],
        menor_espaco: v[3],
        menor_espaco_ocorrencias: v[4],
        soma_variacoes: v[5],
        espaco_final: v[6],
        repeticoes_total: v[7],
        repeticao_atual: v[8],
    }
}

/// Pesos que só usam `frequencia`, `espaco_final` e `repeticao_atual`.
fn pesos_so(frequencia: f64, espaco_final: f64, repeticao_atual: f64) -> Pesos {
    pesos([
        frequencia,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        espaco_final,
        0.0,
        repeticao_atual,
    ])
}

/// Pesos do perfil `padrao_v1` (só para dar números parecidos com os reais).
fn pesos_padrao() -> Pesos {
    pesos([1.0, 1.0, 0.3, 0.5, 0.3, -0.8, 1.2, 0.7, 0.4])
}

/// Linha de features com os nove componentes na ordem de [`NOMES_PESOS`];
/// `frequencia`, `repeticoes_total` e `repeticao_atual` nunca são `NULL`
/// (`None` vira 0). `saiu` fica vazio.
fn linha(concurso: u32, dezena: u32, janela: u32, c: [Option<u32>; 9]) -> FeaturesDezena {
    FeaturesDezena {
        concurso,
        dezena,
        janela,
        frequencia: c[0].unwrap_or(0),
        maior_espaco: c[1],
        maior_espaco_ocorrencias: c[2],
        menor_espaco: c[3],
        menor_espaco_ocorrencias: c[4],
        soma_variacoes: c[5],
        espaco_final: c[6],
        repeticoes_total: c[7].unwrap_or(0),
        repeticao_atual: c[8].unwrap_or(0),
        saiu: None,
    }
}

/// Concurso inteiro de features: `por_dezena[i]` são os componentes da
/// dezena `i + 1`.
fn concurso_de_features(
    concurso: u32,
    janela: u32,
    por_dezena: &[[Option<u32>; 9]],
) -> Vec<FeaturesDezena> {
    (1_u32..)
        .zip(por_dezena)
        .map(|(d, c)| linha(concurso, d, janela, *c))
        .collect()
}

const fn s(v: u32) -> Option<u32> {
    Some(v)
}

/// Sorteios pseudoaleatórios determinísticos (LCG de Knuth, sem crate).
fn sorteios_pseudo(semente: u64, concursos: u32, q: Quantidades) -> Vec<Vec<u32>> {
    let mut x = semente;
    (0..concursos)
        .map(|_| {
            let mut pool: Vec<u32> = (q.dezena_min..=q.dezena_max).collect();
            let mut sorteio = Vec::with_capacity(q.k);
            for _ in 0..q.k {
                x = x
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let i = ((x >> 33) % pool.len() as u64) as usize;
                sorteio.push(pool.swap_remove(i));
            }
            sorteio.sort_unstable();
            sorteio
        })
        .collect()
}

/// Os casos feitos à mão, em ordem de arquivo.
pub fn casos() -> Vec<Caso> {
    vec![
        caso_golden(),
        caso_bordas_null(),
        caso_janelas_fronteiras(),
        caso_componente_constante(),
        caso_todo_null(),
        caso_empate_exato(),
        caso_quase_empate(),
        caso_zscore_janela_10(),
        caso_minmax_janela_50(),
        caso_variacao_primeiro_concurso(),
        caso_formato_lotofacil(),
        caso_meio_exato(),
        caso_empate_exato_zscore(),
    ]
}

/// Exemplo golden do plano: a dezena 7 sai nos concursos 3, 4, 8, 9, 10 e 15;
/// snapshot em t = 20. Faixa 1..=8, K = 2; as outras dezenas giram.
fn caso_golden() -> Caso {
    let outras = [1_u32, 2, 3, 4, 5, 6, 8];
    let com_sete = [3_u32, 4, 8, 9, 10, 15];
    let sorteios = (1_u32..=19)
        .map(|c| {
            let a = outras[(c % 7) as usize];
            let mut v = if com_sete.contains(&c) {
                vec![a, 7]
            } else {
                vec![a, outras[((c + 3) % 7) as usize]]
            };
            v.sort_unstable();
            v
        })
        .collect();
    Caso {
        arquivo: "01_golden_plano.json",
        descricao: "Golden do plano: a dezena 7 sai nos concursos 3, 4, 8, 9, 10 e 15; \
                    em t = 20 (janela total) frequencia 6, maior espaco 4 (1 vez), menor \
                    espaco 0 (3 vezes), soma de variacoes 10, espaco final 4, repeticoes \
                    3, repeticao atual 0. Perfil zscore, janela total.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 8,
        },
        entrada: Entrada::Resultados(sorteios),
        janela: 0,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos_padrao(),
        conferidos: Some(vec![1, 4, 20]),
    }
}

/// Bordas de `NULL`: primeiro concurso, dezena que nunca saiu, uma, duas e
/// três ou mais aparições, aparição em t−1.
fn caso_bordas_null() -> Caso {
    Caso {
        arquivo: "02_bordas_null.json",
        descricao: "Bordas de NULL numa loteria 2 de 5: t = 1 sem historico; a dezena 5 \
                    nunca sai; a 3 sai duas vezes (concursos 2 e 6); a 1 sai em \
                    1, 2, 4, 5 e 7. Minmax, janela total.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 5,
        },
        entrada: Entrada::Resultados(vec![
            vec![1, 2],
            vec![1, 3],
            vec![2, 4],
            vec![1, 4],
            vec![1, 2],
            vec![3, 4],
            vec![1, 4],
        ]),
        janela: 0,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos([1.0, 0.5, -0.25, 0.75, 0.125, -1.5, 1.0, 0.5, 0.25]),
        conferidos: None,
    }
}

/// Fronteiras das janelas 10, 50 e 100: a dezena 1 sai em 1, 40 e 41; a 2 em
/// 50, 90 e 140; a 3 em 60 e 61; as dezenas 4 e 5 completam.
fn caso_janelas_fronteiras() -> Caso {
    let sorteios = (1_u32..=140)
        .map(|c| match c {
            1 | 40 | 41 => vec![1, 4],
            50 | 90 | 140 => vec![2, 5],
            60 | 61 => vec![3, 5],
            _ => vec![4, 5],
        })
        .collect();
    Caso {
        arquivo: "03_janelas_fronteiras.json",
        descricao: "Janelas 10, 50 e 100 e suas fronteiras: a dezena 1 sai em 1, 40 e 41 \
                    (em t = 51, 91 e 141 o concurso 41 e o primeiro dentro da janela 10, \
                    50 e 100 e o 40 ja esta fora); a dezena 2 sai em 50, 90 e 140 (t-1 \
                    das mesmas fronteiras). Perfil zscore, janela 100.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 5,
        },
        entrada: Entrada::Resultados(sorteios),
        janela: 100,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos_padrao(),
        conferidos: Some(vec![1, 50, 51, 52, 90, 91, 92, 140, 141]),
    }
}

/// Componentes constantes, de valor único e todo `NULL`, em z-score.
fn caso_componente_constante() -> Caso {
    let n = None;
    // Ordem: freq, maior, maior_occ, menor, menor_occ, soma, final, rep_tot, rep_atual.
    let c1 = [
        [s(2), s(5), n, s(0), s(3), n, s(0), s(1), s(0)],
        [s(2), s(5), n, n, s(3), n, s(4), s(1), s(0)],
        [s(4), s(5), n, s(2), s(3), s(7), s(0), s(1), s(0)],
        [s(4), s(5), n, n, n, n, s(4), s(1), s(0)],
    ];
    let c2 = [
        [s(2), s(5), n, n, s(3), n, s(1), s(1), s(0)],
        [s(4), s(5), n, n, s(3), n, s(1), s(1), s(0)],
        [s(4), s(5), n, n, s(3), s(7), s(1), s(1), s(0)],
        [s(2), s(5), n, n, n, n, s(1), s(1), s(0)],
    ];
    let c3 = [
        [s(3), s(5), n, n, s(3), n, s(1), s(1), s(0)],
        [s(3), s(5), n, n, s(3), n, s(1), s(1), s(0)],
        [s(3), s(5), n, n, s(3), s(7), s(1), s(1), s(0)],
        [s(3), s(5), n, n, n, n, s(1), s(1), s(0)],
    ];
    let mut features = concurso_de_features(1, 0, &c1);
    features.extend(concurso_de_features(2, 0, &c2));
    features.extend(concurso_de_features(3, 0, &c3));
    Caso {
        arquivo: "04_componente_constante.json",
        descricao: "Z-score com componentes constantes (maior_espaco, repeticoes_total), \
                    todo NULL (maior_espaco_ocorrencias), de valor unico entre NULLs \
                    (soma_variacoes) e com NULL misturado a valores (menor_espaco): tudo \
                    isso recebe o neutro 0. No concurso 3 todos os componentes sao \
                    constantes e o total e 0 para as quatro dezenas.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 4,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos([1.0, 2.0, 3.0, 0.5, 4.0, 5.0, -0.25, 6.0, 7.0]),
        conferidos: None,
    }
}

/// Todo `NULL` em minmax: todos os componentes recebem o neutro 0,5.
fn caso_todo_null() -> Caso {
    let n = None;
    let linha_vazia = [s(0), n, n, n, n, n, n, s(0), s(0)];
    let mut features = concurso_de_features(1, 0, &[linha_vazia; 3]);
    features.extend(concurso_de_features(2, 0, &[linha_vazia; 3]));
    Caso {
        arquivo: "05_todo_null.json",
        descricao: "Minmax com todos os componentes opcionais NULL e os demais zerados: \
                    cada componente recebe o neutro 0,5 e o total de todas as dezenas e \
                    0,5 x soma dos pesos = 14,125.",
        loteria: Quantidades {
            k: 1,
            dezena_min: 1,
            dezena_max: 3,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos([1.0, 2.0, 3.0, 0.5, 4.0, 5.0, -0.25, 6.0, 7.0]),
        conferidos: None,
    }
}

/// Empates exatos (menor dezena na frente), pesos negativos e variação.
fn caso_empate_exato() -> Caso {
    let n = None;
    let ef = |f: u32, e: u32| [s(f), n, n, n, n, n, s(e), s(0), s(0)];
    let c1 = [ef(4, 0), ef(4, 3), ef(0, 0), ef(4, 0), ef(0, 3)];
    let c2 = [ef(0, 3), ef(4, 0), ef(4, 0), ef(0, 3), ef(4, 0)];
    let c3 = [ef(0, 0); 5];
    let mut features = concurso_de_features(1, 0, &c1);
    features.extend(concurso_de_features(2, 0, &c2));
    features.extend(concurso_de_features(3, 0, &c3));
    Caso {
        arquivo: "06_empate_exato.json",
        descricao: "Empates exatos em minmax com peso negativo (espaco_final = -1): \
                    totais [1, 0, 0, 1, -1] no concurso 1 (posicoes 1, 3, 4, 2, 5), \
                    [-1, 1, 1, -1, 1] no 2 e todos 0 no 3 (posicoes 1 a 5); em empate \
                    a menor dezena vem na frente. Mostra a variacao de posicao positiva \
                    e negativa.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 5,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos_so(1.0, -1.0, 0.0),
        conferidos: None,
    }
}

/// Componentes de uma dezena que "liga" (= 1) um só destes, com o resto em 0:
/// 0 = frequencia, 1 = repeticoes_total, 2 = repeticao_atual,
/// 3 = maior_espaco_ocorrencias, 4 = soma_variacoes; outro valor = nenhum.
/// Em minmax o ligado normaliza para 1 e os outros para 0, então o total é
/// exatamente o peso do componente ligado.
fn um_componente_ligado(ligado: usize) -> [Option<u32>; 9] {
    let n = None;
    let mut c = [s(0), n, s(0), n, n, s(0), n, s(0), s(0)];
    match ligado {
        0 => c[0] = s(1),
        1 => c[7] = s(1),
        2 => c[8] = s(1),
        3 => c[2] = s(1),
        4 => c[5] = s(1),
        _ => {}
    }
    c
}

/// Quase empates a 1e-9: o total é arredondado a 1e-9 antes de ordenar.
fn caso_quase_empate() -> Caso {
    let dez = um_componente_ligado;
    let c1 = [dez(0), dez(1), dez(2), dez(3), dez(4), dez(5)];
    let c2 = [dez(1), dez(0), dez(3), dez(2), dez(4), dez(5)];
    let mut features = concurso_de_features(1, 0, &c1);
    features.extend(concurso_de_features(2, 0, &c2));
    Caso {
        arquivo: "07_quase_empate_1e9.json",
        descricao: "Quase empate a 1e-9 em minmax: os totais das dezenas 1 a 4 sao \
                    0,3, 0,3 + 4e-10, 0,3 + 6e-10 e 0,3 - 5,1e-10; arredondados a 1e-9 \
                    ficam 0,3, 0,3, 0,300000001 e 0,299999999 (a 1 e a 2 empatam e a \
                    menor dezena vem na frente, embora a 2 seja maior sem arredondar). \
                    A dezena 5 tem total -4e-10, que arredonda para zero (sem sinal) e \
                    empata com a 6 (total 0). No concurso 2 as dezenas trocam de papel.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 6,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos([
            0.3,
            0.0,
            0.299_999_999_49,
            0.0,
            0.0,
            -4e-10,
            0.0,
            0.300_000_000_4,
            0.300_000_000_6,
        ]),
        conferidos: None,
    }
}

/// Z-score com janela 10 sobre sorteios pseudoaleatórios.
fn caso_zscore_janela_10() -> Caso {
    let q = Quantidades {
        k: 3,
        dezena_min: 1,
        dezena_max: 7,
    };
    Caso {
        arquivo: "08_zscore_janela_10.json",
        descricao: "Z-score, janela 10, loteria 3 de 7, 60 concursos pseudoaleatorios \
                    (LCG de semente 8), pesos de sinais mistos.",
        loteria: q,
        entrada: Entrada::Resultados(sorteios_pseudo(8, 60, q)),
        janela: 10,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos([1.0, 0.7, -0.3, 0.45, 0.2, -0.8, 1.2, 0.35, -0.55]),
        conferidos: Some(vec![1, 2, 11, 12, 61]),
    }
}

/// Minmax com janela 50 e pesos negativos.
fn caso_minmax_janela_50() -> Caso {
    let q = Quantidades {
        k: 3,
        dezena_min: 1,
        dezena_max: 9,
    };
    Caso {
        arquivo: "09_minmax_janela_50.json",
        descricao: "Minmax, janela 50, loteria 3 de 9, 100 concursos pseudoaleatorios \
                    (LCG de semente 9), pesos negativos em quatro componentes.",
        loteria: q,
        entrada: Entrada::Resultados(sorteios_pseudo(9, 100, q)),
        janela: 50,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos([-1.0, 0.6, 0.9, -0.4, 0.15, 0.8, -1.1, 0.25, -0.05]),
        conferidos: Some(vec![1, 2, 51, 101]),
    }
}

/// Um único concurso: `variacao_posicao` é `null` no primeiro e definida no
/// seguinte.
fn caso_variacao_primeiro_concurso() -> Caso {
    Caso {
        arquivo: "10_variacao_primeiro_concurso.json",
        descricao: "Um unico concurso sorteado (dezenas 3 e 4): o snapshot de t = 1 nao \
                    tem concurso anterior (variacao_posicao null) e o de t = 2 sobe a \
                    3 e a 4 (posicoes 1 e 2), variacoes +2 para elas e -2 para a 1 e a 2.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 4,
        },
        entrada: Entrada::Resultados(vec![vec![3, 4]]),
        janela: 0,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos_so(1.0, -1.0, 0.5),
        conferidos: None,
    }
}

/// Dimensões reais da Lotofácil (15 de 25) com os pesos do `padrao_v1`.
fn caso_formato_lotofacil() -> Caso {
    let q = Quantidades {
        k: 15,
        dezena_min: 1,
        dezena_max: 25,
    };
    Caso {
        arquivo: "11_formato_lotofacil.json",
        descricao: "Dimensoes da Lotofacil (15 de 25), 25 concursos pseudoaleatorios \
                    (LCG de semente 11), pesos do padrao_v1, z-score, janela total.",
        loteria: q,
        entrada: Entrada::Resultados(sorteios_pseudo(11, 25, q)),
        janela: 0,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos_padrao(),
        conferidos: Some(vec![1, 26]),
    }
}

#[cfg(test)]
mod tests;

/// Meio exato do arredondamento: `total · 1e9` dá exatamente ±0,5 ou ±2,5,
/// onde o `round` do Rust (meio para longe do zero) e o `np.round` (meio para
/// o par) divergem.
fn caso_meio_exato() -> Caso {
    let dez = um_componente_ligado;
    let c1 = [dez(0), dez(1), dez(2), dez(3), dez(4), dez(5)];
    let c2 = [dez(1), dez(0), dez(3), dez(2), dez(4), dez(5)];
    let mut features = concurso_de_features(1, 0, &c1);
    features.extend(concurso_de_features(2, 0, &c2));
    Caso {
        arquivo: "12_meio_exato_arredondamento.json",
        descricao: "Meio exato do arredondamento a 1e-9 em minmax: os totais das dezenas \
                    1 a 5 sao 2,5e-9, 3e-9, -2,5e-9, -3e-9 e 5e-10 (vezes 1e9 dao \
                    exatamente 2,5, -2,5 e 0,5). Meio para longe do zero: 3e-9, 3e-9, \
                    -3e-9, -3e-9 e 1e-9, entao a 1 empata com a 2 e a 3 com a 4 \
                    (posicoes 1, 2, 5, 6, 3 e 4 para a dezena 6 com total 0). Com meio \
                    para o par a ordem mudaria. No concurso 2 as dezenas trocam de papel.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 6,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Minmax,
        pesos: pesos([2.5e-9, 0.0, -3e-9, 0.0, 0.0, 5e-10, 0.0, 3e-9, -2.5e-9]),
        conferidos: None,
    }
}

/// Empates em z-score: dezenas com componentes iguais e um empate que só
/// existe depois do arredondamento a 1e-9.
fn caso_empate_exato_zscore() -> Caso {
    let n = None;
    let ef = |f: u32, e: u32| [s(f), n, n, n, n, n, s(e), s(0), s(0)];
    let c1 = [ef(4, 0), ef(4, 3), ef(0, 0), ef(4, 0), ef(0, 3)];
    let c2 = [ef(0, 3), ef(4, 0), ef(4, 0), ef(0, 3), ef(4, 0)];
    let mut features = concurso_de_features(1, 0, &c1);
    features.extend(concurso_de_features(2, 0, &c2));
    Caso {
        arquivo: "13_empate_exato_zscore.json",
        descricao: "Empates em z-score com peso negativo (espaco_final = -1): no concurso \
                    1 as dezenas 1 e 4 tem componentes iguais (empate exato) e as dezenas \
                    2 e 3 tem z(4) - z(3) e z(0) - z(0), iguais na conta exata e diferentes \
                    so no ruido do ponto flutuante, que o arredondamento a 1e-9 apaga; em \
                    empate a menor dezena vem na frente. No concurso 2 os papeis mudam.",
        loteria: Quantidades {
            k: 2,
            dezena_min: 1,
            dezena_max: 5,
        },
        entrada: Entrada::Features(features),
        janela: 0,
        normalizacao: Normalizacao::Zscore,
        pesos: pesos_so(1.0, -1.0, 0.0),
        conferidos: None,
    }
}
