//! Testes do `ranking_taxa.json` (tarefa 2.5), o arquivo OPCIONAL de cada
//! loteria no `sync exportar`: conteúdo e formato quando o ranking está em dia,
//! omissão (sem tabela, sem linhas, atrasado, incompleto, divergente do
//! índice), remoção do arquivo antigo, o `index.json` sem a chave, o relatório
//! de `exportar_com_relatorio`, o determinismo, a CLI e o contrato escrito.
//!
//! Os helpers (histórico sintético, perfil, pasta temporária) são cópia dos de
//! `tests/exportar.rs`; o ranking é gravado com poucas réplicas porque o
//! esperado simulado só é impresso, não entra no JSON.

use rusqlite::Connection;
use rust_spider::avaliacao::SEMENTE_ESPERADO;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db::{self, LinhaRankingTaxa};
use rust_spider::error::SpiderError;
use rust_spider::exportacao::{self, disco, ArquivosLoteria};
use rust_spider::motor::{Normalizacao, FEATURE_VERSION};
use rust_spider::orquestracao::{
    dados_para_exportar, exportar_com_relatorio, exportar_tudo_ou_nada, sincronizar_features,
    sincronizar_indice, sincronizar_ranking_com, DadosExportacao, MotivoRankingOmitido,
    RankingDaLoteria, RankingParaExportar, RankingProximo, ResultadoComRelatorio,
    ResultadoExportacao, SituacaoRanking, TabelaConcurso,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::Resultado;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Histórico pequeno: o snapshot é do concurso 13, com janela 10.
const N: u32 = 12;
/// Poucas réplicas: a simulação é o que mais pesa em teste sem otimização.
const REPLICAS: usize = 20;
const GERADO_EM: &str = "2026-10-05T10:00:00Z";
const SLUGS: [&str; 3] = ["lotofacil", "megasena", "quina"];

const CHAVES_CABECALHO: [&str; 11] = [
    "schema_version",
    "loteria",
    "perfil",
    "hash_toml",
    "janela",
    "normalizacao",
    "feature_version",
    "concurso",
    "ultimo_sorteado",
    "taxa_acaso",
    "dezenas",
];
const CHAVES_ENTRADA: [&str; 7] = [
    "dezena",
    "posicao_indice",
    "nota_a",
    "diferenca_acaso",
    "nota_b",
    "posicao_a",
    "posicao_b",
];

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

/// Histórico sintético determinístico (xorshift64): o prefixo de `n`
/// concursos é o mesmo para qualquer `n` maior.
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

/// Resultados, features e índice (sem ranking).
fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
    sincronizar_indice(conn, cfg, &perfil_de(cfg.slug), GERADO_EM).expect("índice");
}

fn ranquear(conn: &mut Connection, cfg: &LoteriaConfig) {
    sincronizar_ranking_com(conn, cfg, &perfil_de(cfg.slug), REPLICAS, SEMENTE_ESPERADO)
        .expect("ranking");
}

/// Banco em memória SEM `sync ranking`.
fn conn_sem_ranking(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, cfg, hist);
    conn
}

/// Banco em memória com o ranking do próximo concurso em dia.
fn conn_com_ranking(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = conn_sem_ranking(cfg, hist);
    ranquear(&mut conn, cfg);
    conn
}

fn dados_de(conn: &mut Connection, slug: &str) -> DadosExportacao {
    dados_para_exportar(conn, cfg_de(slug), &perfil_de(slug)).expect("dados")
}

/// O motivo da omissão; falha se o ranking estiver em dia.
fn motivo_de(conn: &mut Connection, slug: &str) -> MotivoRankingOmitido {
    match dados_de(conn, slug).ranking {
        RankingParaExportar::Omitido(m) => m,
        RankingParaExportar::EmDia(_) => panic!("esperava ranking omitido"),
    }
}

fn lote_de(conn: &mut Connection, slug: &str) -> ArquivosLoteria {
    exportacao::gerar_loteria(&dados_de(conn, slug)).expect("arquivos")
}

/// Uma loteria com o ranking em dia, calculada uma vez por processo de teste
/// (o ranking das três com 12 concursos é a parte cara).
struct Caso {
    slug: &'static str,
    dados: DadosExportacao,
    lote: ArquivosLoteria,
    /// `ranking_taxa` do banco para o concurso último + 1.
    no_banco: Vec<LinhaRankingTaxa>,
}

fn casos() -> &'static [Caso] {
    static CASOS: OnceLock<Vec<Caso>> = OnceLock::new();
    CASOS.get_or_init(|| {
        SLUGS
            .iter()
            .map(|&slug| {
                let cfg = cfg_de(slug);
                let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
                let dados = dados_de(&mut conn, slug);
                let lote = exportacao::gerar_loteria(&dados).expect("arquivos");
                let no_banco = db::ler_ranking_taxa(&conn, "padrao_v1")
                    .expect("ranking do banco")
                    .into_iter()
                    .filter(|l| l.concurso == N + 1)
                    .collect();
                Caso {
                    slug,
                    dados,
                    lote,
                    no_banco,
                }
            })
            .collect()
    })
}

fn lotes_com_ranking() -> Vec<ArquivosLoteria> {
    casos().iter().map(|c| c.lote.clone()).collect()
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

fn ranking_bytes(lote: &ArquivosLoteria) -> &[u8] {
    lote.ranking_taxa.as_deref().expect("ranking_taxa.json")
}

/// Chaves (entre aspas) das linhas com exatamente `recuo` espaços de recuo.
fn chaves_com_recuo(t: &str, recuo: usize) -> Vec<String> {
    t.lines()
        .filter(|l| l.len() - l.trim_start_matches(' ').len() == recuo)
        .filter_map(|l| l.trim_start().strip_prefix('"'))
        .filter_map(|l| l.split_once("\": ").map(|(k, _)| k.to_string()))
        .collect()
}

/// Valores de texto (sem a vírgula final) das linhas `"<chave>": <valor>`.
fn valores_de<'a>(t: &'a str, chave: &str) -> Vec<&'a str> {
    let prefixo = format!("\"{chave}\": ");
    t.lines()
        .filter_map(|l| l.trim().strip_prefix(prefixo.as_str()))
        .map(|v| v.trim_end_matches(','))
        .collect()
}

