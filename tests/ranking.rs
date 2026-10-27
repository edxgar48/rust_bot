//! Testes de `orquestracao::sincronizar_ranking` e do comando `sync ranking`
//! (Fase 1f, tarefa 1f.3) sobre bancos em memória e em arquivo: as três tabelas
//! derivadas (`ranking_taxa`, `taxa_posicao`, `taxa_faixa`), regravação
//! idempotente, recusas sem gravar nada, ponto no tempo no nível do banco, CSV
//! determinístico e a CLI.
//!
//! A matemática (taxas, notas, evidência, esperado) é da `avaliacao` e já tem
//! testes próprios; aqui se confere a ligação com o banco: o que foi gravado é
//! o que as funções puras calculam sobre o índice e as features lidos do banco,
//! e a taxa por posição bate com uma conta independente feita só com
//! `resultados` e as posições do índice.

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::avaliacao::{
    self, LinhaRanking, ObservacaoPosicoes, CONCURSOS_FICTICIOS, NOMES_COMPONENTES,
    SEMENTE_ESPERADO,
};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    sincronizar_features, sincronizar_indice, sincronizar_ranking, sincronizar_ranking_com,
    RelatorioRanking,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::ranking::{self, BOM, CABECALHO};
use rust_spider::Resultado;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const NOME: &str = "teste_v1";
const CRIADO_EM: &str = "2026-10-27T10:00:00Z";
const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
const N: u32 = 40;
/// Poucas réplicas: o esperado só é impresso, e a simulação é o que mais pesa
/// em um teste sem otimização.
const REPLICAS: usize = 20;

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

fn carregado(texto: &str) -> PerfilCarregado {
    perfil::interpretar(texto, Path::new("perfis/lotofacil/teste_v1.toml")).expect("perfil")
}

fn perfil_teste() -> PerfilCarregado {
    carregado(PERFIL)
}

/// Grava `hist`, calcula as features e o índice do perfil de teste.
fn preparar(conn: &mut Connection, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, lotofacil(), false).expect("features");
    sincronizar_indice(conn, lotofacil(), &perfil_teste(), CRIADO_EM).expect("índice");
}

fn banco_com(hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, hist);
    conn
}

fn banco(n: u32) -> Connection {
    banco_com(&historico(lotofacil(), n))
}

fn ranquear_banco(conn: &mut Connection) -> RelatorioRanking {
    sincronizar_ranking_com(
        conn,
        lotofacil(),
        &perfil_teste(),
        REPLICAS,
        SEMENTE_ESPERADO,
    )
    .expect("ranking")
}

/// Todas as linhas de uma consulta, cada valor em `Debug` (o `f64` do `Debug`
/// é o mais curto que volta ao mesmo valor).
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

/// As três tabelas do ranking, ordenadas pela chave.
fn tabelas_do_ranking(conn: &Connection) -> [Vec<String>; 3] {
    [
        despejar(
            conn,
            "SELECT * FROM ranking_taxa ORDER BY concurso, dezena, perfil",
        ),
        despejar(conn, "SELECT * FROM taxa_posicao ORDER BY perfil, posicao"),
        despejar(
            conn,
            "SELECT * FROM taxa_faixa ORDER BY perfil, componente, faixa",
        ),
    ]
}

fn nomes_das_tabelas(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("prepara");
    let nomes = stmt
        .query_map([], |row| row.get(0))
        .expect("consulta")
        .collect::<rusqlite::Result<Vec<String>>>()
        .expect("nomes");
    nomes
}

fn sem_tabelas_do_ranking(conn: &Connection) -> bool {
    let nomes = nomes_das_tabelas(conn);
    !["ranking_taxa", "taxa_posicao", "taxa_faixa"]
        .iter()
        .any(|t| nomes.iter().any(|n| n == t))
}

// ---------------------------------------------------------------------------
// O que é gravado.
// ---------------------------------------------------------------------------

/// A entrada de `calcular_taxas` montada aqui, direto de `indice` e
/// `features_dezena` (sem passar pela montagem da orquestração).
fn observacoes_do_banco(conn: &Connection) -> Vec<ObservacaoPosicoes> {
    let indice = db::ler_indice(conn, NOME).expect("índice");
    let features = db::ler_features_da_janela(conn, 10).expect("features");
    assert_eq!(indice.len(), features.len());
    indice
        .iter()
        .zip(&features)
        .map(|(i, f)| {
            assert_eq!(
                (i.concurso, i.dezena),
                (f.features.concurso, f.features.dezena)
            );
            ObservacaoPosicoes {
                concurso: i.concurso,
                dezena: i.dezena,
                posicao: i.posicao,
                saiu: f.features.saiu,
                componentes: ObservacaoPosicoes::componentes_de(&f.features),
            }
        })
        .collect()
}

