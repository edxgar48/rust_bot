//! Testes do `ranking_posicoes.json` (tarefa 2.6), o segundo arquivo OPCIONAL de
//! cada loteria no `sync exportar` (G-1, índice de força das posições):
//! conteúdo e formato quando as tabelas `posicao_*` estão em dia, omissão (sem
//! tabela, sem estado, perfil não registrado, atrasado, marcas diferentes,
//! tabelas incoerentes, incompleto, divergente do cálculo), remoção do arquivo
//! antigo, o `index.json`, o relatório de `exportar_com_relatorio`, o
//! determinismo, a aditividade (os outros arquivos não mudam), a CLI e o
//! contrato escrito.
//!
//! Os helpers (histórico sintético, perfil, pasta temporária) são cópia dos de
//! `tests/exportar_ranking.rs`.

use rusqlite::{params, Connection};
use rust_spider::avaliacao::SEMENTE_ESPERADO;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::exportacao::{self, disco, ArquivosLoteria};
use rust_spider::orquestracao::{
    dados_para_exportar, exportar_com_relatorio, sincronizar_features, sincronizar_indice,
    sincronizar_posicoes, sincronizar_ranking_com, DadosExportacao, MotivoPosicoesOmitido,
    PosicoesParaExportar, PosicoesProximo, RelatorioPosicoes, ResultadoComRelatorio,
    SituacaoPosicoes, PERFIL_POSICOES,
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

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Histórico pequeno: o snapshot é do concurso 13, com janela 10.
const N: u32 = 12;
const GERADO_EM: &str = "2026-10-05T10:00:00Z";
const SLUGS: [&str; 3] = ["lotofacil", "megasena", "quina"];

const CHAVES_CABECALHO: [&str; 14] = [
    "schema_version",
    "loteria",
    "perfil",
    "hash_toml",
    "janela",
    "normalizacao",
    "feature_version",
    "concurso",
    "ultimo_sorteado",
    "perfil_posicoes",
    "hash_toml_posicoes",
    "janela_posicoes",
    "normalizacao_posicoes",
    "dezenas",
];
const CHAVES_ENTRADA: [&str; 4] = ["dezena", "posicao_indice", "g_1", "variacao_g_1"];

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

/// Perfil das posições: outro nome e outra normalização, para distinguir os
/// campos `*_posicoes` dos da tabela de força.
fn texto_posicoes(slug: &str) -> String {
    texto_perfil(slug)
        .replace("padrao_v1", PERFIL_POSICOES)
        .replace("minmax", "zscore")
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

fn posicoes_de(slug: &str) -> PerfilCarregado {
    perfil::interpretar(
        &texto_posicoes(slug),
        Path::new("perfis/teste/posicoes_v1.toml"),
    )
    .expect("perfil das posições")
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

/// Resultados, features e índice de força (sem ranking e sem posições).
fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
    sincronizar_indice(conn, cfg, &perfil_de(cfg.slug), GERADO_EM).expect("índice");
}

fn posicionar(conn: &mut Connection, cfg: &LoteriaConfig) -> RelatorioPosicoes {
    sincronizar_posicoes(
        conn,
        cfg,
        &perfil_de(cfg.slug),
        &posicoes_de(cfg.slug),
        GERADO_EM,
    )
    .expect("posições")
}

fn conn_sem_posicoes(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, cfg, hist);
    conn
}

fn conn_com_posicoes(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = conn_sem_posicoes(cfg, hist);
    posicionar(&mut conn, cfg);
    conn
}

fn dados_de(conn: &mut Connection, slug: &str) -> DadosExportacao {
    dados_para_exportar(conn, cfg_de(slug), &perfil_de(slug)).expect("dados")
}

fn motivo_de(conn: &mut Connection, slug: &str) -> MotivoPosicoesOmitido {
    match dados_de(conn, slug).posicoes {
        PosicoesParaExportar::Omitido(m) => m,
        PosicoesParaExportar::EmDia(_) => panic!("esperava as posições omitidas"),
    }
}

fn lote_de(conn: &mut Connection, slug: &str) -> ArquivosLoteria {
    exportacao::gerar_loteria(&dados_de(conn, slug)).expect("arquivos")
}

fn posicoes_bytes(lote: &ArquivosLoteria) -> &[u8] {
    lote.ranking_posicoes
        .as_deref()
        .expect("ranking_posicoes.json")
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

/// Chaves (entre aspas) das linhas com exatamente `recuo` espaços de recuo.
fn chaves_com_recuo(t: &str, recuo: usize) -> Vec<String> {
    t.lines()
        .filter(|l| l.len() - l.trim_start_matches(' ').len() == recuo)
        .filter_map(|l| l.trim_start().strip_prefix('"'))
        .filter_map(|l| l.split_once("\": ").map(|(k, _)| k.to_string()))
        .collect()
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
            "rust_spider_exportar_posicoes_{rotulo}_{}_{unico}_{n}",
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

/// As três loterias com as posições em dia: (slug, relatório do `sync
/// posicoes`, dados, arquivos).
fn casos() -> Vec<(
    &'static str,
    RelatorioPosicoes,
    DadosExportacao,
    ArquivosLoteria,
)> {
    SLUGS
        .iter()
        .map(|&slug| {
            let cfg = cfg_de(slug);
            let mut conn = conn_sem_posicoes(cfg, &historico(cfg, N));
            let relatorio = posicionar(&mut conn, cfg);
            let dados = dados_de(&mut conn, slug);
            let lote = exportacao::gerar_loteria(&dados).expect("arquivos");
            (slug, relatorio, dados, lote)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Posições em dia.
// ---------------------------------------------------------------------------

#[test]
fn posicoes_em_dia_marcam_dados_e_lote_como_publicados() {
    for (slug, _, dados, lote) in casos() {
        assert!(
            matches!(dados.posicoes, PosicoesParaExportar::EmDia(_)),
            "{slug}"
        );
        assert!(lote.ranking_posicoes.is_some(), "{slug}");
        assert!(lote.posicoes_omitido.is_none(), "{slug}");
    }
}

#[test]
fn json_tem_as_chaves_e_a_ordem_do_contrato() {
    for (slug, _, _, lote) in casos() {
        let t = texto(posicoes_bytes(&lote));
        assert_eq!(chaves_com_recuo(t, 2), CHAVES_CABECALHO, "{slug}");
        let entrada = chaves_com_recuo(t, 6);
        assert_eq!(
            entrada,
            CHAVES_ENTRADA.repeat(cfg_de(slug).qtd_dezenas_possiveis() as usize),
            "{slug}"
        );
        assert!(
            t.is_ascii() && t.ends_with("}\n") && !t.contains('\r'),
            "{slug}"
        );
    }
}

#[test]
fn cabecalho_repete_o_da_tabela_geral_e_traz_o_perfil_das_posicoes() {
    for (slug, _, _, lote) in casos() {
        let p = json(posicoes_bytes(&lote));
        let t = json(&lote.tabela_geral);
        for k in [
            "schema_version",
            "loteria",
            "perfil",
            "hash_toml",
            "janela",
            "normalizacao",
            "feature_version",
            "concurso",
            "ultimo_sorteado",
        ] {
            assert_eq!(p[k], t[k], "{slug}: {k}");
        }
        assert_eq!(p["concurso"], N + 1);
        assert_eq!(p["ultimo_sorteado"], N);
        assert_eq!(p["perfil_posicoes"], "posicoes_v1");
        assert_eq!(
            p["hash_toml_posicoes"].as_str(),
            Some(posicoes_de(slug).hash_toml.as_str())
        );
        assert_ne!(p["hash_toml_posicoes"], p["hash_toml"]);
        assert_eq!(p["janela_posicoes"], 10);
        assert_eq!(p["normalizacao_posicoes"], "zscore");
        assert_eq!(p["normalizacao"], "minmax");
    }
}

#[test]
fn uma_entrada_por_dezena_em_ordem_crescente_com_g_1_de_1_a_n() {
    for (slug, _, _, lote) in casos() {
        let cfg = cfg_de(slug);
        let p = json(posicoes_bytes(&lote));
        let dezenas = p["dezenas"].as_array().expect("lista");
        let ordem: Vec<u64> = dezenas
            .iter()
            .map(|d| d["dezena"].as_u64().unwrap())
            .collect();
        let esperado: Vec<u64> = cfg.dezenas().map(u64::from).collect();
        assert_eq!(ordem, esperado, "{slug}");
        let mut g: Vec<u64> = dezenas.iter().map(|d| d["g_1"].as_u64().unwrap()).collect();
        g.sort_unstable();
        let um_a_n: Vec<u64> = (1..=u64::from(cfg.qtd_dezenas_possiveis())).collect();
        assert_eq!(g, um_a_n, "{slug}");
    }
}

#[test]
fn posicao_indice_bate_com_a_tabela_geral_e_g_1_com_o_sync_posicoes() {
    for (slug, relatorio, _, lote) in casos() {
        let p = json(posicoes_bytes(&lote));
        let t = json(&lote.tabela_geral);
        let posicao_da = |dezena: u64| {
            t["dezenas"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["dezena"] == dezena)
                .map(|d| d["posicao"].clone())
                .expect("dezena na tabela")
        };
        for d in p["dezenas"].as_array().unwrap() {
            let dezena = d["dezena"].as_u64().unwrap();
            assert_eq!(d["posicao_indice"], posicao_da(dezena), "{slug} d{dezena}");
            // O relatório do `sync posicoes` é a referência do que foi gravado.
            let r = relatorio
                .proximo
                .iter()
                .find(|l| u64::from(l.dezena) == dezena)
                .expect("dezena no relatório");
            assert_eq!(d["g_1"], r.g_1, "{slug} d{dezena}");
            assert!(
                d["variacao_g_1"].is_i64(),
                "com histórico a variação existe: {slug} d{dezena}"
            );
        }
    }
}

#[test]
fn json_sem_variacao_escreve_null_e_recusa_g_1_repetido_ou_posicao_trocada() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    let dados = dados_de(&mut conn, "lotofacil");
    let PosicoesParaExportar::EmDia(base) = dados.posicoes.clone() else {
        panic!("em dia");
    };

    let mut sem_variacao: PosicoesProximo = base.clone();
    sem_variacao.linhas[0].variacao_g_1 = None;
    let t = exportacao::ranking_posicoes_json(&dados, &sem_variacao).unwrap();
    assert!(
        t.contains("\"g_1\": ") && t.contains("\"variacao_g_1\": null\n"),
        "{t}"
    );

    let mut repetido = base.clone();
    repetido.linhas[1].g_1 = repetido.linhas[0].g_1;
    assert!(matches!(
        exportacao::ranking_posicoes_json(&dados, &repetido),
        Err(SpiderError::ExportacaoInvalida(_))
    ));

    let mut trocada = base.clone();
    trocada.linhas[0].posicao_indice += 1;
    assert!(matches!(
        exportacao::ranking_posicoes_json(&dados, &trocada),
        Err(SpiderError::ExportacaoInvalida(_))
    ));

    let mut curta = base;
    curta.linhas.pop();
    assert!(matches!(
        exportacao::ranking_posicoes_json(&dados, &curta),
        Err(SpiderError::ExportacaoInvalida(_))
    ));
}

#[test]
fn index_traz_ranking_posicoes_com_sha256_e_bytes_do_disco() {
    let p = Pasta::nova("idx");
    let lotes: Vec<ArquivosLoteria> = casos().into_iter().map(|c| c.3).collect();
    gravar_tudo(&p.saida(), &lotes, GERADO_EM);
    let bytes_index = ler(&p.saida(), "index.json");
    let idx = json(&bytes_index);

    // Sem ranking por taxa: três chaves e a nova por último.
    let ordem = chaves_com_recuo(texto(&bytes_index), 8);
    let uma = ["ultimo", "tabela_geral", "historico", "ranking_posicoes"];
    assert_eq!(ordem, uma.repeat(3));

    for slug in SLUGS {
        let arquivos = &idx["loterias"][slug]["arquivos"];
        assert_eq!(arquivos.as_object().unwrap().len(), 4, "{slug}");
        let a = &arquivos["ranking_posicoes"];
        assert_eq!(a["caminho"], format!("{slug}/ranking_posicoes.json"));
        let no_disco = ler(&p.saida(), a["caminho"].as_str().unwrap());
        assert_eq!(a["sha256"].as_str(), Some(sha256(&no_disco).as_str()));
        assert_eq!(a["bytes"], no_disco.len() as u64);
    }
    assert_eq!(listar(&p.saida()).len(), 13);
}

#[test]
fn index_com_os_dois_opcionais_lista_ranking_taxa_antes_de_ranking_posicoes() {
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_posicoes(cfg, &historico(cfg, N));
    sincronizar_ranking_com(&mut conn, cfg, &perfil_de("quina"), 20, SEMENTE_ESPERADO)
        .expect("ranking");
    posicionar(&mut conn, cfg);
    let lote = lote_de(&mut conn, "quina");
    assert!(lote.ranking_taxa.is_some() && lote.ranking_posicoes.is_some());
    let p = Pasta::nova("dois");
    gravar_tudo(&p.saida(), &[lote], GERADO_EM);
    let ordem = chaves_com_recuo(texto(&ler(&p.saida(), "index.json")), 8);
    assert_eq!(
        ordem,
        [
            "ultimo",
            "tabela_geral",
            "historico",
            "ranking_taxa",
            "ranking_posicoes"
        ]
    );
}

// ---------------------------------------------------------------------------
// 2. Aditividade: nenhum arquivo existente muda.
// ---------------------------------------------------------------------------

#[test]
fn os_arquivos_existentes_nao_mudam_com_as_posicoes() {
    let cfg = cfg_de("megasena");
    let mut conn = conn_sem_posicoes(cfg, &historico(cfg, N));
    sincronizar_ranking_com(&mut conn, cfg, &perfil_de("megasena"), 20, SEMENTE_ESPERADO)
        .expect("ranking");
    let antes = lote_de(&mut conn, "megasena");
    assert!(antes.ranking_posicoes.is_none());
    posicionar(&mut conn, cfg);
    let depois = lote_de(&mut conn, "megasena");
    assert!(depois.ranking_posicoes.is_some());

    assert_eq!(antes.ultimo, depois.ultimo);
    assert_eq!(antes.tabela_geral, depois.tabela_geral);
    assert_eq!(antes.historico, depois.historico);
    assert_eq!(antes.ranking_taxa, depois.ranking_taxa);

    // Os hashes do `index.json` dos quatro arquivos antigos são os mesmos.
    let (pa, pd) = (Pasta::nova("adit_a"), Pasta::nova("adit_d"));
    disco::gravar(&pa.saida(), &[antes], &SLUGS, GERADO_EM).unwrap();
    disco::gravar(&pd.saida(), &[depois], &SLUGS, GERADO_EM).unwrap();
    let (ia, id) = (
        json(&ler(&pa.saida(), "index.json")),
        json(&ler(&pd.saida(), "index.json")),
    );
    let (aa, ad) = (
        ia["loterias"]["megasena"]["arquivos"].as_object().unwrap(),
        id["loterias"]["megasena"]["arquivos"].as_object().unwrap(),
    );
    assert_eq!(aa.len(), 4);
    assert_eq!(ad.len(), 5);
    for (k, v) in aa {
        assert_eq!(&ad[k], v, "{k}");
    }
    assert_eq!(ia["schema_version"], 1);
    assert_eq!(id["schema_version"], 1);
}

// ---------------------------------------------------------------------------
// 3. Posições ausentes ou atrasadas: omitidas, nunca erro.
// ---------------------------------------------------------------------------

#[test]
fn sem_tabelas_a_loteria_sai_sem_o_arquivo_com_motivo_sem_tabela() {
    let cfg = cfg_de("quina");
    let mut conn = conn_sem_posicoes(cfg, &historico(cfg, N));
    assert_eq!(
        motivo_de(&mut conn, "quina"),
        MotivoPosicoesOmitido::SemTabela
    );
    let lote = lote_de(&mut conn, "quina");
    assert!(lote.ranking_posicoes.is_none());
    assert_eq!(
        lote.posicoes_omitido,
        Some(MotivoPosicoesOmitido::SemTabela)
    );
    assert!(MotivoPosicoesOmitido::SemTabela
        .to_string()
        .contains("sync posicoes"));

    let p = Pasta::nova("sem_tabela");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);
    assert!(!existe(&p.saida(), "quina/ranking_posicoes.json"));
    let idx = json(&ler(&p.saida(), "index.json"));
    let arquivos = idx["loterias"]["quina"]["arquivos"].as_object().unwrap();
    assert!(!arquivos.contains_key("ranking_posicoes"));
    assert_eq!(arquivos.len(), 3);
}

#[test]
fn sem_estado_ou_sem_perfil_registrado_omite() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute("DELETE FROM posicao_perfil", []).unwrap();
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::PerfilNaoRegistrado {
            perfil_posicoes: "posicoes_v1".into()
        }
    );

    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute("DELETE FROM posicao_motor_estado", [])
        .unwrap();
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::SemEstado {
            perfil: "padrao_v1".into()
        }
    );
}

#[test]
fn perfil_registrado_com_hash_adulterado_omite_como_invalido() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute("UPDATE posicao_perfil SET hash_toml = 'x'", [])
        .unwrap();
    assert!(matches!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::PerfilInvalido(_)
    ));
}

