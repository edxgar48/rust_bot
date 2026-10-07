//! Testes de `orquestracao::dataset_de_features` e do comando `sync dataset`
//! (1.5.1): o dataset sai de `features_dezena` já conferida, sem o trancado
//! por padrão, só com concursos sorteados, e os arquivos são gravados sem
//! tocar no banco, com os mesmos bytes a cada execução.

use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig, Trecho};
use rust_spider::dataset::{self, Dataset, Formato, COLUNAS};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::motor::{FeaturesDezena, JANELAS};
use rust_spider::orquestracao::{dataset_de_features, impressao_resultados, sincronizar_features};
use rust_spider::Resultado;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
const N: u32 = 40;
/// Corte artificial para os testes do trancado: 1 … 25 na busca, 26 … 40
/// trancados.
const CORTE: u32 = 25;

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofácil configurada")
}

/// Lotofácil com o corte da busca em [`CORTE`] (o real fica no concurso
/// 3416, longe do histórico sintético).
fn com_corte() -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: CORTE,
        ..*lotofacil()
    }
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
                // 28 dias × 12 meses por ano: datas válidas e crescentes
                // também nos históricos que passam do corte real.
                data_sorteio: format!(
                    "{}-{:02}-{:02}",
                    2020 + c / 336,
                    1 + (c % 336) / 28,
                    1 + c % 28
                ),
                dezenas,
            }
        })
        .collect()
}

/// Grava `hist` e calcula as features.
fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
}

fn em_memoria(hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, lotofacil(), hist);
    conn
}

fn preparado(n: u32) -> Connection {
    em_memoria(&historico(lotofacil(), n))
}

/// Linhas de `features_dezena` gravadas, até `ate`.
fn gravadas_ate(conn: &Connection, ate: u32) -> Vec<FeaturesDezena> {
    db::ler_features(conn)
        .expect("features")
        .into_iter()
        .map(|l| l.features)
        .filter(|f| f.concurso <= ate)
        .collect()
}

fn concursos(ds: &Dataset) -> Vec<u32> {
    ds.concursos().iter().map(|c| c.concurso).collect()
}

// ---------------------------------------------------------------------------
// Orquestração: trecho, ponto no tempo, só sorteados.
// ---------------------------------------------------------------------------

#[test]
fn sem_trancado_so_entram_concursos_ate_o_corte() {
    let cfg = com_corte();
    let mut conn = preparado(N);
    let ds = dataset_de_features(&mut conn, &cfg, false).expect("dataset");

    assert_eq!(concursos(&ds), (1..=CORTE).collect::<Vec<_>>());
    assert!(ds.concursos().iter().all(|c| c.trecho == Trecho::Busca));
    assert!(ds.linhas().iter().all(|f| f.concurso <= CORTE));
    assert_eq!(ds.linhas().len(), (CORTE * 25) as usize * JANELAS.len());
    assert!(!ds.com_trancado());
    assert_eq!(ds.ultimo_concurso(), CORTE);
    // As linhas são as gravadas, sem recálculo nem filtro além do corte.
    assert_eq!(ds.linhas(), gravadas_ate(&conn, CORTE).as_slice());

    let csv = dataset::formatar_csv(&ds);
    assert!(!csv.contains("trancado"));
    let m = dataset::manifesto(&ds, Formato::Csv, csv.as_bytes());
    assert_eq!(
        (m.trecho, m.ultimo_concurso, m.ultimo_concurso_busca),
        ("busca", CORTE, CORTE)
    );
}

