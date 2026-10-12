//! Testes da exportação da Fase 2 (`sync exportar`): os quatro JSON do
//! contrato `v1` (`index.json`, `ultimo.json`, `tabela_geral.json`,
//! `historico.json`), o formato byte a byte, o determinismo, o "tudo ou nada"
//! e a preservação das loterias não regravadas.

use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::exportacao::{self, disco, ArquivosLoteria};
use rust_spider::motor::FEATURE_VERSION;
use rust_spider::orquestracao::{
    dados_para_exportar, sincronizar_features, sincronizar_indice, DadosExportacao,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::Resultado;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Histórico pequeno: o snapshot é do concurso 13, com janela 10.
const N: u32 = 12;
const GERADO_EM: &str = "2026-10-05T10:00:00Z";
const SLUGS: [&str; 3] = ["lotofacil", "megasena", "quina"];

/// O `padrao_v1` é o perfil ativo das três loterias, o único que o CLI lê.
const PERFIL_MODELO: &str = r#"nome = "padrao_v1"
versao = 1
loteria = "@@"
janela = 10
normalizacao = "minmax"

[pesos]
frequencia = 1.0
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"#;

fn texto_perfil(slug: &str) -> String {
    PERFIL_MODELO.replace("@@", slug)
}

fn cfg_de(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn perfil_de(slug: &str) -> PerfilCarregado {
    perfil::interpretar(
        &texto_perfil(slug),
        Path::new("perfis/teste/padrao_v1.toml"),
    )
    .expect("perfil")
}

/// Histórico sintético determinístico (xorshift64).
fn historico(cfg: &LoteriaConfig, n: u32) -> Vec<Resultado> {
    let mut x = SEMENTE;
    let faixa: Vec<u32> = cfg.dezenas().collect();
    (1..=n)
        .map(|c| {
            let mut pool = faixa.clone();
            let mut dezenas = Vec::with_capacity(cfg.qtd_dezenas);
            for _ in 0..cfg.qtd_dezenas {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                dezenas.push(pool.swap_remove((x % pool.len() as u64) as usize));
            }
            Resultado {
                loteria: cfg.slug.to_string(),
                concurso: c,
                data_sorteio: format!("2020-01-{:02}", 1 + c % 28),
                dezenas,
            }
        })
        .collect()
}

/// Histórico em que a dezena `sem` nunca sai: onde ela sairia, entra a
/// primeira dezena da faixa que ainda não está no sorteio.
fn historico_sem(cfg: &LoteriaConfig, n: u32, sem: u32) -> Vec<Resultado> {
    let mut hist = historico(cfg, n);
    for r in &mut hist {
        if let Some(pos) = r.dezenas.iter().position(|&d| d == sem) {
            let troca = cfg
                .dezenas()
                .find(|d| *d != sem && !r.dezenas.contains(d))
                .expect("dezena livre");
            r.dezenas[pos] = troca;
        }
    }
    hist
}

fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
    sincronizar_indice(conn, cfg, &perfil_de(cfg.slug), GERADO_EM).expect("índice");
}

fn conn_com(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, cfg, hist);
    conn
}

fn dados_de(conn: &mut Connection, slug: &str) -> DadosExportacao {
    dados_para_exportar(conn, cfg_de(slug), &perfil_de(slug)).expect("dados")
}

/// Os três arquivos de uma loteria com `n` concursos sintéticos.
fn lote(slug: &str, n: u32) -> ArquivosLoteria {
    let cfg = cfg_de(slug);
    let mut conn = conn_com(cfg, &historico(cfg, n));
    exportacao::gerar_loteria(&dados_de(&mut conn, slug)).expect("arquivos")
}

fn lotes_das_tres() -> Vec<ArquivosLoteria> {
    SLUGS.iter().map(|s| lote(s, N)).collect()
}

fn texto(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("UTF-8")
}

fn json(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).expect("JSON válido")
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn ordenadas(dezenas: &[u32]) -> Vec<u32> {
    let mut v = dezenas.to_vec();
    v.sort_unstable();
    v
}

/// `[1, 2, 3]`, como o contrato escreve as dezenas.
fn linha_de(dezenas: &[u32]) -> String {
    let itens: Vec<String> = ordenadas(dezenas).iter().map(u32::to_string).collect();
    format!("[{}]", itens.join(", "))
}

/// Pasta temporária única, apagada no fim do teste.
struct Pasta(PathBuf);