#[test]
fn concurso_novo_sem_sync_posicoes_omite_como_atrasado_com_os_numeros_reais() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N + 1);
    let mut conn = conn_com_posicoes(cfg, &hist[..N as usize]);
    assert!(lote_de(&mut conn, "lotofacil").ranking_posicoes.is_some());

    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    let m = motivo_de(&mut conn, "lotofacil");
    assert_eq!(
        m,
        MotivoPosicoesOmitido::Atrasado {
            gravado: N,
            esperado: N + 1
        }
    );
    assert!(m.to_string().contains("concurso 12") && m.to_string().contains("o 13"));

    // Refeitas as posições, volta.
    posicionar(&mut conn, cfg);
    assert!(lote_de(&mut conn, "lotofacil").ranking_posicoes.is_some());
}

#[test]
fn resultado_antigo_corrigido_sem_refazer_as_posicoes_omite_como_marcas_diferentes() {
    let cfg = cfg_de("lotofacil");
    let hist = historico(cfg, N);
    let mut conn = conn_com_posicoes(cfg, &hist);

    let mut corrigido = hist[0].clone();
    let nova = cfg
        .dezenas()
        .find(|d| !corrigido.dezenas.contains(d))
        .expect("dezena fora do sorteio");
    corrigido.dezenas[0] = nova;
    corrigido.dezenas.sort_unstable();
    db::salvar_resultados(&mut conn, std::slice::from_ref(&corrigido)).expect("correção");
    sincronizar_features(&mut conn, cfg, true).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");

    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::MarcasDiferentes
    );
    // Os outros arquivos saem normalmente.
    let lote = lote_de(&mut conn, "lotofacil");
    assert!(lote.ranking_posicoes.is_none());
    assert!(!lote.ultimo.is_empty() && !lote.tabela_geral.is_empty());

    posicionar(&mut conn, cfg);
    assert!(lote_de(&mut conn, "lotofacil").ranking_posicoes.is_some());
}