#[test]
fn com_trancado_entram_todos_os_sorteados_marcados_pelo_trecho() {
    let cfg = com_corte();
    let mut conn = preparado(N);
    let ds = dataset_de_features(&mut conn, &cfg, true).expect("dataset");

    assert_eq!(concursos(&ds), (1..=N).collect::<Vec<_>>());
    for c in ds.concursos() {
        assert_eq!(
            c.trecho,
            cfg.trecho_do_concurso(c.concurso),
            "{}",
            c.concurso
        );
    }
    assert_eq!(
        ds.concursos()
            .iter()
            .filter(|c| c.trecho == Trecho::Trancado)
            .count(),
        (N - CORTE) as usize
    );
    assert_eq!(ds.linhas(), gravadas_ate(&conn, N).as_slice());

    let csv = dataset::formatar_csv(&ds);
    for linha in csv.lines().skip(1) {
        let campos: Vec<&str> = linha.split(',').collect();
        let concurso: u32 = campos[0].parse().expect("concurso");
        let esperado = if concurso <= CORTE {
            "busca"
        } else {
            "trancado"
        };
        assert_eq!(campos[4], esperado, "{linha}");
    }
    let m = dataset::manifesto(&ds, Formato::Csv, csv.as_bytes());
    assert_eq!((m.trecho, m.ultimo_concurso), ("busca+trancado", N));
}

#[test]
fn com_o_corte_real_o_historico_curto_e_todo_busca() {
    let mut conn = preparado(N);
    let ds = dataset_de_features(&mut conn, lotofacil(), false).expect("dataset");
    assert_eq!(concursos(&ds), (1..=N).collect::<Vec<_>>());
    assert!(ds.concursos().iter().all(|c| c.trecho == Trecho::Busca));
}

#[test]
fn snapshot_do_proximo_concurso_nunca_entra_e_saiu_bate_com_o_sorteio() {
    let hist = historico(lotofacil(), N);
    let mut conn = em_memoria(&hist);
    // O snapshot do N + 1 está gravado (saiu NULL), mas não pode entrar.
    assert_eq!(
        db::maior_concurso_features(&conn).expect("maior"),
        Some(N + 1)
    );
    let ds = dataset_de_features(&mut conn, &com_corte(), true).expect("dataset");
    assert!(ds.linhas().iter().all(|f| f.concurso <= N));
    for f in ds.linhas() {
        let sorteado = hist[(f.concurso - 1) as usize].dezenas.contains(&f.dezena);
        assert_eq!(f.saiu, Some(sorteado), "{f:?}");
    }
    // Data de cada concurso vem de `resultados`.
    for c in ds.concursos() {
        assert_eq!(c.data, hist[(c.concurso - 1) as usize].data_sorteio);
    }
}

#[test]
fn concursos_novos_nao_mudam_as_linhas_dos_antigos() {
    // Ponto no tempo: o dataset com 30 concursos e o com 40 têm as mesmas
    // linhas nos concursos 1 … 30 (nada do futuro entra no passado).
    let hist = historico(lotofacil(), N);
    let mut curto = em_memoria(&hist[..30]);
    let mut longo = em_memoria(&hist);
    let a = dataset_de_features(&mut curto, lotofacil(), false).expect("curto");
    let b = dataset_de_features(&mut longo, lotofacil(), false).expect("longo");
    let ate_30: Vec<&FeaturesDezena> = b.linhas().iter().filter(|f| f.concurso <= 30).collect();
    assert_eq!(a.linhas().iter().collect::<Vec<_>>(), ate_30);
}

#[test]
fn alterar_o_sorteio_de_t_so_muda_o_saiu_de_t_no_passado() {
    // Trocar as dezenas do concurso 30 não muda nenhum componente dos
    // concursos ≤ 30 (o snapshot de 30 é de antes do sorteio); só o `saiu`
    // do próprio 30 muda.
    let cfg = lotofacil();
    let hist = historico(cfg, N);
    let mut alterado = hist.clone();
    let antigas = alterado[29].dezenas.clone();
    let nova = cfg
        .dezenas()
        .find(|d| !antigas.contains(d))
        .expect("dezena livre");
    alterado[29].dezenas[0] = nova;

    let mut a = em_memoria(&hist);
    let mut b = em_memoria(&alterado);
    let da = dataset_de_features(&mut a, cfg, false).expect("a");
    let db_ = dataset_de_features(&mut b, cfg, false).expect("b");
    let mut saiu_mudou = 0;
    for (x, y) in da.linhas().iter().zip(db_.linhas()) {
        if x.concurso > 30 {
            break;
        }
        let sem_saiu = |f: &FeaturesDezena| FeaturesDezena {
            saiu: None,
            ..f.clone()
        };
        assert_eq!(sem_saiu(x), sem_saiu(y), "concurso {}", x.concurso);
        if x.saiu != y.saiu {
            assert_eq!(x.concurso, 30);
            saiu_mudou += 1;
        }
    }
    // Duas dezenas (a que saiu e a que entrou) × 4 janelas.
    assert_eq!(saiu_mudou, 2 * JANELAS.len());
}