/// `{:.9}` sem o `-0.000000000` (o contrato nunca escreve menos zero).
fn nove_casas(v: f64) -> String {
    let t = format!("{v:.9}");
    let sem = t.trim_start_matches('-');
    if sem.chars().all(|c| c == '0' || c == '.') {
        sem.to_string()
    } else {
        t
    }
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
            "rust_spider_exportar_ranking_{rotulo}_{}_{unico}_{n}",
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

fn ler(saida: &Path, relativo: &str) -> Vec<u8> {
    std::fs::read(disco::pasta_versao(saida).join(relativo)).expect(relativo)
}

fn existe(saida: &Path, relativo: &str) -> bool {
    disco::pasta_versao(saida).join(relativo).exists()
}

fn gravar_tudo(saida: &Path, lotes: &[ArquivosLoteria], gerado_em: &str) {
    disco::gravar(saida, lotes, &SLUGS, gerado_em).expect("gravação");
}

// ---------------------------------------------------------------------------
// 1. Ranking em dia.
// ---------------------------------------------------------------------------

#[test]
fn ranking_em_dia_marca_dados_e_lote_como_publicados() {
    for c in casos() {
        let RankingParaExportar::EmDia(r) = &c.dados.ranking else {
            panic!("{}: esperava EmDia", c.slug);
        };
        let cfg = cfg_de(c.slug);
        assert_eq!(r.qtd_sorteadas as usize, cfg.qtd_dezenas, "{}", c.slug);
        assert_eq!(r.qtd_dezenas, cfg.qtd_dezenas_possiveis(), "{}", c.slug);
        assert_eq!(r.linhas.len(), cfg.qtd_dezenas_possiveis() as usize);
        assert!(c.lote.ranking_taxa.is_some(), "{}", c.slug);
        assert!(c.lote.ranking_omitido.is_none(), "{}", c.slug);
    }
}

#[test]
fn ranking_json_tem_as_chaves_e_a_ordem_do_contrato() {
    for c in casos() {
        let t = texto(ranking_bytes(&c.lote));
        // Recuo 2: o cabeçalho; recuo 6: as 7 chaves de cada dezena, repetidas.
        assert_eq!(chaves_com_recuo(t, 2), CHAVES_CABECALHO, "{}", c.slug);
        let n = cfg_de(c.slug).qtd_dezenas_possiveis() as usize;
        let esperadas: Vec<&str> = CHAVES_ENTRADA.repeat(n);
        assert_eq!(chaves_com_recuo(t, 6), esperadas, "{}", c.slug);
        assert!(t.ends_with("    }\n  ]\n}\n"), "{}", c.slug);
    }
}

#[test]
fn ranking_json_repete_o_cabecalho_da_tabela_geral() {
    for c in casos() {
        let rt = texto(ranking_bytes(&c.lote));
        let tt = texto(&c.lote.tabela_geral);
        // `{` + as 9 chaves comuns: 10 linhas idênticas byte a byte.
        let comum = |t: &str| t.lines().take(10).collect::<Vec<_>>().join("\n");
        assert_eq!(comum(rt), comum(tt), "{}", c.slug);

        let (r, t) = (json(ranking_bytes(&c.lote)), json(&c.lote.tabela_geral));
        for k in &CHAVES_CABECALHO[..9] {
            assert_eq!(r[k], t[k], "{} {k}", c.slug);
        }
        assert_eq!(r["feature_version"], FEATURE_VERSION);
        assert_eq!(r["concurso"], N + 1);
        assert_eq!(r["ultimo_sorteado"], N);
    }
}

#[test]
fn ranking_json_traz_uma_entrada_por_dezena_em_ordem_crescente() {
    for c in casos() {
        let v = json(ranking_bytes(&c.lote));
        let dezenas: Vec<u64> = v["dezenas"]
            .as_array()
            .expect("lista")
            .iter()
            .map(|d| d["dezena"].as_u64().expect("dezena"))
            .collect();
        let esperadas: Vec<u64> = cfg_de(c.slug).dezenas().map(u64::from).collect();
        assert_eq!(dezenas, esperadas, "{}", c.slug);
    }
}

#[test]
fn ranking_json_bate_com_a_tabela_ranking_taxa_do_banco() {
    for c in casos() {
        let v = json(ranking_bytes(&c.lote));
        let entradas = v["dezenas"].as_array().expect("lista");
        assert_eq!(entradas.len(), c.no_banco.len(), "{}", c.slug);
        for (e, l) in entradas.iter().zip(&c.no_banco) {
            assert_eq!(e["dezena"], l.dezena, "{}", c.slug);
            assert_eq!(e["posicao_indice"], l.posicao_indice);
            assert_eq!(e["posicao_a"], l.posicao_ranking_a);
            assert_eq!(e["posicao_b"], l.posicao_ranking_b);
            // Números: o texto é o valor do banco arredondado a 1e-9.
            let a = e["nota_a"].as_f64().expect("nota_a");
            let b = e["nota_b"].as_f64().expect("nota_b");
            assert!((a - l.taxa_posicao).abs() <= 5.1e-10, "{} nota_a", c.slug);
            assert!((b - l.nota_b).abs() <= 5.1e-10, "{} nota_b", c.slug);
        }
        // As linhas lidas do banco são o concurso 13 inteiro.
        assert!(c.no_banco.iter().all(|l| l.concurso == N + 1));
    }
}

#[test]
fn diferenca_acaso_e_nota_a_menos_k_sobre_n_com_nove_casas() {
    for c in casos() {
        let cfg = cfg_de(c.slug);
        // K/N: Lotofácil 15/25, Mega-Sena 6/60, Quina 5/80.
        let acaso = cfg.qtd_dezenas as f64 / f64::from(cfg.qtd_dezenas_possiveis());
        let t = texto(ranking_bytes(&c.lote));
        assert_eq!(
            valores_de(t, "taxa_acaso"),
            [nove_casas(acaso)],
            "{}",
            c.slug
        );

        let diferencas = valores_de(t, "diferenca_acaso");
        assert_eq!(diferencas.len(), c.no_banco.len());
        for (d, l) in diferencas.iter().zip(&c.no_banco) {
            assert_eq!(*d, nove_casas(l.taxa_posicao - acaso), "{}", c.slug);
        }
    }
    // Valores à mão: 15/25 = 0.6, 6/60 = 0.1, 5/80 = 0.0625.
    let acaso = |i: usize| valores_de(texto(ranking_bytes(&casos()[i].lote)), "taxa_acaso")[0];
    assert_eq!(acaso(0), "0.600000000");
    assert_eq!(acaso(1), "0.100000000");
    assert_eq!(acaso(2), "0.062500000");
}

#[test]
fn posicao_indice_do_ranking_bate_com_posicao_da_tabela_geral() {
    for c in casos() {
        let r = json(ranking_bytes(&c.lote));
        let t = json(&c.lote.tabela_geral);
        for e in r["dezenas"].as_array().expect("lista") {
            let na_tabela = t["dezenas"]
                .as_array()
                .expect("lista")
                .iter()
                .find(|d| d["dezena"] == e["dezena"])
                .expect("dezena na tabela");
            assert_eq!(e["posicao_indice"], na_tabela["posicao"], "{}", c.slug);
        }
    }
}

#[test]
fn posicao_a_e_posicao_b_sao_permutacoes_de_1_a_n() {
    for c in casos() {
        let n = u64::from(cfg_de(c.slug).qtd_dezenas_possiveis());
        let v = json(ranking_bytes(&c.lote));
        for chave in ["posicao_a", "posicao_b", "posicao_indice"] {
            let mut posicoes: Vec<u64> = v["dezenas"]
                .as_array()
                .expect("lista")
                .iter()
                .map(|d| d[chave].as_u64().expect(chave))
                .collect();
            posicoes.sort_unstable();
            assert_eq!(posicoes, (1..=n).collect::<Vec<_>>(), "{} {chave}", c.slug);
        }
    }
}

#[test]
fn notas_e_taxa_acaso_saem_com_exatamente_9_casas() {
    for c in casos() {
        let t = texto(ranking_bytes(&c.lote));
        let n = cfg_de(c.slug).qtd_dezenas_possiveis() as usize;
        for (chave, quantos) in [
            ("taxa_acaso", 1),
            ("nota_a", n),
            ("diferenca_acaso", n),
            ("nota_b", n),
        ] {
            let valores = valores_de(t, chave);
            assert_eq!(valores.len(), quantos, "{} {chave}", c.slug);
            for v in valores {
                let (inteira, fracao) = v
                    .split_once('.')
                    .unwrap_or_else(|| panic!("sem ponto: {chave} {v}"));
                assert_eq!(fracao.len(), 9, "{} {chave}: {v}", c.slug);
                assert!(fracao.bytes().all(|b| b.is_ascii_digit()), "{v}");
                assert!(
                    inteira
                        .trim_start_matches('-')
                        .bytes()
                        .all(|b| b.is_ascii_digit()),
                    "{v}"
                );
                assert!(!v.contains(['e', 'E']), "expoente em {v}");
                assert_ne!(v, "-0.000000000", "menos zero em {chave}");
            }
        }
    }
}

#[test]
fn ranking_json_e_ascii_com_lf_e_termina_em_um_unico_lf() {
    for c in casos() {
        let b = ranking_bytes(&c.lote);
        assert!(b.is_ascii(), "{}", c.slug);
        assert!(!b.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM");
        assert!(!b.contains(&b'\r'), "CR em {}", c.slug);
        assert!(!b.contains(&b'\t'), "tab em {}", c.slug);
        assert!(b.ends_with(b"\n") && !b.ends_with(b"\n\n"), "{}", c.slug);
    }
}

#[test]
fn index_traz_ranking_taxa_por_ultimo_com_sha256_e_bytes_do_disco() {
    let p = Pasta::nova("idx_ranking");
    gravar_tudo(&p.saida(), &lotes_com_ranking(), GERADO_EM);
    let bytes_index = ler(&p.saida(), "index.json");
    let idx = json(&bytes_index);

    // Ordem no texto: as chaves de `arquivos` (recuo 8) de cada loteria.
    let ordem = chaves_com_recuo(texto(&bytes_index), 8);
    let uma = ["ultimo", "tabela_geral", "historico", "ranking_taxa"];
    assert_eq!(ordem, uma.repeat(3));

    for slug in SLUGS {
        let arquivos = &idx["loterias"][slug]["arquivos"];
        assert_eq!(arquivos.as_object().expect("objeto").len(), 4, "{slug}");
        for chave in uma {
            let a = &arquivos[chave];
            let caminho = a["caminho"].as_str().expect("caminho");
            assert_eq!(caminho, format!("{slug}/{chave}.json"));
            let no_disco = ler(&p.saida(), caminho);
            assert_eq!(a["sha256"].as_str(), Some(sha256(&no_disco).as_str()));
            assert_eq!(a["bytes"], no_disco.len() as u64, "{caminho}");
        }
    }
    // 3 loterias x 4 arquivos + index, sem sobras.
    assert_eq!(listar(&p.saida()).len(), 13);
}

#[test]
fn ranking_gravado_no_disco_tem_os_mesmos_bytes_gerados() {
    let p = Pasta::nova("bytes_ranking");
    gravar_tudo(&p.saida(), &lotes_com_ranking(), GERADO_EM);
    for c in casos() {
        assert_eq!(
            ler(&p.saida(), &format!("{}/ranking_taxa.json", c.slug)),
            ranking_bytes(&c.lote),
            "{}",
            c.slug
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Ranking ausente (sem `sync ranking`).
// ---------------------------------------------------------------------------

/// `index.json` no formato de antes do ranking: só as três chaves, sem
/// `ranking_taxa`, só a quina com arquivos (as outras `null`).
fn index_antigo_so_da_quina(l: &ArquivosLoteria, data: &str, ultimo: u32) -> String {
    let arq = |chave: &str, bytes: &[u8], fim: &str| {
        format!(
            "        \"{chave}\": {{\n          \"caminho\": \"quina/{chave}.json\",\n          \
             \"sha256\": \"{}\",\n          \"bytes\": {}\n        }}{fim}\n",
            sha256(bytes),
            bytes.len()
        )
    };
    format!(
        "{{\n  \"schema_version\": 1,\n  \"gerado_em\": \"{GERADO_EM}\",\n  \"loterias\": {{\n    \
         \"lotofacil\": null,\n    \"megasena\": null,\n    \"quina\": {{\n      \
         \"ultimo_concurso\": {ultimo},\n      \"data\": \"{data}\",\n      \"arquivos\": {{\n{}{}{}      }}\n    }}\n  }}\n}}\n",
        arq("ultimo", &l.ultimo, ","),
        arq("tabela_geral", &l.tabela_geral, ","),
        arq("historico", &l.historico, ""),
    )
}

#[test]
fn sem_tabela_ranking_a_loteria_sai_sem_ranking_com_motivo_sem_tabela() {
    for slug in SLUGS {
        let cfg = cfg_de(slug);
        let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
        let dados = dados_de(&mut conn, slug);
        assert_eq!(
            dados.ranking,
            RankingParaExportar::Omitido(MotivoRankingOmitido::SemTabela)
        );
        let lote = exportacao::gerar_loteria(&dados).expect("arquivos");
        assert!(lote.ranking_taxa.is_none(), "{slug}");
        assert_eq!(lote.ranking_omitido, Some(MotivoRankingOmitido::SemTabela));
    }
}

#[test]
fn sem_ranking_os_tres_arquivos_obrigatorios_sao_os_de_sempre() {
    for slug in SLUGS {
        let cfg = cfg_de(slug);
        let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
        let dados = dados_de(&mut conn, slug);
        let lote = exportacao::gerar_loteria(&dados).expect("arquivos");
        // Mesmos bytes que as funções por arquivo dão para o mesmo banco.
        assert_eq!(
            lote.ultimo,
            exportacao::ultimo_json(&dados).unwrap().into_bytes()
        );
        assert_eq!(
            lote.tabela_geral,
            exportacao::tabela_geral_json(&dados).unwrap().into_bytes()
        );
        assert_eq!(
            lote.historico,
            exportacao::historico_json(&dados).unwrap().into_bytes()
        );
        // E iguais aos de um banco que tem ranking em dia: o ranking não mexe
        // nos outros arquivos.
        let com = casos().iter().find(|c| c.slug == slug).expect("caso");
        assert_eq!(lote.ultimo, com.lote.ultimo, "{slug}");
        assert_eq!(lote.tabela_geral, com.lote.tabela_geral, "{slug}");
        assert_eq!(lote.historico, com.lote.historico, "{slug}");
    }
}

#[test]
fn sem_ranking_a_gravacao_nao_cria_o_arquivo_e_o_index_nao_tem_a_chave() {
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
    let lote = lote_de(&mut conn, "quina");
    let p = Pasta::nova("sem_ranking");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);

    assert!(!existe(&p.saida(), "quina/ranking_taxa.json"));
    assert_eq!(listar(&p.saida()).len(), 4); // 3 arquivos + index

    let bytes = ler(&p.saida(), "index.json");
    assert!(!texto(&bytes).contains("ranking_taxa"));
    let idx = json(&bytes);
    let arquivos = idx["loterias"]["quina"]["arquivos"]
        .as_object()
        .expect("objeto");
    // Chave ausente, não `null`.
    assert!(!arquivos.contains_key("ranking_taxa"));
    assert_eq!(arquivos.len(), 3);
}

#[test]
fn sem_ranking_o_index_e_identico_ao_formato_de_antes_da_mudanca() {
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
    let lote = lote_de(&mut conn, "quina");
    let p = Pasta::nova("index_antigo");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);
    // Concurso 12: data 2020-01-(1 + 12 % 28) = 2020-01-13.
    let esperado = index_antigo_so_da_quina(&lote, "2020-01-13", N);
    assert_eq!(texto(&ler(&p.saida(), "index.json")), esperado);
}

// ---------------------------------------------------------------------------
// 3. Ranking atrasado, sem linhas, incompleto, divergente.
// ---------------------------------------------------------------------------

#[test]
fn ranking_atrasado_apos_concurso_novo_e_omitido_com_os_numeros_reais() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com_ranking(cfg, &hist[..N as usize]);
    // Concurso 13 salvo; features e índice em dia; ranking não.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");

    // O ranking gravado é do próximo concurso de antes (13); o snapshot é do 14.
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoRankingOmitido::Atrasado {
            gravado: N + 1,
            esperado: N + 2
        }
    );
    let lote = lote_de(&mut conn, "lotofacil");
    assert!(lote.ranking_taxa.is_none());
    // O resto é exportado, do concurso novo.
    assert_eq!(json(&lote.ultimo)["concurso"], N + 1);
    assert_eq!(json(&lote.tabela_geral)["concurso"], N + 2);

    let p = Pasta::nova("atrasado");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);
    assert!(!existe(&p.saida(), "lotofacil/ranking_taxa.json"));
    let idx = json(&ler(&p.saida(), "index.json"));
    let arquivos = idx["loterias"]["lotofacil"]["arquivos"]
        .as_object()
        .expect("objeto");
    assert!(!arquivos.contains_key("ranking_taxa"));
}

