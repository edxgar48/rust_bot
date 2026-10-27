//! Testes de `orquestracao::avaliar_ranking_do_banco` e de `sync ranking
//! --avaliar` (Fase 1f, tarefa 1f.4) sobre bancos em memória e em arquivo: só lê
//! o banco, o trancado fica fechado, o ponto no tempo vale no nível do banco, as
//! recusas são as do `sync ranking`/`sync avaliar` e a nota A bate com a
//! avaliação da 1c.
//!
//! A matemática (métricas, top-k, bootstrap, dobras) é da `avaliacao` e tem
//! testes próprios, inclusive os valores à mão na MINI; aqui se confere a
//! ligação com o banco.

use rusqlite::Connection;
use rust_spider::avaliacao::{self, brier, log_loss};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    avaliar_perfil, avaliar_ranking_do_banco, sincronizar_features, sincronizar_indice,
    sincronizar_ranking_com, OpcoesAvaliacao, RelatorioAvaliacaoRanking, TrechoAvaliado,
};
use rust_spider::perfil::{self, PerfilCarregado};
use rust_spider::ranking;
use rust_spider::Resultado;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const NOME: &str = "teste_v1";
const CRIADO_EM: &str = "2026-10-27T10:00:00Z";
const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Concursos do banco dos testes: 149 avaliados, dobras de 29 (a última de 33)
/// e 120 concursos nas dobras de teste, o bastante para o bootstrap.
const N: u32 = 150;

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

/// Do concurso `a_partir_de` em diante, outro sorteio: a permutação cíclica das
/// dezenas mantém cada concurso válido e muda `saiu`, as features e o índice
/// daí em diante.
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

fn carregado(texto: &str) -> PerfilCarregado {
    perfil::interpretar(texto, Path::new("perfis/lotofacil/teste_v1.toml")).expect("perfil")
}

fn perfil_teste() -> PerfilCarregado {
    carregado(PERFIL)
}

/// Grava `hist`, calcula as features e o índice do perfil de teste com `cfg`.
fn preparar(conn: &mut Connection, cfg: &LoteriaConfig, hist: &[Resultado]) {
    db::salvar_resultados(conn, hist).expect("resultados");
    sincronizar_features(conn, cfg, false).expect("features");
    sincronizar_indice(conn, cfg, &perfil_teste(), CRIADO_EM).expect("índice");
}

fn banco_com(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = db::abrir(Path::new(":memory:")).expect("banco em memória");
    preparar(&mut conn, cfg, hist);
    conn
}

fn banco(n: u32) -> Connection {
    banco_com(lotofacil(), &historico(lotofacil(), n))
}

fn avaliar_banco(conn: &mut Connection, cfg: &LoteriaConfig) -> RelatorioAvaliacaoRanking {
    avaliar_ranking_do_banco(conn, cfg, &perfil_teste()).expect("avaliação do ranking")
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
// Só lê.
// ---------------------------------------------------------------------------

#[test]
fn avaliar_so_le_o_banco_e_nao_cria_as_tabelas_do_ranking() {
    let mut conn = banco(N);
    let antes = nomes_das_tabelas(&conn);
    let contagem = |conn: &Connection, tabela: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {tabela}"), [], |r| r.get(0))
            .expect("contagem")
    };
    let linhas_antes: Vec<i64> = ["resultados", "features_dezena", "indice", "perfis"]
        .iter()
        .map(|t| contagem(&conn, t))
        .collect();

    let r = avaliar_banco(&mut conn, lotofacil());

    assert_eq!(nomes_das_tabelas(&conn), antes);
    assert!(sem_tabelas_do_ranking(&conn));
    let linhas_depois: Vec<i64> = ["resultados", "features_dezena", "indice", "perfis"]
        .iter()
        .map(|t| contagem(&conn, t))
        .collect();
    assert_eq!(linhas_depois, linhas_antes);
    // Identifica o perfil e o trecho lido.
    assert_eq!(r.perfil, NOME);
    assert_eq!((r.primeiro_concurso, r.ultimo_concurso), (1, N));
    assert_eq!(r.corte, lotofacil().ultimo_concurso_busca);
    assert_eq!(r.avaliacao.k, 15);
}