#[test]
fn manifesto_traz_a_impressao_de_motor_estado() {
    let hist = historico(lotofacil(), N);
    let mut conn = em_memoria(&hist);
    let ds = dataset_de_features(&mut conn, lotofacil(), false).expect("dataset");
    let salvo = db::ler_motor_estado(&conn)
        .expect("estado")
        .expect("gravado");
    let m = dataset::manifesto(&ds, Formato::Csv, b"");
    assert_eq!(m.impressao_resultados_banco, salvo.impressao);
    assert_eq!(m.impressao_resultados_banco, impressao_resultados(&hist));
    // Sem corte no meio do histórico, os incluídos são o banco inteiro.
    assert_eq!(
        m.impressao_resultados_incluidos,
        m.impressao_resultados_banco
    );
}

#[test]
fn impressao_dos_incluidos_nao_muda_com_concurso_novo_acima_do_corte() {
    // Corte artificial em 25: com 40 e com 41 concursos o arquivo da busca
    // traz 1 a 25. A impressão do banco muda com o 41; a dos incluídos, não.
    let cfg = com_corte();
    let hist = historico(lotofacil(), N + 1);
    let mut antes = em_memoria(&hist[..N as usize]);
    let mut depois = em_memoria(&hist);
    let a = dataset::manifesto(
        &dataset_de_features(&mut antes, &cfg, false).expect("antes"),
        Formato::Csv,
        b"",
    );
    let b = dataset::manifesto(
        &dataset_de_features(&mut depois, &cfg, false).expect("depois"),
        Formato::Csv,
        b"",
    );
    assert_eq!(
        a.impressao_resultados_incluidos,
        b.impressao_resultados_incluidos
    );
    assert_eq!(
        a.impressao_resultados_incluidos,
        impressao_resultados(&hist[..CORTE as usize])
    );
    assert_ne!(a.impressao_resultados_banco, b.impressao_resultados_banco);
    assert_eq!(
        b.impressao_resultados_banco,
        impressao_resultados(&hist),
        "a do banco é a de todos os concursos"
    );

    // Com o trancado, o arquivo vai até o último sorteado: as duas coincidem
    // e acompanham o concurso novo.
    let t = dataset::manifesto(
        &dataset_de_features(&mut depois, &cfg, true).expect("trancado"),
        Formato::Csv,
        b"",
    );
    assert_eq!(
        t.impressao_resultados_incluidos,
        t.impressao_resultados_banco
    );
    assert_eq!(t.impressao_resultados_banco, b.impressao_resultados_banco);
}

#[test]
fn dataset_e_deterministico_nos_dois_formatos() {
    let mut a = preparado(N);
    let mut b = preparado(N);
    let da = dataset_de_features(&mut a, &com_corte(), true).expect("a");
    let db_ = dataset_de_features(&mut b, &com_corte(), true).expect("b");
    for formato in [Formato::Csv, Formato::Parquet] {
        let x = dataset::montar(&da, formato).expect("x");
        let y = dataset::montar(&db_, formato).expect("y");
        assert_eq!(x, y, "{formato:?}");
        assert_eq!(
            dataset::formatar_manifesto(&dataset::manifesto(&da, formato, &x)).expect("m"),
            dataset::formatar_manifesto(&dataset::manifesto(&db_, formato, &y)).expect("m"),
        );
    }
}

// ---------------------------------------------------------------------------
// Recusas.
// ---------------------------------------------------------------------------

#[test]
fn banco_sem_resultados_e_recusado() {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco");
    assert!(matches!(
        dataset_de_features(&mut conn, lotofacil(), false),
        Err(SpiderError::SemResultados(_))
    ));
}

