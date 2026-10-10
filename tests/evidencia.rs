//! Testes de `orquestracao::evidencia_do_banco` e do comando
//! `sync evidencia` (1d.2): evidência bayesiana por dezena sobre
//! `resultados`, só leitura, com a recusa do `validar`.

use rust_spider::avaliacao::{self, Grau, Janela, DESVIOS_PRIORI, QTD_PRIORIS};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::evidencia_do_banco;
use rust_spider::Resultado;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const SEMENTE: u64 = 0x9E37_79B9_7F4A_7C15;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

/// Histórico sintético determinístico (xorshift64), concursos 1..=`n`.
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
                data_sorteio: format!(
                    "{}-{:02}-{:02}",
                    1990 + c / 336,
                    1 + (c % 336) / 28,
                    1 + c % 28
                ),
                dezenas,
            }
        })
        .collect()
}

fn banco(resultados: &[Resultado]) -> rusqlite::Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    db::salvar_resultados(&mut conn, resultados).expect("resultados");
    conn
}

#[test]
fn relatorio_cobre_as_duas_janelas_para_o_proximo_concurso() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let n = 700;
        let conn = banco(&historico(c, n));
        let r = evidencia_do_banco(&conn, c).expect("relatório");

        assert_eq!(r.loteria, slug);
        assert_eq!(r.ultimo_concurso, n);
        assert_eq!(r.qtd_dezenas, c.qtd_dezenas);
        assert_eq!(r.dezenas_possiveis, c.qtd_dezenas_possiveis());
        assert!(r.avisos.is_empty());
        assert_eq!(r.janelas.len(), 2);

        let (inteira, recente) = (&r.janelas[0], &r.janelas[1]);
        assert_eq!(inteira.evidencia.janela, Janela::Inteira);
        assert_eq!(
            recente.evidencia.janela,
            Janela::Ultimos(avaliacao::JANELA_SECUNDARIA)
        );
        // Snapshot do próximo concurso: só os já sorteados.
        for j in &r.janelas {
            assert_eq!(j.evidencia.t, n + 1);
            assert_eq!(j.evidencia.ultimo_concurso, n);
            assert_eq!(
                j.evidencia.dezenas.len(),
                c.qtd_dezenas_possiveis() as usize
            );
            // O esperado é simulado com o mesmo n da janela, nas 3 prioris.
            assert_eq!(j.esperado.concursos, j.evidencia.concursos);
            assert_eq!(j.esperado.replicas, avaliacao::REPLICAS_ESPERADO);
            assert_eq!(j.esperado.semente, avaliacao::SEMENTE_ESPERADO);
            assert_eq!(j.esperado.por_priori.len(), QTD_PRIORIS);
            for (esp, s) in j.esperado.por_priori.iter().zip(DESVIOS_PRIORI) {
                assert_eq!(esp.desvio, s);
            }
        }
        assert_eq!(inteira.evidencia.concursos, n as usize);
        assert_eq!(inteira.evidencia.primeiro_concurso, 1);
        assert_eq!(recente.evidencia.concursos, 500);
        assert_eq!(recente.evidencia.primeiro_concurso, n - 499);
    }
}

#[test]
fn historico_curto_faz_as_duas_janelas_coincidirem() {
    let c = cfg("lotofacil");
    let conn = banco(&historico(c, 40));
    let r = evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(r.janelas[1].evidencia.concursos, 40);
    assert_eq!(
        r.janelas[0].evidencia.dezenas,
        r.janelas[1].evidencia.dezenas
    );
}

#[test]
fn relatorio_e_deterministico() {
    let c = cfg("megasena");
    let conn = banco(&historico(c, 300));
    let a = evidencia_do_banco(&conn, c).expect("relatório");
    let b = evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(a, b);
}

#[test]
fn banco_vazio_e_sem_resultados() {
    let c = cfg("quina");
    let conn = banco(&[]);
    assert!(matches!(
        evidencia_do_banco(&conn, c),
        Err(SpiderError::SemResultados(_))
    ));
}

#[test]
fn buraco_e_recusado_pelo_validar() {
    let c = cfg("lotofacil");
    let mut hist = historico(c, 50);
    hist.remove(24); // sem o concurso 25
    let conn = banco(&hist);
    match evidencia_do_banco(&conn, c) {
        Err(SpiderError::ResultadosReprovados { loteria, erros, .. }) => {
            assert_eq!(loteria, "lotofacil");
            assert!(!erros.is_empty());
        }
        outro => panic!("esperado ResultadosReprovados, veio {outro:?}"),
    }
}

#[test]
fn dezena_fora_da_faixa_e_recusada_pelo_validar() {
    let c = cfg("lotofacil");
    let mut hist = historico(c, 20);
    hist[5].dezenas[0] = 99;
    let conn = banco(&hist);
    assert!(matches!(
        evidencia_do_banco(&conn, c),
        Err(SpiderError::ResultadosReprovados { .. })
    ));
}

