//! Testes de `orquestracao::tabela_do_concurso` e do comando `sync tabela`
//! (1b.4): a tabela geral de um concurso sai do índice e das features já
//! conferidos, em ordem de posição, e o CSV é gravado sem tocar no banco.

use rusqlite::Connection;
use rust_spider::avaliacao::{self, Direcao, Grau, Janela};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{sincronizar_features, sincronizar_indice, tabela_do_concurso};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::tabela::{self, BOM, CABECALHO};
use rust_spider::Resultado;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const NOME: &str = "teste_v1";
const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
const N: u32 = 40;

const PERFIL: &str = r#"nome = "teste_v1"
versao = 1
loteria = "lotofacil"
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

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofácil configurada")
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

fn perfil_teste() -> PerfilCarregado {
    perfil::interpretar(PERFIL, Path::new("perfis/lotofacil/teste_v1.toml")).expect("perfil")
}

/// Grava `hist`, calcula as features e o índice do perfil de teste.
fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
    sincronizar_indice(conn, cfg, &perfil_teste(), "2026-10-05T10:00:00Z").expect("índice");
}

fn preparado(n: u32) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, lotofacil(), &historico(lotofacil(), n));
    conn
}

// ---------------------------------------------------------------------------
// Orquestração.
// ---------------------------------------------------------------------------

#[test]
fn tabela_do_proximo_concurso_segue_o_indice_em_ordem_de_posicao() {
    let cfg = lotofacil();
    let mut conn = preparado(N);
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), None).expect("tabela");

    assert_eq!(t.concurso, N + 1);
    assert_eq!(t.ultimo_sorteado, N);
    assert_eq!(t.data_sorteio, None);
    assert_eq!(
        (t.loteria.as_str(), t.perfil.as_str(), t.janela),
        ("lotofacil", NOME, 10)
    );
    assert_eq!(t.linhas.len(), 25);

    let posicoes: Vec<u32> = t.linhas.iter().map(|l| l.posicao).collect();
    assert_eq!(posicoes, (1..=25).collect::<Vec<_>>());

    // Cada linha é a do índice gravado e a das features da janela do perfil.
    let indice = db::ler_indice(&conn, NOME).expect("índice");
    let features = db::ler_features_da_janela(&conn, 10).expect("features");
    for l in &t.linhas {
        let i = indice
            .iter()
            .find(|i| i.concurso == N + 1 && i.dezena == l.dezena)
            .expect("linha do índice");
        assert_eq!(i.total.to_bits(), l.total.to_bits());
        assert_eq!(
            (i.posicao, i.variacao_posicao),
            (l.posicao, l.variacao_posicao)
        );
        let f = features
            .iter()
            .find(|f| f.features.concurso == N + 1 && f.features.dezena == l.dezena)
            .expect("linha das features");
        assert_eq!(f.features, l.features);
        assert_eq!(l.saiu, None);
    }
}

#[test]
fn tabela_de_concurso_sorteado_tem_data_e_saiu_das_dezenas_sorteadas() {
    let cfg = lotofacil();
    let hist = historico(cfg, N);
    let mut conn = preparado(N);
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(30)).expect("tabela");

    assert_eq!(t.concurso, 30);
    assert_eq!(
        t.data_sorteio.as_deref(),
        Some(hist[29].data_sorteio.as_str())
    );
    let mut sairam: Vec<u32> = t
        .linhas
        .iter()
        .filter(|l| l.saiu == Some(true))
        .map(|l| l.dezena)
        .collect();
    sairam.sort_unstable();
    let mut sorteadas = hist[29].dezenas.clone();
    sorteadas.sort_unstable();
    assert_eq!(sairam, sorteadas);
    assert!(t.linhas.iter().all(|l| l.saiu.is_some()));
}

#[test]
fn concurso_fora_de_1_ao_proximo_e_recusado() {
    let cfg = lotofacil();
    let mut conn = preparado(N);
    for concurso in [0, N + 2, u32::MAX] {
        match tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(concurso)) {
            Err(SpiderError::TabelaConcursoInvalido {
                concurso: c,
                maximo,
                ..
            }) => assert_eq!((c, maximo), (concurso, N + 1)),
            outro => panic!("concurso {concurso}: esperava TabelaConcursoInvalido, veio {outro:?}"),
        }
    }
    // As pontas valem.
    for concurso in [1, N + 1] {
        tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(concurso)).expect("ponta");
    }
}