impl Pasta {
    fn nova(rotulo: &str) -> Self {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("relógio")
            .as_nanos();
        let n = CONTADOR.fetch_add(1, Ordering::Relaxed);
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_exportar_{rotulo}_{}_{unico}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&caminho).expect("pasta temporária");
        Pasta(caminho)
    }

    fn saida(&self) -> PathBuf {
        self.0.join("publico")
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Todos os arquivos (não pastas) sob `dir`, ordenados; vazio se não existe.
fn listar(dir: &Path) -> Vec<PathBuf> {
    let mut achados = Vec::new();
    if let Ok(entradas) = std::fs::read_dir(dir) {
        for e in entradas {
            let caminho = e.expect("entrada").path();
            if caminho.is_dir() {
                achados.extend(listar(&caminho));
            } else {
                achados.push(caminho);
            }
        }
    }
    achados.sort();
    achados
}

fn gravar_tudo(saida: &Path, lotes: &[ArquivosLoteria], gerado_em: &str) {
    disco::gravar(saida, lotes, &SLUGS, gerado_em).expect("gravação");
}

fn ler(saida: &Path, relativo: &str) -> Vec<u8> {
    std::fs::read(disco::pasta_versao(saida).join(relativo)).expect(relativo)
}

// ---------------------------------------------------------------------------
// 1. Golden pequeno.
// ---------------------------------------------------------------------------

#[test]
fn golden_ultimo_json_texto_completo() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let arquivos = lote("lotofacil", N);
    let ultimo = &hist[N as usize - 1];
    // Concurso 12: data 2020-01-(1 + 12 % 28) = 2020-01-13; dezenas em ordem crescente.
    assert_eq!(ultimo.data_sorteio, "2020-01-13");
    let esperado = format!(
        "{{\n  \"schema_version\": 1,\n  \"loteria\": \"lotofacil\",\n  \"concurso\": 12,\n  \
         \"data\": \"2020-01-13\",\n  \"dezenas\": {}\n}}\n",
        linha_de(&ultimo.dezenas)
    );
    assert_eq!(texto(&arquivos.ultimo), esperado);
}

#[test]
fn golden_historico_json_um_concurso_por_linha() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let arquivos = lote("lotofacil", N);
    let linhas: Vec<String> = hist
        .iter()
        .map(|r| {
            format!(
                "    [{}, \"{}\", {}]",
                r.concurso,
                r.data_sorteio,
                linha_de(&r.dezenas)
            )
        })
        .collect();
    let esperado = format!(
        "{{\n  \"schema_version\": 1,\n  \"loteria\": \"lotofacil\",\n  \"concursos\": [\n{}\n  ]\n}}\n",
        linhas.join(",\n")
    );
    assert_eq!(texto(&arquivos.historico), esperado);
    // 12 concursos: 12 linhas de item.
    assert_eq!(linhas.len(), 12);
}

#[test]
fn golden_tabela_geral_cabecalho_e_primeira_entrada_em_texto() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com(cfg, &historico(cfg, N));
    let dados = dados_de(&mut conn, "lotofacil");
    let arquivos = exportacao::gerar_loteria(&dados).expect("arquivos");
    let t = texto(&arquivos.tabela_geral);
    // Contrato v1: nenhum acento nos JSON (identificadores estáveis).
    for bytes in [
        &arquivos.ultimo,
        &arquivos.tabela_geral,
        &arquivos.historico,
    ] {
        assert!(bytes.is_ascii(), "JSON com caractere fora do ASCII");
    }

    let hash = perfil::hash_toml(&texto_perfil("lotofacil"));
    let cabecalho = format!(
        "{{\n  \"schema_version\": 1,\n  \"loteria\": \"lotofacil\",\n  \"perfil\": \"padrao_v1\",\n  \
         \"hash_toml\": \"{hash}\",\n  \"janela\": 10,\n  \"normalizacao\": \"minmax\",\n  \
         \"feature_version\": {FEATURE_VERSION},\n  \"concurso\": 13,\n  \"ultimo_sorteado\": 12,\n  \
         \"dezenas\": [\n    {{\n"
    );
    assert!(t.starts_with(&cabecalho), "cabeçalho diferente:\n{t}");

    // Primeira entrada: campos na ordem do contrato, indentação de 6 espaços.
    let l = &dados.tabela.linhas[0];
    let f = &l.features;
    let op = |v: Option<u32>| v.map_or("null".to_string(), |v| v.to_string());
    let txt = |v: Option<&str>| v.map_or("null".to_string(), |v| format!("\"{v}\""));
    let entrada = format!(
        "      \"dezena\": {},\n      \"posicao\": 1,\n      \"total\": {:.9},\n      \
         \"variacao_posicao\": {},\n      \"frequencia\": {},\n      \"maior_espaco\": {},\n      \
         \"maior_espaco_ocorrencias\": {},\n      \"menor_espaco\": {},\n      \
         \"menor_espaco_ocorrencias\": {},\n      \"soma_variacoes\": {},\n      \
         \"espaco_final\": {},\n      \"repeticoes_total\": {},\n      \"repeticao_atual\": {},\n      \
         \"evidencia_grau\": {},\n      \"evidencia_direcao\": {}\n    }}",
        l.dezena,
        l.total,
        l.variacao_posicao.map_or("null".to_string(), |v| v.to_string()),
        f.frequencia,
        op(f.maior_espaco),
        op(f.maior_espaco_ocorrencias),
        op(f.menor_espaco),
        op(f.menor_espaco_ocorrencias),
        op(f.soma_variacoes),
        op(f.espaco_final),
        f.repeticoes_total,
        f.repeticao_atual,
        txt(l.evidencia.map(|e| exportacao::grau_no_contrato(e.grau))),
        txt(l.evidencia.map(|e| exportacao::direcao_no_contrato(e.direcao))),
    );
    assert!(
        t[cabecalho.len()..].starts_with(&entrada),
        "primeira entrada diferente:\n{}",
        &t[cabecalho.len()..cabecalho.len() + 900.min(t.len() - cabecalho.len())]
    );
    // A evidência do concurso 13 existe (há histórico): rótulos em texto.
    assert!(l.evidencia.is_some());
    assert!(t.ends_with("    }\n  ]\n}\n"));
}