#[test]
fn grava_o_que_as_funcoes_puras_calculam_sobre_o_banco() {
    let cfg = lotofacil();
    let mut conn = banco(N);
    let r = ranquear_banco(&mut conn);

    let serie = avaliacao::calcular_taxas(cfg, &observacoes_do_banco(&conn)).expect("taxas");
    let ranking = avaliacao::ranquear(cfg, &serie.linhas).expect("ranking puro");
    let gravadas = db::ler_ranking_taxa(&conn, NOME).expect("lê");
    assert_eq!(gravadas.len(), (N as usize + 1) * 25);
    assert_eq!(gravadas.len(), ranking.len());
    for (g, p) in gravadas.iter().zip(&ranking) {
        assert_eq!((g.concurso, g.dezena), (p.concurso, p.dezena));
        assert_eq!(g.posicao_indice, p.posicao_indice);
        // Bit a bit: o REAL do SQLite guarda o `f64` inteiro.
        assert_eq!(g.taxa_posicao.to_bits(), p.nota_a.to_bits());
        assert_eq!(g.nota_b.to_bits(), p.nota_b.to_bits());
        assert_eq!(
            (g.posicao_ranking_a, g.posicao_ranking_b),
            (p.posicao_ranking_a, p.posicao_ranking_b)
        );
    }

    // taxa_posicao: uma linha por posição, igual às contagens do histórico
    // inteiro e à evidência da 1d.
    let ev = avaliacao::evidencia_das_taxas(cfg, &serie.acumuladas).expect("evidência");
    let posicoes = db::ler_taxa_posicao(&conn, NOME).expect("lê");
    assert_eq!(posicoes.len(), 25);
    for (l, e) in posicoes.iter().zip(&ev.posicoes) {
        let c = serie.acumuladas.contagem_posicao(l.posicao).expect("conta");
        assert_eq!((l.saidas, l.exposicoes), (c.saidas, c.total));
        assert_eq!(l.exposicoes, u64::from(N));
        assert_eq!(
            l.taxa.to_bits(),
            serie
                .acumuladas
                .taxa_posicao(l.posicao)
                .expect("taxa")
                .to_bits()
        );
        let c = e.avaliacao.as_ref().expect("com exposição").classificacao[0];
        assert_eq!(l.grau.as_deref(), Some(c.grau.nome()));
        assert_eq!(l.direcao.as_deref(), Some(c.direcao.nome()));
    }

    // taxa_faixa: 9 componentes × 4 faixas, com os nomes que o código usa.
    let faixas = db::ler_taxa_faixa(&conn, NOME).expect("lê");
    assert_eq!(faixas.len(), 36);
    for e in &ev.faixas {
        let l = faixas
            .iter()
            .find(|l| l.componente == NOMES_COMPONENTES[e.componente] && l.faixa == e.faixa.nome())
            .expect("linha da faixa");
        assert_eq!((l.saidas, l.exposicoes), (e.saidas, e.total));
        assert_eq!(
            l.taxa.to_bits(),
            serie
                .acumuladas
                .taxa_faixa(e.componente, e.faixa)
                .expect("taxa")
                .to_bits()
        );
        assert_eq!(
            l.grau.as_deref(),
            e.avaliacao.as_ref().map(|a| a.classificacao[0].grau.nome())
        );
        assert_eq!(
            l.direcao.as_deref(),
            e.avaliacao
                .as_ref()
                .map(|a| a.classificacao[0].direcao.nome())
        );
    }
    // Cada par concurso × dezena cai numa faixa de cada componente.
    for c in NOMES_COMPONENTES {
        let pares: u64 = faixas
            .iter()
            .filter(|l| l.componente == c)
            .map(|l| l.exposicoes)
            .sum();
        assert_eq!(pares, u64::from(N) * 25, "{c}");
    }

    // O relatório devolvido é o que foi gravado e o que as funções puras dão.
    assert_eq!(r.taxas_posicao, posicoes);
    // O relatório traz as faixas na ordem do código (componente, faixa); a
    // leitura do banco, na ordem alfabética do texto.
    let mut do_relatorio = r.taxas_faixa.clone();
    do_relatorio.sort_by(|a, b| (&a.componente, &a.faixa).cmp(&(&b.componente, &b.faixa)));
    assert_eq!(do_relatorio, faixas);
    assert_eq!(r.evidencia, ev);
    assert_eq!(
        r.esperado,
        avaliacao::esperado_das_taxas(cfg, &serie.linhas, REPLICAS, SEMENTE_ESPERADO)
            .expect("esperado")
    );
    let proximo: Vec<LinhaRanking> = ranking
        .into_iter()
        .filter(|l| l.concurso == N + 1)
        .collect();
    assert_eq!(r.proximo, proximo);
    assert_eq!(
        (
            r.perfil.as_str(),
            r.versao,
            r.janela,
            r.normalizacao.as_str()
        ),
        (NOME, 1, 10, "minmax")
    );
    assert_eq!(r.hash_toml, perfil_teste().hash_toml);
    assert_eq!(
        (
            r.primeiro_concurso,
            r.ultimo_concurso,
            r.proximo_concurso,
            r.concursos_sorteados
        ),
        (1, N, N + 1, u64::from(N))
    );
    assert_eq!(
        (
            r.linhas_apagadas,
            r.linhas_gravadas,
            r.posicoes_gravadas,
            r.faixas_gravadas
        ),
        (0, gravadas.len(), 25, 36)
    );
}

