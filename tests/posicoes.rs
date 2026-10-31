//! Testes de `orquestracao::sincronizar_posicoes` e do comando `sync posicoes`
//! (Fase 1g, tarefa 1g.3) sobre bancos em memória e em arquivo: as cinco tabelas
//! `posicao_*`, independência do que já existe (nenhuma tabela anterior muda e
//! `perfis` não recebe o perfil das posições), retomada do estado igual ao
//! cálculo do zero, recusas sem gravar nada, ponto no tempo no nível do banco,
//! CSV determinístico e a CLI.
//!
//! A matemática é de `motor::posicoes` e já tem testes próprios; aqui se
//! confere a ligação com o banco: o que foi gravado é o que as funções puras
//! calculam sobre o `indice` e os `resultados` lidos do banco.

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::db_posicoes;
use rust_spider::error::SpiderError;
use rust_spider::motor::{self, FeaturesDezena, Normalizacao};
use rust_spider::orquestracao::{
    sincronizar_features, sincronizar_indice, sincronizar_posicoes, ModoPosicoes, RegistroPerfil,
    RelatorioPosicoes,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::posicoes::{self as relatorio, BOM, CABECALHO};
use rust_spider::Resultado;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const NOME_FORCA: &str = "teste_v1";
const NOME_POS: &str = "pos_teste_v1";
const CRIADO_EM: &str = "2026-10-31T10:00:00Z";
const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
const N: u32 = 40;
const POSICOES: u32 = 25;
const K: usize = 15;

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

/// Tabelas que já existiam: o `sync posicoes` não pode alterar nenhuma.
const TABELAS_EXISTENTES: [&str; 5] = [
    "resultados",
    "features_dezena",
    "indice",
    "perfis",
    "motor_estado",
];

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofácil configurada")
}

/// Histórico sintético determinístico (xorshift64): o prefixo de `n` concursos
/// é o mesmo para qualquer `n` maior.
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

/// Grava `hist` e calcula as features e o índice de força do perfil de teste.
fn preparar(conn: &mut Connection, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    atualizar_forca(conn);
}

fn atualizar_forca(conn: &mut Connection) {
    sincronizar_features(conn, lotofacil(), false).expect("features");
    sincronizar_indice(conn, lotofacil(), &perfil_forca(), CRIADO_EM).expect("índice");
}

fn banco_com(hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, hist);
    conn
}

fn sincronizar(conn: &mut Connection) -> RelatorioPosicoes {
    sincronizar_posicoes(conn, lotofacil(), &perfil_forca(), &perfil_pos(), CRIADO_EM)
        .expect("posições")
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

fn contar(conn: &Connection, tabela: &str) -> usize {
    conn.query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |r| {
        r.get::<_, i64>(0)
    })
    .expect("contagem") as usize
}

/// As cinco tabelas `posicao_*`, ordenadas pela chave.
fn tabelas_posicoes(conn: &Connection) -> Vec<Vec<String>> {
    [
        "SELECT * FROM posicao_marcas ORDER BY perfil, concurso, posicao",
        "SELECT * FROM posicao_features ORDER BY perfil, janela, concurso, posicao",
        "SELECT * FROM posicao_indice ORDER BY perfil, perfil_posicoes, concurso, posicao",
        "SELECT * FROM posicao_perfil ORDER BY nome",
        "SELECT * FROM posicao_motor_estado ORDER BY perfil",
    ]
    .iter()
    .map(|sql| despejar(conn, sql))
    .collect()
}

fn tabelas_existentes(conn: &Connection) -> Vec<Vec<String>> {
    TABELAS_EXISTENTES
        .iter()
        .map(|t| despejar(conn, &format!("SELECT * FROM {t} ORDER BY 1, 2, 3")))
        .collect()
}

fn sem_tabelas_de_posicoes(conn: &Connection) -> bool {
    despejar(
        conn,
        "SELECT name FROM sqlite_master WHERE name LIKE 'posicao\\_%' ESCAPE '\\'",
    )
    .is_empty()
}