#[test]
fn golden_tabela_geral_por_parsing_campos_e_tipos() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com(cfg, &historico(cfg, N));
    let dados = dados_de(&mut conn, "lotofacil");
    let arquivos = exportacao::gerar_loteria(&dados).expect("arquivos");
    let v = json(&arquivos.tabela_geral);

    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["loteria"], "lotofacil");
    assert_eq!(v["perfil"], "padrao_v1");
    assert_eq!(
        v["hash_toml"].as_str(),
        Some(perfil::hash_toml(&texto_perfil("lotofacil")).as_str())
    );
    assert_eq!(v["janela"], 10);
    assert_eq!(v["normalizacao"], "minmax");
    assert_eq!(v["feature_version"], FEATURE_VERSION);
    assert_eq!(v["concurso"], N + 1);
    assert_eq!(v["ultimo_sorteado"], N);

    let dezenas = v["dezenas"].as_array().expect("lista de dezenas");
    assert_eq!(dezenas.len(), 25);
    let chaves_esperadas = [
        "dezena",
        "posicao",
        "total",
        "variacao_posicao",
        "frequencia",
        "maior_espaco",
        "maior_espaco_ocorrencias",
        "menor_espaco",
        "menor_espaco_ocorrencias",
        "soma_variacoes",
        "espaco_final",
        "repeticoes_total",
        "repeticao_atual",
        "evidencia_grau",
        "evidencia_direcao",
    ];
    let graus = ["muito_fraca", "fraca", "moderada", "forte", "muito_forte"];
    let direcoes = ["vies", "acaso"];
    let mut vistas = Vec::new();
    for (i, d) in dezenas.iter().enumerate() {
        let obj = d.as_object().expect("objeto");
        assert_eq!(obj.len(), 15, "15 chaves por dezena: {d}");
        for k in chaves_esperadas {
            assert!(obj.contains_key(k), "falta `{k}` em {d}");
        }
        // Posição 1..=25 na ordem em que aparecem.
        assert_eq!(d["posicao"], (i + 1) as u64);
        let l = &dados.tabela.linhas[i];
        assert_eq!(d["dezena"], l.dezena);
        vistas.push(l.dezena);
        // `total` é número com o mesmo valor (a 1e-9), `frequencia` é inteiro.
        let total = d["total"].as_f64().expect("total numérico");
        assert!((total - l.total).abs() < 1e-9);
        assert!(d["frequencia"].is_u64());
        assert_eq!(d["frequencia"], l.features.frequencia);
        assert!(d["repeticoes_total"].is_u64() && d["repeticao_atual"].is_u64());
        assert!(d["variacao_posicao"].is_null() || d["variacao_posicao"].is_i64());
        for k in [
            "maior_espaco",
            "maior_espaco_ocorrencias",
            "menor_espaco",
            "menor_espaco_ocorrencias",
            "soma_variacoes",
            "espaco_final",
        ] {
            assert!(d[k].is_null() || d[k].is_u64(), "`{k}` em {d}");
        }
        assert!(graus.contains(&d["evidencia_grau"].as_str().expect("grau texto")));
        assert!(direcoes.contains(&d["evidencia_direcao"].as_str().expect("direção texto")));
    }
    // As 25 dezenas aparecem uma vez cada.
    vistas.sort_unstable();
    assert_eq!(vistas, (1..=25).collect::<Vec<_>>());
}