#[test]
fn avaliar_funciona_com_o_ranking_ja_gravado_e_nao_o_usa() {
    // Gravar as tabelas antes não muda a avaliação (ela recalcula em memória),
    // nem a avaliação mexe nas tabelas gravadas.
    let mut conn = banco(N);
    let sem_tabelas = avaliar_banco(&mut conn, lotofacil());
    sincronizar_ranking_com(&mut conn, lotofacil(), &perfil_teste(), 10, 1).expect("ranking");
    let dump = |conn: &Connection| -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT concurso, dezena, taxa_posicao, nota_b FROM ranking_taxa ORDER BY 1, 2",
            )
            .expect("prepara");
        let linhas = stmt
            .query_map([], |r| {
                Ok(format!(
                    "{}|{}|{}|{}",
                    r.get::<_, u32>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, f64>(2)?.to_bits(),
                    r.get::<_, f64>(3)?.to_bits()
                ))
            })
            .expect("consulta")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("linhas");
        linhas
    };
    let gravado = dump(&conn);
    let com_tabelas = avaliar_banco(&mut conn, lotofacil());
    assert_eq!(com_tabelas, sem_tabelas);
    assert_eq!(dump(&conn), gravado);
}

// ---------------------------------------------------------------------------
// A é a calibração da 1c.
// ---------------------------------------------------------------------------