#[test]
fn marcas_ou_features_fora_do_estado_omitem_como_tabelas_incoerentes() {
    let cfg = cfg_de("quina");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute(
        "DELETE FROM posicao_marcas WHERE concurso = 1 AND posicao = 1",
        [],
    )
    .unwrap();
    assert_eq!(
        motivo_de(&mut conn, "quina"),
        MotivoPosicoesOmitido::TabelasIncoerentes {
            tabela: "posicao_marcas"
        }
    );

    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute(
        "DELETE FROM posicao_features WHERE concurso = 1 AND posicao = 1 AND janela = 10",
        [],
    )
    .unwrap();
    assert_eq!(
        motivo_de(&mut conn, "quina"),
        MotivoPosicoesOmitido::TabelasIncoerentes {
            tabela: "posicao_features"
        }
    );
}

#[test]
fn posicao_apagada_do_snapshot_omite_como_incompleto() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute(
        "DELETE FROM posicao_indice WHERE concurso = ?1 AND posicao = 1",
        params![N + 1],
    )
    .unwrap();
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::Incompleto {
            // Os dois retratos (último sorteado e próximo) contam: 2 x 25.
            linhas: 49,
            esperadas: 50
        }
    );

    // Faltar uma posição do último sorteado também omite.
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute(
        "DELETE FROM posicao_indice WHERE concurso = ?1 AND posicao = 1",
        params![N],
    )
    .unwrap();
    assert!(matches!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::Incompleto { .. }
    ));
}