#[test]
fn ultimo_e_historico_por_parsing_batem_com_o_banco() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let arquivos = lote("lotofacil", N);

    let u = json(&arquivos.ultimo);
    assert_eq!(u["concurso"], N);
    assert_eq!(u["data"], "2020-01-13");
    assert_eq!(
        u["dezenas"],
        serde_json::json!(ordenadas(&hist[N as usize - 1].dezenas))
    );

    let h = json(&arquivos.historico);
    assert_eq!(h["schema_version"], 1);
    assert_eq!(h["loteria"], "lotofacil");
    let concursos = h["concursos"].as_array().expect("lista");
    assert_eq!(concursos.len(), N as usize);
    for (c, r) in concursos.iter().zip(&hist) {
        assert_eq!(c[0], r.concurso);
        assert_eq!(c[1], r.data_sorteio.as_str());
        assert_eq!(c[2], serde_json::json!(ordenadas(&r.dezenas)));
    }
}

// ---------------------------------------------------------------------------
// 2. Determinismo e index.json.
// ---------------------------------------------------------------------------

#[test]
fn duas_exportacoes_dao_arquivos_identicos_byte_a_byte() {
    let (a, b) = (Pasta::nova("det_a"), Pasta::nova("det_b"));
    gravar_tudo(&a.saida(), &lotes_das_tres(), "2026-10-05T10:00:00Z");
    gravar_tudo(&b.saida(), &lotes_das_tres(), "2030-01-01T00:00:00Z");

    for slug in SLUGS {
        for nome in ["ultimo.json", "tabela_geral.json", "historico.json"] {
            let rel = format!("{slug}/{nome}");
            assert_eq!(ler(&a.saida(), &rel), ler(&b.saida(), &rel), "{rel}");
        }
    }
}

#[test]
fn index_das_duas_exportacoes_so_difere_em_gerado_em() {
    let (a, b) = (Pasta::nova("idx_a"), Pasta::nova("idx_b"));
    gravar_tudo(&a.saida(), &lotes_das_tres(), "2026-10-05T10:00:00Z");
    gravar_tudo(&b.saida(), &lotes_das_tres(), "2030-01-01T00:00:00Z");

    let (ia, ib) = (ler(&a.saida(), "index.json"), ler(&b.saida(), "index.json"));
    assert_ne!(ia, ib);
    let (mut va, mut vb) = (json(&ia), json(&ib));
    assert_eq!(va["gerado_em"], "2026-10-05T10:00:00Z");
    assert_eq!(vb["gerado_em"], "2030-01-01T00:00:00Z");
    va.as_object_mut().expect("objeto").remove("gerado_em");
    vb.as_object_mut().expect("objeto").remove("gerado_em");
    assert_eq!(va, vb);
    // No texto, trocar o carimbo basta para igualar os bytes.
    let ta = texto(&ia).replace("2026-10-05T10:00:00Z", "X");
    let tb = texto(&ib).replace("2030-01-01T00:00:00Z", "X");
    assert_eq!(ta, tb);
}