#[test]
fn calcular_nao_grava_nem_cria_tabela() {
    let c = cfg("lotofacil");
    let conn = banco(&historico(c, 120));
    let tabelas = |conn: &rusqlite::Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master ORDER BY name")
            .expect("consulta");
        stmt.query_map([], |row| row.get(0))
            .expect("linhas")
            .collect::<Result<_, _>>()
            .expect("nomes")
    };
    let antes = tabelas(&conn);
    let total_antes = db::listar_crescente(&conn, c.slug).expect("lista").len();
    evidencia_do_banco(&conn, c).expect("relatório");
    assert_eq!(tabelas(&conn), antes);
    assert_eq!(
        db::listar_crescente(&conn, c.slug).expect("lista").len(),
        total_antes
    );
    for nome in ["features_dezena", "motor_estado", "indice", "perfis"] {
        assert!(!db::tabela_existe(&conn, nome).expect("consulta"), "{nome}");
    }
}

#[test]
fn dezena_viciada_aparece_como_muito_forte_a_favor_de_vies() {
    // A dezena 7 sai em todos os 300 concursos; é a única que o relatório
    // deve apontar como "muito forte".
    let c = cfg("lotofacil");
    let mut hist = historico(c, 300);
    for r in &mut hist {
        if !r.dezenas.contains(&7) {
            r.dezenas[0] = 7;
            r.dezenas.sort_unstable();
            r.dezenas.dedup();
        }
        assert_eq!(r.dezenas.len(), 15, "o ajuste não deve repetir dezena");
    }
    let conn = banco(&hist);
    let r = evidencia_do_banco(&conn, c).expect("relatório");
    let d7 = &r.janelas[0].evidencia.dezenas[6];
    assert_eq!(d7.saidas, 300);
    assert_eq!(d7.classificacao[0].grau, Grau::MuitoForte);
    assert!(d7.log10_fb[0] > 2.0);
}

// ---------------------------------------------------------------------------
// CLI.
// ---------------------------------------------------------------------------

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
            "rust_spider_evidencia_{rotulo}_{}_{unico}_{n}",
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

fn sync(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn banco_em_arquivo(pasta: &Pasta, resultados: &[Resultado]) -> PathBuf {
    let caminho = pasta.0.join("resultados_lotofacil.db");
    let mut conn = db::abrir(&caminho).expect("banco em arquivo");
    db::salvar_resultados(&mut conn, resultados).expect("resultados");
    caminho
}

#[test]
fn cli_imprime_o_relatorio_sem_tocar_no_banco() {
    let pasta = Pasta::nova("ok");
    let c = cfg("lotofacil");
    let caminho = banco_em_arquivo(&pasta, &historico(c, 80));
    let antes = std::fs::read(&caminho).expect("bytes do banco");

    let saida = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let texto = String::from_utf8(saida.stdout.clone()).expect("UTF-8");
    for esperado in [
        "Evidência bayesiana por dezena de Lotofácil",
        "para o concurso 81",
        "K/N = 15/25 = 0.600000",
        "5.0 %",
        "2.5 %",
        "10.0 %",
        "AVISO: com 25 dezenas",
        "Janela 1: histórico inteiro (principal)",
        "Janela 2: últimos 500 concursos (secundária)",
        "Priori 5.0 % (principal):",
        "Priori 2.5 %:",
        "Priori 10.0 %:",
        "a favor do acaso",
        "esperadas sob o acaso",
    ] {
        assert!(texto.contains(esperado), "faltou `{esperado}` em:\n{texto}");
    }
    // Uma linha por dezena em cada janela: n = 80 na principal e na secundária.
    assert_eq!(texto.matches("      80  ").count(), 50, "{texto}");

    // Só leitura: o banco continua byte a byte igual.
    assert_eq!(std::fs::read(&caminho).expect("bytes do banco"), antes);

    // Mesma entrada, mesmos bytes (o tempo vai para o stderr).
    let de_novo = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert_eq!(de_novo.stdout, saida.stdout);
}

#[test]
fn cli_recusa_todas_e_banco_inexistente_e_buraco() {
    let pasta = Pasta::nova("recusas");

    let todas = sync(&pasta.0, &["evidencia", "todas"]);
    assert!(!todas.status.success());
    let erro = String::from_utf8_lossy(&todas.stderr);
    assert!(
        erro.contains("evidencia") && erro.contains("não aceita"),
        "{erro}"
    );
    assert!(todas.stdout.is_empty());

    let sem_banco = pasta.0.join("nao_existe.db");
    let saida = sync(
        &pasta.0,
        &["evidencia", "quina", "--db", sem_banco.to_str().unwrap()],
    );
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("banco não encontrado"));
    assert!(!sem_banco.exists(), "o comando não deve criar o banco");

    let c = cfg("lotofacil");
    let mut hist = historico(c, 30);
    hist.remove(14);
    let caminho = banco_em_arquivo(&pasta, &hist);
    let saida = sync(
        &pasta.0,
        &["evidencia", "lotofacil", "--db", caminho.to_str().unwrap()],
    );
    assert!(!saida.status.success());
    assert!(!String::from_utf8_lossy(&saida.stdout).contains("Evidência bayesiana"));
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(
        erro.contains("erro(s) de integridade") || erro.contains("faltam"),
        "{erro}"
    );
}