#[test]
fn g_1_adulterado_omite_como_diverge_do_calculo() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    // Troca o g_1 de duas posições do snapshot: ainda é uma permutação 1..=N.
    conn.execute(
        "UPDATE posicao_indice SET g_1 = CASE g_1 WHEN 1 THEN 2 WHEN 2 THEN 1 ELSE g_1 END
         WHERE concurso = ?1",
        params![N + 1],
    )
    .unwrap();
    assert!(matches!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::DivergeDoCalculo { posicao } if (1..=25).contains(&posicao)
    ));

    // Uma variação alterada também é pega (igualdade exata).
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute(
        "UPDATE posicao_indice SET variacao_g_1 = 99 WHERE concurso = ?1 AND posicao = 3",
        params![N + 1],
    )
    .unwrap();
    assert_eq!(
        motivo_de(&mut conn, "lotofacil"),
        MotivoPosicoesOmitido::DivergeDoCalculo { posicao: 3 }
    );
}

#[test]
fn omissao_nao_muda_os_demais_arquivos() {
    let cfg = cfg_de("quina");
    let hist = historico(cfg, N);
    let mut sem = conn_sem_posicoes(cfg, &hist);
    let mut com = conn_com_posicoes(cfg, &hist);
    let (a, b) = (lote_de(&mut sem, "quina"), lote_de(&mut com, "quina"));
    assert_eq!(
        (&a.ultimo, &a.tabela_geral, &a.historico),
        (&b.ultimo, &b.tabela_geral, &b.historico)
    );
}