#[test]
fn tabela_sem_linhas_do_perfil_e_omitida_como_sem_linhas() {
    let cfg = cfg_de("quina");
    let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
    // As linhas passam a ser de outro perfil: a tabela existe, mas vazia
    // para `padrao_v1`.
    conn.execute("UPDATE ranking_taxa SET perfil = 'outro_v1'", [])
        .expect("update");
    assert_eq!(
        motivo_de(&mut conn, "quina"),
        MotivoRankingOmitido::SemLinhas {
            perfil: "padrao_v1".into()
        }
    );
    assert!(lote_de(&mut conn, "quina").ranking_taxa.is_none());
}

#[test]
fn dezena_apagada_do_ranking_e_omitida_como_incompleto() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
    conn.execute("DELETE FROM ranking_taxa WHERE dezena = 7", [])
        .expect("delete");
    // 25 dezenas menos a 7.
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoRankingOmitido::Incompleto {
            linhas: 24,
            esperadas: 25
        }
    );
    assert!(lote_de(&mut conn, "lotofacil").ranking_taxa.is_none());
}

#[test]
fn posicao_indice_adulterada_e_omitida_como_diverge_do_indice() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
    let dados = dados_de(&mut conn, "lotofacil");
    let verdadeira = dados
        .tabela
        .linhas
        .iter()
        .find(|l| l.dezena == 1)
        .expect("dezena 1")
        .posicao;
    conn.execute(
        "UPDATE ranking_taxa SET posicao_indice = posicao_indice + 100 WHERE dezena = 1",
        [],
    )
    .expect("update");
    // A dezena 1 é a primeira conferida (as dezenas vão em ordem).
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoRankingOmitido::DivergeDoIndice {
            dezena: 1,
            no_ranking: verdadeira + 100,
            no_indice: verdadeira
        }
    );
    assert!(lote_de(&mut conn, "lotofacil").ranking_taxa.is_none());
}