// ---------------------------------------------------------------------------
// O que é gravado.
// ---------------------------------------------------------------------------

#[test]
fn grava_o_que_o_motor_calcula_sobre_o_banco() {
    let hist = historico(lotofacil(), N);
    let mut conn = banco_com(&hist);
    let r = sincronizar(&mut conn);

    assert_eq!(
        r.modo,
        ModoPosicoes::Completo("sem estado salvo das posições".into())
    );
    assert_eq!(r.registro, RegistroPerfil::Novo);
    assert_eq!((r.ultimo_concurso, r.proximo_concurso), (N, N + 1));
    assert_eq!(r.concursos_novos, N as usize);

    // Contagens: N concursos marcados; features com o snapshot do próximo.
    let linhas_features = ((N + 1) * POSICOES * 4) as usize;
    assert_eq!(contar(&conn, "posicao_marcas"), (N * POSICOES) as usize);
    assert_eq!(contar(&conn, "posicao_features"), linhas_features);
    assert_eq!(
        contar(&conn, "posicao_indice"),
        ((N + 1) * POSICOES) as usize
    );
    assert_eq!(contar(&conn, "posicao_perfil"), 1);
    assert_eq!(contar(&conn, "posicao_motor_estado"), 1);
    assert_eq!(r.marcas_gravadas, (N * POSICOES) as usize);
    assert_eq!(r.features_gravadas, linhas_features);
    assert_eq!(
        (r.indice_apagado, r.indice_gravado),
        (0, ((N + 1) * POSICOES) as usize)
    );

    // Cada concurso marca exatamente K posições.
    let por_concurso = despejar(
        &conn,
        &format!(
            "SELECT concurso FROM posicao_marcas WHERE saiu = 1 \
             GROUP BY concurso HAVING COUNT(*) = {K}"
        ),
    );
    assert_eq!(por_concurso.len(), N as usize);

    // Conta independente: as funções puras sobre o índice e os resultados do banco.
    let resultados = db::listar_crescente(&conn, "lotofacil").expect("resultados");
    let indice = db::ler_indice(&conn, NOME_FORCA).expect("índice de força");
    let pos = perfil_pos();
    let (_, features, indice_pos) = motor::calcular_posicoes(
        lotofacil(),
        &resultados,
        &indice,
        pos.perfil.janela,
        Normalizacao::Zscore,
        &pos.perfil.pesos,
    )
    .expect("motor");
    let janela: Vec<FeaturesDezena> = features
        .into_iter()
        .filter(|f| f.janela == pos.perfil.janela)
        .collect();
    assert_eq!(
        db_posicoes::ler_features_da_janela(&conn, NOME_FORCA, pos.perfil.janela).unwrap(),
        janela
    );
    assert_eq!(
        db_posicoes::ler_indice(&conn, NOME_FORCA, NOME_POS).unwrap(),
        indice_pos
    );

    // O próximo concurso: G-1 de 1 a 25, cada dezena uma vez.
    assert_eq!(r.proximo.len(), POSICOES as usize);
    let g_1: Vec<u32> = r.proximo.iter().map(|l| l.g_1).collect();
    assert_eq!(g_1, (1..=POSICOES).collect::<Vec<_>>());
    let mut dezenas: Vec<u32> = r.proximo.iter().map(|l| l.dezena).collect();
    dezenas.sort_unstable();
    assert_eq!(dezenas, (1..=POSICOES).collect::<Vec<_>>());
}

#[test]
fn nao_altera_nada_do_que_ja_existe_e_nao_toca_em_perfis() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    let antes = tabelas_existentes(&conn);
    let tabelas_antes = despejar(&conn, "SELECT name FROM sqlite_master ORDER BY name");

    sincronizar(&mut conn);

    assert_eq!(tabelas_existentes(&conn), antes);
    // Só tabelas `posicao_*` foram acrescentadas.
    let novas: Vec<String> = despejar(&conn, "SELECT name FROM sqlite_master ORDER BY name")
        .into_iter()
        .filter(|n| !tabelas_antes.contains(n))
        .collect();
    assert!(!novas.is_empty());
    assert!(
        novas.iter().all(|n| n.contains("posicao_")),
        "tabelas novas fora de posicao_*: {novas:?}"
    );
    // O perfil das posições mora em `posicao_perfil`, nunca em `perfis`.
    let em = |tabela: &str| {
        despejar(
            &conn,
            &format!("SELECT nome FROM {tabela} WHERE nome = '{NOME_POS}'"),
        )
        .len()
    };
    assert_eq!(em("perfis"), 0);
    assert_eq!(em("posicao_perfil"), 1);
}