#[test]
fn taxa_por_posicao_confere_com_conta_independente_dos_resultados() {
    let cfg = lotofacil();
    let hist = historico(cfg, N);
    let mut conn = banco_com(&hist);
    ranquear_banco(&mut conn);

    // Conta independente: só `resultados` (as dezenas sorteadas) e a posição
    // que o índice deu a cada dezena; nada de `features_dezena.saiu`.
    let sorteadas: BTreeMap<u32, BTreeSet<u32>> = hist
        .iter()
        .map(|r| (r.concurso, r.dezenas.iter().copied().collect()))
        .collect();
    let indice = db::ler_indice(&conn, NOME).expect("índice");
    let acaso = cfg.qtd_dezenas as f64 / f64::from(cfg.qtd_dezenas_possiveis());
    let m = f64::from(CONCURSOS_FICTICIOS);
    let taxa = |saidas: u64, antes: u32| (saidas as f64 + m * acaso) / (f64::from(antes) + m);

    let gravadas = db::ler_ranking_taxa(&conn, NOME).expect("lê");
    let mut saidas = [0_u64; 26];
    for t in 1..=N + 1 {
        let do_concurso: Vec<_> = gravadas.iter().filter(|g| g.concurso == t).collect();
        assert_eq!(do_concurso.len(), 25);
        for g in &do_concurso {
            let esperada = taxa(saidas[g.posicao_indice as usize], t - 1);
            assert!(
                (g.taxa_posicao - esperada).abs() < 1e-12,
                "concurso {t}, dezena {}: {} contra {esperada}",
                g.dezena,
                g.taxa_posicao
            );
        }
        // Só depois a posição de cada dezena de `t` ganha o resultado de `t`.
        if let Some(saiu) = sorteadas.get(&t) {
            for i in indice.iter().filter(|i| i.concurso == t) {
                saidas[i.posicao as usize] += u64::from(saiu.contains(&i.dezena));
            }
        }
    }
    // O histórico inteiro vale para o snapshot (e para `taxa_posicao`).
    let posicoes = db::ler_taxa_posicao(&conn, NOME).expect("lê");
    for l in &posicoes {
        assert_eq!(l.saidas, saidas[l.posicao as usize]);
        assert!((l.taxa - taxa(l.saidas, N)).abs() < 1e-12);
    }
    assert_eq!(saidas.iter().sum::<u64>(), u64::from(N) * 15);

    // A ordem pela nota A: taxa arredondada decrescente, empate pela menor
    // dezena; e as posições formam a permutação 1..=N em cada concurso.
    for t in 1..=N + 1 {
        let mut do_concurso: Vec<_> = gravadas.iter().filter(|g| g.concurso == t).collect();
        do_concurso.sort_by_key(|g| g.posicao_ranking_a);
        let posicoes: Vec<u32> = do_concurso.iter().map(|g| g.posicao_ranking_a).collect();
        assert_eq!(posicoes, (1..=25).collect::<Vec<_>>());
        for par in do_concurso.windows(2) {
            let arredondada = |x: f64| (x * 1e9).round() / 1e9;
            let (a, b) = (
                arredondada(par[0].taxa_posicao),
                arredondada(par[1].taxa_posicao),
            );
            assert!(
                a > b || (a == b && par[0].dezena < par[1].dezena),
                "concurso {t}"
            );
        }
    }
}