// ---------------------------------------------------------------------------
// 4. Remoção do arquivo antigo e relatório.
// ---------------------------------------------------------------------------

fn perfis_em(pasta: &Pasta) -> PathBuf {
    let raiz = pasta.0.join("perfis");
    for slug in SLUGS {
        let dir = raiz.join(slug);
        std::fs::create_dir_all(&dir).expect("pasta de perfis");
        std::fs::write(dir.join("padrao_v1.toml"), texto_perfil(slug)).expect("perfil");
        std::fs::write(dir.join("posicoes_v1.toml"), texto_posicoes(slug)).expect("perfil");
    }
    raiz
}

/// Banco em disco da loteria, com features e índice e, se pedido, posições.
fn banco_em(pasta: &Pasta, slug: &str, com_posicoes: bool) -> PathBuf {
    let cfg = cfg_de(slug);
    let caminho = pasta.0.join(format!("{slug}.db"));
    let mut conn = db::abrir(&caminho).expect("banco");
    preparar(&mut conn, cfg, &historico(cfg, N));
    if com_posicoes {
        posicionar(&mut conn, cfg);
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
    let ResultadoComRelatorio::Gravado(r) =
        exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("exportação")
    else {
        panic!("esperava gravação");
    };
    assert_eq!(r.posicoes.len(), 2);
    assert_eq!(r.posicoes[0].slug, "quina");
    assert_eq!(
        r.posicoes[0].situacao,
        SituacaoPosicoes::Omitido(MotivoPosicoesOmitido::SemTabela)
    );
    assert_eq!(r.posicoes[1].slug, "lotofacil");
    assert_eq!(r.posicoes[1].situacao, SituacaoPosicoes::Publicado);
    assert!(r.removidos_posicoes.is_empty());
    assert!(existe(&p.saida(), "lotofacil/ranking_posicoes.json"));
    assert!(!existe(&p.saida(), "quina/ranking_posicoes.json"));
    // Omitir as posições não cancela nada nem mexe no relatório do ranking.
    assert_eq!(r.rankings.len(), 2);
}

#[test]
fn posicoes_que_ficam_atrasadas_apagam_o_arquivo_antigo_e_a_chave() {
    let cfg = cfg_de("lotofacil");
    let p = Pasta::nova("remocao");
    let perfis = perfis_em(&p);
    let caminho_db = banco_em(&p, "lotofacil", true);
    let alvos = vec![alvo("lotofacil", caminho_db.clone())];

    // 1) Em dia: o arquivo existe e está no índice.
    exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("1");
    assert!(existe(&p.saida(), "lotofacil/ranking_posicoes.json"));
    assert!(
        json(&ler(&p.saida(), "index.json"))["loterias"]["lotofacil"]["arquivos"]
            ["ranking_posicoes"]
            .is_object()
    );

    // 2) Concurso novo, posições não refeitas: o arquivo some.
    {
        let hist = historico(cfg, N + 1);
        let mut conn = db::abrir(&caminho_db).expect("banco");
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("novo");
        sincronizar_features(&mut conn, cfg, false).expect("features");
        sincronizar_indice(&mut conn, cfg, &perfil_de("lotofacil"), GERADO_EM).expect("índice");
    }
    let ResultadoComRelatorio::Gravado(r) =
        exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("2")
    else {
        panic!("esperava gravação");
    };
    let antigo = disco::pasta_versao(&p.saida())
        .join("lotofacil")
        .join("ranking_posicoes.json");
    assert_eq!(r.removidos_posicoes, vec![antigo.clone()]);
    assert!(r.removidos.is_empty());
    assert!(!antigo.exists());
    let idx = json(&ler(&p.saida(), "index.json"));
    assert!(!idx["loterias"]["lotofacil"]["arquivos"]
        .as_object()
        .unwrap()
        .contains_key("ranking_posicoes"));
    assert_eq!(idx["loterias"]["lotofacil"]["ultimo_concurso"], N + 1);
    assert!(matches!(
        r.posicoes[0].situacao,
        SituacaoPosicoes::Omitido(MotivoPosicoesOmitido::Atrasado { .. })
    ));
    assert_eq!(listar(&p.saida()).len(), 4);

    // 3) Posições refeitas: o arquivo volta, com a chave.
    {
        let mut conn = db::abrir(&caminho_db).expect("banco");
        posicionar(&mut conn, cfg);
    }
    let ResultadoComRelatorio::Gravado(r) =
        exportar_com_relatorio(&alvos, &perfis, &p.saida(), relogio).expect("3")
    else {
        panic!("esperava gravação");
    };
    assert!(r.removidos_posicoes.is_empty());
    assert!(existe(&p.saida(), "lotofacil/ranking_posicoes.json"));
    assert_eq!(listar(&p.saida()).len(), 5);
}

#[test]
fn loteria_preservada_mantem_o_arquivo_e_a_chave_no_index_refeito() {
    let cfg = cfg_de("quina");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    let quina = lote_de(&mut conn, "quina");
    let p = Pasta::nova("preservada");
    gravar_tudo(&p.saida(), std::slice::from_ref(&quina), GERADO_EM);

    // Outra loteria exportada depois: a quina fica como estava.
    let cfg_l = cfg_de("lotofacil");
    let mut conn_l = conn_sem_posicoes(cfg_l, &historico(cfg_l, N));
    let lotofacil = lote_de(&mut conn_l, "lotofacil");
    disco::gravar(&p.saida(), &[lotofacil], &SLUGS, "2026-10-06T10:00:00Z").unwrap();
    assert_eq!(
        ler(&p.saida(), "quina/ranking_posicoes.json"),
        posicoes_bytes(&quina)
    );
    let idx = json(&ler(&p.saida(), "index.json"));
    assert!(idx["loterias"]["quina"]["arquivos"]["ranking_posicoes"].is_object());
    assert!(!idx["loterias"]["lotofacil"]["arquivos"]
        .as_object()
        .unwrap()
        .contains_key("ranking_posicoes"));
}

// ---------------------------------------------------------------------------
// 5. Determinismo.
// ---------------------------------------------------------------------------

#[test]
fn duas_exportacoes_do_mesmo_banco_dao_os_mesmos_bytes() {
    let cfg = cfg_de("lotofacil");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    let (a, b) = (
        lote_de(&mut conn, "lotofacil"),
        lote_de(&mut conn, "lotofacil"),
    );
    assert_eq!(a, b);
    let (pa, pb) = (Pasta::nova("det_a"), Pasta::nova("det_b"));
    disco::gravar(&pa.saida(), &[a], &SLUGS, "2026-10-05T10:00:00Z").unwrap();
    disco::gravar(&pb.saida(), &[b], &SLUGS, "2030-01-01T00:00:00Z").unwrap();
    for nome in [
        "ultimo.json",
        "tabela_geral.json",
        "historico.json",
        "ranking_posicoes.json",
    ] {
        let rel = format!("lotofacil/{nome}");
        assert_eq!(ler(&pa.saida(), &rel), ler(&pb.saida(), &rel), "{rel}");
    }
    let ta = texto(&ler(&pa.saida(), "index.json")).replace("2026-10-05T10:00:00Z", "X");
    let tb = texto(&ler(&pb.saida(), "index.json")).replace("2030-01-01T00:00:00Z", "X");
    assert_eq!(ta, tb);
}

#[test]
fn posicoes_regeradas_do_zero_ou_rodadas_de_novo_dao_o_mesmo_json() {
    let cfg = cfg_de("megasena");
    let hist = historico(cfg, N);
    let mut a = conn_com_posicoes(cfg, &hist);
    let original = lote_de(&mut a, "megasena");

    // Rodar de novo (incremental, sem novidade).
    posicionar(&mut a, cfg);
    assert_eq!(
        posicoes_bytes(&lote_de(&mut a, "megasena")),
        posicoes_bytes(&original)
    );
    // Outro banco, mesma entrada.
    let mut b = conn_com_posicoes(cfg, &hist);
    assert_eq!(
        posicoes_bytes(&lote_de(&mut b, "megasena")),
        posicoes_bytes(&original)
    );
    // Calculado em duas etapas (11 concursos, depois o 12º) = de uma vez.
    let mut c = conn_com_posicoes(cfg, &hist[..N as usize - 1]);
    db::salvar_resultados(&mut c, &hist[N as usize - 1..]).unwrap();
    sincronizar_features(&mut c, cfg, false).unwrap();
    sincronizar_indice(&mut c, cfg, &perfil_de("megasena"), GERADO_EM).unwrap();
    posicionar(&mut c, cfg);
    assert_eq!(
        posicoes_bytes(&lote_de(&mut c, "megasena")),
        posicoes_bytes(&original)
    );
}

// ---------------------------------------------------------------------------
// 6. CLI: `sync exportar`.
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

#[test]
fn cli_sem_posicoes_avisa_que_o_arquivo_foi_omitido_e_termina_com_sucesso() {
    let p = Pasta::nova("cli_sem");
    let perfis = perfis_em(&p);
    let db = banco_em(&p, "lotofacil", false);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &db));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("Aviso: lotofacil: ranking_posicoes.json omitido:"),
        "{stdout}"
    );
    assert!(stdout.contains("sync posicoes"), "{stdout}");
    assert!(!existe(&saida, "lotofacil/ranking_posicoes.json"));
    assert_eq!(listar(&saida).len(), 4);
}