// ---------------------------------------------------------------------------
// Estado de motor próprio: retomar = recalcular.
// ---------------------------------------------------------------------------

#[test]
fn retomar_do_estado_deixa_as_tabelas_iguais_ao_calculo_do_zero() {
    let hist = historico(lotofacil(), N);
    let mut a = banco_com(&hist[..30]);
    sincronizar(&mut a);
    db::salvar_resultados(&mut a, &hist[30..]).expect("concursos novos");
    atualizar_forca(&mut a);
    let r = sincronizar(&mut a);
    assert_eq!(r.modo, ModoPosicoes::Incremental);
    assert_eq!(r.concursos_novos, (N - 30) as usize);
    assert_eq!(r.marcas_gravadas, ((N - 30) * POSICOES) as usize);
    assert_eq!(r.registro, RegistroPerfil::JaRegistrado);

    let mut b = banco_com(&hist);
    sincronizar(&mut b);
    assert_eq!(tabelas_posicoes(&a), tabelas_posicoes(&b));
}

#[test]
fn rodar_de_novo_sem_novidade_nao_muda_nada() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    let primeira = sincronizar(&mut conn);
    let tabelas = tabelas_posicoes(&conn);
    let segunda = sincronizar(&mut conn);
    assert_eq!(segunda.modo, ModoPosicoes::Incremental);
    assert_eq!(segunda.concursos_novos, 0);
    assert_eq!(segunda.indice_apagado, segunda.indice_gravado);
    assert_eq!(segunda.proximo, primeira.proximo);
    assert_eq!(tabelas_posicoes(&conn), tabelas);
}

#[test]
fn resultado_antigo_corrigido_recalcula_do_zero_e_iguala_o_banco_novo() {
    let hist = historico(lotofacil(), N);
    let mut corrigido = hist.clone();
    for d in &mut corrigido[9].dezenas {
        *d = *d % 25 + 1;
    }
    assert_ne!(hist[9].dezenas, corrigido[9].dezenas);

    let mut a = banco_com(&hist);
    sincronizar(&mut a);
    db::salvar_resultados(&mut a, &corrigido[9..10]).expect("correção");
    atualizar_forca(&mut a);
    let r = sincronizar(&mut a);
    assert!(
        matches!(&r.modo, ModoPosicoes::Completo(m) if m.contains("marcas já incorporadas mudaram")),
        "{:?}",
        r.modo
    );

    let mut b = banco_com(&corrigido);
    sincronizar(&mut b);
    assert_eq!(tabelas_posicoes(&a), tabelas_posicoes(&b));
}

#[test]
fn concurso_novo_tira_o_indice_velho_de_outro_perfil_das_posicoes() {
    let hist = historico(lotofacil(), N);
    let mut conn = banco_com(&hist[..30]);
    sincronizar(&mut conn);
    let v2 = carregado(
        &PERFIL_POS
            .replace("pos_teste_v1", "pos_teste_v2")
            .replace("versao = 1", "versao = 2"),
        "perfis/lotofacil/pos_teste_v2.toml",
    );
    sincronizar_posicoes(&mut conn, lotofacil(), &perfil_forca(), &v2, CRIADO_EM).unwrap();
    assert_eq!(contar(&conn, "posicao_indice"), 2 * 31 * POSICOES as usize);

    db::salvar_resultados(&mut conn, &hist[30..]).unwrap();
    atualizar_forca(&mut conn);
    let r = sincronizar(&mut conn);
    assert_eq!(r.modo, ModoPosicoes::Incremental);
    // Só o índice do perfil que rodou acompanha o novo próximo concurso.
    assert_eq!(
        contar(&conn, "posicao_indice"),
        ((N + 1) * POSICOES) as usize
    );
    assert!(db_posicoes::ler_indice(&conn, NOME_FORCA, "pos_teste_v2")
        .unwrap()
        .is_empty());
}