#[test]
fn sha256_e_bytes_do_index_conferem_com_os_arquivos_no_disco() {
    let p = Pasta::nova("sha");
    gravar_tudo(&p.saida(), &lotes_das_tres(), GERADO_EM);
    let idx = json(&ler(&p.saida(), "index.json"));

    assert_eq!(idx["schema_version"], 1);
    assert_eq!(idx["gerado_em"], GERADO_EM);
    for slug in SLUGS {
        let e = &idx["loterias"][slug];
        assert_eq!(e["ultimo_concurso"], N);
        assert_eq!(e["data"], "2020-01-13");
        for chave in ["ultimo", "tabela_geral", "historico"] {
            let a = &e["arquivos"][chave];
            let caminho = a["caminho"].as_str().expect("caminho");
            // Relativo a `v1/`, separador `/`.
            assert_eq!(caminho, format!("{slug}/{chave}.json"));
            let bytes = ler(&p.saida(), caminho);
            assert_eq!(
                a["sha256"].as_str(),
                Some(sha256(&bytes).as_str()),
                "{caminho}"
            );
            assert_eq!(a["bytes"], bytes.len() as u64, "{caminho}");
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Formato dos bytes.
// ---------------------------------------------------------------------------

#[test]
fn todos_os_arquivos_sao_utf8_sem_bom_sem_cr_e_terminam_em_um_lf() {
    let p = Pasta::nova("formato");
    gravar_tudo(&p.saida(), &lotes_das_tres(), GERADO_EM);
    let arquivos = listar(&disco::pasta_versao(&p.saida()));
    // 3 loterias x 3 arquivos + index.
    assert_eq!(arquivos.len(), 10);
    for caminho in arquivos {
        let bytes = std::fs::read(&caminho).expect("arquivo");
        let nome = caminho.display();
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM em {nome}");
        assert!(!bytes.contains(&b'\r'), "CR em {nome}");
        assert!(!bytes.contains(&b'\t'), "tab em {nome}");
        assert!(bytes.ends_with(b"\n"), "sem LF final em {nome}");
        assert!(!bytes.ends_with(b"\n\n"), "LF duplo no fim de {nome}");
        let t = std::str::from_utf8(&bytes).expect("UTF-8");
        // Indentação só em múltiplos de 2 espaços.
        for linha in t.lines() {
            let recuo = linha.len() - linha.trim_start_matches(' ').len();
            assert_eq!(recuo % 2, 0, "{nome}: {linha}");
        }
        serde_json::from_slice::<Value>(&bytes).expect("JSON válido");
    }
}

// ---------------------------------------------------------------------------
// 4. `total` com 9 casas e `null` explícito.
// ---------------------------------------------------------------------------

#[test]
fn total_sempre_com_exatamente_9_casas() {
    for slug in SLUGS {
        let arquivos = lote(slug, N);
        let t = texto(&arquivos.tabela_geral);
        let valores: Vec<&str> = t
            .lines()
            .filter_map(|l| l.trim().strip_prefix("\"total\": "))
            .collect();
        assert_eq!(
            valores.len(),
            cfg_de(slug).qtd_dezenas_possiveis() as usize,
            "{slug}"
        );
        for v in valores {
            let v = v.trim_end_matches(',');
            let (inteira, fracao) = v
                .split_once('.')
                .unwrap_or_else(|| panic!("sem ponto: {v}"));
            assert_eq!(fracao.len(), 9, "{slug}: {v}");
            assert!(fracao.bytes().all(|b| b.is_ascii_digit()), "{v}");
            assert!(
                inteira
                    .trim_start_matches('-')
                    .bytes()
                    .all(|b| b.is_ascii_digit()),
                "{v}"
            );
            assert!(!v.contains(['e', 'E']), "expoente em {v}");
        }
    }
}

#[test]
fn dezena_que_nunca_saiu_tem_componentes_null_explicitos() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com(cfg, &historico_sem(cfg, N, 25));
    let arquivos = exportacao::gerar_loteria(&dados_de(&mut conn, "lotofacil")).expect("arquivos");
    let t = texto(&arquivos.tabela_geral);
    assert!(t.contains("\"maior_espaco\": null"), "{t}");

    let v = json(&arquivos.tabela_geral);
    let d25 = v["dezenas"]
        .as_array()
        .expect("lista")
        .iter()
        .find(|d| d["dezena"] == 25)
        .expect("dezena 25");
    // Nunca saiu: frequência 0 e todos os componentes de espaço ausentes.
    assert_eq!(d25["frequencia"], 0);
    for k in [
        "maior_espaco",
        "maior_espaco_ocorrencias",
        "menor_espaco",
        "menor_espaco_ocorrencias",
        "soma_variacoes",
    ] {
        assert!(
            d25.get(k).is_some_and(Value::is_null),
            "`{k}` deveria ser null: {d25}"
        );
    }
    // A chave existe (não foi omitida) mesmo com valor null.
    assert_eq!(d25.as_object().expect("objeto").len(), 15);
}

// ---------------------------------------------------------------------------
// 5. Recusas (biblioteca).
// ---------------------------------------------------------------------------

#[test]
fn lib_recusa_features_desatualizadas() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com(cfg, &hist[..N as usize]);
    // Concurso novo gravado sem rodar `sync features`.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    match dados_para_exportar(&mut conn, cfg, &perfil_de("lotofacil")) {
        Err(SpiderError::FeaturesDesatualizadas { .. }) => {}
        outro => panic!("esperava FeaturesDesatualizadas, veio {outro:?}"),
    }
}

#[test]
fn lib_recusa_indice_desatualizado() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com(cfg, &hist[..N as usize]);
    // Features em dia, índice não.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    match dados_para_exportar(&mut conn, cfg, &perfil_de("lotofacil")) {
        Err(SpiderError::IndiceDesatualizado { esperado, .. }) => assert_eq!(esperado, N + 2),
        outro => panic!("esperava IndiceDesatualizado, veio {outro:?}"),
    }
}

// ---------------------------------------------------------------------------
// 6. `null` de loteria sem arquivos no index.
// ---------------------------------------------------------------------------

#[test]
fn loterias_sem_arquivos_ficam_null_no_index_na_ordem_do_config() {
    let p = Pasta::nova("nulos");
    disco::gravar(&p.saida(), &[lote("megasena", N)], &SLUGS, GERADO_EM).expect("gravação");
    let bytes = ler(&p.saida(), "index.json");
    let t = texto(&bytes);

    let v = json(&bytes);
    assert!(v["loterias"]["lotofacil"].is_null());
    assert!(v["loterias"]["quina"].is_null());
    assert!(v["loterias"]["megasena"].is_object());
    assert_eq!(v["loterias"].as_object().expect("objeto").len(), 3);

    // Ordem do texto: lotofacil, megasena, quina (o `serde_json` ordenaria
    // por conta própria, então confere pelo texto).
    let pos = |s: &str| {
        t.find(&format!("\"{s}\": "))
            .unwrap_or_else(|| panic!("{s}"))
    };
    assert!(pos("lotofacil") < pos("megasena") && pos("megasena") < pos("quina"));
    assert!(t.contains("    \"lotofacil\": null,\n"), "{t}");
    assert!(t.contains("    \"quina\": null\n  }\n}\n"), "{t}");
    // Só os arquivos da megasena foram criados.
    let pasta = disco::pasta_versao(&p.saida());
    assert!(!pasta.join("lotofacil").exists() && !pasta.join("quina").exists());
}

