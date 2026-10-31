//! Testes de `orquestracao::avaliar_g1_do_banco` e do comando
//! `sync posicoes --avaliar` (Fase 1g, tarefa 1g.4) sobre bancos em memória e em
//! arquivo: só lê, recusa o que não está em dia, bate com as tabelas gravadas
//! pelo `sync posicoes` e pelo `sync ranking`, e a saída é determinística.
//!
//! A matemática (régua da 1c/1e, bootstrap, top-k) é de `avaliacao` e já tem
//! testes próprios; aqui se confere a ligação com o banco.

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::avaliacao::{self, LinhaG1, AQUECIMENTO};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    avaliar_g1_do_banco, sincronizar_features, sincronizar_indice, sincronizar_posicoes,
    sincronizar_ranking_com,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::Resultado;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const NOME_FORCA: &str = "teste_v1";
const NOME_POS: &str = "pos_teste_v1";
const CRIADO_EM: &str = "2026-10-31T10:00:00Z";
const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Passa do aquecimento (100) com folga para as 5 dobras do walk-forward.
const N: u32 = 160;

const PERFIL_FORCA: &str = r#"nome = "teste_v1"
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

const PERFIL_POS: &str = r#"nome = "pos_teste_v1"
versao = 1
loteria = "lotofacil"
janela = 10
normalizacao = "zscore"

[pesos]
frequencia = 1.0
maior_espaco = 0.5
maior_espaco_ocorrencias = 0.0
menor_espaco = 0.0
menor_espaco_ocorrencias = 0.0
soma_variacoes = -0.5
espaco_final = 1.0
repeticoes_total = 0.5
repeticao_atual = 0.5
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

fn carregado(texto: &str, arquivo: &str) -> PerfilCarregado {
    perfil::interpretar(texto, Path::new(arquivo)).expect("perfil")
}

fn perfil_forca() -> PerfilCarregado {
    carregado(PERFIL_FORCA, "perfis/lotofacil/teste_v1.toml")
}

fn perfil_pos() -> PerfilCarregado {
    carregado(PERFIL_POS, "perfis/lotofacil/pos_teste_v1.toml")
}

fn preparar(conn: &mut Connection, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, lotofacil(), false).expect("features");
    sincronizar_indice(conn, lotofacil(), &perfil_forca(), CRIADO_EM).expect("índice");
}

fn banco(n: u32) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, &historico(lotofacil(), n));
    conn
}

/// Todas as linhas de uma consulta, cada valor em `Debug`.
fn despejar(conn: &Connection, sql: &str) -> Vec<String> {
    let mut stmt = conn.prepare(sql).expect("prepara");
    let colunas = stmt.column_count();
    let linhas = stmt
        .query_map([], |row| {
            let mut texto = Vec::with_capacity(colunas);
            for i in 0..colunas {
                texto.push(format!("{:?}", row.get::<_, Value>(i)?));
            }
            Ok(texto.join("|"))
        })
        .expect("consulta");
    linhas
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("linhas")
}

/// O banco inteiro: o nome de cada tabela e todas as suas linhas.
fn tudo(conn: &Connection) -> Vec<String> {
    let nomes = despejar(
        conn,
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    );
    let mut saida = Vec::new();
    for nome in nomes {
        // `despejar` devolve o valor em `Debug`: `Text("tabela")`.
        let tabela = nome
            .strip_prefix("Text(\"")
            .and_then(|t| t.strip_suffix("\")"))
            .expect("nome de tabela em texto");
        saida.push(format!("== {tabela}"));
        saida.extend(despejar(conn, &format!("SELECT * FROM {tabela}")));
    }
    saida
}

fn sem_tabelas_de_posicoes(conn: &Connection) -> bool {
    despejar(
        conn,
        "SELECT name FROM sqlite_master WHERE name LIKE 'posicao\\_%' ESCAPE '\\'",
    )
    .is_empty()
}

// ---------------------------------------------------------------------------
// A avaliação.
// ---------------------------------------------------------------------------

