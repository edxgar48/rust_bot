//! Testes de `orquestracao::sincronizar_indice` (comando `sync indice`, 1b.3)
//! sobre bancos em memória: gravação em `indice` e `perfis`, idempotência,
//! recusas sem gravar nada, atomicidade e ponto no tempo no nível do banco.
//!
//! O cálculo em si (normalização, total, posição, variação) é do motor e já
//! tem golden em `src/motor/indice.rs`; aqui o índice gravado é comparado,
//! bit a bit, com o motor puro aplicado às features do motor puro.

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db::{self, PerfilRegistrado};
use rust_spider::error::SpiderError;
use rust_spider::motor::{self, IndiceDezena, FEATURE_VERSION, JANELAS};
use rust_spider::orquestracao::{
    sincronizar_features, sincronizar_indice, RegistroPerfil, ResumoIndice,
};
use rust_spider::perfil::{self, Normalizacao, PerfilCarregado};
use rust_spider::Resultado;
use std::collections::BTreeMap;
use std::path::Path;

const CRIADO_EM: &str = "2026-10-05T10:00:00Z";

// ---------------------------------------------------------------------------
// Auxiliares.
// ---------------------------------------------------------------------------

fn cfg_de(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn quina() -> &'static LoteriaConfig {
    cfg_de("quina")
}

fn qtd_dezenas(cfg: &LoteriaConfig) -> usize {
    cfg.qtd_dezenas_possiveis() as usize
}

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: format!("2020-01-{:02}", 1 + concurso % 28),
        dezenas: dezenas.to_vec(),
    }
}

/// Histórico sintético determinístico (xorshift64): o prefixo de `n` concursos
/// é o mesmo para qualquer `n` maior com a mesma semente.
fn historico(cfg: &LoteriaConfig, n: u32, semente: u64) -> Vec<Resultado> {
    let mut x = semente;
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
            res(cfg, c, &dezenas)
        })
        .collect()
}

/// Mesmo concurso com outro conjunto de dezenas: cada dezena vira a seguinte
/// da faixa (a última volta à primeira).
fn com_dezenas_trocadas(cfg: &LoteriaConfig, r: &Resultado) -> Resultado {
    let mut novo = r.clone();
    novo.dezenas = r
        .dezenas
        .iter()
        .map(|&d| {
            if d == cfg.dezena_max {
                cfg.dezena_min
            } else {
                d + 1
            }
        })
        .collect();
    novo
}

fn banco() -> Connection {
    db::abrir(Path::new(":memory:")).expect("banco em memória")
}

fn gravar(conn: &mut Connection, resultados: &[Resultado]) {
    db::salvar_resultados(conn, resultados).expect("resultados gravados");
}

/// Banco novo com `hist` gravado e as features calculadas.
fn calculado(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = banco();
    gravar(&mut conn, hist);
    sincronizar_features(&mut conn, cfg, false).expect("features calculadas");
    conn
}

/// Texto de um perfil válido; `frequencia` varia para gerar conteúdos
/// (e hashes) diferentes.
fn texto_perfil(
    nome: &str,
    loteria: &str,
    janela: u32,
    normalizacao: Normalizacao,
    frequencia: f64,
) -> String {
    let versao: u32 = nome
        .rsplit("_v")
        .next()
        .and_then(|v| v.parse().ok())
        .expect("nome termina em _v<N>");
    format!(
        r#"nome = "{nome}"
versao = {versao}
loteria = "{loteria}"
janela = {janela}
normalizacao = "{norm}"

[pesos]
frequencia = {frequencia:?}
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"#,
        norm = normalizacao.como_str()
    )
}

fn carregar_texto(texto: &str, slug: &str, nome: &str) -> PerfilCarregado {
    perfil::interpretar(texto, Path::new(&format!("perfis/{slug}/{nome}.toml")))
        .expect("perfil válido")
}

fn perfil_de(slug: &str, nome: &str, janela: u32, normalizacao: Normalizacao) -> PerfilCarregado {
    let texto = texto_perfil(nome, slug, janela, normalizacao, 1.0);
    carregar_texto(&texto, slug, nome)
}

/// Perfil padrão dos testes de recusa: Quina, janela 10, minmax.
fn perfil_quina() -> PerfilCarregado {
    perfil_de("quina", "teste_v1", 10, Normalizacao::Minmax)
}

/// Índice esperado: motor puro sobre as features do motor puro (mais o
/// snapshot do próximo concurso), com os parâmetros do perfil.
fn referencia(cfg: &LoteriaConfig, hist: &[Resultado], p: &PerfilCarregado) -> Vec<IndiceDezena> {
    let (estado, mut features) = motor::calcular(cfg, hist).expect("motor");
    features.extend(estado.snapshot());
    motor::calcular_indice(
        &features,
        p.perfil.janela,
        p.perfil.normalizacao,
        &p.perfil.pesos,
    )
    .expect("índice puro")
}

type LinhaBits = (u32, u32, u64, u32, Option<i64>);

/// Linhas com o `total` em bits: igualdade byte a byte, sem tolerância.
fn bits(linhas: &[IndiceDezena]) -> Vec<LinhaBits> {
    linhas
        .iter()
        .map(|l| {
            (
                l.concurso,
                l.dezena,
                l.total.to_bits(),
                l.posicao,
                l.variacao_posicao,
            )
        })
        .collect()
}

fn lido(conn: &Connection, nome: &str) -> Vec<IndiceDezena> {
    db::ler_indice(conn, nome).expect("índice lido")
}

fn indexar(conn: &mut Connection, cfg: &LoteriaConfig, p: &PerfilCarregado) -> ResumoIndice {
    sincronizar_indice(conn, cfg, p, CRIADO_EM).expect("índice sincronizado")
}

fn contar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0))
        .expect("contagem lida")
}

/// Nomes de `indice` e `perfis` que existem no banco.
fn tabelas_do_indice(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name IN ('indice', 'perfis') ORDER BY name",
        )
        .expect("prepara");
    let nomes = stmt
        .query_map([], |row| row.get(0))
        .expect("consulta")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("nomes");
    nomes
}

/// Tudo o que `sync indice` pode alterar, para comparar antes e depois.
#[derive(Debug, PartialEq)]
struct Foto {
    tabelas: Vec<String>,
    total_de_linhas: i64,
    indices: Vec<(String, Vec<IndiceDezena>)>,
    perfis: Vec<Option<PerfilRegistrado>>,
}