#[test]
fn nota_a_gravada_da_o_log_loss_e_o_brier_do_avaliar_nas_dobras_2_a_5() {
    let cfg = lotofacil();
    let mut conn = banco(N);
    sincronizar_ranking_com(&mut conn, cfg, &perfil_teste(), 10, 1).expect("ranking");
    let r = avaliar_banco(&mut conn, cfg);

    // `sync avaliar` sobre o mesmo banco.
    let pasta = std::env::temp_dir();
    let avaliar = avaliar_perfil(
        &mut conn,
        cfg,
        &perfil_teste(),
        &OpcoesAvaliacao::default(),
        &pasta,
        "2026-10-27T10:00:00Z",
    )
    .expect("avaliar");
    let TrechoAvaliado::Busca(wf) = &avaliar.trecho else {
        panic!("esperava a busca");
    };

    // A nota A gravada (`ranking_taxa.taxa_posicao`) e o `saiu` das features.
    let features = db::ler_features_da_janela(&conn, 10).expect("features");
    let mut stmt = conn
        .prepare(
            "SELECT concurso, dezena, taxa_posicao FROM ranking_taxa ORDER BY concurso, dezena",
        )
        .expect("prepara");
    let notas: Vec<(u32, u32, f64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("consulta")
        .collect::<rusqlite::Result<_>>()
        .expect("notas");
    // Mesmas chaves: as duas séries vão do concurso 1 ao snapshot do próximo.
    assert_eq!(notas.len(), features.len());
    let acaso = avaliacao::taxa_do_acaso(cfg);

    for dobra in &r.avaliacao.dobras {
        // Por concurso da dobra: log loss e Brier médios de A (e do acaso).
        let mut por_concurso = Vec::new();
        for c in dobra.faixa.primeiro..=dobra.faixa.ultimo {
            let (mut ll, mut br) = (0.0_f64, 0.0_f64);
            let mut pares = 0.0_f64;
            for (f, (nc, nd, p)) in features
                .iter()
                .filter(|f| f.features.concurso == c)
                .zip(notas.iter().filter(|n| n.0 == c))
            {
                assert_eq!((f.features.dezena, f.features.concurso), (*nd, *nc));
                let saiu = f.features.saiu.expect("sorteado");
                ll += log_loss(*p, saiu);
                br += brier(*p, saiu);
                pares += 1.0;
            }
            assert_eq!(pares, 25.0);
            por_concurso.push((ll / pares, br / pares));
        }
        let t = por_concurso.len() as f64;
        let ll_a = por_concurso.iter().map(|x| x.0).sum::<f64>() / t;
        let br_a = por_concurso.iter().map(|x| x.1).sum::<f64>() / t;
        let da_1c = &wf.dobras[dobra.dobra as usize - 1];
        assert_eq!(da_1c.faixa, dobra.faixa, "mesma dobra {}", dobra.dobra);
        // Os mesmos bits: A é o `calibrar` da 1c.
        assert_eq!(
            ll_a.to_bits(),
            da_1c.metricas.log_loss.perfil.to_bits(),
            "log loss, dobra {}",
            dobra.dobra
        );
        assert_eq!(
            br_a.to_bits(),
            da_1c.metricas.brier.perfil.to_bits(),
            "Brier, dobra {}",
            dobra.dobra
        );
    }
    // E o acaso que as duas avaliações usam é o mesmo, o K/N do config.
    assert_eq!(acaso, 15.0 / 25.0);
    for (i, d) in r.avaliacao.a.linear.dobras.iter().enumerate() {
        assert_eq!(
            d.log_loss.acaso.to_bits(),
            wf.dobras[i + 1].metricas.log_loss.acaso.to_bits()
        );
    }
}

// ---------------------------------------------------------------------------
// Trancado fechado e ponto no tempo.
// ---------------------------------------------------------------------------

#[test]
fn nada_acima_do_corte_chega_a_avaliacao() {
    // Corte pequeno só para o teste: o trancado são os concursos 101 a 160.
    let mut cfg = *lotofacil();
    cfg.ultimo_concurso_busca = 100;
    let hist = historico(&cfg, 160);

    let mut longo = banco_com(&cfg, &hist);
    let mut outro_trancado = banco_com(&cfg, &com_sorteios_trocados(&hist, 101));
    let mut sem_trancado = banco_com(&cfg, &hist[..100]);

    let a = avaliar_banco(&mut longo, &cfg);
    let b = avaliar_banco(&mut outro_trancado, &cfg);
    let c = avaliar_banco(&mut sem_trancado, &cfg);

    assert_eq!(a.corte, 100);
    assert_eq!(a.avaliacao.faixa.ultimo, 100, "o último avaliado é o corte");
    // O trancado trocado por outro sorteio não muda nada; o banco que nem
    // chegou a ter o trancado dá a mesma avaliação.
    assert_eq!(a, b);
    assert_eq!(a.avaliacao, c.avaliacao);
    assert_eq!((a.ultimo_concurso, c.ultimo_concurso), (160, 100));
    // E o corte mexe de fato: com o corte padrão, outro trecho.
    let mut padrao = banco_com(lotofacil(), &hist);
    assert_ne!(
        avaliar_banco(&mut padrao, lotofacil()).avaliacao,
        a.avaliacao
    );
}

#[test]
fn dobra_1_fica_fora_e_o_trecho_comeca_na_dobra_2() {
    let mut conn = banco(N);
    let r = avaliar_banco(&mut conn, lotofacil()).avaliacao;
    // 149 concursos avaliados (2 a 150), dobras de 29 (a última de 33).
    let numeros: Vec<u32> = r.dobras.iter().map(|d| d.dobra).collect();
    assert_eq!(numeros, [2, 3, 4, 5]);
    assert_eq!((r.faixa.primeiro, r.faixa.ultimo), (31, 150));
    assert_eq!(r.concursos, 120);
    assert_eq!(r.b.posicao.total.primeiro_concurso, 31);
    assert_eq!(r.a.topk.total.primeiro_concurso, 31);
    assert!(r.b.bootstrap.is_some());
}

#[test]
fn mudar_o_ultimo_concurso_nao_mexe_nas_dobras_anteriores() {
    let cfg = lotofacil();
    let hist = historico(cfg, N);
    let mut a = banco_com(cfg, &hist);
    let mut b = banco_com(cfg, &com_sorteios_trocados(&hist, N));
    let (ra, rb) = (
        avaliar_banco(&mut a, cfg).avaliacao,
        avaliar_banco(&mut b, cfg).avaliacao,
    );
    // As dobras 2, 3 e 4 acabam no concurso 117: nada do que vem depois chega.
    assert_eq!(ra.a.topk.dobras[..3], rb.a.topk.dobras[..3]);
    assert_eq!(ra.b.topk.dobras[..3], rb.b.topk.dobras[..3]);
    assert_eq!(ra.a.linear.dobras[..3], rb.a.linear.dobras[..3]);
    assert_eq!(ra.b.posicao.dobras[..3], rb.b.posicao.dobras[..3]);
    assert_eq!(ra.b.linear.dobras[..3], rb.b.linear.dobras[..3]);
    // A última dobra tem o concurso 150 e muda.
    assert_ne!(ra.b.posicao.dobras[3], rb.b.posicao.dobras[3]);
    assert_ne!(ra.a.linear.dobras[3], rb.a.linear.dobras[3]);
}

#[test]
fn duas_execucoes_dao_o_mesmo_resultado_e_o_mesmo_texto() {
    let cfg = lotofacil();
    let mut a = banco(N);
    let mut b = banco(N);
    let (ra, rb) = (avaliar_banco(&mut a, cfg), avaliar_banco(&mut b, cfg));
    assert_eq!(ra, rb);
    assert_eq!(avaliar_banco(&mut a, cfg), ra);
    assert_eq!(
        ranking::formatar_avaliacao(cfg, &ra),
        ranking::formatar_avaliacao(cfg, &rb)
    );
}

// ---------------------------------------------------------------------------
// Recusas: as mesmas do `sync ranking`.
// ---------------------------------------------------------------------------

#[test]
fn recusas_nao_criam_nada() {
    let cfg = lotofacil();
    let hist = historico(cfg, N + 1);

    // Sem resultados.
    let mut vazio = db::abrir(Path::new(":memory:")).expect("banco");
    assert!(matches!(
        avaliar_ranking_do_banco(&mut vazio, cfg, &perfil_teste()),
        Err(SpiderError::SemResultados(_))
    ));
    assert_eq!(nomes_das_tabelas(&vazio), ["resultados"]);

    // Resultados e features, perfil nunca registrado.
    db::salvar_resultados(&mut vazio, &hist[..N as usize]).expect("resultados");
    sincronizar_features(&mut vazio, cfg, false).expect("features");
    assert!(matches!(
        avaliar_ranking_do_banco(&mut vazio, cfg, &perfil_teste()),
        Err(SpiderError::PerfilNaoRegistrado { .. })
    ));
    assert!(sem_tabelas_do_ranking(&vazio));

    // Um concurso novo sem `sync features`.
    let mut conn = banco(N);
    db::salvar_resultados(&mut conn, &hist[N as usize..]).expect("concurso novo");
    assert!(matches!(
        avaliar_ranking_do_banco(&mut conn, cfg, &perfil_teste()),
        Err(SpiderError::FeaturesDesatualizadas { .. })
    ));
    // Features em dia, índice defasado.
    sincronizar_features(&mut conn, cfg, false).expect("features em dia");
    assert!(matches!(
        avaliar_ranking_do_banco(&mut conn, cfg, &perfil_teste()),
        Err(SpiderError::IndiceDesatualizado { .. })
    ));

    // Perfil editado sob o mesmo nome (hash diferente) e de outra loteria.
    let mut em_dia = banco(N);
    let editado = carregado(&PERFIL.replace("frequencia = 1.0", "frequencia = 2.0"));
    assert!(matches!(
        avaliar_ranking_do_banco(&mut em_dia, cfg, &editado),
        Err(SpiderError::PerfilAlterado { .. })
    ));
    let de_quina = carregado(&PERFIL.replace("lotofacil", "quina"));
    assert!(matches!(
        avaliar_ranking_do_banco(&mut em_dia, cfg, &de_quina),
        Err(SpiderError::PerfilDeOutraLoteria { .. })
    ));
    assert!(sem_tabelas_do_ranking(&em_dia));
}

#[test]
fn historico_curto_demais_para_as_dobras_e_recusado() {
    // 5 concursos avaliados são o mínimo (uma dobra por concurso); 4 não.
    let mut curto = banco(5);
    assert!(matches!(
        avaliar_ranking_do_banco(&mut curto, lotofacil(), &perfil_teste()),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    let mut ok = banco(6);
    assert!(avaliar_ranking_do_banco(&mut ok, lotofacil(), &perfil_teste()).is_ok());
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
            "rust_spider_ranking_avaliar_{rotulo}_{}_{unico}_{n}",
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

/// O `sync` roda dentro da pasta temporária: a trava (`sync.lock`) e o log
/// (`logs/`) são relativos ao diretório de trabalho.
fn sync(dir: &Path, args: &[OsString]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn args_avaliar(db_caminho: &Path, perfis: &Path) -> Vec<OsString> {
    vec![
        "ranking".into(),
        "lotofacil".into(),
        "--avaliar".into(),
        "--perfil".into(),
        NOME.into(),
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
fn cli_avalia_sem_gravar_nada_e_repete_a_mesma_saida() {
    let (pasta, db_caminho, perfis) = ambiente("cli");
    let args = args_avaliar(&db_caminho, &perfis);
    let banco_antes = std::fs::read(&db_caminho).expect("banco");

    let primeira = sync(&pasta.0, &args);
    assert!(primeira.status.success(), "{}", texto(&primeira.stderr));
    let stdout = texto(&primeira.stdout);
    for trecho in [
        "Avaliação do ranking por taxa histórica da Lotofácil",
        "AVISO: isto é a descrição do passado, não uma previsão",
        "dobras de teste 2 a 5",
        "Corte da busca: concurso 3416",
        "== Ordem de A (principal)",
        "== Ordem de B (EXPERIMENTAL)",
        "Veredito de B no trecho inteiro",
        "Bootstrap circular por blocos de 50 concursos",
    ] {
        assert!(stdout.contains(trecho), "falta `{trecho}` em:\n{stdout}");
    }
    assert!(texto(&primeira.stderr).contains("Tempo:"));

    // O texto é o da biblioteca sobre o mesmo banco.
    let mut conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    let r = avaliar_ranking_do_banco(&mut conn, lotofacil(), &perfil_teste()).expect("avaliação");
    assert_eq!(stdout, ranking::formatar_avaliacao(lotofacil(), &r));
    drop(conn);

    // Nada foi gravado: nem tabelas, nem CSV, nem a pasta de saída, nem
    // registro do trancado; o arquivo do banco não mudou.
    assert_eq!(std::fs::read(&db_caminho).expect("banco"), banco_antes);
    assert!(!pasta.0.join("ranking").exists());
    assert!(!pasta.0.join("registros").exists());
    let conn = db::abrir_somente_leitura(&db_caminho).expect("banco");
    assert!(sem_tabelas_do_ranking(&conn));
    drop(conn);

    // Segunda execução: a mesma saída, byte a byte.
    let segunda = sync(&pasta.0, &args);
    assert!(segunda.status.success(), "{}", texto(&segunda.stderr));
    assert_eq!(segunda.stdout, primeira.stdout);
}

#[test]
fn cli_recusa_saida_junto_e_banco_inexistente_e_perfil_ausente() {
    let (pasta, db_caminho, perfis) = ambiente("recusas");

    // `--saida` não faz sentido: nada é gravado.
    let mut com_saida = args_avaliar(&db_caminho, &perfis);
    com_saida.extend(["--saida".into(), pasta.0.join("x").into()]);
    let r = sync(&pasta.0, &com_saida);
    assert!(!r.status.success());
    assert!(!pasta.0.join("x").exists());

    let sem_banco = sync(
        &pasta.0,
        &args_avaliar(&pasta.0.join("nao_existe.db"), &perfis),
    );
    assert!(!sem_banco.status.success());
    assert!(texto(&sem_banco.stderr).contains("banco não encontrado"));
    assert!(!pasta.0.join("nao_existe.db").exists());

    // Sem `--perfil`, o padrão é o perfil ativo (`padrao_v1`), ausente da pasta.
    let sem_perfil = sync(
        &pasta.0,
        &[
            "ranking".into(),
            "lotofacil".into(),
            "--avaliar".into(),
            "--db".into(),
            db_caminho.as_os_str().to_owned(),
            "--perfis".into(),
            perfis.as_os_str().to_owned(),
        ],
    );
    assert!(!sem_perfil.status.success());
    assert!(texto(&sem_perfil.stderr).contains("padrao_v1"));

    // `todas` continua recusada.
    let todas = sync(
        &pasta.0,
        &["ranking".into(), "todas".into(), "--avaliar".into()],
    );
    assert!(!todas.status.success());
    assert!(texto(&todas.stderr).contains("`ranking` não aceita `todas`"));
}