// ---------------------------------------------------------------------------
// 8. Saída inconsistente.
// ---------------------------------------------------------------------------

#[test]
fn saida_com_so_ultimo_json_de_loteria_preservada_e_recusada_sem_alterar_nada() {
    let p = Pasta::nova("inconsistente");
    // Megasena na saída com só `ultimo.json`; vamos regravar só a lotofacil.
    let ultimo_mega = lote("megasena", N).ultimo;
    let pasta_mega = disco::pasta_versao(&p.saida()).join("megasena");
    std::fs::create_dir_all(&pasta_mega).expect("pasta");
    std::fs::write(pasta_mega.join("ultimo.json"), &ultimo_mega).expect("ultimo");
    let antes = listar(&p.saida());

    match disco::gravar(&p.saida(), &[lote("lotofacil", N)], &SLUGS, GERADO_EM) {
        Err(SpiderError::ExportacaoInvalida(motivo)) => {
            assert!(motivo.contains("tabela_geral.json") || motivo.contains("historico.json"));
        }
        outro => panic!("esperava ExportacaoInvalida, veio {outro:?}"),
    }
    assert_eq!(listar(&p.saida()), antes, "nada pode mudar na saída");
    assert_eq!(
        std::fs::read(pasta_mega.join("ultimo.json")).expect("ultimo"),
        ultimo_mega
    );
}

// ---------------------------------------------------------------------------
// 9. Sem sobras.
// ---------------------------------------------------------------------------

#[test]
fn nao_sobram_arquivos_tmp_apos_gravar_nem_apos_regravar() {
    let p = Pasta::nova("tmp");
    gravar_tudo(&p.saida(), &lotes_das_tres(), GERADO_EM);
    // Regrava só uma loteria por cima.
    disco::gravar(&p.saida(), &[lote("quina", N)], &SLUGS, GERADO_EM).expect("regravação");
    let arquivos = listar(&p.saida());
    assert_eq!(arquivos.len(), 10);
    for a in arquivos {
        let nome = a.file_name().expect("nome").to_string_lossy().into_owned();
        assert!(nome.ends_with(".json"), "sobra: {}", a.display());
        assert!(!nome.contains("tmp"), "sobra: {}", a.display());
    }
}

// ---------------------------------------------------------------------------
// 10. Incremental.
// ---------------------------------------------------------------------------

#[test]
fn concurso_novo_muda_ultimo_tabela_e_acrescenta_uma_linha_ao_historico() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com(cfg, &hist[..N as usize]);
    let antes = exportacao::gerar_loteria(&dados_de(&mut conn, "lotofacil")).expect("antes");

    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    let depois = exportacao::gerar_loteria(&dados_de(&mut conn, "lotofacil")).expect("depois");

    // ultimo.json: concurso 13, data 2020-01-(1 + 13 % 28) = 2020-01-14.
    let (ua, ud) = (json(&antes.ultimo), json(&depois.ultimo));
    assert_eq!(
        (ua["concurso"].clone(), ua["data"].clone()),
        (N.into(), "2020-01-13".into())
    );
    assert_eq!(
        (ud["concurso"].clone(), ud["data"].clone()),
        ((N + 1).into(), "2020-01-14".into())
    );
    assert_eq!(
        ud["dezenas"],
        serde_json::json!(ordenadas(&hist[N as usize].dezenas))
    );

    // tabela_geral.json: o próximo agora é o 14.
    let (ta, td) = (json(&antes.tabela_geral), json(&depois.tabela_geral));
    assert_eq!(
        (ta["concurso"].clone(), ta["ultimo_sorteado"].clone()),
        ((N + 1).into(), N.into())
    );
    assert_eq!(
        (td["concurso"].clone(), td["ultimo_sorteado"].clone()),
        ((N + 2).into(), (N + 1).into())
    );
    assert_ne!(antes.tabela_geral, depois.tabela_geral);

    // historico.json: uma linha a mais; o prefixo não muda (a vírgula da
    // antiga última linha é a única diferença de bytes).
    let (la, ld): (Vec<&str>, Vec<&str>) = (
        texto(&antes.historico).lines().collect(),
        texto(&depois.historico).lines().collect(),
    );
    assert_eq!(ld.len(), la.len() + 1);
    // Linhas de item: da 5ª (índice 4) até antes de `  ]` e `}`.
    let itens_a = &la[4..la.len() - 2];
    let itens_d = &ld[4..ld.len() - 2];
    assert_eq!(itens_a.len(), N as usize);
    assert_eq!(itens_d.len(), N as usize + 1);
    for (a, d) in itens_a.iter().zip(itens_d) {
        assert_eq!(a.trim_end_matches(','), d.trim_end_matches(','));
    }
    assert!(itens_d[N as usize - 1].ends_with(','));
    assert!(itens_d[N as usize].starts_with("    [13, \"2020-01-14\", ["));
    assert_eq!(la[la.len() - 2..], ld[ld.len() - 2..]);
}