fn foto(conn: &Connection, nomes: &[&str]) -> Foto {
    let tem_indice = db::tabela_existe(conn, "indice").expect("sqlite_master");
    let tem_perfis = db::tabela_existe(conn, "perfis").expect("sqlite_master");
    Foto {
        tabelas: tabelas_do_indice(conn),
        total_de_linhas: if tem_indice {
            contar(conn, "SELECT COUNT(*) FROM indice")
        } else {
            0
        },
        indices: nomes
            .iter()
            .map(|n| {
                let linhas = if tem_indice {
                    lido(conn, n)
                } else {
                    Vec::new()
                };
                (n.to_string(), linhas)
            })
            .collect(),
        perfis: nomes
            .iter()
            .map(|n| {
                if tem_perfis {
                    db::buscar_perfil(conn, n).expect("perfil lido")
                } else {
                    None
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 1. Caminho feliz: índice gravado == motor puro, nas 3 loterias, com janelas
//    e normalizações variadas.
// ---------------------------------------------------------------------------

fn caminho_feliz(slug: &str, n: u32, janela: u32, normalizacao: Normalizacao, semente: u64) {
    let cfg = cfg_de(slug);
    let hist = historico(cfg, n, semente);
    let p = perfil_de(slug, "teste_v1", janela, normalizacao);
    let mut conn = calculado(cfg, &hist);

    let resumo = indexar(&mut conn, cfg, &p);
    let linhas = (n as usize + 1) * qtd_dezenas(cfg);
    // n concursos sorteados + o snapshot de n + 1; banco novo: nada apagado.
    assert_eq!(
        resumo,
        ResumoIndice {
            perfil: "teste_v1".to_string(),
            registro: RegistroPerfil::Novo,
            concursos: n as usize + 1,
            primeiro_concurso: 1,
            ultimo_concurso: n,
            proximo_concurso: n + 1,
            linhas_apagadas: 0,
            linhas_gravadas: linhas,
        },
        "{slug} janela {janela}"
    );

    let esperado = referencia(cfg, &hist, &p);
    assert_eq!(esperado.len(), linhas);
    let gravado = lido(&conn, "teste_v1");
    assert_eq!(gravado.len(), linhas, "{slug}: linhas gravadas");
    assert_eq!(
        bits(&gravado),
        bits(&esperado),
        "{slug} janela {janela} {}: índice difere do motor",
        normalizacao.como_str()
    );
}

#[test]
fn lotofacil_janela_total_zscore_igual_ao_motor() {
    caminho_feliz("lotofacil", 100, 0, Normalizacao::Zscore, 0x1a2b_3c4d);
}

#[test]
fn lotofacil_janela_10_minmax_igual_ao_motor() {
    caminho_feliz("lotofacil", 100, 10, Normalizacao::Minmax, 0x2b3c_4d5e);
}

#[test]
fn megasena_janela_total_minmax_igual_ao_motor() {
    caminho_feliz("megasena", 60, 0, Normalizacao::Minmax, 0x3c4d_5e6f);
}

#[test]
fn megasena_janela_50_zscore_igual_ao_motor() {
    caminho_feliz("megasena", 60, 50, Normalizacao::Zscore, 0x4d5e_6f70);
}

#[test]
fn megasena_janela_100_maior_que_o_historico_igual_ao_motor() {
    // Janela maior que os 60 concursos: a janela nunca enche.
    caminho_feliz("megasena", 60, 100, Normalizacao::Minmax, 0x5e6f_7081);
}

#[test]
fn quina_janela_10_zscore_igual_ao_motor() {
    caminho_feliz("quina", 80, 10, Normalizacao::Zscore, 0x6f70_8192);
}

#[test]
fn quina_janela_100_minmax_igual_ao_motor() {
    caminho_feliz("quina", 120, 100, Normalizacao::Minmax, 0x7081_92a3);
}

#[test]
fn quina_todas_as_janelas_e_normalizacoes_no_mesmo_banco_igual_ao_motor() {
    let cfg = quina();
    let hist = historico(cfg, 60, 0x8192_a3b4);
    let mut conn = calculado(cfg, &hist);
    for (i, &janela) in JANELAS.iter().enumerate() {
        for normalizacao in [Normalizacao::Zscore, Normalizacao::Minmax] {
            let nome = format!("combo{}{}_v1", i, normalizacao.como_str());
            let p = perfil_de("quina", &nome, janela, normalizacao);
            indexar(&mut conn, cfg, &p);
            assert_eq!(
                bits(&lido(&conn, &nome)),
                bits(&referencia(cfg, &hist, &p)),
                "janela {janela} {}",
                normalizacao.como_str()
            );
        }
    }
    // 4 janelas × 2 normalizações, cada uma com 61 concursos × 80 dezenas.
    assert_eq!(
        contar(&conn, "SELECT COUNT(*) FROM indice"),
        8 * 61 * 80,
        "cada perfil grava só as próprias linhas"
    );
    assert_eq!(contar(&conn, "SELECT COUNT(*) FROM perfis"), 8);
}

#[test]
fn perfis_reais_padrao_v1_das_tres_loterias_igual_ao_motor() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("perfis");
    for (slug, n) in [("lotofacil", 70), ("megasena", 60), ("quina", 70)] {
        let cfg = cfg_de(slug);
        let p = perfil::carregar(&raiz, cfg, "padrao_v1").expect("perfil real");
        let hist = historico(cfg, n, 0x92a3_b4c5);
        let mut conn = calculado(cfg, &hist);
        let resumo = indexar(&mut conn, cfg, &p);
        assert_eq!(resumo.linhas_gravadas, (n as usize + 1) * qtd_dezenas(cfg));
        assert_eq!(
            bits(&lido(&conn, "padrao_v1")),
            bits(&referencia(cfg, &hist, &p)),
            "{slug}"
        );
    }
}

#[test]
fn indice_cobre_do_concurso_1_ao_snapshot_do_proximo() {
    let cfg = quina();
    let n = 25;
    let mut conn = calculado(cfg, &historico(cfg, n, 5));
    indexar(&mut conn, cfg, &perfil_quina());
    assert_eq!(contar(&conn, "SELECT MIN(concurso) FROM indice"), 1);
    assert_eq!(contar(&conn, "SELECT MAX(concurso) FROM indice"), 26);
    assert_eq!(
        contar(&conn, "SELECT COUNT(DISTINCT concurso) FROM indice"),
        26
    );
    // O snapshot (n + 1) tem as 80 dezenas indexadas e, nas features, `saiu`
    // ainda NULL: o índice dele nasce de um concurso que não aconteceu.
    assert_eq!(
        contar(&conn, "SELECT COUNT(*) FROM indice WHERE concurso = 26"),
        80
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM features_dezena WHERE concurso = 26 AND saiu IS NULL"
        ),
        80 * 4,
        "snapshot de 26 nas 4 janelas, com saiu NULL"
    );
}

// ---------------------------------------------------------------------------
// 2. Estrutura do índice gravado: variação NULL só no concurso 1, posições
//    em permutação, colunas coerentes entre si.
// ---------------------------------------------------------------------------

#[test]
fn variacao_posicao_nula_so_no_concurso_1_nas_tres_loterias() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg_de(slug);
        let mut conn = calculado(cfg, &historico(cfg, 30, 21));
        indexar(
            &mut conn,
            cfg,
            &perfil_de(slug, "teste_v1", 10, Normalizacao::Zscore),
        );
        assert_eq!(
            contar(
                &conn,
                "SELECT COUNT(*) FROM indice WHERE variacao_posicao IS NULL"
            ),
            qtd_dezenas(cfg) as i64,
            "{slug}: uma variação NULL por dezena"
        );
        assert_eq!(
            contar(
                &conn,
                "SELECT COUNT(*) FROM indice
                 WHERE variacao_posicao IS NULL AND concurso <> 1"
            ),
            0,
            "{slug}: NULL fora do concurso 1"
        );
    }
}

#[test]
fn posicoes_de_cada_concurso_sao_uma_permutacao_de_1_a_n() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg_de(slug);
        let mut conn = calculado(cfg, &historico(cfg, 30, 22));
        indexar(
            &mut conn,
            cfg,
            &perfil_de(slug, "teste_v1", 0, Normalizacao::Minmax),
        );
        let mut por_concurso: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for l in lido(&conn, "teste_v1") {
            por_concurso.entry(l.concurso).or_default().push(l.posicao);
        }
        assert_eq!(por_concurso.len(), 31, "{slug}: concursos");
        let esperado: Vec<u32> = (1..=qtd_dezenas(cfg) as u32).collect();
        for (concurso, mut posicoes) in por_concurso {
            posicoes.sort_unstable();
            assert_eq!(posicoes, esperado, "{slug}: concurso {concurso}");
        }
    }
}