/// Posição de cada dezena no snapshot (último + 1), em ordem de dezena.
fn posicoes_do_snapshot(conn: &mut Connection, slug: &str) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = dados_de(conn, slug)
        .tabela
        .linhas
        .iter()
        .map(|l| (l.dezena, l.posicao))
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn resultado_antigo_corrigido_sem_refazer_o_ranking_omite_ate_o_sync_ranking() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let mut conn = conn_com_ranking(cfg, &hist);
    let antes = posicoes_do_snapshot(&mut conn, "lotofacil");
    assert!(lote_de(&mut conn, "lotofacil").ranking_taxa.is_some());

    // Procura uma correção de resultado antigo (último igual) que NÃO mude as
    // posições do snapshot: o cenário em que só a checagem das posições
    // deixaria passar um ranking velho. Só usa o índice (barato).
    let mut corrigido = None;
    'busca: for c in 0..(N as usize - 1) {
        for nova in cfg.dezenas() {
            if hist[c].dezenas.contains(&nova) {
                continue;
            }
            let mut h = hist.clone();
            h[c].dezenas[0] = nova;
            h[c].dezenas.sort_unstable();
            let mut tentativa = conn_sem_ranking(cfg, &hist);
            db::salvar_resultados(&mut tentativa, &h[c..=c]).expect("correção");
            sincronizar_features(&mut tentativa, cfg, true).expect("features");
            sincronizar_indice(&mut tentativa, cfg, &perfil_de("lotofacil"), GERADO_EM)
                .expect("índice");
            if posicoes_do_snapshot(&mut tentativa, "lotofacil") == antes {
                corrigido = Some(h);
                break 'busca;
            }
        }
    }
    let h = corrigido.expect("alguma correção deixa as posições do snapshot iguais");

    // Aplica a correção no banco que tem o ranking, refaz features e índice,
    // mas não o ranking.
    let c = h
        .iter()
        .zip(&hist)
        .position(|(a, b)| a != b)
        .expect("uma linha mudou");
    db::salvar_resultados(&mut conn, &h[c..=c]).expect("correção");
    sincronizar_features(&mut conn, cfg, true).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    assert_eq!(posicoes_do_snapshot(&mut conn, "lotofacil"), antes);

    let MotivoRankingOmitido::GeradoDeOutroEstado { dezena } = motivo_de(&mut conn, "lotofacil")
    else {
        panic!("esperava GeradoDeOutroEstado");
    };
    assert!(cfg.dezenas().any(|d| d == dezena));
    assert!(MotivoRankingOmitido::GeradoDeOutroEstado { dezena }
        .to_string()
        .contains("sync ranking"));

    // Os demais arquivos saem; o ranking não, e a chave some do index.json.
    let lote = lote_de(&mut conn, "lotofacil");
    assert!(lote.ranking_taxa.is_none());
    let p = Pasta::nova("corrigido");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);
    for f in ["ultimo.json", "tabela_geral.json", "historico.json"] {
        assert!(existe(&p.saida(), &format!("lotofacil/{f}")), "{f}");
    }
    assert!(!existe(&p.saida(), "lotofacil/ranking_taxa.json"));
    let idx = json(&ler(&p.saida(), "index.json"));
    let arquivos = idx["loterias"]["lotofacil"]["arquivos"]
        .as_object()
        .expect("objeto");
    assert!(!arquivos.contains_key("ranking_taxa"));

    // Refeito o ranking, volta a publicar.
    ranquear(&mut conn, cfg);
    assert!(lote_de(&mut conn, "lotofacil").ranking_taxa.is_some());
}