#[test]
fn avalia_sem_gravar_nada_e_com_o_recorte_esperado() {
    let mut conn = banco(N);
    let antes = tudo(&conn);

    let r = avaliar_g1_do_banco(&mut conn, lotofacil(), &perfil_forca(), &perfil_pos())
        .expect("avaliação");

    // Só leu: nenhuma tabela nova nem linha alterada.
    assert_eq!(tudo(&conn), antes);
    assert!(sem_tabelas_de_posicoes(&conn));
    assert!(!r.registrado, "o `sync posicoes` ainda não rodou");

    let av = &r.avaliacao;
    assert_eq!((r.ultimo_concurso, av.aquecimento, av.k), (N, 100, 15));
    assert_eq!(
        av.dobras.iter().map(|d| d.dobra).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    // A série avaliada começa depois do aquecimento e vai até o último sorteio.
    assert!(av.faixa.primeiro > AQUECIMENTO + 1);
    assert_eq!(av.faixa.ultimo, N);
    assert_eq!(av.concursos as u64, av.faixa.tamanho());
    // Mesmas dobras e mesmo total de concursos nas duas ordens.
    assert_eq!(av.g.posicao.total.concursos_avaliados, av.concursos);
    assert_eq!(av.a.posicao.total.concursos_avaliados, av.concursos);
}

#[test]
fn bate_com_as_tabelas_gravadas_pelo_sync_posicoes_e_pelo_sync_ranking() {
    let mut conn = banco(N);
    let avaliada = avaliar_g1_do_banco(&mut conn, lotofacil(), &perfil_forca(), &perfil_pos())
        .expect("avaliação");

    // Caminho independente: grava as duas tabelas e monta as linhas só com SQL.
    sincronizar_posicoes(
        &mut conn,
        lotofacil(),
        &perfil_forca(),
        &perfil_pos(),
        CRIADO_EM,
    )
    .expect("posições");
    sincronizar_ranking_com(&mut conn, lotofacil(), &perfil_forca(), 2, 1).expect("ranking");

    let resultados = db::listar_crescente(&conn, "lotofacil").expect("resultados");
    let sorteadas: HashMap<u32, Vec<u32>> = resultados
        .iter()
        .map(|r| (r.concurso, r.dezenas.clone()))
        .collect();
    let g_1: HashMap<(u32, u32), u32> = {
        let mut stmt = conn
            .prepare("SELECT concurso, posicao, g_1 FROM posicao_indice")
            .unwrap();
        let linhas = stmt
            .query_map([], |r| {
                Ok((
                    (r.get::<_, u32>(0)?, r.get::<_, u32>(1)?),
                    r.get::<_, u32>(2)?,
                ))
            })
            .unwrap();
        linhas.map(|l| l.unwrap()).collect()
    };
    let mut stmt = conn
        .prepare(
            "SELECT concurso, dezena, posicao_indice, posicao_ranking_a FROM ranking_taxa \
             WHERE concurso <= ?1 ORDER BY concurso, dezena",
        )
        .unwrap();
    let linhas: Vec<LinhaG1> = stmt
        .query_map([N], |r| {
            let (concurso, dezena, posicao_indice, a): (u32, u32, u32, u32) =
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
            Ok(LinhaG1 {
                concurso,
                dezena,
                posicao_ranking_a: a,
                g_1: g_1[&(concurso, posicao_indice)],
                saiu: sorteadas[&concurso].contains(&dezena),
            })
        })
        .unwrap()
        .map(|l| l.unwrap())
        .collect();
    assert_eq!(linhas.len(), (N * 25) as usize);
    let esperada = avaliacao::avaliar_g1(lotofacil(), &linhas, 15).expect("pura");
    assert_eq!(avaliada.avaliacao, esperada);
}

#[test]
fn avaliar_nao_depende_de_o_sync_posicoes_ter_rodado_mas_acusa_o_registro() {
    let mut conn = banco(N);
    let antes =
        avaliar_g1_do_banco(&mut conn, lotofacil(), &perfil_forca(), &perfil_pos()).expect("antes");
    assert!(!antes.registrado);

    sincronizar_posicoes(
        &mut conn,
        lotofacil(),
        &perfil_forca(),
        &perfil_pos(),
        CRIADO_EM,
    )
    .expect("posições");
    let depois = avaliar_g1_do_banco(&mut conn, lotofacil(), &perfil_forca(), &perfil_pos())
        .expect("depois");
    assert!(depois.registrado);
    // Registrar o perfil não muda nenhum número.
    assert_eq!(antes.avaliacao, depois.avaliacao);
}

#[test]
fn recusas_nao_gravam_nada() {
    let cfg = lotofacil();

    // Perfil das posições alterado depois de registrado.
    let mut conn = banco(N);
    sincronizar_posicoes(&mut conn, cfg, &perfil_forca(), &perfil_pos(), CRIADO_EM).unwrap();
    let antes = tudo(&conn);
    let alterado = carregado(
        &PERFIL_POS.replace("frequencia = 1.0", "frequencia = 2.0"),
        "perfis/lotofacil/pos_teste_v1.toml",
    );
    let erro = avaliar_g1_do_banco(&mut conn, cfg, &perfil_forca(), &alterado).unwrap_err();
    assert!(matches!(erro, SpiderError::PerfilAlterado { .. }), "{erro}");
    assert_eq!(tudo(&conn), antes);

    // Perfil das posições de outra loteria.
    let quina = carregado(
        &PERFIL_POS.replace("lotofacil", "quina"),
        "perfis/quina/pos_teste_v1.toml",
    );
    let erro = avaliar_g1_do_banco(&mut conn, cfg, &perfil_forca(), &quina).unwrap_err();
    assert!(
        matches!(erro, SpiderError::PerfilDeOutraLoteria { .. }),
        "{erro}"
    );

    // Índice de força defasado: concurso novo sem `sync features`/`sync indice`.
    let hist = historico(cfg, N + 1);
    let mut defasado = banco(N);
    db::salvar_resultados(&mut defasado, &hist[N as usize..]).unwrap();
    let erro = avaliar_g1_do_banco(&mut defasado, cfg, &perfil_forca(), &perfil_pos()).unwrap_err();
    assert!(erro.to_string().contains("sync features"), "{erro}");

    // Menos concursos que o aquecimento mais as dobras: nada para avaliar.
    let mut curto = banco(40);
    let erro = avaliar_g1_do_banco(&mut curto, cfg, &perfil_forca(), &perfil_pos()).unwrap_err();
    assert!(
        matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
        "{erro}"
    );
    assert!(sem_tabelas_de_posicoes(&curto));
}

#[test]
fn o_trancado_nao_entra_na_avaliacao_do_banco() {
    // Corte da busca baixo (140 de 160 concursos): o que está acima dele é o
    // trancado. Trocar os sorteios do 141 em diante, e com eles as features e o
    // índice de força, não pode mudar nenhum número da avaliação.
    let cfg = LoteriaConfig {
        ultimo_concurso_busca: 140,
        ..*lotofacil()
    };
    let hist = historico(&cfg, N);
    let mut trocado = hist.clone();
    for r in trocado.iter_mut().filter(|r| r.concurso > 140) {
        for d in &mut r.dezenas {
            *d = *d % 25 + 1;
        }
    }
    assert_ne!(hist[150].dezenas, trocado[150].dezenas);

    let avaliar = |hist: &[Resultado]| {
        let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
        preparar(&mut conn, hist);
        avaliar_g1_do_banco(&mut conn, &cfg, &perfil_forca(), &perfil_pos()).expect("avaliação")
    };
    let (a, b) = (avaliar(&hist), avaliar(&trocado));
    assert_eq!(a.avaliacao.faixa.ultimo, 140);
    assert_eq!(a.avaliacao, b.avaliacao);
    // O último concurso lido é o do banco inteiro, só para as conferências.
    assert_eq!(a.ultimo_concurso, N);
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
            "rust_spider_posicoes_aval_{rotulo}_{}_{unico}_{n}",
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

/// Banco em arquivo com `N` concursos, features e índice de força, e os dois
/// `.toml` numa pasta `perfis/`.
fn ambiente(rotulo: &str) -> (Pasta, PathBuf, PathBuf) {
    let pasta = Pasta::nova(rotulo);
    let db_caminho = pasta.0.join("resultados_lotofacil.db");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco em arquivo");
        preparar(&mut conn, &historico(lotofacil(), N));
    }
    let perfis = pasta.0.join("perfis");
    std::fs::create_dir_all(perfis.join("lotofacil")).expect("pasta de perfis");
    std::fs::write(perfis.join("lotofacil").join("teste_v1.toml"), PERFIL_FORCA).unwrap();
    std::fs::write(
        perfis.join("lotofacil").join("pos_teste_v1.toml"),
        PERFIL_POS,
    )
    .unwrap();
    (pasta, db_caminho, perfis)
}

/// O `sync` roda dentro da pasta temporária: a trava e o log são relativos ao
/// diretório de trabalho, e o repositório não pode ganhar arquivos por teste.
fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_avaliar(db_caminho: &Path, perfis: &Path) -> Vec<OsString> {
    vec![
        "posicoes".into(),
        "lotofacil".into(),
        "--avaliar".into(),
        "--perfil".into(),
        NOME_FORCA.into(),
        "--perfil-posicoes".into(),
        NOME_POS.into(),
        "--db".into(),
        db_caminho.into(),
        "--perfis".into(),
        perfis.into(),
    ]
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn cli_avalia_so_le_e_a_saida_e_deterministica() {
    let (pasta, db_caminho, perfis) = ambiente("cli");
    let args = args_avaliar(&db_caminho, &perfis);
    let antes = {
        let conn = db::abrir_somente_leitura(&db_caminho).unwrap();
        tudo(&conn)
    };

    let primeira = sync(&pasta.0, &args);
    assert!(primeira.status.success(), "{}", texto(&primeira.stderr));
    let stdout = texto(&primeira.stdout);
    for trecho in [
        "Avaliação do índice de força das posições (G-1)",
        "AVISO: isto é a descrição do passado, não uma previsão",
        "Aquecimento: os concursos 1 a 100 ficam fora",
        "== Ordem G-1",
        "== Referência, nota A da 1f",
        "Veredito de G-1",
        "Top-15 de G-1",
        "Top-15 de A",
        "ainda não está em posicao_perfil",
        "descrição do passado",
    ] {
        assert!(stdout.contains(trecho), "falta `{trecho}` em:\n{stdout}");
    }
    // Sem CSV nem tabelas, e o banco ficou idêntico.
    assert!(!pasta.0.join("posicoes").exists());
    let conn = db::abrir_somente_leitura(&db_caminho).unwrap();
    assert_eq!(tudo(&conn), antes);
    assert!(sem_tabelas_de_posicoes(&conn));
    drop(conn);

    // Mesma entrada, mesmos bytes; o tempo vai para a saída de erro.
    let segunda = sync(&pasta.0, &args);
    assert!(segunda.status.success());
    assert_eq!(texto(&segunda.stdout), stdout);
    assert!(texto(&segunda.stderr).contains("Tempo:"));
    assert!(!stdout.contains("Tempo:"));
}

#[test]
fn cli_recusa_avaliar_com_saida_banco_inexistente_e_indice_defasado() {
    let (pasta, db_caminho, perfis) = ambiente("recusas");

    let com_saida = sync(
        &pasta.0,
        &[
            "posicoes".into(),
            "quina".into(),
            "--avaliar".into(),
            "--saida".into(),
            "x".into(),
        ],
    );
    assert!(!com_saida.status.success());

    let sem_banco = sync(
        &pasta.0,
        &args_avaliar(&pasta.0.join("nao_existe.db"), &perfis),
    );
    assert!(!sem_banco.status.success());
    assert!(texto(&sem_banco.stderr).contains("banco não encontrado"));
    assert!(!pasta.0.join("nao_existe.db").exists());

    {
        let mut conn = db::abrir(&db_caminho).expect("banco");
        let hist = historico(lotofacil(), N + 1);
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    }
    let defasado = sync(&pasta.0, &args_avaliar(&db_caminho, &perfis));
    assert!(!defasado.status.success());
    assert!(
        texto(&defasado.stderr).contains("sync features lotofacil"),
        "{}",
        texto(&defasado.stderr)
    );
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert!(sem_tabelas_de_posicoes(&conn));
}