#[test]
fn posicao_acompanha_o_total_decrescente_com_empate_pela_menor_dezena() {
    // Confere só que as colunas gravadas são coerentes entre si (a ordem
    // que o motor define está gravada sem troca de colunas).
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 30, 23));
    indexar(
        &mut conn,
        cfg,
        &perfil_de("quina", "teste_v1", 10, Normalizacao::Zscore),
    );
    let mut por_concurso: BTreeMap<u32, Vec<IndiceDezena>> = BTreeMap::new();
    for l in lido(&conn, "teste_v1") {
        por_concurso.entry(l.concurso).or_default().push(l);
    }
    for (concurso, mut linhas) in por_concurso {
        linhas.sort_by_key(|l| l.posicao);
        for par in linhas.windows(2) {
            let (a, b) = (&par[0], &par[1]);
            assert!(
                a.total > b.total || (a.total == b.total && a.dezena < b.dezena),
                "concurso {concurso}: dezena {} (pos {}, total {}) antes da {} (pos {}, total {})",
                a.dezena,
                a.posicao,
                a.total,
                b.dezena,
                b.posicao,
                b.total
            );
        }
    }
}

#[test]
fn variacao_gravada_e_a_posicao_anterior_menos_a_atual() {
    // Convenção da doc do motor: positivo = subiu. Autojunção no SQL, para
    // pegar coluna trocada ou variação gravada na linha errada.
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 30, 24));
    indexar(
        &mut conn,
        cfg,
        &perfil_de("quina", "teste_v1", 50, Normalizacao::Minmax),
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice a JOIN indice b
               ON b.dezena = a.dezena AND b.perfil = a.perfil AND b.concurso = a.concurso - 1
             WHERE a.variacao_posicao = b.posicao - a.posicao"
        ),
        30 * 80,
        "concursos 2..=31 × 80 dezenas"
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice WHERE concurso > 1 AND variacao_posicao IS NULL"
        ),
        0
    );
}

// ---------------------------------------------------------------------------
// 3. Esquema de `indice`.
// ---------------------------------------------------------------------------

#[test]
fn esquema_de_indice_igual_ao_plano() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 10, 31));
    indexar(&mut conn, cfg, &perfil_quina());

    // (nome, tipo, notnull, posição na chave primária ou 0)
    let mut stmt = conn
        .prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info('indice') ORDER BY cid")
        .expect("prepara");
    let colunas = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .expect("consulta")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("colunas");
    let esperado = [
        ("concurso", "INTEGER", 1, 1),
        ("dezena", "INTEGER", 1, 2),
        ("perfil", "TEXT", 1, 3),
        ("total", "REAL", 1, 0),
        ("posicao", "INTEGER", 1, 0),
        ("variacao_posicao", "INTEGER", 0, 0),
    ]
    .map(|(n, t, nn, pk)| (n.to_string(), t.to_string(), nn, pk));
    assert_eq!(colunas, esperado);
}

#[test]
fn indice_e_without_rowid() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 10, 32));
    indexar(&mut conn, cfg, &perfil_quina());
    let sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'indice'",
            [],
            |row| row.get(0),
        )
        .expect("sql da tabela");
    assert!(
        sql.to_uppercase().contains("WITHOUT ROWID"),
        "esquema sem WITHOUT ROWID: {sql}"
    );
}

#[test]
fn colunas_gravadas_tem_os_tipos_do_sqlite_esperados() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 15, 33));
    indexar(&mut conn, cfg, &perfil_quina());
    // `total` é sempre real (inclusive quando vale 0.0); `posicao` e
    // `variacao_posicao` são inteiros (esta, NULL só no concurso 1).
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice WHERE typeof(total) <> 'real'"
        ),
        0
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice WHERE typeof(posicao) <> 'integer'"
        ),
        0
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice
             WHERE typeof(variacao_posicao) NOT IN ('integer', 'null')"
        ),
        0
    );
}

#[test]
fn indice_recusa_total_nulo_e_chave_duplicada() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 5, 34));
    indexar(&mut conn, cfg, &perfil_quina());
    assert!(
        conn.execute(
            "INSERT INTO indice (concurso, dezena, perfil, total, posicao, variacao_posicao)
             VALUES (99, 1, 'teste_v1', NULL, 1, NULL)",
            [],
        )
        .is_err(),
        "total NULL deveria violar NOT NULL"
    );
    assert!(
        conn.execute(
            "INSERT INTO indice (concurso, dezena, perfil, total, posicao, variacao_posicao)
             VALUES (1, 1, 'teste_v1', 0.0, 1, NULL)",
            [],
        )
        .is_err(),
        "chave (concurso, dezena, perfil) repetida deveria ser recusada"
    );
}

// ---------------------------------------------------------------------------
// 4. Regenerar é idempotente byte a byte.
// ---------------------------------------------------------------------------

fn tabela_indice_crua(conn: &Connection) -> Vec<Vec<Value>> {
    let mut stmt = conn
        .prepare(
            "SELECT concurso, dezena, perfil, total, posicao, variacao_posicao
             FROM indice ORDER BY concurso, dezena, perfil",
        )
        .expect("prepara");
    let linhas = stmt
        .query_map([], |row| {
            (0..6)
                .map(|i| row.get::<_, Value>(i))
                .collect::<rusqlite::Result<Vec<Value>>>()
        })
        .expect("consulta")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("linhas");
    linhas
}