#[test]
fn resultados_sem_features_sao_recusados() {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco");
    db::salvar_resultados(&mut conn, &historico(lotofacil(), N)).expect("resultados");
    assert!(matches!(
        dataset_de_features(&mut conn, lotofacil(), false),
        Err(SpiderError::SemFeatures(_))
    ));
}

#[test]
fn features_desatualizadas_sao_recusadas() {
    let hist = historico(lotofacil(), N + 1);
    let mut conn = em_memoria(&hist[..N as usize]);
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    match dataset_de_features(&mut conn, lotofacil(), false) {
        Err(SpiderError::FeaturesDesatualizadas {
            ultimo_concurso,
            snapshot,
            ..
        }) => assert_eq!((ultimo_concurso, snapshot), (N + 1, N + 1)),
        outro => panic!("esperava FeaturesDesatualizadas, veio {outro:?}"),
    }
}

#[test]
fn resultado_antigo_corrigido_sem_sync_features_e_recusado() {
    let cfg = lotofacil();
    let hist = historico(cfg, N);
    let mut conn = em_memoria(&hist);
    let mut corrigido = hist[9].clone();
    corrigido.data_sorteio = "2019-12-31".to_string();
    db::salvar_resultados(&mut conn, &[corrigido]).expect("correção");
    assert!(matches!(
        dataset_de_features(&mut conn, cfg, false),
        Err(SpiderError::FeaturesDeResultadosAlterados { .. })
    ));
    // Depois do `sync features`, passa.
    sincronizar_features(&mut conn, cfg, false).expect("features");
    dataset_de_features(&mut conn, cfg, false).expect("dataset em dia");
}

#[test]
fn features_de_outra_versao_sao_recusadas() {
    let mut conn = preparado(N);
    conn.execute(
        "UPDATE features_dezena SET feature_version = 99 WHERE concurso = 5 AND janela = 50",
        [],
    )
    .expect("versão alterada");
    match dataset_de_features(&mut conn, lotofacil(), false) {
        Err(SpiderError::FeaturesDeOutraVersao { encontrada, .. }) => assert_eq!(encontrada, 99),
        outro => panic!("esperava FeaturesDeOutraVersao, veio {outro:?}"),
    }
}

#[test]
fn janela_incompleta_e_recusada() {
    let mut conn = preparado(N);
    conn.execute(
        "DELETE FROM features_dezena WHERE concurso = 7 AND dezena = 3 AND janela = 100",
        [],
    )
    .expect("linha apagada");
    match dataset_de_features(&mut conn, lotofacil(), false) {
        Err(SpiderError::FeaturesIncoerentes { janela, .. }) => assert_eq!(janela, 100),
        outro => panic!("esperava FeaturesIncoerentes, veio {outro:?}"),
    }
}

#[test]
fn dataset_nao_grava_nada_no_banco() {
    let mut conn = preparado(N);
    let antes = db::ler_features(&conn).expect("features");
    let estado = db::ler_motor_estado(&conn).expect("estado");
    dataset_de_features(&mut conn, &com_corte(), true).expect("dataset");
    assert_eq!(db::ler_features(&conn).expect("features"), antes);
    assert_eq!(db::ler_motor_estado(&conn).expect("estado"), estado);
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
            "rust_spider_dataset_{rotulo}_{}_{unico}_{n}",
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

/// Banco em arquivo com `N` concursos e features.
fn ambiente(rotulo: &str) -> (Pasta, PathBuf) {
    let pasta = Pasta::nova(rotulo);
    let db_caminho = pasta.0.join("resultados_lotofacil.db");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco em arquivo");
        preparar(&mut conn, lotofacil(), &historico(lotofacil(), N));
    }
    (pasta, db_caminho)
}

fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_dataset(db: &Path, saida: &Path, extras: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["dataset".into(), "lotofacil".into()];
    for (opcao, caminho) in [("--db", db), ("--saida", saida)] {
        args.push(opcao.into());
        args.push(caminho.into());
    }
    args.extend(extras.iter().map(OsString::from));
    args
}