#[test]
fn marcas_mexidas_a_mao_recalculam_do_zero() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    sincronizar(&mut conn);
    let tabelas = tabelas_posicoes(&conn);
    conn.execute("DELETE FROM posicao_marcas WHERE concurso = 7", [])
        .unwrap();
    let r = sincronizar(&mut conn);
    assert!(
        matches!(&r.modo, ModoPosicoes::Completo(m) if m.contains("posicao_marcas")),
        "{:?}",
        r.modo
    );
    assert_eq!(tabelas_posicoes(&conn), tabelas);
}

#[test]
fn estado_corrompido_ou_features_apagadas_recalculam_do_zero() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    sincronizar(&mut conn);
    let tabelas = tabelas_posicoes(&conn);

    conn.execute("UPDATE posicao_motor_estado SET estado = 'lixo'", [])
        .unwrap();
    let r = sincronizar(&mut conn);
    assert!(
        matches!(&r.modo, ModoPosicoes::Completo(m) if m.contains("ilegível")),
        "{:?}",
        r.modo
    );
    assert_eq!(tabelas_posicoes(&conn), tabelas);

    conn.execute("DELETE FROM posicao_features WHERE concurso = 5", [])
        .unwrap();
    let r = sincronizar(&mut conn);
    assert!(
        matches!(&r.modo, ModoPosicoes::Completo(m) if m.contains("posicao_features")),
        "{:?}",
        r.modo
    );
    assert_eq!(tabelas_posicoes(&conn), tabelas);
}

// ---------------------------------------------------------------------------
// Recusas.
// ---------------------------------------------------------------------------

#[test]
fn recusas_nao_gravam_nem_criam_nada() {
    let cfg = lotofacil();

    // Sem features nem índice de força.
    let mut vazio = db::abrir(Path::new(":memory:")).unwrap();
    db::salvar_resultados(&mut vazio, &historico(cfg, 10)).unwrap();
    sincronizar_posicoes(&mut vazio, cfg, &perfil_forca(), &perfil_pos(), CRIADO_EM).unwrap_err();
    assert!(sem_tabelas_de_posicoes(&vazio));

    // Índice de força defasado: concurso novo sem `sync features`/`sync indice`.
    let hist = historico(cfg, N + 1);
    let mut defasado = banco_com(&hist[..N as usize]);
    db::salvar_resultados(&mut defasado, &hist[N as usize..]).unwrap();
    let erro = sincronizar_posicoes(
        &mut defasado,
        cfg,
        &perfil_forca(),
        &perfil_pos(),
        CRIADO_EM,
    )
    .unwrap_err();
    assert!(erro.to_string().contains("sync features"), "{erro}");
    assert!(sem_tabelas_de_posicoes(&defasado));

    // Perfil das posições de outra loteria.
    let mut conn = banco_com(&hist[..N as usize]);
    let quina = carregado(
        &PERFIL_POS.replace("lotofacil", "quina"),
        "perfis/quina/pos_teste_v1.toml",
    );
    let erro =
        sincronizar_posicoes(&mut conn, cfg, &perfil_forca(), &quina, CRIADO_EM).unwrap_err();
    assert!(
        matches!(erro, SpiderError::PerfilDeOutraLoteria { .. }),
        "{erro}"
    );
    assert!(sem_tabelas_de_posicoes(&conn));
}