#[test]
fn linhas_de_concursos_anteriores_no_ranking_nao_atrapalham_o_em_dia() {
    // O ranking guarda o histórico (um retrato por concurso); só o do próximo conta.
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let mut conn = conn_sem_ranking(cfg, &hist[..N as usize - 1]);
    ranquear(&mut conn, cfg); // ranking do concurso 12
    db::salvar_resultados(&mut conn, &hist[N as usize - 1..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    ranquear(&mut conn, cfg); // ranking do concurso 13
    let linhas = db::ler_ranking_taxa(&conn, "padrao_v1").expect("linhas");
    // Retratos dos concursos 1 a 13, 25 dezenas cada.
    assert_eq!(linhas.len(), 13 * 25);
    let dados = dados_de(&mut conn, "lotofacil");
    assert!(matches!(dados.ranking, RankingParaExportar::EmDia(_)));
}

// ---------------------------------------------------------------------------
// 4. Remoção do arquivo antigo.
// ---------------------------------------------------------------------------

#[test]
fn ranking_que_fica_atrasado_apaga_o_arquivo_antigo_e_a_chave() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com_ranking(cfg, &hist[..N as usize]);
    let p = Pasta::nova("remocao");

    // 1) Em dia: o arquivo existe.
    let em_dia = lote_de(&mut conn, "lotofacil");
    gravar_tudo(&p.saida(), std::slice::from_ref(&em_dia), GERADO_EM);
    assert!(existe(&p.saida(), "lotofacil/ranking_taxa.json"));
    assert!(
        json(&ler(&p.saida(), "index.json"))["loterias"]["lotofacil"]["arquivos"]["ranking_taxa"]
            .is_object()
    );

    // 2) Concurso novo, ranking não refeito: o arquivo some.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    let atrasado = lote_de(&mut conn, "lotofacil");
    assert!(atrasado.ranking_taxa.is_none());
    let g = disco::gravar_com_relatorio(&p.saida(), &[atrasado], &SLUGS, GERADO_EM)
        .expect("regravação");

    let antigo = disco::pasta_versao(&p.saida())
        .join("lotofacil")
        .join("ranking_taxa.json");
    assert_eq!(g.removidos, vec![antigo.clone()]);
    assert!(!antigo.exists());
    let idx = json(&ler(&p.saida(), "index.json"));
    let arquivos = idx["loterias"]["lotofacil"]["arquivos"]
        .as_object()
        .expect("objeto");
    assert!(!arquivos.contains_key("ranking_taxa"));
    assert_eq!(idx["loterias"]["lotofacil"]["ultimo_concurso"], N + 1);
    // 3 arquivos + index, e nenhum temporário.
    let sobras = listar(&p.saida());
    assert_eq!(sobras.len(), 4, "{sobras:?}");
    assert!(sobras
        .iter()
        .all(|f| f.extension().is_some_and(|e| e == "json")));
    // O index que `gravar_com_relatorio` devolve é o último gravado.
    assert_eq!(
        g.gravados.last().expect("index").caminho,
        disco::pasta_versao(&p.saida()).join("index.json")
    );

    // 3) Ranking refeito: o arquivo volta, com a chave.
    ranquear(&mut conn, cfg);
    let de_novo = lote_de(&mut conn, "lotofacil");
    let g = disco::gravar_com_relatorio(
        &p.saida(),
        std::slice::from_ref(&de_novo),
        &SLUGS,
        GERADO_EM,
    )
    .expect("regravação");
    assert!(g.removidos.is_empty());
    assert_eq!(
        ler(&p.saida(), "lotofacil/ranking_taxa.json"),
        ranking_bytes(&de_novo)
    );
    let idx = json(&ler(&p.saida(), "index.json"));
    let r = &idx["loterias"]["lotofacil"]["arquivos"]["ranking_taxa"];
    assert_eq!(r["caminho"], "lotofacil/ranking_taxa.json");
    assert_eq!(
        r["sha256"].as_str(),
        Some(sha256(ranking_bytes(&de_novo)).as_str())
    );
    assert_eq!(listar(&p.saida()).len(), 5);
}

#[test]
fn remover_arquivo_que_nao_existe_nao_e_erro_e_nao_aparece_em_removidos() {
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
    let lote = lote_de(&mut conn, "quina");
    let p = Pasta::nova("remocao_vazia");
    let g = disco::gravar_com_relatorio(&p.saida(), &[lote], &SLUGS, GERADO_EM).expect("grava");
    assert!(g.removidos.is_empty());
}

#[test]
fn loteria_preservada_mantem_o_ranking_e_a_chave_no_index_refeito() {
    let p = Pasta::nova("preserva");
    gravar_tudo(&p.saida(), &lotes_com_ranking(), GERADO_EM);
    let antes = ler(&p.saida(), "megasena/ranking_taxa.json");

    // Regrava só a quina, agora sem ranking (omitido): a megasena e a
    // lotofácil não são tocadas.
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_ranking(cfg, &historico(cfg, N));
    let quina = lote_de(&mut conn, "quina");
    let g = disco::gravar_com_relatorio(&p.saida(), &[quina], &SLUGS, "2030-01-01T00:00:00Z")
        .expect("regravação");

    assert_eq!(
        g.removidos,
        vec![disco::pasta_versao(&p.saida())
            .join("quina")
            .join("ranking_taxa.json")]
    );
    assert_eq!(ler(&p.saida(), "megasena/ranking_taxa.json"), antes);
    assert!(existe(&p.saida(), "lotofacil/ranking_taxa.json"));
    assert!(!existe(&p.saida(), "quina/ranking_taxa.json"));

    let idx = json(&ler(&p.saida(), "index.json"));
    assert_eq!(idx["gerado_em"], "2030-01-01T00:00:00Z");
    for slug in ["lotofacil", "megasena"] {
        let r = &idx["loterias"][slug]["arquivos"]["ranking_taxa"];
        assert_eq!(r["bytes"], ranking_bytes(&casos_lote(slug)).len() as u64);
        assert_eq!(
            r["sha256"].as_str(),
            Some(sha256(ranking_bytes(&casos_lote(slug))).as_str()),
            "{slug}"
        );
    }
    assert!(idx["loterias"]["quina"]["arquivos"]
        .get("ranking_taxa")
        .is_none());
}

fn casos_lote(slug: &str) -> ArquivosLoteria {
    casos()
        .iter()
        .find(|c| c.slug == slug)
        .expect("caso")
        .lote
        .clone()
}

// ---------------------------------------------------------------------------
// 5. exportar_com_relatorio e exportar_tudo_ou_nada.
// ---------------------------------------------------------------------------

fn perfis_em(pasta: &Pasta) -> PathBuf {
    let raiz = pasta.0.join("perfis");
    for slug in SLUGS {
        let dir = raiz.join(slug);
        std::fs::create_dir_all(&dir).expect("pasta de perfis");
        std::fs::write(dir.join("padrao_v1.toml"), texto_perfil(slug)).expect("perfil");
    }
    raiz
}

/// Banco em disco da loteria, com features e índice e, se pedido, ranking.
fn banco_em(pasta: &Pasta, slug: &str, com_ranking: bool) -> PathBuf {
    let cfg = cfg_de(slug);
    let caminho = pasta.0.join(format!("{slug}.db"));
    let mut conn = db::abrir(&caminho).expect("banco");
    preparar(&mut conn, cfg, &historico(cfg, N));
    if com_ranking {
        ranquear(&mut conn, cfg);
    }
    caminho
}

fn alvo(slug: &str, db: PathBuf) -> (&'static LoteriaConfig, PathBuf) {
    (cfg_de(slug), db)
}

fn relogio() -> rust_spider::Result<String> {
    Ok(GERADO_EM.to_string())
}

#[test]
fn relatorio_traz_publicado_e_omitido_na_ordem_dos_alvos() {
    let p = Pasta::nova("relatorio");
    let perfis = perfis_em(&p);
    let alvos = vec![
        alvo("quina", banco_em(&p, "quina", false)),
        alvo("lotofacil", banco_em(&p, "lotofacil", true)),
    ];
    let r = exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("exportação");
    let ResultadoComRelatorio::Gravado(rel) = r else {
        panic!("esperava Gravado, veio {r:?}");
    };
    assert_eq!(
        rel.rankings,
        vec![
            RankingDaLoteria {
                slug: "quina",
                situacao: SituacaoRanking::Omitido(MotivoRankingOmitido::SemTabela)
            },
            RankingDaLoteria {
                slug: "lotofacil",
                situacao: SituacaoRanking::Publicado
            },
        ]
    );
    assert!(rel.removidos.is_empty());
    // Quina: 3 arquivos; lotofácil: 4; mais o index.
    assert_eq!(rel.gravados.len(), 8);
    assert_eq!(listar(&p.saida()).len(), 8);
    assert!(existe(&p.saida(), "lotofacil/ranking_taxa.json"));
    assert!(!existe(&p.saida(), "quina/ranking_taxa.json"));
}

#[test]
fn exportar_tudo_ou_nada_grava_a_mesma_lista_de_arquivos_do_relatorio() {
    let p = Pasta::nova("tn_mesma_lista");
    let perfis = perfis_em(&p);
    let alvos = vec![
        alvo("lotofacil", banco_em(&p, "lotofacil", true)),
        alvo("quina", banco_em(&p, "quina", false)),
    ];
    let (saida_a, saida_b) = (p.0.join("a"), p.0.join("b"));
    let ResultadoComRelatorio::Gravado(rel) =
        exportar_com_relatorio(&alvos, &perfis, &saida_a, relogio).expect("relatório")
    else {
        panic!("esperava Gravado");
    };
    let ResultadoExportacao::Gravado(gravados) =
        exportar_tudo_ou_nada(&alvos, &perfis, &saida_b, relogio).expect("tudo ou nada")
    else {
        panic!("esperava Gravado");
    };
    assert_eq!(gravados.len(), 8);
    // Mesmos caminhos relativos e mesmas impressões (sha256 e bytes).
    let relativos = |raiz: &Path, l: &[disco::ArquivoGravado]| -> Vec<(PathBuf, String, u64)> {
        l.iter()
            .map(|g| {
                (
                    g.caminho
                        .strip_prefix(raiz)
                        .expect("sob a saída")
                        .to_path_buf(),
                    g.impressao.sha256.clone(),
                    g.impressao.bytes,
                )
            })
            .collect()
    };
    assert_eq!(
        relativos(&saida_a, &rel.gravados),
        relativos(&saida_b, &gravados)
    );
    // O index vem por último.
    assert!(gravados
        .last()
        .expect("index")
        .caminho
        .ends_with("index.json"));
}

#[test]
fn ranking_omitido_nao_cancela_a_exportacao_de_nenhuma_loteria() {
    let p = Pasta::nova("nao_cancela");
    let perfis = perfis_em(&p);
    let alvos: Vec<_> = SLUGS
        .iter()
        .map(|s| alvo(s, banco_em(&p, s, false)))
        .collect();
    let r = exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("exportação");
    let ResultadoComRelatorio::Gravado(rel) = r else {
        panic!("esperava Gravado, veio {r:?}");
    };
    assert_eq!(rel.rankings.len(), 3);
    assert!(rel
        .rankings
        .iter()
        .all(|r| r.situacao == SituacaoRanking::Omitido(MotivoRankingOmitido::SemTabela)));
    // 3 loterias x 3 arquivos + index.
    assert_eq!(listar(&p.saida()).len(), 10);
}

#[test]
fn ranking_atrasado_no_banco_em_disco_apaga_o_arquivo_pela_exportacao() {
    let p = Pasta::nova("atrasado_disco");
    let perfis = perfis_em(&p);
    let db = banco_em(&p, "lotofacil", true);
    let alvos = vec![alvo("lotofacil", db.clone())];
    exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("1ª");
    assert!(existe(&p.saida(), "lotofacil/ranking_taxa.json"));

    // Concurso novo com features e índice, mas sem ranking.
    {
        let cfg = cfg_de("lotofacil");
        let mut conn = db::abrir(&db).expect("banco");
        let hist = historico(cfg, N + 1);
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
        sincronizar_features(&mut conn, cfg, false).expect("features");
        sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    }
    let ResultadoComRelatorio::Gravado(rel) =
        exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("2ª")
    else {
        panic!("esperava Gravado");
    };
    assert_eq!(
        rel.rankings[0].situacao,
        SituacaoRanking::Omitido(MotivoRankingOmitido::Atrasado {
            gravado: N + 1,
            esperado: N + 2
        })
    );
    assert_eq!(rel.removidos.len(), 1);
    assert!(rel.removidos[0].ends_with("ranking_taxa.json"));
    assert!(!existe(&p.saida(), "lotofacil/ranking_taxa.json"));
}

// ---------------------------------------------------------------------------
// 6. Determinismo.
// ---------------------------------------------------------------------------

#[test]
fn duas_exportacoes_do_mesmo_banco_dao_os_mesmos_bytes_nos_quatro_arquivos() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
    let (a, b) = (
        lote_de(&mut conn, "lotofacil"),
        lote_de(&mut conn, "lotofacil"),
    );
    assert_eq!(a, b);

    let (pa, pb) = (Pasta::nova("det_a"), Pasta::nova("det_b"));
    disco::gravar(&pa.saida(), &[a], &SLUGS, "2026-10-05T10:00:00Z").expect("a");
    disco::gravar(&pb.saida(), &[b], &SLUGS, "2030-01-01T00:00:00Z").expect("b");
    for nome in [
        "ultimo.json",
        "tabela_geral.json",
        "historico.json",
        "ranking_taxa.json",
    ] {
        let rel = format!("lotofacil/{nome}");
        assert_eq!(ler(&pa.saida(), &rel), ler(&pb.saida(), &rel), "{rel}");
    }
}