#[test]
fn rodar_duas_vezes_sem_mudanca_deixa_indice_e_perfil_identicos() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 40, 41));
    let p = perfil_quina();
    let primeiro = indexar(&mut conn, cfg, &p);
    let bits_antes = bits(&lido(&conn, "teste_v1"));
    let cru_antes = tabela_indice_crua(&conn);
    let perfil_antes = db::buscar_perfil(&conn, "teste_v1").expect("busca");

    let segundo = sincronizar_indice(&mut conn, cfg, &p, "2031-12-31T23:59:59Z").expect("de novo");

    assert_eq!(primeiro.registro, RegistroPerfil::Novo);
    assert_eq!(segundo.registro, RegistroPerfil::JaRegistrado);
    // Substituiu exatamente as linhas da 1ª execução.
    assert_eq!(segundo.linhas_apagadas, primeiro.linhas_gravadas);
    assert_eq!(segundo.linhas_gravadas, primeiro.linhas_gravadas);
    assert_eq!(bits(&lido(&conn, "teste_v1")), bits_antes);
    assert_eq!(tabela_indice_crua(&conn), cru_antes);
    assert_eq!(
        db::buscar_perfil(&conn, "teste_v1").expect("busca"),
        perfil_antes,
        "linha de perfis (inclusive criado_em) não muda"
    );
    assert_eq!(contar(&conn, "SELECT COUNT(*) FROM perfis"), 1);
}

#[test]
fn rodar_tres_vezes_o_resumo_so_muda_no_que_deve() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 20, 42));
    let p = perfil_quina();
    let a = indexar(&mut conn, cfg, &p);
    let b = indexar(&mut conn, cfg, &p);
    let c = indexar(&mut conn, cfg, &p);
    assert_eq!(b, c);
    assert_eq!(
        (
            a.concursos,
            a.primeiro_concurso,
            a.ultimo_concurso,
            a.proximo_concurso
        ),
        (
            b.concursos,
            b.primeiro_concurso,
            b.ultimo_concurso,
            b.proximo_concurso
        )
    );
    assert_eq!((a.linhas_apagadas, b.linhas_apagadas), (0, 21 * 80));
}

// ---------------------------------------------------------------------------
// 5. Substitui só o perfil em questão.
// ---------------------------------------------------------------------------

#[test]
fn regenerar_um_perfil_nao_toca_nos_outros() {
    let cfg = quina();
    let hist = historico(cfg, 60, 51);
    let teste = perfil_de("quina", "teste_v1", 10, Normalizacao::Minmax);
    let outro = perfil_de("quina", "outro_v1", 50, Normalizacao::Zscore);

    let mut conn = banco();
    gravar(&mut conn, &hist[..40]);
    sincronizar_features(&mut conn, cfg, false).expect("features 40");
    indexar(&mut conn, cfg, &teste);
    indexar(&mut conn, cfg, &outro);
    let outro_antes = lido(&conn, "outro_v1");
    let perfil_outro_antes = db::buscar_perfil(&conn, "outro_v1").expect("busca");
    assert_eq!(outro_antes.len(), 41 * 80);

    gravar(&mut conn, &hist[40..]);
    sincronizar_features(&mut conn, cfg, false).expect("features 60");
    let resumo = indexar(&mut conn, cfg, &teste);
    assert_eq!(resumo.linhas_apagadas, 41 * 80);
    assert_eq!(resumo.linhas_gravadas, 61 * 80);

    // O perfil rodado cobre os concursos novos e é igual ao calculado do zero.
    let novo = lido(&conn, "teste_v1");
    assert_eq!(novo.iter().map(|l| l.concurso).max(), Some(61));
    assert_eq!(bits(&novo), bits(&referencia(cfg, &hist, &teste)));

    // O outro perfil continua exatamente como estava: nada dos concursos novos.
    assert_eq!(bits(&lido(&conn, "outro_v1")), bits(&outro_antes));
    assert_eq!(
        db::buscar_perfil(&conn, "outro_v1").expect("busca"),
        perfil_outro_antes
    );
    assert_eq!(
        contar(
            &conn,
            "SELECT COUNT(*) FROM indice WHERE perfil = 'outro_v1' AND concurso > 41"
        ),
        0
    );
}

#[test]
fn perfis_independentes_nao_interferem_na_ordem_de_execucao() {
    // A, depois B == B, depois A: cada perfil só depende de si e das features.
    let cfg = quina();
    let hist = historico(cfg, 35, 52);
    let a = perfil_de("quina", "a_v1", 10, Normalizacao::Minmax);
    let b = perfil_de("quina", "b_v1", 0, Normalizacao::Zscore);

    let mut ab = calculado(cfg, &hist);
    indexar(&mut ab, cfg, &a);
    indexar(&mut ab, cfg, &b);
    let mut ba = calculado(cfg, &hist);
    indexar(&mut ba, cfg, &b);
    indexar(&mut ba, cfg, &a);

    assert_eq!(tabela_indice_crua(&ab), tabela_indice_crua(&ba));
}

// ---------------------------------------------------------------------------
// 6. Determinismo.
// ---------------------------------------------------------------------------

#[test]
fn dois_bancos_independentes_com_o_mesmo_historico_dao_indices_identicos() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg_de(slug);
        let hist = historico(cfg, 40, 61);
        let p = perfil_de(slug, "teste_v1", 10, Normalizacao::Zscore);
        let mut a = calculado(cfg, &hist);
        let mut b = calculado(cfg, &hist);
        indexar(&mut a, cfg, &p);
        indexar(&mut b, cfg, &p);
        assert_eq!(
            bits(&lido(&a, "teste_v1")),
            bits(&lido(&b, "teste_v1")),
            "{slug}"
        );
    }
}

#[test]
fn indice_de_banco_incremental_igual_ao_de_banco_calculado_de_uma_vez() {
    // Features incrementais já são iguais às completas (tests/features.rs);
    // aqui o índice por cima também.
    let cfg = quina();
    let hist = historico(cfg, 50, 62);
    let p = perfil_quina();

    let mut de_uma_vez = calculado(cfg, &hist);
    indexar(&mut de_uma_vez, cfg, &p);

    let mut em_lotes = banco();
    // Linhas gravadas pelo lote anterior (concursos 1 … último + 1, inclusive
    // o snapshot do próximo concurso): o lote seguinte as regrava, mas cada
    // uma tem de sair idêntica, bit a bit — concursos posteriores não mexem
    // no índice de t.
    let mut anterior: Vec<LinhaBits> = Vec::new();
    for corte in [(0, 7), (7, 30), (30, 50)] {
        gravar(&mut em_lotes, &hist[corte.0..corte.1]);
        sincronizar_features(&mut em_lotes, cfg, false).expect("lote");
        indexar(&mut em_lotes, cfg, &p);

        let atual = bits(&lido(&em_lotes, "teste_v1"));
        let ultimo_do_lote = hist[corte.1 - 1].concurso;
        assert_eq!(
            atual.iter().map(|l| l.0).max(),
            Some(ultimo_do_lote + 1),
            "lote {corte:?}: índice até o snapshot do próximo concurso"
        );
        if let Some(t) = anterior.iter().map(|l| l.0).max() {
            assert!(t < ultimo_do_lote + 1, "o lote trouxe concursos novos");
            let ate_t: Vec<LinhaBits> = atual.iter().filter(|l| l.0 <= t).cloned().collect();
            assert_eq!(
                ate_t, anterior,
                "lote {corte:?}: linhas com concurso <= {t} mudaram"
            );
        }
        anterior = atual;
    }
    assert_eq!(
        bits(&lido(&em_lotes, "teste_v1")),
        bits(&lido(&de_uma_vez, "teste_v1"))
    );
}