#[test]
fn perfil_das_posicoes_alterado_e_recusado_e_deixa_tudo_como_estava() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    sincronizar(&mut conn);
    let tabelas = tabelas_posicoes(&conn);

    let alterado = carregado(
        &PERFIL_POS.replace("frequencia = 1.0", "frequencia = 2.0"),
        "perfis/lotofacil/pos_teste_v1.toml",
    );
    let erro = sincronizar_posicoes(
        &mut conn,
        lotofacil(),
        &perfil_forca(),
        &alterado,
        CRIADO_EM,
    )
    .unwrap_err();
    assert!(
        matches!(&erro, SpiderError::PerfilAlterado { nome, proximo_nome, .. }
            if nome == NOME_POS && proximo_nome == "pos_teste_v2"),
        "{erro}"
    );
    assert_eq!(tabelas_posicoes(&conn), tabelas);
}

#[test]
fn dois_perfis_de_posicoes_convivem_sem_se_tocar() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    sincronizar(&mut conn);
    let indice_v1 = db_posicoes::ler_indice(&conn, NOME_FORCA, NOME_POS).unwrap();

    let v2 = carregado(
        &PERFIL_POS
            .replace("pos_teste_v1", "pos_teste_v2")
            .replace("versao = 1", "versao = 2")
            .replace("frequencia = 1.0", "frequencia = 3.0"),
        "perfis/lotofacil/pos_teste_v2.toml",
    );
    let r = sincronizar_posicoes(&mut conn, lotofacil(), &perfil_forca(), &v2, CRIADO_EM).unwrap();
    // Marcas e features já estavam em dia: só o índice do novo perfil é gravado.
    assert_eq!(r.modo, ModoPosicoes::Incremental);
    assert_eq!(r.indice_apagado, 0);
    assert_eq!(
        db_posicoes::ler_indice(&conn, NOME_FORCA, NOME_POS).unwrap(),
        indice_v1
    );
    assert_ne!(
        db_posicoes::ler_indice(&conn, NOME_FORCA, "pos_teste_v2").unwrap(),
        indice_v1
    );
    assert_eq!(contar(&conn, "posicao_perfil"), 2);
}

// ---------------------------------------------------------------------------
// Ponto no tempo.
// ---------------------------------------------------------------------------

/// Do concurso `a_partir_de` em diante, outro sorteio (permutação cíclica das
/// dezenas, que mantém cada concurso válido).
fn com_sorteios_trocados(hist: &[Resultado], a_partir_de: u32) -> Vec<Resultado> {
    hist.iter()
        .map(|r| {
            let mut r = r.clone();
            if r.concurso >= a_partir_de {
                for d in &mut r.dezenas {
                    *d = *d % 25 + 1;
                }
            }
            r
        })
        .collect()
}

#[test]
fn linhas_do_concurso_t_nao_mudam_com_o_que_vem_depois_de_t() {
    let t = 25;
    let hist = historico(lotofacil(), 60);
    let alterado = com_sorteios_trocados(&hist, t + 1);
    assert_ne!(hist[t as usize].dezenas, alterado[t as usize].dezenas);
    let (mut a, mut b) = (banco_com(&hist), banco_com(&alterado));
    sincronizar(&mut a);
    sincronizar(&mut b);

    let ate = |conn: &Connection, tabela: &str, ordem: &str| {
        despejar(
            conn,
            &format!("SELECT * FROM {tabela} WHERE concurso <= {t} ORDER BY {ordem}"),
        )
    };
    for (tabela, ordem) in [
        ("posicao_marcas", "perfil, concurso, posicao"),
        ("posicao_features", "perfil, janela, concurso, posicao"),
        (
            "posicao_indice",
            "perfil, perfil_posicoes, concurso, posicao",
        ),
    ] {
        let (la, lb) = (ate(&a, tabela, ordem), ate(&b, tabela, ordem));
        assert!(!la.is_empty());
        assert_eq!(la, lb, "{tabela}");
    }
    // Depois de t as linhas mudam: o teste não passa por entrada igual.
    assert_ne!(tabelas_posicoes(&a)[1], tabelas_posicoes(&b)[1]);
}

// ---------------------------------------------------------------------------
// CSV e relatório em texto.
// ---------------------------------------------------------------------------