#[test]
fn cli_com_posicoes_lista_o_arquivo_e_grava_os_mesmos_bytes_da_biblioteca() {
    let p = Pasta::nova("cli_com");
    let perfis = perfis_em(&p);
    let db = banco_em(&p, "lotofacil", true);
    let saida = p.saida();
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &db));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}");
    assert!(
        !stdout.contains("ranking_posicoes.json omitido"),
        "{stdout}"
    );
    assert!(stdout.contains("ranking_posicoes.json"), "{stdout}");
    assert_eq!(listar(&saida).len(), 5);

    let cfg = cfg_de("lotofacil");
    let mut conn = db::abrir(&db).expect("banco");
    let lote = lote_de(&mut conn, "lotofacil");
    let _ = cfg;
    assert_eq!(
        ler(&saida, "lotofacil/ranking_posicoes.json"),
        posicoes_bytes(&lote)
    );

    // Atrasa o banco (concurso novo, sem `sync posicoes`): a CLI avisa a remoção.
    let hist = historico(cfg_de("lotofacil"), N + 1);
    db::salvar_resultados(&mut conn, &hist[N as usize..]).unwrap();
    sincronizar_features(&mut conn, cfg_de("lotofacil"), false).unwrap();
    sincronizar_indice(
        &mut conn,
        cfg_de("lotofacil"),
        &perfil_de("lotofacil"),
        GERADO_EM,
    )
    .unwrap();
    drop(conn);
    let out = sync(&p.0, &args_exportar("lotofacil", &saida, &perfis, &db));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("Aviso: ranking_posicoes.json antigo removido:"),
        "{stdout}"
    );
    assert!(!existe(&saida, "lotofacil/ranking_posicoes.json"));
}