#[test]
fn ranking_regerado_do_zero_no_banco_novo_da_o_mesmo_json() {
    // Outro banco em memória, mesma entrada: mesmo ranking (a semente é fixa).
    let cfg = cfg_de("quina");
    let mut conn = conn_com_ranking(cfg, &historico(cfg, N));
    let outro = lote_de(&mut conn, "quina");
    assert_eq!(
        ranking_bytes(&outro),
        ranking_bytes(&casos_lote("quina")),
        "ranking_taxa.json deve ser determinístico"
    );
}

#[test]
fn index_de_duas_exportacoes_com_ranking_so_difere_em_gerado_em() {
    let (a, b) = (Pasta::nova("idx_a"), Pasta::nova("idx_b"));
    gravar_tudo(&a.saida(), &lotes_com_ranking(), "2026-10-05T10:00:00Z");
    gravar_tudo(&b.saida(), &lotes_com_ranking(), "2030-01-01T00:00:00Z");
    let (ia, ib) = (ler(&a.saida(), "index.json"), ler(&b.saida(), "index.json"));
    assert_ne!(ia, ib);
    let ta = texto(&ia).replace("2026-10-05T10:00:00Z", "X");
    let tb = texto(&ib).replace("2030-01-01T00:00:00Z", "X");
    assert_eq!(ta, tb);
}