fn sucesso(out: &std::process::Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn cli_grava_os_dois_formatos_com_manifesto_sem_tocar_no_banco() {
    let (pasta, db_caminho) = ambiente("formatos");
    let saida = pasta.0.join("datasets");
    let banco_antes = std::fs::read(&db_caminho).expect("banco lido");
    let pasta_lot = saida.join("lotofacil");

    for (formato, ext) in [
        (None, "parquet"),
        (Some("csv"), "csv"),
        (Some("parquet"), "parquet"),
    ] {
        let extras: Vec<&str> = formato.map_or_else(Vec::new, |f| vec!["--formato", f]);
        let stdout = sucesso(&sync(&pasta.0, &args_dataset(&db_caminho, &saida, &extras)));
        assert!(stdout.contains("(busca)"), "{stdout}");
        assert!(stdout.contains(&format!("features_v1.{ext}")), "{stdout}");

        let bytes = std::fs::read(pasta_lot.join(format!("features_v1.{ext}"))).expect("dados");
        let texto = std::fs::read_to_string(pasta_lot.join(format!("manifesto_{ext}.json")))
            .expect("manifesto");
        let m: serde_json::Value = serde_json::from_str(&texto).expect("JSON");
        assert_eq!(m["loteria"], "lotofacil");
        assert_eq!(m["formato"], ext);
        assert_eq!(m["arquivo"], format!("features_v1.{ext}"));
        assert_eq!(m["linhas"], N * 25 * JANELAS.len() as u32);
        assert_eq!(m["ultimo_concurso"], N);
        assert_eq!(
            m["ultimo_concurso_busca"],
            lotofacil().ultimo_concurso_busca
        );
        assert_eq!(m["trecho"], "busca");
        assert_eq!(m["sha256"], dataset::sha256_hex(&bytes));
        assert!(!texto.contains("gerado"), "sem data de geração: {texto}");
    }
    // Os dois formatos convivem na pasta, cada um com seu manifesto.
    let csv = std::fs::read_to_string(pasta_lot.join("features_v1.csv")).expect("CSV");
    assert_eq!(csv.lines().next(), Some(COLUNAS.join(",").as_str()));
    assert_eq!(csv.lines().count(), 1 + (N * 25) as usize * JANELAS.len());
    assert!(pasta_lot.join("manifesto_parquet.json").exists());
    assert_eq!(std::fs::read(&db_caminho).expect("banco lido"), banco_antes);
}

#[test]
fn cli_duas_execucoes_dao_os_mesmos_bytes() {
    let (pasta, db_caminho) = ambiente("deterministico");
    for ext in ["csv", "parquet"] {
        let mut copias = Vec::new();
        for rodada in 0..2 {
            let saida = pasta.0.join(format!("saida_{rodada}"));
            sucesso(&sync(
                &pasta.0,
                &args_dataset(&db_caminho, &saida, &["--formato", ext]),
            ));
            let pasta_lot = saida.join("lotofacil");
            copias.push((
                std::fs::read(pasta_lot.join(format!("features_v1.{ext}"))).expect("dados"),
                std::fs::read(pasta_lot.join(format!("manifesto_{ext}.json"))).expect("manifesto"),
            ));
        }
        assert!(copias[0] == copias[1], "{ext}: bytes diferentes");
    }
}

#[test]
fn cli_com_trancado_avisa_e_marca_o_manifesto() {
    let (pasta, db_caminho) = ambiente("trancado");
    let saida = pasta.0.join("datasets");
    let out = sync(
        &pasta.0,
        &args_dataset(&db_caminho, &saida, &["--formato", "csv", "--trancado"]),
    );
    let stdout = sucesso(&out);
    assert!(stdout.contains("(busca+trancado)"), "{stdout}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("TRANCADO"),
        "aviso na saída de erro"
    );
    let pasta_lot = saida.join("lotofacil");
    let texto =
        std::fs::read_to_string(pasta_lot.join("manifesto_trancado_csv.json")).expect("manifesto");
    assert!(texto.contains("\"trecho\": \"busca+trancado\""), "{texto}");
    assert!(
        texto.contains("\"arquivo\": \"features_v1_trancado.csv\""),
        "{texto}"
    );
    assert!(pasta_lot.join("features_v1_trancado.csv").exists());
    // Só o par do trancado: nenhum arquivo da busca foi criado.
    assert!(!pasta_lot.join("features_v1.csv").exists());
    assert!(!pasta_lot.join("manifesto_csv.json").exists());
}

/// Lê todos os arquivos da pasta: nome -> bytes, em ordem de nome.
fn conteudo(pasta: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(pasta)
        .expect("pasta")
        .map(|e| {
            let e = e.expect("entrada");
            (
                e.file_name().to_string_lossy().into_owned(),
                std::fs::read(e.path()).expect("arquivo"),
            )
        })
        .collect()
}

#[test]
fn cli_trancado_nao_sobrescreve_nem_apaga_os_arquivos_da_busca() {
    let (pasta, db_caminho) = ambiente("busca_e_trancado");
    let saida = pasta.0.join("datasets");
    let pasta_lot = saida.join("lotofacil");
    for ext in ["csv", "parquet"] {
        sucesso(&sync(
            &pasta.0,
            &args_dataset(&db_caminho, &saida, &["--formato", ext]),
        ));
    }
    let busca = conteudo(&pasta_lot);
    assert_eq!(
        busca.keys().cloned().collect::<Vec<_>>(),
        [
            "features_v1.csv",
            "features_v1.parquet",
            "manifesto_csv.json",
            "manifesto_parquet.json"
        ]
    );

    for ext in ["csv", "parquet"] {
        sucesso(&sync(
            &pasta.0,
            &args_dataset(&db_caminho, &saida, &["--formato", ext, "--trancado"]),
        ));
    }
    let depois = conteudo(&pasta_lot);
    // Os arquivos da busca continuam com os mesmos bytes...
    for (nome, bytes) in &busca {
        assert_eq!(depois.get(nome), Some(bytes), "{nome} mudou ou sumiu");
    }
    // ...e os do trancado existem ao lado, cada manifesto apontando o seu.
    for ext in ["csv", "parquet"] {
        let dados = &depois[&format!("features_v1_trancado.{ext}")];
        let m: serde_json::Value =
            serde_json::from_slice(&depois[&format!("manifesto_trancado_{ext}.json")])
                .expect("JSON");
        assert_eq!(m["arquivo"], format!("features_v1_trancado.{ext}"));
        assert_eq!(m["trecho"], "busca+trancado");
        assert_eq!(m["sha256"], dataset::sha256_hex(dados));
    }
    assert_eq!(depois.len(), 8);

    // Regravar a busca depois não toca no trancado.
    let trancado: Vec<(String, Vec<u8>)> = depois
        .into_iter()
        .filter(|(n, _)| n.contains("trancado"))
        .collect();
    sucesso(&sync(
        &pasta.0,
        &args_dataset(&db_caminho, &saida, &["--formato", "csv"]),
    ));
    let por_ultimo = conteudo(&pasta_lot);
    for (nome, bytes) in &trancado {
        assert_eq!(por_ultimo.get(nome), Some(bytes), "{nome} mudou ou sumiu");
    }
}

/// Concursos sintéticos além do corte real da Mega-Sena no teste da CLI com
/// trancado de verdade.
const MEGA_ALEM_DO_CORTE: u32 = 3;

/// Trecho de cada concurso num CSV do dataset (confere que todas as linhas
/// do concurso têm o mesmo).
fn trechos_do_csv(caminho: &Path) -> BTreeMap<u32, String> {
    let csv = std::fs::read_to_string(caminho).expect("CSV");
    let mut trechos = BTreeMap::new();
    for linha in csv.lines().skip(1) {
        let campos: Vec<&str> = linha.split(',').collect();
        let c: u32 = campos[0].parse().expect("concurso");
        let trecho = campos[4].to_string();
        if let Some(anterior) = trechos.insert(c, trecho.clone()) {
            assert_eq!(anterior, trecho, "{linha}");
        }
    }
    trechos
}

#[test]
fn cli_com_trancado_de_verdade_marca_as_linhas_acima_do_corte_real() {
    // A CLI não aceita outro corte (é fixo em `LoteriaConfig`, de propósito),
    // então o banco sintético tem de passar do corte real. A Mega-Sena tem o
    // menor corte das três (e 60 dezenas), o banco mais barato para isso.
    let mega = config::buscar("megasena").expect("mega-sena configurada");
    let corte = mega.ultimo_concurso_busca;
    let n = corte + MEGA_ALEM_DO_CORTE;
    let pasta = Pasta::nova("mega_trancado");
    let db_caminho = pasta.0.join("resultados_megasena.db");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco em arquivo");
        preparar(&mut conn, mega, &historico(mega, n));
    }
    let saida = pasta.0.join("datasets");
    let pasta_mega = saida.join("megasena");
    let args = |extras: &[&str]| -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["dataset".into(), "megasena".into()];
        for (opcao, caminho) in [("--db", &db_caminho), ("--saida", &saida)] {
            a.push(opcao.into());
            a.push(caminho.into());
        }
        a.extend(extras.iter().map(OsString::from));
        a
    };
    sucesso(&sync(&pasta.0, &args(&["--formato", "csv"])));
    sucesso(&sync(&pasta.0, &args(&["--formato", "csv", "--trancado"])));

    let busca = trechos_do_csv(&pasta_mega.join("features_v1.csv"));
    assert_eq!(busca.len(), corte as usize);
    assert_eq!(busca.keys().next_back(), Some(&corte));
    assert!(busca.values().all(|t| t == "busca"));

    let trancado = trechos_do_csv(&pasta_mega.join("features_v1_trancado.csv"));
    assert_eq!(trancado.len(), n as usize);
    for (c, t) in &trancado {
        let esperado = if *c <= corte { "busca" } else { "trancado" };
        assert_eq!(t, esperado, "concurso {c}");
    }
    assert_eq!(
        trancado.values().filter(|t| *t == "trancado").count(),
        MEGA_ALEM_DO_CORTE as usize
    );

    let ler = |nome: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(pasta_mega.join(nome)).expect("manifesto"))
            .expect("JSON")
    };
    let mt = ler("manifesto_trancado_csv.json");
    assert_eq!(mt["ultimo_concurso"], n);
    assert_eq!(mt["ultimo_concurso_busca"], corte);
    assert_eq!(
        mt["impressao_resultados_incluidos"],
        mt["impressao_resultados_banco"]
    );
    let mb = ler("manifesto_csv.json");
    assert_eq!(mb["ultimo_concurso"], corte);
    assert_eq!(
        mb["impressao_resultados_banco"],
        mt["impressao_resultados_banco"]
    );
    assert_ne!(
        mb["impressao_resultados_incluidos"],
        mb["impressao_resultados_banco"]
    );
}