#[test]
fn indice_desatualizado_e_recusado() {
    let cfg = lotofacil();
    let hist = historico(cfg, N + 1);
    let mut conn = preparado(N);
    // Concurso novo com features em dia, mas sem rodar o `sync indice`.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    sincronizar_features(&mut conn, cfg, false).expect("features");
    match tabela_do_concurso(&mut conn, cfg, &perfil_teste(), None) {
        Err(SpiderError::IndiceDesatualizado { esperado, .. }) => assert_eq!(esperado, N + 2),
        outro => panic!("esperava IndiceDesatualizado, veio {outro:?}"),
    }
}

#[test]
fn features_desatualizadas_sao_recusadas() {
    let cfg = lotofacil();
    let hist = historico(cfg, N + 1);
    let mut conn = preparado(N);
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    assert!(matches!(
        tabela_do_concurso(&mut conn, cfg, &perfil_teste(), None),
        Err(SpiderError::FeaturesDesatualizadas { .. })
    ));
}

#[test]
fn csv_tem_cabecalho_e_uma_linha_por_dezena_em_ordem_de_posicao() {
    let cfg = lotofacil();
    let mut conn = preparado(N);
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(30)).expect("tabela");
    let csv = tabela::formatar_csv(&t);
    let linhas: Vec<&str> = csv.strip_prefix(BOM).expect("BOM").lines().collect();
    assert_eq!(linhas[0], CABECALHO);
    assert_eq!(linhas.len(), 26);
    for (i, linha) in linhas[1..].iter().enumerate() {
        let colunas: Vec<&str> = linha.split(';').collect();
        assert_eq!(colunas.len(), 21, "{linha}");
        assert_eq!(colunas.len(), CABECALHO.split(';').count());
        // Concurso 30 tem 29 concursos de histórico: grau e direção preenchidos.
        assert!(
            !colunas[19].is_empty() && !colunas[20].is_empty(),
            "{linha}"
        );
        assert_eq!(colunas[2], "30");
        assert_eq!(colunas[5], (i + 1).to_string(), "posição em ordem");
        assert!(colunas[6].contains(','), "vírgula decimal: {linha}");
        assert!(matches!(colunas[8], "0" | "1"), "saiu preenchido: {linha}");
    }
    assert_eq!(tabela::nome_arquivo(&t), "tabela_lotofacil_teste_v1_30.csv");
}

// ---------------------------------------------------------------------------
// Evidência por dezena (1d.4).
// ---------------------------------------------------------------------------

/// Grau e direção por dezena, lidos das duas últimas colunas do CSV.
fn evidencia_do_csv(csv: &str) -> BTreeMap<u32, (String, String)> {
    let mut linhas = csv.strip_prefix(BOM).expect("BOM").lines();
    let cabecalho: Vec<&str> = linhas.next().expect("cabeçalho").split(';').collect();
    let n = cabecalho.len();
    // Pelo nome da coluna, não pela posição.
    let col = |nome: &str| cabecalho.iter().position(|c| *c == nome).expect(nome);
    let (cd, cg, cdir) = (
        col("dezena"),
        col("evidencia_grau"),
        col("evidencia_direcao"),
    );
    assert_eq!((cg, cdir), (n - 2, n - 1), "colunas novas no fim");
    linhas
        .map(|l| {
            let c: Vec<&str> = l.split(';').collect();
            (
                c[cd].parse().expect("dezena"),
                (c[cg].to_string(), c[cdir].to_string()),
            )
        })
        .collect()
}

/// Histórico em que a dezena 7 sai em todos os concursos (viés enorme).
fn historico_com_dezena_7_viciada(n: u32) -> Vec<Resultado> {
    let mut hist = historico(lotofacil(), n);
    for r in &mut hist {
        if !r.dezenas.contains(&7) {
            r.dezenas[0] = 7;
        }
        assert_eq!(r.dezenas.len(), 15);
    }
    hist
}