#[test]
fn faixa_sem_exposicao_fica_na_tabela_com_taxa_do_acaso_e_grau_nulo() {
    // Um só concurso sorteado: no concurso 1 os componentes que nunca são NULL
    // (`frequencia`, `repeticoes_total`, `repeticao_atual`) são constantes
    // (todas em "médio") e os outros 6 são NULL (faixa "nulo"). As faixas
    // "baixo" e "alto" ficam sem nenhum par.
    let cfg = lotofacil();
    let mut conn = banco(1);
    ranquear_banco(&mut conn);
    let faixas = db::ler_taxa_faixa(&conn, NOME).expect("lê");
    assert_eq!(faixas.len(), 36);
    let acaso = cfg.qtd_dezenas as f64 / f64::from(cfg.qtd_dezenas_possiveis());
    let de = |componente: &str, faixa: &str| {
        faixas
            .iter()
            .find(|l| l.componente == componente && l.faixa == faixa)
            .expect("linha")
    };
    let baixo = de("frequencia", "baixo");
    assert_eq!((baixo.saidas, baixo.exposicoes), (0, 0));
    assert!((baixo.taxa - acaso).abs() < 1e-15, "sem exposição vale K/N");
    assert_eq!((baixo.grau.clone(), baixo.direcao.clone()), (None, None));
    let medio = de("frequencia", "médio");
    assert_eq!(medio.exposicoes, 25);
    assert!(medio.grau.is_some() && medio.direcao.is_some());
    assert_eq!(de("maior_espaco", "nulo").exposicoes, 25);
    // NULL de verdade no banco.
    let nulos: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM taxa_faixa WHERE grau IS NULL AND direcao IS NULL",
            [],
            |r| r.get(0),
        )
        .expect("conta");
    assert_eq!(nulos, 36 - 9, "só as 9 células com pares têm grau");
}

// ---------------------------------------------------------------------------
// Regravar e determinismo.
// ---------------------------------------------------------------------------

#[test]
fn regravar_deixa_as_tres_tabelas_e_o_csv_identicos() {
    let mut conn = banco(N);
    let primeira = ranquear_banco(&mut conn);
    let tabelas = tabelas_do_ranking(&conn);
    let csv = ranking::formatar_csv(&primeira);
    let texto = ranking::formatar_relatorio(lotofacil(), &primeira);
    assert_eq!(tabelas[0].len(), (N as usize + 1) * 25);

    let segunda = ranquear_banco(&mut conn);
    assert_eq!(tabelas_do_ranking(&conn), tabelas);
    assert_eq!(ranking::formatar_csv(&segunda), csv);
    assert_eq!(ranking::formatar_relatorio(lotofacil(), &segunda), texto);
    // Substituiu o que havia, sem acumular linhas.
    assert_eq!(segunda.linhas_apagadas, primeira.linhas_gravadas);
    assert_eq!(segunda.linhas_gravadas, primeira.linhas_gravadas);
    assert_eq!(
        (segunda.proximo.clone(), segunda.evidencia.clone()),
        (primeira.proximo.clone(), primeira.evidencia.clone())
    );
}

#[test]
fn evidencia_e_esperado_nao_alimentam_o_que_e_gravado() {
    // Outras réplicas e outra semente mudam o esperado do relatório, mas nada
    // nas tabelas: grau, direção e esperado só descrevem.
    let mut conn = banco(N);
    let a = sincronizar_ranking_com(&mut conn, lotofacil(), &perfil_teste(), 20, 1).expect("a");
    let tabelas = tabelas_do_ranking(&conn);
    let b = sincronizar_ranking_com(&mut conn, lotofacil(), &perfil_teste(), 33, 99).expect("b");
    assert_eq!(tabelas_do_ranking(&conn), tabelas);
    assert_eq!(a.proximo, b.proximo);
    assert_eq!(a.evidencia, b.evidencia);
    assert_ne!(a.esperado, b.esperado);
    assert_eq!((b.esperado.replicas, b.esperado.semente), (33, 99));
}

#[test]
fn dois_bancos_iguais_dao_as_mesmas_tabelas() {
    let mut a = banco(N);
    let mut b = banco(N);
    let ra = ranquear_banco(&mut a);
    let rb = ranquear_banco(&mut b);
    assert_eq!(tabelas_do_ranking(&a), tabelas_do_ranking(&b));
    assert_eq!(ra, rb);
    assert_eq!(ranking::formatar_csv(&ra), ranking::formatar_csv(&rb));
}