#[test]
fn csv_tem_cabecalho_bom_e_uma_linha_por_posicao_em_ordem_de_g_1() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    let r = sincronizar(&mut conn);
    let csv = relatorio::formatar_csv(&r);
    assert!(csv.starts_with(BOM));
    let linhas: Vec<&str> = csv.trim_start_matches(BOM).lines().collect();
    assert_eq!(linhas[0], CABECALHO);
    assert_eq!(linhas.len(), 1 + POSICOES as usize);
    for (i, l) in linhas[1..].iter().enumerate() {
        let campos: Vec<&str> = l.split(';').collect();
        assert_eq!(campos.len(), 9, "{l}");
        assert_eq!(&campos[..4], ["lotofacil", NOME_FORCA, NOME_POS, "41"]);
        assert_eq!(campos[4], (i + 1).to_string());
    }
    assert_eq!(
        relatorio::nome_arquivo(&r),
        "posicoes_lotofacil_teste_v1_pos_teste_v1_41.csv"
    );
    // Mesma entrada → mesmos bytes.
    assert_eq!(relatorio::formatar_csv(&r), csv);
}

#[test]
fn relatorio_em_texto_traz_o_necessario() {
    let mut conn = banco_com(&historico(lotofacil(), N));
    let r = sincronizar(&mut conn);
    let texto = relatorio::formatar_relatorio(lotofacil(), &r);
    for trecho in [
        "Índice de força das posições da Lotofácil: concurso 41",
        "perfil de força teste_v1",
        "perfil pos_teste_v1",
        "Recalculou do zero",
        "registrado agora em posicao_perfil",
        "G-1",
        "descrição do passado, não previsão",
    ] {
        assert!(texto.contains(trecho), "falta `{trecho}` em:\n{texto}");
    }
    assert_eq!(texto, relatorio::formatar_relatorio(lotofacil(), &r));
    // Dez linhas de posição no texto.
    let linhas_de_posicao = texto
        .lines()
        .filter(|l| {
            l.starts_with("  ") && l.trim().chars().next().is_some_and(|c| c.is_ascii_digit())
        })
        .count();
    assert_eq!(linhas_de_posicao, relatorio::TOPO_NO_TEXTO);
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
            "rust_spider_posicoes_{rotulo}_{}_{unico}_{n}",
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

fn args_posicoes(db_caminho: &Path, perfis: &Path, saida: &Path) -> Vec<OsString> {
    vec![
        "posicoes".into(),
        "lotofacil".into(),
        "--perfil".into(),
        NOME_FORCA.into(),
        "--perfil-posicoes".into(),
        NOME_POS.into(),
        "--db".into(),
        db_caminho.into(),
        "--perfis".into(),
        perfis.into(),
        "--saida".into(),
        saida.into(),
    ]
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn cli_grava_tabelas_e_csv_e_retoma_igual() {
    let (pasta, db_caminho, perfis) = ambiente("cli");
    let saida = pasta.0.join("posicoes");
    let args = args_posicoes(&db_caminho, &perfis, &saida);

    let primeira = sync(&pasta.0, &args);
    assert!(primeira.status.success(), "{}", texto(&primeira.stderr));
    let stdout = texto(&primeira.stdout);
    for trecho in [
        "Índice de força das posições",
        "G-1",
        "Aviso: é descrição do passado, não previsão",
        "posicoes_lotofacil_teste_v1_pos_teste_v1_41.csv",
        "Recalculou do zero",
    ] {
        assert!(stdout.contains(trecho), "falta `{trecho}` em:\n{stdout}");
    }

    let arquivo = saida
        .join("lotofacil")
        .join("posicoes_lotofacil_teste_v1_pos_teste_v1_41.csv");
    let csv = std::fs::read(&arquivo).expect("CSV gravado");
    let mut conn = db::abrir(&db_caminho).expect("banco");
    let tabelas = tabelas_posicoes(&conn);
    // O CSV é o da biblioteca sobre o banco gravado (mesmos bytes).
    let r = sincronizar(&mut conn);
    assert_eq!(csv, relatorio::formatar_csv(&r).into_bytes());
    assert_eq!(tabelas_posicoes(&conn), tabelas);
    drop(conn);

    // Segunda execução: retoma do estado, sem concursos novos; mesmo CSV.
    let segunda = sync(&pasta.0, &args);
    assert!(segunda.status.success(), "{}", texto(&segunda.stderr));
    let saida2 = texto(&segunda.stdout);
    assert!(
        saida2.contains("Retomou do estado salvo: 0 concurso(s) novo(s)"),
        "{saida2}"
    );
    assert_eq!(std::fs::read(&arquivo).expect("CSV"), csv);
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert_eq!(tabelas_posicoes(&conn), tabelas);
    // O texto não leva tempo nem data: o tempo vai para a saída de erro.
    assert!(texto(&segunda.stderr).contains("Tempo:"));
}

#[test]
fn cli_recusa_todas_banco_inexistente_e_perfil_ausente() {
    let (pasta, db_caminho, perfis) = ambiente("recusas");
    let saida = pasta.0.join("posicoes");

    let todas = sync(&pasta.0, &["posicoes".into(), "todas".into()]);
    assert!(!todas.status.success());
    assert!(texto(&todas.stderr).contains("`posicoes` não aceita `todas`"));
    assert!(!saida.exists());

    let sem_banco = sync(
        &pasta.0,
        &args_posicoes(&pasta.0.join("nao_existe.db"), &perfis, &saida),
    );
    assert!(!sem_banco.status.success());
    assert!(texto(&sem_banco.stderr).contains("banco não encontrado"));
    assert!(!pasta.0.join("nao_existe.db").exists());

    // `--perfil-posicoes` padrão é `posicoes_v1`, que não está na pasta de teste.
    let sem_perfil = sync(
        &pasta.0,
        &[
            "posicoes".into(),
            "lotofacil".into(),
            "--perfil".into(),
            NOME_FORCA.into(),
            "--db".into(),
            db_caminho.as_os_str().to_owned(),
            "--perfis".into(),
            perfis.as_os_str().to_owned(),
            "--saida".into(),
            saida.as_os_str().to_owned(),
        ],
    );
    assert!(!sem_perfil.status.success());
    assert!(texto(&sem_perfil.stderr).contains("posicoes_v1"));
    assert!(!saida.exists(), "nada foi gravado");
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert!(sem_tabelas_de_posicoes(&conn));
}

#[test]
fn cli_recusa_indice_de_forca_defasado_sem_gravar_csv() {
    let (pasta, db_caminho, perfis) = ambiente("defasado");
    let saida = pasta.0.join("posicoes");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco");
        let hist = historico(lotofacil(), N + 1);
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    }
    let r = sync(&pasta.0, &args_posicoes(&db_caminho, &perfis, &saida));
    assert!(!r.status.success());
    assert!(
        texto(&r.stderr).contains("sync features lotofacil"),
        "{}",
        texto(&r.stderr)
    );
    assert!(!saida.exists(), "nenhum CSV");
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert!(sem_tabelas_de_posicoes(&conn));
}

#[test]
fn perfis_posicoes_v1_existem_e_sao_validos_nas_tres_loterias() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("perfis");
    for cfg in config::LOTERIAS {
        let c = perfil::carregar(&raiz, cfg, "posicoes_v1")
            .unwrap_or_else(|e| panic!("{}: {e}", cfg.slug));
        assert_eq!(c.perfil.loteria, cfg.slug);
        // Pesos iniciais iguais aos do perfil ativo, até a busca de pesos.
        let ativo = perfil::carregar(&raiz, cfg, cfg.perfil_ativo).expect("perfil ativo");
        assert_eq!(c.perfil.pesos, ativo.perfil.pesos, "{}", cfg.slug);
        assert_eq!(c.perfil.janela, ativo.perfil.janela, "{}", cfg.slug);
        assert_eq!(
            c.perfil.normalizacao, ativo.perfil.normalizacao,
            "{}",
            cfg.slug
        );
    }
}

#[test]
fn constantes_do_teste_batem_com_a_loteria() {
    assert_eq!(lotofacil().qtd_dezenas_possiveis(), POSICOES);
    assert_eq!(lotofacil().qtd_dezenas, K);
}