// ---------------------------------------------------------------------------
// CLI: `sync exportar`.
// ---------------------------------------------------------------------------

fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_exportar(alvo: &str, saida: &Path, perfis: &Path, extras: &[&Path]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["exportar".into(), alvo.into()];
    args.push("--saida".into());
    args.push(saida.into());
    args.push("--perfis".into());
    args.push(perfis.into());
    if let Some(db) = extras.first() {
        args.push("--db".into());
        args.push((*db).into());
    }
    args
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Pasta de trabalho com os três bancos padrão (`resultados_<slug>.db`, com
/// `n` concursos, features e índice) e `perfis/<slug>/padrao_v1.toml`.
fn ambiente_das_tres(rotulo: &str, n: u32) -> (Pasta, PathBuf) {
    let p = Pasta::nova(rotulo);
    let perfis = p.0.join("perfis");
    for slug in SLUGS {
        let cfg = cfg_de(slug);
        {
            let mut conn = db::abrir(&p.0.join(format!("resultados_{slug}.db"))).expect("banco");
            preparar(&mut conn, cfg, &historico(cfg, n));
        }
        std::fs::create_dir_all(perfis.join(slug)).expect("pasta de perfis");
        std::fs::write(perfis.join(slug).join("padrao_v1.toml"), texto_perfil(slug))
            .expect("perfil");
    }
    (p, perfis)
}

fn caminho_db(p: &Pasta, slug: &str) -> PathBuf {
    p.0.join(format!("resultados_{slug}.db"))
}

/// Acrescenta o concurso `n + 1` ao banco da loteria; com `indice`, também
/// atualiza o índice.
fn acrescentar_concurso(p: &Pasta, slug: &str, n: u32, indice: bool) {
    let cfg = cfg_de(slug);
    let hist = historico(cfg, n + 1);
    let mut conn = db::abrir(&caminho_db(p, slug)).expect("banco");
    db::salvar_resultados(&mut conn, &hist[n as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    if indice {
        sincronizar_indice(&mut conn, cfg, &perfil_de(slug), GERADO_EM).expect("índice");
    }
}

#[test]
fn cli_exportar_todas_grava_os_dez_arquivos_iguais_aos_da_biblioteca() {
    let (p, perfis) = ambiente_das_tres("cli_todas", N);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("todas", &saida, &perfis, &[]));
    assert!(out.status.success(), "{}", stderr(&out));

    assert_eq!(listar(&saida).len(), 10);
    for l in lotes_das_tres() {
        for (nome, bytes) in [
            ("ultimo.json", &l.ultimo),
            ("tabela_geral.json", &l.tabela_geral),
            ("historico.json", &l.historico),
        ] {
            assert_eq!(
                &ler(&saida, &format!("{}/{nome}", l.slug)),
                bytes,
                "{} {nome}",
                l.slug
            );
        }
    }
    let idx = json(&ler(&saida, "index.json"));
    for slug in SLUGS {
        assert!(idx["loterias"][slug].is_object(), "{slug}");
    }
    // O `gerado_em` é um carimbo ISO 8601 UTC.
    let g = idx["gerado_em"].as_str().expect("gerado_em");
    assert!(g.len() == 20 && g.ends_with('Z') && g.contains('T'), "{g}");
}

#[test]
fn cli_exportar_uma_loteria_com_db_explicito_deixa_as_outras_null() {
    let (p, perfis) = ambiente_das_tres("cli_uma", N);
    let saida = p.saida();
    let db = caminho_db(&p, "quina");
    let out = sync(&p.0, &args_exportar("quina", &saida, &perfis, &[&db]));
    assert!(out.status.success(), "{}", stderr(&out));

    assert_eq!(listar(&saida).len(), 4); // 3 da quina + index
    let idx = json(&ler(&saida, "index.json"));
    assert!(idx["loterias"]["lotofacil"].is_null());
    assert!(idx["loterias"]["megasena"].is_null());
    assert!(idx["loterias"]["quina"].is_object());
}

#[test]
fn cli_features_desatualizadas_recusa_sem_gravar_nada() {
    let (p, perfis) = ambiente_das_tres("cli_feat", N);
    // Concurso novo sem `sync features`.
    let cfg = cfg_de("lotofacil");
    {
        let hist = historico(cfg, N + 1);
        let mut conn = db::abrir(&caminho_db(&p, "lotofacil")).expect("banco");
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    }
    let saida = p.saida();
    let db = caminho_db(&p, "lotofacil");
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &[&db]));
    assert!(!out.status.success());
    assert!(listar(&saida).is_empty(), "nada pode ser gravado");
    assert!(!saida.exists());
}

#[test]
fn cli_indice_desatualizado_recusa_sem_gravar_nada() {
    let (p, perfis) = ambiente_das_tres("cli_ind", N);
    acrescentar_concurso(&p, "megasena", N, false);
    let saida = p.saida();
    let db = caminho_db(&p, "megasena");
    let out = sync(&p.0, &args_exportar("megasena", &saida, &perfis, &[&db]));
    assert!(!out.status.success());
    assert!(listar(&saida).is_empty(), "nada pode ser gravado");
    assert!(!saida.exists());
}

#[test]
fn cli_todas_com_uma_loteria_desatualizada_nao_grava_nada_em_nenhuma() {
    let (p, perfis) = ambiente_das_tres("cli_tudo_ou_nada", N);
    // Quina com features em dia e índice defasado.
    acrescentar_concurso(&p, "quina", N, false);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("todas", &saida, &perfis, &[]));
    assert!(!out.status.success());
    assert!(stderr(&out).contains("quina"), "{}", stderr(&out));
    // Nem os arquivos das loterias em dia, nem o index.json.
    assert!(listar(&saida).is_empty(), "{:?}", listar(&saida));
    assert!(!saida.exists());
}