#[test]
fn outro_perfil_nao_e_tocado_ao_regravar() {
    let cfg = lotofacil();
    let mut conn = banco(N);
    let outro_texto = PERFIL
        .replace("teste_v1", "outro_v1")
        .replace("frequencia = 1.0", "frequencia = 3.0");
    let outro = carregado(&outro_texto);
    sincronizar_indice(&mut conn, cfg, &outro, CRIADO_EM).expect("índice do outro");

    ranquear_banco(&mut conn);
    sincronizar_ranking_com(&mut conn, cfg, &outro, REPLICAS, SEMENTE_ESPERADO).expect("outro");
    let do_outro = |conn: &Connection| {
        [
            despejar(
                conn,
                "SELECT * FROM ranking_taxa WHERE perfil = 'outro_v1' ORDER BY concurso, dezena",
            ),
            despejar(
                conn,
                "SELECT * FROM taxa_posicao WHERE perfil = 'outro_v1' ORDER BY posicao",
            ),
            despejar(
                conn,
                "SELECT * FROM taxa_faixa WHERE perfil = 'outro_v1' ORDER BY componente, faixa",
            ),
        ]
    };
    let antes = do_outro(&conn);
    assert!(antes.iter().all(|t| !t.is_empty()));

    // Regravar o primeiro perfil não mexe nas linhas do segundo.
    ranquear_banco(&mut conn);
    assert_eq!(do_outro(&conn), antes);
    // E os dois perfis têm posições diferentes (o teste não passa por vazio).
    let posicoes = |perfil: &str| {
        db::ler_ranking_taxa(&conn, perfil)
            .expect("lê")
            .iter()
            .map(|l| l.posicao_indice)
            .collect::<Vec<_>>()
    };
    assert_ne!(posicoes(NOME), posicoes("outro_v1"));
}

// ---------------------------------------------------------------------------
// Recusas.
// ---------------------------------------------------------------------------

#[test]
fn recusas_nao_gravam_nem_criam_nada() {
    let cfg = lotofacil();
    let hist = historico(cfg, N + 1);

    // Sem resultados.
    let mut vazio = db::abrir(Path::new(":memory:")).expect("banco");
    assert!(matches!(
        sincronizar_ranking(&mut vazio, cfg, &perfil_teste()),
        Err(SpiderError::SemResultados(_))
    ));
    assert_eq!(nomes_das_tabelas(&vazio), ["resultados"]);

    // Resultados, mas sem features.
    db::salvar_resultados(&mut vazio, &hist[..N as usize]).expect("resultados");
    assert!(matches!(
        sincronizar_ranking(&mut vazio, cfg, &perfil_teste()),
        Err(SpiderError::PerfilNaoRegistrado { .. })
    ));
    assert_eq!(nomes_das_tabelas(&vazio), ["resultados"]);

    // Features em dia, índice nunca gravado: o perfil não está em `perfis`.
    sincronizar_features(&mut vazio, cfg, false).expect("features");
    assert!(matches!(
        sincronizar_ranking(&mut vazio, cfg, &perfil_teste()),
        Err(SpiderError::PerfilNaoRegistrado { .. })
    ));
    assert!(sem_tabelas_do_ranking(&vazio));
    assert!(!nomes_das_tabelas(&vazio).contains(&"perfis".to_string()));

    // Um concurso novo sem `sync features`: features defasadas.
    let mut conn = banco(N);
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    match sincronizar_ranking(&mut conn, cfg, &perfil_teste()) {
        Err(SpiderError::FeaturesDesatualizadas {
            ultimo_concurso,
            snapshot,
            ..
        }) => assert_eq!((ultimo_concurso, snapshot), (N + 1, N + 1)),
        outro => panic!("esperava FeaturesDesatualizadas, veio {outro:?}"),
    }
    assert!(sem_tabelas_do_ranking(&conn));

    // Features em dia, mas o índice do perfil ficou no concurso anterior.
    sincronizar_features(&mut conn, cfg, false).expect("features em dia");
    match sincronizar_ranking(&mut conn, cfg, &perfil_teste()) {
        Err(SpiderError::IndiceDesatualizado {
            ultimo_indice,
            esperado,
            ..
        }) => assert_eq!((ultimo_indice, esperado), (Some(N + 1), N + 2)),
        outro => panic!("esperava IndiceDesatualizado, veio {outro:?}"),
    }
    assert!(sem_tabelas_do_ranking(&conn));

    // Perfil editado sob o mesmo nome: o hash difere do registrado.
    let editado = carregado(&PERFIL.replace("frequencia = 1.0", "frequencia = 2.0"));
    assert!(matches!(
        sincronizar_ranking(&mut conn, cfg, &editado),
        Err(SpiderError::PerfilAlterado { .. })
    ));
    assert!(sem_tabelas_do_ranking(&conn));

    // Perfil de outra loteria.
    let de_quina = carregado(&PERFIL.replace("lotofacil", "quina"));
    assert!(matches!(
        sincronizar_ranking(&mut conn, cfg, &de_quina),
        Err(SpiderError::PerfilDeOutraLoteria { .. })
    ));
    assert!(sem_tabelas_do_ranking(&conn));

    // Menos de 2 réplicas para o esperado (num banco em dia).
    let mut em_dia = banco(N);
    assert!(matches!(
        sincronizar_ranking_com(&mut em_dia, cfg, &perfil_teste(), 1, 1),
        Err(SpiderError::EvidenciaEntradaInvalida(_))
    ));
    assert!(sem_tabelas_do_ranking(&em_dia));
}