fn conn_com(hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, lotofacil(), hist);
    conn
}

#[test]
fn colunas_de_evidencia_confirmam_evidencia_por_dezena_no_concurso() {
    let cfg = lotofacil();
    let n = 200;
    let hist = historico_com_dezena_7_viciada(n);
    let mut conn = conn_com(&hist);
    // O próximo (histórico inteiro), um do meio e o segundo.
    for concurso in [n + 1, 120, 2] {
        let t =
            tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(concurso)).expect("tabela");
        let esperado = avaliacao::evidencia_por_dezena(cfg, &hist, concurso, Janela::Inteira)
            .expect("evidência");
        assert_eq!(esperado.concursos as u32, concurso - 1);
        let csv = tabela::formatar_csv(&t);
        let do_csv = evidencia_do_csv(&csv);
        assert_eq!(do_csv.len(), 25);
        for d in &esperado.dezenas {
            let c = d.classificacao[0];
            assert_eq!(
                do_csv[&d.dezena],
                (c.grau.nome().to_string(), c.direcao.nome().to_string()),
                "concurso {concurso}, dezena {}",
                d.dezena
            );
        }
    }

    // O viés enorme aparece, com os rótulos em texto.
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), None).expect("tabela");
    let do_csv = evidencia_do_csv(&tabela::formatar_csv(&t));
    assert_eq!(
        do_csv[&7],
        (
            Grau::MuitoForte.nome().to_string(),
            Direcao::Vies.nome().to_string()
        )
    );
}

#[test]
fn concurso_1_nao_tem_historico_e_as_colunas_de_evidencia_ficam_vazias() {
    let cfg = lotofacil();
    let mut conn = preparado(N);
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(1)).expect("tabela");
    let csv = tabela::formatar_csv(&t);
    let do_csv = evidencia_do_csv(&csv);
    assert_eq!(do_csv.len(), 25);
    assert!(do_csv.values().all(|(g, d)| g.is_empty() && d.is_empty()));
    // O concurso 2 já tem um concurso de histórico.
    let t = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(2)).expect("tabela");
    assert!(evidencia_do_csv(&tabela::formatar_csv(&t))
        .values()
        .all(|(g, d)| !g.is_empty() && !d.is_empty()));
}

#[test]
fn evidencia_do_csv_de_t_nao_depende_do_concurso_t_nem_dos_posteriores() {
    let cfg = lotofacil();
    let n = 120;
    let t = 90;
    let hist = historico_com_dezena_7_viciada(n);
    // Do concurso t em diante, outro sorteio (a permutação cíclica das dezenas
    // mantém cada concurso válido e muda `saiu`, as features e o índice de t).
    let alterado: Vec<Resultado> = hist
        .iter()
        .map(|r| {
            let mut r = r.clone();
            if r.concurso >= t {
                for d in &mut r.dezenas {
                    *d = *d % 25 + 1;
                }
            }
            r
        })
        .collect();
    assert_ne!(
        hist[t as usize - 1].dezenas,
        alterado[t as usize - 1].dezenas
    );

    let csv = |h: &[Resultado]| {
        let mut conn = conn_com(h);
        let tabela = tabela_do_concurso(&mut conn, cfg, &perfil_teste(), Some(t)).expect("tabela");
        tabela::formatar_csv(&tabela)
    };
    let (a, b) = (csv(&hist), csv(&alterado));
    let (ev_a, ev_b) = (evidencia_do_csv(&a), evidencia_do_csv(&b));
    assert_eq!(ev_a, ev_b);
    // Não é trivial: a dezena 7 vem viciada de 1 a t−1 e a evidência é forte.
    assert_eq!(ev_a[&7].0, Grau::MuitoForte.nome());
    // O resto do CSV muda (saiu), para o teste não passar por entrada igual.
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// CLI.
// ---------------------------------------------------------------------------

/// Pasta temporária apagada no fim do teste.
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
            "rust_spider_tabela_{rotulo}_{}_{unico}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&caminho).expect("pasta temporária");
        Pasta(caminho)
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Banco em arquivo com `N` concursos, features e índice, e o `.toml` do
/// perfil numa pasta `perfis/`.
fn ambiente(rotulo: &str) -> (Pasta, PathBuf, PathBuf) {
    let pasta = Pasta::nova(rotulo);
    let db_caminho = pasta.0.join("resultados_lotofacil.db");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco em arquivo");
        preparar(&mut conn, lotofacil(), &historico(lotofacil(), N));
    }
    let perfis = pasta.0.join("perfis");
    std::fs::create_dir_all(perfis.join("lotofacil")).expect("pasta de perfis");
    std::fs::write(perfis.join("lotofacil").join("teste_v1.toml"), PERFIL).expect("perfil");
    (pasta, db_caminho, perfis)
}

fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_tabela(db: &Path, perfis: &Path, saida: &Path, extras: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["tabela", "lotofacil", "--perfil", NOME]
        .iter()
        .map(OsString::from)
        .collect();
    for (opcao, caminho) in [("--db", db), ("--perfis", perfis), ("--saida", saida)] {
        args.push(opcao.into());
        args.push(caminho.into());
    }
    args.extend(extras.iter().map(OsString::from));
    args
}

#[test]
fn cli_grava_o_csv_do_proximo_concurso_sem_tocar_no_banco() {
    let (pasta, db_caminho, perfis) = ambiente("proximo");
    let saida = pasta.0.join("tabelas");
    let banco_antes = std::fs::read(&db_caminho).expect("banco lido");

    let out = sync(&pasta.0, &args_tabela(&db_caminho, &perfis, &saida, &[]));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("próximo, ainda não sorteado"), "{stdout}");
    assert!(stdout.contains("Top 10: "), "{stdout}");

    let arquivo = saida
        .join("lotofacil")
        .join(format!("tabela_lotofacil_{NOME}_{}.csv", N + 1));
    let bytes = std::fs::read(&arquivo).expect("CSV gravado");
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF], "arquivo começa com o BOM");
    let csv = String::from_utf8(bytes).expect("UTF-8");
    assert!(csv.starts_with(&format!("{BOM}{CABECALHO}")));
    assert_eq!(csv.lines().count(), 26);
    assert_eq!(std::fs::read(&db_caminho).expect("banco lido"), banco_antes);

    // Mesmo concurso de novo: mesmos bytes.
    let out = sync(&pasta.0, &args_tabela(&db_caminho, &perfis, &saida, &[]));
    assert!(out.status.success());
    assert_eq!(std::fs::read_to_string(&arquivo).expect("CSV"), csv);
}

#[test]
fn cli_com_concurso_sorteado_marca_as_dezenas_que_sairam() {
    let (pasta, db_caminho, perfis) = ambiente("sorteado");
    let saida = pasta.0.join("tabelas");
    let out = sync(
        &pasta.0,
        &args_tabela(&db_caminho, &perfis, &saida, &["--concurso", "30"]),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("sorteado em"), "{stdout}");
    assert!(stdout.contains("(* = saiu)"), "{stdout}");
    let csv = std::fs::read_to_string(
        saida
            .join("lotofacil")
            .join(format!("tabela_lotofacil_{NOME}_30.csv")),
    )
    .expect("CSV gravado");
    let sairam = csv
        .lines()
        .skip(1)
        .filter(|l| l.split(';').nth(8) == Some("1"))
        .count();
    assert_eq!(sairam, 15);
}

#[test]
fn cli_concurso_com_todas_e_recusado() {
    let pasta = Pasta::nova("todas");
    let args: Vec<OsString> = ["tabela", "todas", "--perfil", NOME, "--concurso", "5"]
        .iter()
        .map(OsString::from)
        .collect();
    let out = sync(&pasta.0, &args);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--concurso"));
}

#[test]
fn cli_banco_inexistente_falha_sem_criar_arquivo() {
    let pasta = Pasta::nova("sem_banco");
    let db_caminho = pasta.0.join("nao_existe.db");
    let saida = pasta.0.join("tabelas");
    let out = sync(
        &pasta.0,
        &args_tabela(&db_caminho, &pasta.0.join("perfis"), &saida, &[]),
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("banco não encontrado"));
    assert!(!db_caminho.exists());
    assert!(!saida.exists());
}