#[test]
fn cli_todas_e_recusado() {
    let pasta = Pasta::nova("todas");
    let out = sync(&pasta.0, &["dataset".into(), "todas".into()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("`dataset` não aceita `todas`"));
}

#[test]
fn cli_formato_desconhecido_e_recusado() {
    let pasta = Pasta::nova("formato");
    let args: Vec<OsString> = ["dataset", "lotofacil", "--formato", "xlsx"]
        .iter()
        .map(OsString::from)
        .collect();
    assert!(!sync(&pasta.0, &args).status.success());
}

#[test]
fn cli_banco_inexistente_falha_sem_criar_arquivo() {
    let pasta = Pasta::nova("sem_banco");
    let db_caminho = pasta.0.join("nao_existe.db");
    let saida = pasta.0.join("datasets");
    let out = sync(&pasta.0, &args_dataset(&db_caminho, &saida, &[]));
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("banco não encontrado"));
    assert!(!db_caminho.exists());
    assert!(!saida.exists());
}

#[test]
fn cli_features_desatualizadas_falha_sem_gravar() {
    let (pasta, db_caminho) = ambiente("desatualizado");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco");
        let hist = historico(lotofacil(), N + 1);
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    }
    let saida = pasta.0.join("datasets");
    let out = sync(&pasta.0, &args_dataset(&db_caminho, &saida, &[]));
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("sync features"));
    assert!(!saida.exists());
}