#[test]
fn recusa_depois_de_gravar_deixa_o_ranking_anterior_intacto() {
    let cfg = lotofacil();
    let hist = historico(cfg, N + 1);
    let mut conn = banco_com(&hist[..N as usize]);
    ranquear_banco(&mut conn);
    let tabelas = tabelas_do_ranking(&conn);

    // Chega um concurso e as features/índice ficam para trás.
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    assert!(matches!(
        sincronizar_ranking(&mut conn, cfg, &perfil_teste()),
        Err(SpiderError::FeaturesDesatualizadas { .. })
    ));
    assert_eq!(tabelas_do_ranking(&conn), tabelas);

    // Atualizando tudo, o ranking passa a ter o concurso novo.
    sincronizar_features(&mut conn, cfg, false).expect("features");
    sincronizar_indice(&mut conn, cfg, &perfil_teste(), CRIADO_EM).expect("índice");
    let r = ranquear_banco(&mut conn);
    assert_eq!(r.proximo_concurso, N + 2);
    assert_eq!(r.linhas_apagadas, (N as usize + 1) * 25);
    assert_eq!(r.linhas_gravadas, (N as usize + 2) * 25);
}

// ---------------------------------------------------------------------------
// Ponto no tempo.
// ---------------------------------------------------------------------------

/// Do concurso `a_partir_de` em diante, outro sorteio: a permutação cíclica
/// das dezenas mantém cada concurso válido e muda `saiu`, as features e o
/// índice daí em diante.
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

fn linhas_ate(conn: &Connection, concurso: u32) -> Vec<String> {
    despejar(
        conn,
        &format!(
            "SELECT * FROM ranking_taxa WHERE concurso <= {concurso} \
             ORDER BY concurso, dezena, perfil"
        ),
    )
}

#[test]
fn linha_do_concurso_t_nao_muda_com_o_que_vem_depois_de_t() {
    let cfg = lotofacil();
    let t = 25;
    let hist = historico(cfg, 60);

    // Mesmo prefixo até t−1, resultados diferentes de t em diante.
    let alterado = com_sorteios_trocados(&hist, t);
    assert_ne!(
        hist[t as usize - 1].dezenas,
        alterado[t as usize - 1].dezenas
    );
    let (mut a, mut b) = (banco_com(&hist), banco_com(&alterado));
    ranquear_banco(&mut a);
    ranquear_banco(&mut b);
    let (la, lb) = (linhas_ate(&a, t), linhas_ate(&b, t));
    assert_eq!(la.len(), t as usize * 25);
    // As taxas de t veem só 1..t−1, e a posição de t vem de features que olham
    // 1..t−1: nada do sorteio de t, nem do que vem depois, chega à linha de t.
    assert_eq!(la, lb);
    // Depois de t as linhas mudam: o teste não passa por entrada igual.
    assert_ne!(linhas_ate(&a, 60), linhas_ate(&b, 60));
    assert_ne!(tabelas_do_ranking(&a)[1], tabelas_do_ranking(&b)[1]);

    // Acrescentar concursos no fim também não muda nenhuma linha já gravada:
    // o snapshot do próximo (n+1) de um banco curto é a linha do concurso n+1
    // do banco longo.
    let mut curto = banco_com(&hist[..40]);
    let mut longo = banco_com(&hist);
    ranquear_banco(&mut curto);
    ranquear_banco(&mut longo);
    assert_eq!(linhas_ate(&curto, 41), linhas_ate(&longo, 41));
    assert_eq!(linhas_ate(&curto, 41).len(), 41 * 25);
}

// ---------------------------------------------------------------------------
// CSV e relatório em texto.
// ---------------------------------------------------------------------------

fn linha_ranking(
    dezena: u32,
    posicao_indice: u32,
    a: f64,
    b: f64,
    ra: u32,
    rb: u32,
) -> LinhaRanking {
    LinhaRanking {
        concurso: N + 1,
        dezena,
        posicao_indice,
        saiu: None,
        nota_a: a,
        nota_b: b,
        posicao_ranking_a: ra,
        posicao_ranking_b: rb,
    }
}

#[test]
fn csv_confere_byte_a_byte_com_linhas_conhecidas() {
    let mut conn = banco(N);
    let mut r = ranquear_banco(&mut conn);
    // Linhas à mão, fora de ordem, com -0,0 e uma nota B negativa.
    r.proximo = vec![
        linha_ranking(7, 3, 0.5, -0.0, 2, 3),
        linha_ranking(3, 1, 0.625_000_000_4, 0.123_456_789_49, 1, 1),
        linha_ranking(12, 25, 0.25, -1.5, 3, 2),
    ];
    let esperado = format!(
        "{BOM}{CABECALHO}\n\
         lotofacil;teste_v1;41;3;1;0,625000000;0,123456789;1;1\n\
         lotofacil;teste_v1;41;7;3;0,500000000;0,000000000;2;3\n\
         lotofacil;teste_v1;41;12;25;0,250000000;-1,500000000;3;2\n"
    );
    assert_eq!(ranking::formatar_csv(&r), esperado);
    assert_eq!(
        ranking::nome_arquivo(&r),
        "ranking_lotofacil_teste_v1_41.csv"
    );
    assert!(CABECALHO.starts_with("loteria;perfil;concurso;dezena;posicao_indice;taxa_posicao;"));
}