// ---------------------------------------------------------------------------
// 7. Recusas sem gravar nada.
// ---------------------------------------------------------------------------

#[test]
fn banco_sem_resultados_e_recusado_sem_criar_tabelas() {
    let mut conn = banco();
    let erro = sincronizar_indice(&mut conn, quina(), &perfil_quina(), CRIADO_EM)
        .expect_err("sem resultados");
    assert!(matches!(erro, SpiderError::SemResultados(_)), "{erro:?}");
    assert!(tabelas_do_indice(&conn).is_empty());
}

#[test]
fn resultados_sem_tabela_de_features_sao_recusados_sem_criar_tabelas() {
    let mut conn = banco();
    gravar(&mut conn, &historico(quina(), 10, 71));
    let erro = sincronizar_indice(&mut conn, quina(), &perfil_quina(), CRIADO_EM)
        .expect_err("sem features");
    assert!(matches!(erro, SpiderError::SemFeatures(_)), "{erro:?}");
    assert!(tabelas_do_indice(&conn).is_empty());
    assert!(
        !db::tabela_existe(&conn, "features_dezena").expect("sqlite_master"),
        "o comando de índice não cria features"
    );
}

#[test]
fn tabela_de_features_vazia_e_recusada_sem_criar_tabelas() {
    let mut conn = banco();
    gravar(&mut conn, &historico(quina(), 10, 72));
    db::criar_tabelas_features(&conn).expect("cria vazia");
    let erro = sincronizar_indice(&mut conn, quina(), &perfil_quina(), CRIADO_EM)
        .expect_err("features vazias");
    assert!(matches!(erro, SpiderError::SemFeatures(_)), "{erro:?}");
    assert!(tabelas_do_indice(&conn).is_empty());
}

/// Aplica `mutar` a um banco só com features e a outro já indexado; nos dois,
/// `sincronizar_indice` tem de dar erro que `confere` aceite. No primeiro,
/// `indice` e `perfis` não podem passar a existir; no segundo, o índice e o
/// perfil anteriores ficam intactos.
fn confere_recusa(
    rotulo: &str,
    mutar: impl Fn(&mut Connection),
    confere: impl Fn(&SpiderError) -> bool,
) {
    let cfg = quina();
    let hist = historico(cfg, 30, 73);
    let p = perfil_quina();

    let mut sem_indice = calculado(cfg, &hist);
    mutar(&mut sem_indice);
    let erro = sincronizar_indice(&mut sem_indice, cfg, &p, CRIADO_EM)
        .expect_err(&format!("{rotulo}: deveria recusar"));
    assert!(confere(&erro), "{rotulo}: erro inesperado {erro:?}");
    assert!(
        tabelas_do_indice(&sem_indice).is_empty(),
        "{rotulo}: criou tabelas ao recusar"
    );

    let mut com_indice = calculado(cfg, &hist);
    indexar(&mut com_indice, cfg, &p);
    let antes = foto(&com_indice, &["teste_v1"]);
    mutar(&mut com_indice);
    let erro = sincronizar_indice(&mut com_indice, cfg, &p, "2040-01-01T00:00:00Z")
        .expect_err(&format!("{rotulo}: deveria recusar (já indexado)"));
    assert!(confere(&erro), "{rotulo}: erro inesperado {erro:?}");
    assert_eq!(
        foto(&com_indice, &["teste_v1"]),
        antes,
        "{rotulo}: recusa alterou o índice ou o perfil"
    );
}

#[test]
fn resultado_novo_sem_rodar_features_e_recusado_como_desatualizado() {
    let cfg = quina();
    confere_recusa(
        "resultado novo",
        |conn| gravar(conn, &historico(cfg, 31, 73)[30..]),
        |e| {
            // Último resultado 31; o snapshot das features continua em 31 (≠ 32).
            matches!(
                e,
                SpiderError::FeaturesDesatualizadas {
                    ultimo_concurso: 31,
                    snapshot: 31,
                    ..
                }
            )
        },
    );
}

/// Resultado já incorporado corrigido sem rodar `sync features`: último
/// concurso e snapshot continuam iguais, só a impressão de `motor_estado`
/// denuncia (decisão de 2026-10-05).
fn resultado_corrigido(concurso: u32, so_data: bool) -> impl Fn(&mut Connection) {
    move |conn| {
        let cfg = quina();
        let original = &historico(cfg, 30, 73)[(concurso - 1) as usize];
        let mut corrigido = if so_data {
            original.clone()
        } else {
            com_dezenas_trocadas(cfg, original)
        };
        if so_data {
            corrigido.data_sorteio = "1999-12-31".to_string();
        }
        gravar(conn, &[corrigido]);
    }
}

fn eh_resultados_alterados(e: &SpiderError) -> bool {
    matches!(e, SpiderError::FeaturesDeResultadosAlterados { .. })
}

#[test]
fn resultado_antigo_corrigido_sem_rodar_features_e_recusado() {
    confere_recusa(
        "concurso 5 corrigido",
        resultado_corrigido(5, false),
        eh_resultados_alterados,
    );
}

#[test]
fn ultimo_resultado_corrigido_sem_rodar_features_e_recusado() {
    confere_recusa(
        "concurso 30 corrigido",
        resultado_corrigido(30, false),
        eh_resultados_alterados,
    );
}

#[test]
fn so_a_data_corrigida_sem_rodar_features_e_recusada() {
    confere_recusa(
        "data do concurso 12 corrigida",
        resultado_corrigido(12, true),
        eh_resultados_alterados,
    );
}

#[test]
fn features_sem_motor_estado_sao_recusadas() {
    confere_recusa(
        "motor_estado apagado",
        |conn| {
            conn.execute("DELETE FROM motor_estado", [])
                .expect("apaga o estado");
        },
        eh_resultados_alterados,
    );
}

#[test]
fn depois_de_rodar_features_o_resultado_corrigido_e_indexado() {
    let cfg = quina();
    let hist = historico(cfg, 30, 73);
    let p = perfil_quina();
    let mut conn = calculado(cfg, &hist);
    indexar(&mut conn, cfg, &p);

    resultado_corrigido(5, false)(&mut conn);
    sincronizar_features(&mut conn, cfg, false).expect("features recalculadas");
    indexar(&mut conn, cfg, &p);

    let mut corrigido = hist.clone();
    corrigido[4] = com_dezenas_trocadas(cfg, &hist[4]);
    assert_eq!(
        bits(&lido(&conn, "teste_v1")),
        bits(&referencia(cfg, &corrigido, &p))
    );
}