// ---------------------------------------------------------------------------
// 7. O contrato escrito cobre todas as chaves.
// ---------------------------------------------------------------------------

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
fn toda_chave_do_ranking_posicoes_json_e_do_index_esta_no_contrato() {
    let doc = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/contrato-v1.md"))
        .expect("docs/contrato-v1.md");
    let p = Pasta::nova("doc");
    let lotes: Vec<ArquivosLoteria> = casos().into_iter().map(|c| c.3).collect();
    gravar_tudo(&p.saida(), &lotes, GERADO_EM);

    let mut chaves = BTreeSet::new();
    chaves_do_contrato(&json(&ler(&p.saida(), "index.json")), "", &mut chaves);
    for slug in SLUGS {
        let bytes = ler(&p.saida(), &format!("{slug}/ranking_posicoes.json"));
        chaves_do_contrato(&json(&bytes), "", &mut chaves);
    }
    for k in CHAVES_CABECALHO.iter().chain(&CHAVES_ENTRADA) {
        assert!(chaves.contains(*k), "chave `{k}` não coletada");
    }
    assert!(chaves.contains("ranking_posicoes"));

    let ini = doc
        .find("## JSON público do app")
        .expect("cabeçalho '## JSON público do app' não encontrado no contrato");
    let fim = doc
        .find("## Dataset para o Python")
        .expect("cabeçalho '## Dataset para o Python' não encontrado no contrato");
    assert!(ini < fim);
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

// ---------------------------------------------------------------------------
// 8. `variacao_g_1` é da DEZENA (g_1 que ela tinha em `ultimo` − g_1 atual).
// ---------------------------------------------------------------------------

/// `dezena -> posição` no índice de força do concurso, lido do banco.
fn posicao_das_dezenas(conn: &Connection, concurso: u32) -> std::collections::BTreeMap<u32, u32> {
    let mut stmt = conn
        .prepare("SELECT dezena, posicao FROM indice WHERE perfil = 'padrao_v1' AND concurso = ?1")
        .unwrap();
    stmt.query_map(params![concurso], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// `posição -> g_1` do índice das posições no concurso, lido do banco.
fn g_1_das_posicoes(conn: &Connection, concurso: u32) -> std::collections::BTreeMap<u32, u32> {
    let mut stmt = conn
        .prepare(
            "SELECT posicao, g_1 FROM posicao_indice
             WHERE perfil = 'padrao_v1' AND perfil_posicoes = 'posicoes_v1' AND concurso = ?1",
        )
        .unwrap();
    stmt.query_map(params![concurso], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn variacao_g_1_e_da_dezena_e_nao_da_posicao_que_ela_ocupa() {
    for slug in SLUGS {
        let cfg = cfg_de(slug);
        let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
        let antes = posicao_das_dezenas(&conn, N);
        let depois = posicao_das_dezenas(&conn, N + 1);
        let g_antes = g_1_das_posicoes(&conn, N);
        let g_depois = g_1_das_posicoes(&conn, N + 1);
        let variacao_da_posicao: std::collections::BTreeMap<u32, Option<i64>> = {
            let mut stmt = conn
                .prepare(
                    "SELECT posicao, variacao_g_1 FROM posicao_indice
                     WHERE perfil = 'padrao_v1' AND perfil_posicoes = 'posicoes_v1'
                       AND concurso = ?1",
                )
                .unwrap();
            stmt.query_map(params![N + 1], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };

        let lote = lote_de(&mut conn, slug);
        let p = json(posicoes_bytes(&lote));
        let mut trocou_e_difere = 0;
        for d in p["dezenas"].as_array().unwrap() {
            let dezena = d["dezena"].as_u64().unwrap() as u32;
            let (pos_antes, pos_depois) = (antes[&dezena], depois[&dezena]);
            let esperado = i64::from(g_antes[&pos_antes]) - i64::from(g_depois[&pos_depois]);
            assert_eq!(d["variacao_g_1"], esperado, "{slug} d{dezena}");
            assert_eq!(d["g_1"], g_depois[&pos_depois]);
            if pos_antes != pos_depois && variacao_da_posicao[&pos_depois] != Some(esperado) {
                trocou_e_difere += 1;
            }
        }
        // A prova: há dezenas que trocaram de posição e cujo valor NÃO é o
        // `variacao_g_1` da posição que ocupam agora.
        assert!(
            trocou_e_difere > 0,
            "{slug}: nenhuma dezena provou a diferença"
        );
    }
}

// ---------------------------------------------------------------------------
// 9. Erro de leitura das tabelas `posicao_*` nunca impede o exportar.
// ---------------------------------------------------------------------------

#[test]
fn esquema_incompativel_em_posicao_indice_omite_com_erro_de_leitura() {
    let cfg = cfg_de("quina");
    let mut conn = conn_com_posicoes(cfg, &historico(cfg, N));
    conn.execute_batch(
        "DROP TABLE posicao_indice;
         CREATE TABLE posicao_indice (perfil TEXT, concurso INTEGER);",
    )
    .unwrap();
    assert!(matches!(
        motivo_de(&mut conn, "quina"),
        MotivoPosicoesOmitido::ErroDeLeitura(_)
    ));
    // O resto é publicado normalmente.
    let lote = lote_de(&mut conn, "quina");
    assert!(lote.ranking_posicoes.is_none());
    assert!(!lote.ultimo.is_empty() && !lote.tabela_geral.is_empty() && !lote.historico.is_empty());
    let p = Pasta::nova("esquema");
    gravar_tudo(&p.saida(), std::slice::from_ref(&lote), GERADO_EM);
    assert!(existe(&p.saida(), "quina/ultimo.json"));
    assert!(!existe(&p.saida(), "quina/ranking_posicoes.json"));
    assert!(MotivoPosicoesOmitido::ErroDeLeitura("x".into())
        .to_string()
        .contains("sync posicoes"));
}