#[test]
fn csv_do_banco_tem_uma_linha_por_dezena_em_ordem_da_nota_a() {
    let mut conn = banco(N);
    let r = ranquear_banco(&mut conn);
    let csv = ranking::formatar_csv(&r);
    assert_eq!(&csv.as_bytes()[..3], [0xEF, 0xBB, 0xBF]);
    assert_eq!(csv.matches('\u{FEFF}').count(), 1);
    let corpo = csv.strip_prefix(BOM).expect("BOM");
    let mut linhas = corpo.lines();
    assert_eq!(linhas.next(), Some(CABECALHO));
    let colunas = CABECALHO.split(';').count();
    let campos: Vec<Vec<&str>> = linhas.map(|l| l.split(';').collect()).collect();
    assert_eq!(campos.len(), 25);
    for (i, c) in campos.iter().enumerate() {
        assert_eq!(c.len(), colunas);
        assert_eq!((c[0], c[1], c[2]), ("lotofacil", NOME, "41"));
        assert_eq!(c[7], (i + 1).to_string(), "ordem pela nota A");
        assert!(c[5].contains(',') && !c[5].contains('.'), "vírgula decimal");
    }
    // As dezenas do CSV são as do ranking gravado no próximo concurso.
    let do_banco: BTreeMap<u32, u32> = db::ler_ranking_taxa(&conn, NOME)
        .expect("lê")
        .into_iter()
        .filter(|l| l.concurso == N + 1)
        .map(|l| (l.dezena, l.posicao_ranking_a))
        .collect();
    for c in &campos {
        assert_eq!(
            do_banco[&c[3].parse::<u32>().expect("dezena")],
            c[7].parse::<u32>().expect("pos")
        );
    }
}

#[test]
fn relatorio_em_texto_traz_o_necessario_e_e_deterministico() {
    let cfg = lotofacil();
    let mut conn = banco(N);
    let r = ranquear_banco(&mut conn);
    let texto = ranking::formatar_relatorio(cfg, &r);
    assert_eq!(texto, ranking::formatar_relatorio(cfg, &r.clone()));
    for trecho in [
        "Lotofácil",
        "concurso 41 (o próximo, ainda não sorteado)",
        "Perfil teste_v1 (versão 1, janela 10, minmax, hash ",
        &perfil_teste().hash_toml,
        "AVISO: isto é a descrição do passado, não uma previsão",
        "Os sorteios são independentes",
        "Ranking pela nota A (principal)",
        "Ranking pela nota B (EXPERIMENTAL",
        "Nunca é a nota principal",
        "Taxa por posição do índice",
        "Taxa por (componente, faixa)",
        "observadas / esperadas sob o acaso",
        "Priori 5.0 % (principal)",
        "Moderada ou maior (as duas direções)",
        "simulação de 20 réplicas",
    ] {
        assert!(texto.contains(trecho), "falta `{trecho}` em:\n{texto}");
    }
    // 25 posições e 36 faixas, uma linha cada.
    for c in NOMES_COMPONENTES {
        assert!(texto.contains(c), "{c}");
    }
    assert!(texto.lines().filter(|l| l.contains("a favor d")).count() >= 25);
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
            "rust_spider_ranking_{rotulo}_{}_{unico}_{n}",
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
        preparar(&mut conn, &historico(lotofacil(), N));
    }
    let perfis = pasta.0.join("perfis");
    std::fs::create_dir_all(perfis.join("lotofacil")).expect("pasta de perfis");
    std::fs::write(perfis.join("lotofacil").join("teste_v1.toml"), PERFIL).expect("perfil");
    (pasta, db_caminho, perfis)
}