#[test]
fn features_a_frente_dos_resultados_sao_recusadas_como_desatualizadas() {
    confere_recusa(
        "último resultado apagado",
        |conn| {
            let apagadas = conn
                .execute("DELETE FROM resultados WHERE concurso = 30", [])
                .expect("apaga");
            assert_eq!(apagadas, 1);
        },
        |e| {
            // Último resultado 29; o snapshot das features está em 31 (≠ 30).
            matches!(
                e,
                SpiderError::FeaturesDesatualizadas {
                    ultimo_concurso: 29,
                    snapshot: 31,
                    ..
                }
            )
        },
    );
}

#[test]
fn feature_version_diferente_na_janela_do_perfil_e_recusada() {
    // Perfil da janela 10: basta uma única linha dela com outra versão.
    confere_recusa(
        "feature_version na janela do perfil",
        |conn| {
            let n = conn
                .execute(
                    "UPDATE features_dezena SET feature_version = ?1
                     WHERE concurso = 7 AND dezena = 3 AND janela = 10",
                    [FEATURE_VERSION + 1],
                )
                .expect("adultera");
            assert_eq!(n, 1);
        },
        |e| {
            matches!(
                e,
                SpiderError::FeaturesDeOutraVersao { encontrada, atual, .. }
                    if *encontrada == FEATURE_VERSION + 1 && *atual == FEATURE_VERSION
            )
        },
    );
}

#[test]
fn feature_version_diferente_so_em_outra_janela_nao_e_recusada() {
    // Comportamento implementado (documentado aqui de propósito): só as linhas
    // da janela do perfil são conferidas.
    let cfg = quina();
    let hist = historico(cfg, 30, 74);
    let p = perfil_quina(); // janela 10
    let mut conn = calculado(cfg, &hist);
    let n = conn
        .execute(
            "UPDATE features_dezena SET feature_version = ?1 WHERE janela <> 10",
            [FEATURE_VERSION + 1],
        )
        .expect("adultera");
    assert!(n > 0);
    indexar(&mut conn, cfg, &p);
    assert_eq!(
        bits(&lido(&conn, "teste_v1")),
        bits(&referencia(cfg, &hist, &p))
    );
}

fn e_incoerente(janela: u32) -> impl Fn(&SpiderError) -> bool {
    move |e| matches!(e, SpiderError::FeaturesIncoerentes { janela: j, .. } if *j == janela)
}

fn executar(conn: &mut Connection, sql: &str) {
    conn.execute(sql, []).expect("mutação aplicada");
}

#[test]
fn dezena_fora_do_universo_na_janela_do_perfil_e_incoerente() {
    // A Quina vai de 1 a 80: a 80 vira 81 (sem colidir com a chave).
    confere_recusa(
        "dezena 81",
        |conn| {
            executar(
                conn,
                "UPDATE features_dezena SET dezena = 81 WHERE dezena = 80 AND janela = 10",
            )
        },
        e_incoerente(10),
    );
}

#[test]
fn falta_uma_dezena_num_concurso_da_janela_do_perfil_e_incoerente() {
    confere_recusa(
        "dezena faltando",
        |conn| {
            executar(
                conn,
                "DELETE FROM features_dezena WHERE concurso = 12 AND dezena = 33 AND janela = 10",
            )
        },
        e_incoerente(10),
    );
}

#[test]
fn janela_do_perfil_sem_nenhuma_linha_e_incoerente() {
    confere_recusa(
        "janela vazia",
        |conn| executar(conn, "DELETE FROM features_dezena WHERE janela = 10"),
        e_incoerente(10),
    );
}

#[test]
fn janela_do_perfil_sem_o_concurso_1_e_incoerente() {
    confere_recusa(
        "sem concurso 1",
        |conn| {
            executar(
                conn,
                "DELETE FROM features_dezena WHERE concurso = 1 AND janela = 10",
            )
        },
        e_incoerente(10),
    );
}

#[test]
fn buraco_de_concurso_inteiro_na_janela_do_perfil_e_incoerente() {
    confere_recusa(
        "buraco no concurso 12",
        |conn| {
            executar(
                conn,
                "DELETE FROM features_dezena WHERE concurso = 12 AND janela = 10",
            )
        },
        e_incoerente(10),
    );
}

#[test]
fn snapshot_ausente_na_janela_do_perfil_e_incoerente() {
    // Outras janelas ainda têm o concurso 31, então o teste de "desatualizado"
    // passa; quem pega é a conferência da série da janela do perfil.
    confere_recusa(
        "sem snapshot na janela",
        |conn| {
            executar(
                conn,
                "DELETE FROM features_dezena WHERE concurso = 31 AND janela = 10",
            )
        },
        e_incoerente(10),
    );
}

#[test]
fn mutacoes_so_em_outra_janela_nao_afetam_o_indice() {
    let cfg = quina();
    let hist = historico(cfg, 30, 75);
    let p = perfil_quina(); // janela 10
    let esperado = bits(&referencia(cfg, &hist, &p));
    for mutacao in [
        "UPDATE features_dezena SET dezena = 81 WHERE dezena = 80 AND janela = 50",
        "DELETE FROM features_dezena WHERE concurso = 12 AND dezena = 33 AND janela = 50",
        "DELETE FROM features_dezena WHERE janela = 50",
        "DELETE FROM features_dezena WHERE concurso = 1 AND janela = 50",
        "DELETE FROM features_dezena WHERE concurso = 12 AND janela = 100",
        "DELETE FROM features_dezena WHERE concurso = 31 AND janela = 0",
    ] {
        let mut conn = calculado(cfg, &hist);
        executar(&mut conn, mutacao);
        indexar(&mut conn, cfg, &p);
        assert_eq!(bits(&lido(&conn, "teste_v1")), esperado, "{mutacao}");
    }
}

#[test]
fn perfil_de_outra_loteria_e_recusado_sem_criar_tabelas() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 20, 76));
    let megasena = perfil_de("megasena", "mega_v1", 10, Normalizacao::Minmax);
    let erro = sincronizar_indice(&mut conn, cfg, &megasena, CRIADO_EM).expect_err("outra loteria");
    match erro {
        SpiderError::PerfilDeOutraLoteria {
            nome,
            loteria_perfil,
            loteria_banco,
        } => {
            assert_eq!(nome, "mega_v1");
            assert_eq!(loteria_perfil, "megasena");
            assert_eq!(loteria_banco, "quina");
        }
        outro => panic!("esperava PerfilDeOutraLoteria, veio {outro:?}"),
    }
    assert!(tabelas_do_indice(&conn).is_empty());
}