// ---------------------------------------------------------------------------
// 7. CLI: `sync exportar`.
// ---------------------------------------------------------------------------

fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_exportar(alvo: &str, saida: &Path, perfis: &Path, db: &Path) -> Vec<OsString> {
    vec![
        "exportar".into(),
        alvo.into(),
        "--saida".into(),
        saida.into(),
        "--perfis".into(),
        perfis.into(),
        "--db".into(),
        db.into(),
    ]
}

/// Pasta com o banco da lotofácil (com ou sem ranking) e os perfis.
fn ambiente_cli(rotulo: &str, com_ranking: bool) -> (Pasta, PathBuf, PathBuf) {
    let p = Pasta::nova(rotulo);
    let perfis = perfis_em(&p);
    let db = banco_em(&p, "lotofacil", com_ranking);
    (p, perfis, db)
}

#[test]
fn cli_sem_ranking_avisa_que_o_ranking_foi_omitido_e_termina_com_sucesso() {
    let (p, perfis, db) = ambiente_cli("cli_sem", false);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &db));
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(out.status.success(), "{stderr}");
    assert!(
        stdout.contains("Aviso: lotofacil: ranking_taxa.json omitido"),
        "{stdout}"
    );
    assert!(stdout.contains("sync ranking"), "{stdout}");
    assert!(!existe(&saida, "lotofacil/ranking_taxa.json"));
    assert_eq!(listar(&saida).len(), 4);
}