/// O `sync` roda dentro da pasta temporária: a trava (`sync.lock`) e o log
/// (`logs/`) são relativos ao diretório de trabalho, e o repositório não pode
/// ganhar arquivos por causa de um teste.
fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_ranking(db_caminho: &Path, perfis: &Path, saida: &Path) -> Vec<OsString> {
    vec![
        "ranking".into(),
        "lotofacil".into(),
        "--perfil".into(),
        NOME.into(),
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
fn cli_grava_tabelas_e_csv_e_regenera_igual() {
    let (pasta, db_caminho, perfis) = ambiente("cli");
    let saida = pasta.0.join("ranking");
    let args = args_ranking(&db_caminho, &perfis, &saida);

    let primeira = sync(&pasta.0, &args);
    assert!(primeira.status.success(), "{}", texto(&primeira.stderr));
    let stdout = texto(&primeira.stdout);
    for trecho in [
        "Ranking por taxa histórica",
        "EXPERIMENTAL",
        "AVISO: isto é a descrição do passado, não uma previsão",
        "observadas / esperadas sob o acaso",
        "ranking_lotofacil_teste_v1_41.csv",
        "simulação de 1000 réplicas",
    ] {
        assert!(stdout.contains(trecho), "falta `{trecho}` em:\n{stdout}");
    }

    let arquivo = saida
        .join("lotofacil")
        .join("ranking_lotofacil_teste_v1_41.csv");
    let csv = std::fs::read(&arquivo).expect("CSV gravado");
    // O CSV é o da biblioteca sobre o banco gravado (mesmos bytes).
    let mut conn = db::abrir(&db_caminho).expect("banco");
    let r = sincronizar_ranking(&mut conn, lotofacil(), &perfil_teste()).expect("ranking");
    assert_eq!(csv, ranking::formatar_csv(&r).into_bytes());
    let tabelas = tabelas_do_ranking(&conn);
    drop(conn);

    // Segunda execução: mesmo relatório, mesmo CSV, mesmas tabelas. A linha de
    // gravação difere só em quantas linhas foram substituídas (0, depois todas).
    let segunda = sync(&pasta.0, &args);
    assert!(segunda.status.success(), "{}", texto(&segunda.stderr));
    let relatorio = |saida: &str| {
        let i = saida.find("Ranking por taxa histórica").expect("relatório");
        saida[i..].to_string()
    };
    assert_eq!(relatorio(&texto(&segunda.stdout)), relatorio(&stdout));
    assert!(stdout.contains("(0 substituída(s))"), "{stdout}");
    assert!(
        texto(&segunda.stdout).contains(&format!("({} substituída(s))", (N as usize + 1) * 25)),
        "{}",
        texto(&segunda.stdout)
    );
    assert_eq!(std::fs::read(&arquivo).expect("CSV"), csv);
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert_eq!(tabelas_do_ranking(&conn), tabelas);
    // O texto não leva tempo nem data: o tempo vai para a saída de erro.
    assert!(texto(&segunda.stderr).contains("Tempo:"));
}

#[test]
fn cli_recusa_todas_banco_inexistente_e_perfil_padrao_ausente() {
    let (pasta, db_caminho, perfis) = ambiente("recusas");
    let saida = pasta.0.join("ranking");

    let todas = sync(&pasta.0, &["ranking".into(), "todas".into()]);
    assert!(!todas.status.success());
    let erro = texto(&todas.stderr);
    assert!(erro.contains("`ranking` não aceita `todas`"), "{erro}");
    assert!(!saida.exists());

    let sem_banco = sync(
        &pasta.0,
        &args_ranking(&pasta.0.join("nao_existe.db"), &perfis, &saida),
    );
    assert!(!sem_banco.status.success());
    assert!(texto(&sem_banco.stderr).contains("banco não encontrado"));
    assert!(!pasta.0.join("nao_existe.db").exists());

    // Sem `--perfil`, o padrão é o perfil ativo da loteria (`padrao_v1`), que
    // não está na pasta de perfis do teste.
    let sem_perfil = sync(
        &pasta.0,
        &[
            "ranking".into(),
            "lotofacil".into(),
            "--db".into(),
            db_caminho.as_os_str().to_owned(),
            "--perfis".into(),
            perfis.as_os_str().to_owned(),
            "--saida".into(),
            saida.as_os_str().to_owned(),
        ],
    );
    assert!(!sem_perfil.status.success());
    assert!(texto(&sem_perfil.stderr).contains("padrao_v1"));
    assert!(!saida.exists(), "nada foi gravado");
}

#[test]
fn cli_recusa_indice_defasado_sem_gravar_csv() {
    let (pasta, db_caminho, perfis) = ambiente("defasado");
    let saida = pasta.0.join("ranking");
    {
        let mut conn = db::abrir(&db_caminho).expect("banco");
        let hist = historico(lotofacil(), N + 1);
        db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    }
    let r = sync(&pasta.0, &args_ranking(&db_caminho, &perfis, &saida));
    assert!(!r.status.success());
    let erro = texto(&r.stderr);
    assert!(erro.contains("sync features lotofacil"), "{erro}");
    assert!(!saida.exists(), "nenhum CSV");
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert!(sem_tabelas_do_ranking(&conn));
}