#[test]
fn perfil_alterado_sob_o_mesmo_nome_e_recusado_e_preserva_o_anterior() {
    let cfg = quina();
    let hist = historico(cfg, 30, 77);
    let original = perfil_quina();
    let mut conn = calculado(cfg, &hist);
    indexar(&mut conn, cfg, &original);
    let antes = foto(&conn, &["teste_v1"]);

    let texto = texto_perfil("teste_v1", "quina", 10, Normalizacao::Minmax, 2.5);
    let editado = carregar_texto(&texto, "quina", "teste_v1");
    assert_ne!(editado.hash_toml, original.hash_toml);
    let erro =
        sincronizar_indice(&mut conn, cfg, &editado, "2040-01-01T00:00:00Z").expect_err("alterado");
    match erro {
        SpiderError::PerfilAlterado {
            nome,
            hash_registrado,
            hash_arquivo,
            proximo_nome,
            proxima_versao,
        } => {
            assert_eq!(nome, "teste_v1");
            assert_eq!(hash_registrado, original.hash_toml);
            assert_eq!(hash_arquivo, editado.hash_toml);
            assert_eq!(proximo_nome, "teste_v2");
            assert_eq!(proxima_versao, 2);
        }
        outro => panic!("esperava PerfilAlterado, veio {outro:?}"),
    }
    assert_eq!(foto(&conn, &["teste_v1"]), antes);
}

#[test]
fn perfil_alterado_nao_toca_no_indice_mesmo_com_concursos_novos() {
    // O índice antigo (41 concursos) fica como estava: a recusa vem antes de
    // apagar, mesmo havendo features novas que o índice ainda não cobre.
    let cfg = quina();
    let hist = historico(cfg, 40, 78);
    let mut conn = banco();
    gravar(&mut conn, &hist[..30]);
    sincronizar_features(&mut conn, cfg, false).expect("features");
    indexar(&mut conn, cfg, &perfil_quina());
    let antes = foto(&conn, &["teste_v1"]);

    gravar(&mut conn, &hist[30..]);
    sincronizar_features(&mut conn, cfg, false).expect("features novas");
    let texto = texto_perfil("teste_v1", "quina", 10, Normalizacao::Minmax, 9.0);
    let editado = carregar_texto(&texto, "quina", "teste_v1");
    assert!(matches!(
        sincronizar_indice(&mut conn, cfg, &editado, CRIADO_EM),
        Err(SpiderError::PerfilAlterado { .. })
    ));
    assert_eq!(foto(&conn, &["teste_v1"]), antes);
    assert_eq!(antes.indices[0].1.len(), 31 * 80);
}

// --- Atomicidade (gatilho que falha no meio da inserção) --------------------

const GATILHO_INSERT_INDICE: &str = "CREATE TRIGGER falha
     BEFORE INSERT ON indice WHEN NEW.concurso = 20
     BEGIN SELECT RAISE(ABORT, 'falha simulada'); END;";

fn e_falha_simulada(erro: &SpiderError) -> bool {
    matches!(erro, SpiderError::Db(_)) && erro.to_string().contains("falha simulada")
}

#[test]
fn falha_no_meio_da_regeneracao_devolve_o_indice_anterior() {
    let cfg = quina();
    let hist = historico(cfg, 40, 79);
    let p = perfil_quina();
    let mut conn = banco();
    gravar(&mut conn, &hist[..30]);
    sincronizar_features(&mut conn, cfg, false).expect("features 30");
    indexar(&mut conn, cfg, &p);
    let antes = foto(&conn, &["teste_v1"]);
    assert_eq!(antes.indices[0].1.len(), 31 * 80);

    gravar(&mut conn, &hist[30..]);
    sincronizar_features(&mut conn, cfg, false).expect("features 40");
    conn.execute_batch(GATILHO_INSERT_INDICE).expect("gatilho");
    let erro = sincronizar_indice(&mut conn, cfg, &p, "2040-01-01T00:00:00Z")
        .expect_err("gatilho deveria abortar");
    assert!(e_falha_simulada(&erro), "{erro:?}");
    // As linhas apagadas antes da falha voltaram: nada mudou.
    assert_eq!(foto(&conn, &["teste_v1"]), antes);

    // Sem o gatilho, a execução seguinte iguala a referência do zero.
    conn.execute_batch("DROP TRIGGER falha").expect("remove");
    let resumo = indexar(&mut conn, cfg, &p);
    assert_eq!(resumo.linhas_apagadas, 31 * 80);
    assert_eq!(resumo.linhas_gravadas, 41 * 80);
    assert_eq!(
        bits(&lido(&conn, "teste_v1")),
        bits(&referencia(cfg, &hist, &p))
    );
}

#[test]
fn falha_na_primeira_execucao_nao_deixa_o_perfil_registrado() {
    let cfg = quina();
    let hist = historico(cfg, 30, 80);
    let p = perfil_quina();
    let mut conn = calculado(cfg, &hist);
    // O gatilho precisa de `indice` já existente.
    db::criar_tabela_indice(&conn).expect("cria indice");
    conn.execute_batch(GATILHO_INSERT_INDICE).expect("gatilho");

    let erro = sincronizar_indice(&mut conn, cfg, &p, CRIADO_EM).expect_err("gatilho");
    assert!(e_falha_simulada(&erro), "{erro:?}");
    // Registro do perfil e índice estão na mesma transação: nada sobrou.
    assert!(
        !db::tabela_existe(&conn, "perfis").expect("sqlite_master"),
        "tabela perfis não deveria ter sido criada (rollback)"
    );
    assert_eq!(contar(&conn, "SELECT COUNT(*) FROM indice"), 0);

    conn.execute_batch("DROP TRIGGER falha").expect("remove");
    let resumo = indexar(&mut conn, cfg, &p);
    assert_eq!(resumo.registro, RegistroPerfil::Novo);
    assert_eq!(resumo.linhas_apagadas, 0);
    assert_eq!(
        bits(&lido(&conn, "teste_v1")),
        bits(&referencia(cfg, &hist, &p))
    );
    assert_eq!(
        db::buscar_perfil(&conn, "teste_v1")
            .expect("busca")
            .map(|r| r.criado_em),
        Some(CRIADO_EM.to_string())
    );
}

#[test]
fn falha_simulada_nao_afeta_o_indice_de_outro_perfil() {
    let cfg = quina();
    let hist = historico(cfg, 30, 81);
    let a = perfil_de("quina", "a_v1", 10, Normalizacao::Minmax);
    let b = perfil_de("quina", "b_v1", 50, Normalizacao::Zscore);
    let mut conn = calculado(cfg, &hist);
    indexar(&mut conn, cfg, &a);
    let antes = foto(&conn, &["a_v1", "b_v1"]);

    conn.execute_batch(GATILHO_INSERT_INDICE).expect("gatilho");
    let erro = sincronizar_indice(&mut conn, cfg, &b, CRIADO_EM).expect_err("gatilho");
    assert!(e_falha_simulada(&erro), "{erro:?}");
    assert_eq!(foto(&conn, &["a_v1", "b_v1"]), antes);
}

// ---------------------------------------------------------------------------
// 8. Ponto no tempo no nível do banco.
// ---------------------------------------------------------------------------