#[test]
fn cli_com_ranking_nao_avisa_e_lista_o_ranking_taxa_json() {
    let (p, perfis, db) = ambiente_cli("cli_com", true);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &db));
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(out.status.success(), "{stderr}");
    assert!(!stdout.contains("omitido"), "{stdout}");
    assert!(!stdout.contains("Aviso"), "{stdout}");
    assert!(stdout.contains("ranking_taxa.json"), "{stdout}");
    assert!(existe(&saida, "lotofacil/ranking_taxa.json"));
    assert_eq!(listar(&saida).len(), 5);
    // O arquivo da CLI é o da biblioteca.
    assert_eq!(
        ler(&saida, "lotofacil/ranking_taxa.json"),
        ranking_bytes(&casos_lote("lotofacil"))
    );
}

// ---------------------------------------------------------------------------
// 8. Borda do JSON: nada de `-0`.
// ---------------------------------------------------------------------------

fn dados_a_mao(linhas: Vec<LinhaRankingTaxa>) -> (DadosExportacao, RankingProximo) {
    let ranking = RankingProximo {
        qtd_sorteadas: 5,
        qtd_dezenas: 80,
        linhas,
    };
    let dados = DadosExportacao {
        tabela: TabelaConcurso {
            loteria: "quina".into(),
            perfil: "padrao_v1".into(),
            janela: 10,
            concurso: 3,
            ultimo_sorteado: 2,
            data_sorteio: None,
            linhas: vec![],
        },
        hash_toml: "abc".into(),
        normalizacao: Normalizacao::Minmax,
        feature_version: 1,
        resultados: vec![],
        ranking: RankingParaExportar::Omitido(MotivoRankingOmitido::SemTabela),
    };
    (dados, ranking)
}

fn linha(dezena: u32, nota_a: f64, nota_b: f64) -> LinhaRankingTaxa {
    LinhaRankingTaxa {
        concurso: 3,
        dezena,
        posicao_indice: dezena,
        taxa_posicao: nota_a,
        nota_b,
        posicao_ranking_a: dezena,
        posicao_ranking_b: dezena,
    }
}

#[test]
fn json_nao_escreve_menos_zero_em_nota_b_nem_em_diferenca_acaso() {
    // K/N = 5/80 = 0.0625. As diferenças (+-3e-10) arredondam para zero.
    let acaso = 0.0625;
    let (dados, ranking) = dados_a_mao(vec![
        linha(3, acaso - 3e-10, -1e-10),
        linha(1, acaso + 3e-10, -0.0),
        linha(2, acaso - 0.01, -0.5),
    ]);
    let t = exportacao::ranking_taxa_json(&dados, &ranking).expect("json");
    assert!(!t.contains("-0.000000000"), "{t}");
    assert!(!t.contains("\"-0"), "{t}");
    // Entradas em ordem crescente de dezena, mesmo com a entrada fora de ordem.
    assert_eq!(valores_de(&t, "dezena"), ["1", "2", "3"]);
    // dezena 1: +3e-10 e -0.0; dezena 2: -0.01 e -0.5; dezena 3: -3e-10 e -1e-10.
    assert_eq!(
        valores_de(&t, "diferenca_acaso"),
        ["0.000000000", "-0.010000000", "0.000000000"]
    );
    assert_eq!(
        valores_de(&t, "nota_b"),
        ["0.000000000", "-0.500000000", "0.000000000"]
    );
    assert_eq!(valores_de(&t, "taxa_acaso"), ["0.062500000"]);
    assert!(t.is_ascii() && t.ends_with("}\n"));
    // O arquivo continua sendo JSON válido.
    json(t.as_bytes());
}

#[test]
fn json_recusa_linha_de_ranking_de_outro_concurso() {
    let mut errada = linha(1, 0.06, 0.1);
    errada.concurso = 2;
    let (dados, ranking) = dados_a_mao(vec![errada]);
    match exportacao::ranking_taxa_json(&dados, &ranking) {
        Err(SpiderError::ExportacaoInvalida(m)) => {
            assert!(m.contains("concurso 2") && m.contains("do 3"), "{m}");
        }
        outro => panic!("esperava ExportacaoInvalida, veio {outro:?}"),
    }
}

#[test]
fn json_recusa_numero_nao_finito() {
    let (dados, ranking) = dados_a_mao(vec![linha(1, f64::NAN, 0.0)]);
    assert!(matches!(
        exportacao::ranking_taxa_json(&dados, &ranking),
        Err(SpiderError::ExportacaoInvalida(_))
    ));
}

// ---------------------------------------------------------------------------
// 9. Contrato escrito.
// ---------------------------------------------------------------------------

/// Nomes de chave de todos os objetos, sem repetir. Em `loterias` as chaves
/// são os slugs (dados, não campos do contrato): só os valores são percorridos.
fn chaves_do_contrato(v: &Value, pai: &str, achadas: &mut BTreeSet<String>) {
    match v {
        Value::Object(campos) => {
            for (chave, valor) in campos {
                if pai != "loterias" {
                    achadas.insert(chave.clone());
                }
                chaves_do_contrato(valor, chave, achadas);
            }
        }
        Value::Array(itens) => itens
            .iter()
            .for_each(|i| chaves_do_contrato(i, pai, achadas)),
        _ => {}
    }
}

#[test]
fn toda_chave_do_ranking_taxa_json_e_do_index_com_ranking_esta_no_contrato() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/contrato-v1.md"))
        .expect("docs/contrato-v1.md");
    let p = Pasta::nova("doc_ranking");
    gravar_tudo(&p.saida(), &lotes_com_ranking(), GERADO_EM);

    let mut chaves = BTreeSet::new();
    chaves_do_contrato(&json(&ler(&p.saida(), "index.json")), "", &mut chaves);
    for slug in SLUGS {
        let bytes = ler(&p.saida(), &format!("{slug}/ranking_taxa.json"));
        chaves_do_contrato(&json(&bytes), "", &mut chaves);
    }
    // Sanidade: as chaves novas estão entre as coletadas.
    for k in CHAVES_CABECALHO.iter().chain(&CHAVES_ENTRADA) {
        assert!(chaves.contains(*k), "chave `{k}` não coletada");
    }
    assert!(chaves.contains("ranking_taxa"));

    let ini = doc
        .find("## JSON público do app")
        .expect("cabeçalho '## JSON público do app' não encontrado no contrato");
    let fim = doc
        .find("## Dataset para o Python")
        .expect("cabeçalho '## Dataset para o Python' não encontrado no contrato");
    assert!(ini < fim, "seção do app deve vir antes da do dataset");
    let secao = &doc[ini..fim];
    let faltando: Vec<&String> = chaves
        .iter()
        .filter(|c| !secao.contains(&format!("| `{c}` |")))
        .collect();
    assert!(
        faltando.is_empty(),
        "chaves sem documentação em docs/contrato-v1.md: {faltando:?}"
    );
}