#[test]
fn cli_exportar_uma_loteria_preserva_os_arquivos_das_outras_e_refaz_o_index() {
    let (p, perfis) = ambiente_das_tres("cli_preserva", N);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("todas", &saida, &perfis, &[]));
    assert!(out.status.success(), "{}", stderr(&out));

    let rel = |slug: &str| {
        ["ultimo.json", "tabela_geral.json", "historico.json"].map(|n| format!("{slug}/{n}"))
    };
    let antes: Vec<Vec<u8>> = SLUGS
        .iter()
        .flat_map(|s| rel(s))
        .map(|r| ler(&saida, &r))
        .collect();

    // O banco da megasena ganha o concurso 13; exporta só ela.
    acrescentar_concurso(&p, "megasena", N, true);
    let out = sync(&p.0, &args_exportar("megasena", &saida, &perfis, &[]));
    assert!(out.status.success(), "{}", stderr(&out));

    let depois: Vec<Vec<u8>> = SLUGS
        .iter()
        .flat_map(|s| rel(s))
        .map(|r| ler(&saida, &r))
        .collect();
    for (i, (a, d)) in antes.iter().zip(&depois).enumerate() {
        let slug = SLUGS[i / 3];
        if slug == "megasena" {
            assert_ne!(a, d, "arquivo {i} da megasena deveria ter mudado");
        } else {
            assert_eq!(a, d, "arquivo {i} de {slug} deveria estar intacto");
        }
    }

    // O index reflete o disco: hashes e bytes conferem; megasena no 13.
    let idx = json(&ler(&saida, "index.json"));
    assert_eq!(idx["loterias"]["megasena"]["ultimo_concurso"], N + 1);
    assert_eq!(idx["loterias"]["lotofacil"]["ultimo_concurso"], N);
    assert_eq!(idx["loterias"]["quina"]["ultimo_concurso"], N);
    for slug in SLUGS {
        for chave in ["ultimo", "tabela_geral", "historico"] {
            let a = &idx["loterias"][slug]["arquivos"][chave];
            let bytes = ler(&saida, a["caminho"].as_str().expect("caminho"));
            assert_eq!(
                a["sha256"].as_str(),
                Some(sha256(&bytes).as_str()),
                "{slug} {chave}"
            );
            assert_eq!(a["bytes"], bytes.len() as u64);
        }
    }
    assert_eq!(listar(&saida).len(), 10, "sem sobras");
}

#[test]
fn cli_banco_inexistente_falha_sem_criar_banco_nem_saida() {
    let p = Pasta::nova("cli_sem_banco");
    let db = p.0.join("nao_existe.db");
    let out = sync(
        &p.0,
        &args_exportar("lotofacil", &p.saida(), &p.0.join("perfis"), &[&db]),
    );
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("banco não encontrado"),
        "{}",
        stderr(&out)
    );
    assert!(!db.exists());
    assert!(!p.saida().exists());
}

#[test]
fn cli_db_com_todas_e_recusado() {
    let p = Pasta::nova("cli_db_todas");
    let db = p.0.join("x.db");
    let out = sync(
        &p.0,
        &args_exportar("todas", &p.saida(), &p.0.join("perfis"), &[&db]),
    );
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--db"), "{}", stderr(&out));
    assert!(!p.saida().exists());
}