/// Dois bancos com o mesmo histórico até `t − 1` e concursos `t ..= n`
/// diferentes: o índice (de dois perfis, janela total/zscore e janela
/// 10/minmax) tem de ser idêntico até o concurso `t` e diferir em `t + 1`.
fn confere_ponto_no_tempo(slug: &str, n: u32, t: u32, semente: u64) {
    let cfg = cfg_de(slug);
    let hist_a = historico(cfg, n, semente);
    let hist_b: Vec<Resultado> = hist_a
        .iter()
        .map(|r| {
            if r.concurso >= t {
                com_dezenas_trocadas(cfg, r)
            } else {
                r.clone()
            }
        })
        .collect();
    assert_ne!(
        hist_a[t as usize - 1].dezenas,
        hist_b[t as usize - 1].dezenas
    );

    let mut a = calculado(cfg, &hist_a);
    let mut b = calculado(cfg, &hist_b);
    for (nome, janela, normalizacao) in [
        ("total_v1", 0, Normalizacao::Zscore),
        ("dez_v1", 10, Normalizacao::Minmax),
    ] {
        let p = perfil_de(slug, nome, janela, normalizacao);
        indexar(&mut a, cfg, &p);
        indexar(&mut b, cfg, &p);
        let ia = lido(&a, nome);
        let ib = lido(&b, nome);
        let ate_t = |v: &[IndiceDezena]| -> Vec<LinhaBits> {
            bits(
                &v.iter()
                    .filter(|l| l.concurso <= t)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            ate_t(&ia),
            ate_t(&ib),
            "{slug} t={t} {nome}: índice até t mudou com dados de t em diante"
        );
        // Contraprova: sem ela o teste passaria com índices que ignoram os dados.
        let em_t_mais_1 = |v: &[IndiceDezena]| -> Vec<LinhaBits> {
            bits(
                &v.iter()
                    .filter(|l| l.concurso == t + 1)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        assert_ne!(
            em_t_mais_1(&ia),
            em_t_mais_1(&ib),
            "{slug} t={t} {nome}: o concurso t+1 deveria enxergar o t alterado"
        );
    }
}

#[test]
fn indice_ate_t_nao_depende_dos_concursos_de_t_em_diante_t_2() {
    confere_ponto_no_tempo("quina", 60, 2, 0xa1);
}

#[test]
fn indice_ate_t_nao_depende_dos_concursos_de_t_em_diante_t_50() {
    confere_ponto_no_tempo("quina", 80, 50, 0xa2);
}

#[test]
fn indice_ate_t_nao_depende_do_ultimo_concurso_t_igual_a_n() {
    // t = n: só o último sorteio difere; o snapshot n + 1 é o único que o vê.
    confere_ponto_no_tempo("quina", 60, 60, 0xa3);
}

#[test]
fn indice_ate_t_nao_depende_dos_concursos_de_t_em_diante_lotofacil() {
    confere_ponto_no_tempo("lotofacil", 60, 30, 0xa4);
}

#[test]
fn saiu_nunca_entra_no_indice() {
    let cfg = quina();
    let hist = historico(cfg, 40, 0xb1);
    let mut conn = calculado(cfg, &hist);
    let perfis = [
        perfil_de("quina", "total_v1", 0, Normalizacao::Zscore),
        perfil_de("quina", "dez_v1", 10, Normalizacao::Minmax),
        perfil_de("quina", "cem_v1", 100, Normalizacao::Zscore),
    ];
    for p in &perfis {
        indexar(&mut conn, cfg, p);
    }
    let antes: Vec<Vec<LinhaBits>> = perfis
        .iter()
        .map(|p| bits(&lido(&conn, &p.perfil.nome)))
        .collect();

    let alteradas = conn
        .execute(
            "UPDATE features_dezena SET saiu = 1 - saiu WHERE saiu IS NOT NULL",
            [],
        )
        .expect("inverte saiu");
    assert_eq!(
        alteradas,
        40 * 80 * 4,
        "concursos sorteados × dezenas × janelas"
    );
    for p in &perfis {
        indexar(&mut conn, cfg, p);
    }
    let depois: Vec<Vec<LinhaBits>> = perfis
        .iter()
        .map(|p| bits(&lido(&conn, &p.perfil.nome)))
        .collect();
    assert_eq!(antes, depois);
}

// ---------------------------------------------------------------------------
// 9. `criado_em` e conteúdo gravado em `perfis`.
// ---------------------------------------------------------------------------

#[test]
fn primeiro_registro_grava_o_perfil_inteiro_com_o_criado_em_recebido() {
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 15, 91));
    let texto = texto_perfil("teste_v1", "quina", 10, Normalizacao::Minmax, 1.0);
    let p = carregar_texto(&texto, "quina", "teste_v1");
    sincronizar_indice(&mut conn, cfg, &p, "texto livre, não é validado").expect("índice");
    assert_eq!(
        db::buscar_perfil(&conn, "teste_v1").expect("busca"),
        Some(PerfilRegistrado {
            nome: "teste_v1".to_string(),
            versao: 1,
            loteria: "quina".to_string(),
            janela: 10,
            normalizacao: "minmax".to_string(),
            hash_toml: perfil::hash_toml(&texto),
            toml: texto,
            criado_em: "texto livre, não é validado".to_string(),
        })
    );
}

#[test]
fn criado_em_do_primeiro_registro_sobrevive_a_novas_execucoes_com_concursos_novos() {
    let cfg = quina();
    let hist = historico(cfg, 30, 92);
    let p = perfil_quina();
    let mut conn = banco();
    gravar(&mut conn, &hist[..20]);
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &p, "primeiro").expect("1ª");

    gravar(&mut conn, &hist[20..]);
    sincronizar_features(&mut conn, cfg, false).expect("features");
    let resumo = sincronizar_indice(&mut conn, cfg, &p, "segundo").expect("2ª");
    assert_eq!(resumo.registro, RegistroPerfil::JaRegistrado);
    sincronizar_indice(&mut conn, cfg, &p, "terceiro").expect("3ª");
    assert_eq!(
        db::buscar_perfil(&conn, "teste_v1")
            .expect("busca")
            .map(|r| r.criado_em),
        Some("primeiro".to_string())
    );
}

#[test]
fn perfil_com_fim_de_linha_crlf_e_o_mesmo_perfil_ja_registrado() {
    // O hash é do texto normalizado para LF: o mesmo arquivo salvo no
    // Windows não conta como "alterado".
    let cfg = quina();
    let mut conn = calculado(cfg, &historico(cfg, 15, 93));
    let texto = texto_perfil("teste_v1", "quina", 10, Normalizacao::Minmax, 1.0);
    sincronizar_indice(
        &mut conn,
        cfg,
        &carregar_texto(&texto, "quina", "teste_v1"),
        "a",
    )
    .expect("1ª");
    let crlf = carregar_texto(&texto.replace('\n', "\r\n"), "quina", "teste_v1");
    let resumo = sincronizar_indice(&mut conn, cfg, &crlf, "b").expect("2ª");
    assert_eq!(resumo.registro, RegistroPerfil::JaRegistrado);
}
