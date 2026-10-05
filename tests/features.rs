//! Testes de `orquestracao::sincronizar_features` (comando `sync features`)
//! sobre bancos em memória: gravação em `features_dezena` e `motor_estado`,
//! retomada incremental, regeneração e recusa.

use rusqlite::types::Value;
use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db::{self, MotorEstadoSalvo};
use rust_spider::error::SpiderError;
use rust_spider::motor::{
    self, EstadoMotor, FeaturesDezena, FEATURE_VERSION, JANELAS, JANELA_TOTAL,
};
use rust_spider::orquestracao::{
    impressao_resultados, sincronizar_features, ModoFeatures, MotivoRegeneracao, ResumoFeatures,
};
use rust_spider::validacao::Problema;
use rust_spider::Resultado;
use std::path::Path;

fn quina() -> &'static LoteriaConfig {
    config::buscar("quina").expect("quina configurada")
}

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: format!("2020-01-{:02}", 1 + concurso % 28),
        dezenas: dezenas.to_vec(),
    }
}

/// Histórico sintético determinístico (xorshift64), sem dependência nova.
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

fn banco() -> Connection {
    db::abrir(Path::new(":memory:")).expect("banco em memória")
}

fn gravar(conn: &mut Connection, resultados: &[Resultado]) {
    db::salvar_resultados(conn, resultados).expect("resultados gravados");
}

fn tabela(conn: &Connection) -> Vec<db::LinhaFeatures> {
    db::ler_features(conn).expect("features lidas")
}

#[test]
fn completo_igual_ao_incremental_em_lotes() {
    let cfg = quina();
    let n = 130;
    let hist = historico(cfg, n, 0x5eed_1a2b);

    // Referência: tudo de uma vez.
    let mut completo = banco();
    gravar(&mut completo, &hist);
    let resumo = sincronizar_features(&mut completo, cfg, false).expect("completo");
    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::SemEstado)
    );
    let referencia = tabela(&completo);
    let por_concurso = cfg.qtd_dezenas_possiveis() as usize * JANELAS.len();
    assert_eq!(referencia.len(), (n as usize + 1) * por_concurso);
    assert_eq!(resumo.linhas_gravadas, referencia.len());

    // Mesmas linhas que o motor puro produz (+ snapshot do próximo).
    let (estado, mut esperadas) = motor::calcular(cfg, &hist).expect("motor");
    esperadas.extend(estado.snapshot());
    let lidas: Vec<FeaturesDezena> = referencia.iter().map(|l| l.features.clone()).collect();
    assert_eq!(lidas, esperadas);
    assert!(referencia
        .iter()
        .all(|l| l.feature_version == FEATURE_VERSION));

    // Incremental: lotes de tamanhos variados, com execuções sem nada novo.
    let mut incremental = banco();
    let mut feito = 0;
    for corte in [1, 1, 2, 10, 10, 47, 99, 100, 101, 130, 130] {
        gravar(&mut incremental, &hist[feito..corte]);
        let resumo = sincronizar_features(&mut incremental, cfg, false).expect("incremental");
        if feito > 0 {
            assert_eq!(resumo.modo, ModoFeatures::Incremental);
        }
        assert_eq!(resumo.concursos_incorporados, corte - feito);
        assert_eq!(resumo.ultimo_concurso, corte as u32);
        assert_eq!(resumo.proximo_concurso, corte as u32 + 1);
        feito = corte;
    }
    assert_eq!(tabela(&incremental), referencia);
}

// ---------------------------------------------------------------------------
// Auxiliares dos testes de 1a.2.
// ---------------------------------------------------------------------------

fn por_concurso(cfg: &LoteriaConfig) -> usize {
    cfg.qtd_dezenas_possiveis() as usize * JANELAS.len()
}

fn estado_salvo(conn: &Connection) -> MotorEstadoSalvo {
    db::ler_motor_estado(conn)
        .expect("motor_estado lido")
        .expect("motor_estado gravado")
}

/// Banco novo com `hist` gravado e as features calculadas de uma vez.
fn calculado(cfg: &LoteriaConfig, hist: &[Resultado]) -> Connection {
    let mut conn = banco();
    gravar(&mut conn, hist);
    sincronizar_features(&mut conn, cfg, false).expect("features calculadas");
    conn
}

/// Tabela e `motor_estado` de um banco novo calculado do zero com `hist`.
fn referencia(
    cfg: &LoteriaConfig,
    hist: &[Resultado],
) -> (Vec<db::LinhaFeatures>, MotorEstadoSalvo) {
    let conn = calculado(cfg, hist);
    (tabela(&conn), estado_salvo(&conn))
}

/// Quantas tabelas derivadas existem no banco (`features_dezena`, `motor_estado`).
fn tabelas_derivadas(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE name IN ('features_dezena', 'motor_estado')",
        [],
        |row| row.get(0),
    )
    .expect("sqlite_master lida")
}

/// Mesmo concurso com outro conjunto de dezenas: cada dezena vira a seguinte
/// da faixa (a última volta à primeira). Como a quantidade sorteada é menor
/// que a faixa, o conjunto nunca coincide com o original.
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

fn incremental_igual_ao_completo(slug: &str, n: usize, cortes: &[usize], semente: u64) {
    let cfg = config::buscar(slug).expect("loteria configurada");
    let hist = historico(cfg, n as u32, semente);
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(ref_tabela.len(), (n + 1) * por_concurso(cfg));

    let mut conn = banco();
    let mut feito = 0;
    for &corte in cortes {
        gravar(&mut conn, &hist[feito..corte]);
        let resumo = sincronizar_features(&mut conn, cfg, false).expect("incremental");
        assert_eq!(resumo.concursos_incorporados, corte - feito);
        assert_eq!(resumo.ultimo_concurso, corte as u32);
        feito = corte;
    }
    assert_eq!(feito, n);
    assert_eq!(tabela(&conn), ref_tabela, "{slug}: tabela difere");
    assert_eq!(estado_salvo(&conn), ref_estado, "{slug}: estado difere");
}

// ---------------------------------------------------------------------------
// 1. Incremental = completo nos extremos (k = 0 e k = n) e nas três loterias.
// ---------------------------------------------------------------------------

#[test]
fn rodar_de_novo_sem_nada_novo_nao_grava_nem_incorpora() {
    let cfg = quina();
    let hist = historico(cfg, 40, 11);
    let mut conn = calculado(cfg, &hist);
    let tabela_antes = tabela(&conn);
    let estado_antes = estado_salvo(&conn);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("segunda execução");

    assert_eq!(resumo.modo, ModoFeatures::Incremental);
    assert_eq!(resumo.concursos_incorporados, 0);
    assert_eq!(resumo.linhas_gravadas, 0);
    assert_eq!(resumo.ultimo_concurso, 40);
    assert_eq!(resumo.proximo_concurso, 41);
    assert_eq!(tabela(&conn), tabela_antes);
    assert_eq!(estado_salvo(&conn), estado_antes);
}

#[test]
fn banco_vazio_recusa_com_sem_resultados_e_nao_cria_tabelas_derivadas() {
    let cfg = quina();
    let mut conn = banco();

    let erro = sincronizar_features(&mut conn, cfg, false).expect_err("banco vazio");

    assert!(
        matches!(&erro, SpiderError::SemResultados(slug) if slug == "quina"),
        "erro inesperado: {erro:?}"
    );
    assert_eq!(tabelas_derivadas(&conn), 0);
}

#[test]
fn banco_vazio_recusa_tambem_com_forcar_completo() {
    let cfg = quina();
    let mut conn = banco();

    let erro = sincronizar_features(&mut conn, cfg, true).expect_err("banco vazio");

    assert!(matches!(erro, SpiderError::SemResultados(_)));
    assert_eq!(tabelas_derivadas(&conn), 0);
}

#[test]
fn recusa_por_banco_vazio_e_depois_gravar_tudo_igual_ao_completo_direto() {
    let cfg = quina();
    let hist = historico(cfg, 60, 12);
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);

    let mut conn = banco();
    assert!(sincronizar_features(&mut conn, cfg, false).is_err());
    gravar(&mut conn, &hist);
    let resumo = sincronizar_features(&mut conn, cfg, false).expect("após gravar");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::SemEstado)
    );
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

#[test]
fn incremental_igual_ao_completo_na_quina_com_lotes_variados() {
    incremental_igual_ao_completo("quina", 70, &[1, 2, 2, 11, 40, 69, 70, 70], 21);
}

#[test]
fn incremental_igual_ao_completo_na_lotofacil_com_lotes_variados() {
    incremental_igual_ao_completo("lotofacil", 60, &[1, 5, 5, 6, 30, 59, 60], 22);
}

#[test]
fn incremental_igual_ao_completo_na_megasena_com_lotes_variados() {
    incremental_igual_ao_completo("megasena", 40, &[1, 3, 3, 20, 39, 40], 23);
}

#[test]
fn incremental_em_um_unico_lote_igual_ao_completo_nas_tres_loterias() {
    for slug in ["quina", "lotofacil", "megasena"] {
        incremental_igual_ao_completo(slug, 25, &[25], 24);
    }
}

// ---------------------------------------------------------------------------
// 2. Determinismo.
// ---------------------------------------------------------------------------

#[test]
fn forcar_completo_regenera_com_tabela_e_estado_identicos() {
    let cfg = quina();
    let hist = historico(cfg, 50, 31);
    let mut conn = calculado(cfg, &hist);
    let tabela_antes = tabela(&conn);
    let estado_antes = estado_salvo(&conn);

    let resumo = sincronizar_features(&mut conn, cfg, true).expect("forçado");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::Forcada)
    );
    assert_eq!(resumo.concursos_incorporados, 50);
    assert_eq!(tabela(&conn), tabela_antes);
    assert_eq!(estado_salvo(&conn), estado_antes);
}

#[test]
fn dois_bancos_independentes_com_o_mesmo_historico_dao_tabelas_identicas() {
    let cfg = quina();
    let hist = historico(cfg, 50, 32);

    let a = calculado(cfg, &hist);
    let b = calculado(cfg, &hist);

    assert_eq!(tabela(&a), tabela(&b));
    assert_eq!(estado_salvo(&a), estado_salvo(&b));
}

// ---------------------------------------------------------------------------
// 3. Snapshot do próximo concurso.
// ---------------------------------------------------------------------------

#[test]
fn snapshot_do_proximo_concurso_tem_saiu_nulo_e_so_ele() {
    let cfg = quina();
    let m = 40;
    let hist = historico(cfg, m, 41);
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("calculado");

    assert_eq!(resumo.proximo_concurso, m + 1);
    let proximo = db::ler_features_do_concurso(&conn, m + 1).expect("lido");
    assert_eq!(proximo.len(), por_concurso(cfg));
    assert!(proximo.iter().all(|l| l.features.saiu.is_none()));
    let sem_alvo: Vec<u32> = tabela(&conn)
        .iter()
        .filter(|l| l.features.saiu.is_none())
        .map(|l| l.features.concurso)
        .collect();
    assert_eq!(sem_alvo.len(), por_concurso(cfg));
    assert!(sem_alvo.iter().all(|&c| c == m + 1));
}

#[test]
fn snapshot_vira_alvo_real_quando_o_concurso_e_gravado() {
    let cfg = quina();
    let m = 40;
    let hist = historico(cfg, m + 1, 42);
    let mut conn = banco();
    gravar(&mut conn, &hist[..m as usize]);
    sincronizar_features(&mut conn, cfg, false).expect("até m");
    let antes = db::ler_features_do_concurso(&conn, m + 1).expect("snapshot");

    gravar(&mut conn, &hist[m as usize..]);
    let resumo = sincronizar_features(&mut conn, cfg, false).expect("m + 1");

    assert_eq!(resumo.modo, ModoFeatures::Incremental);
    let sorteadas = &hist[m as usize].dezenas;
    let depois = db::ler_features_do_concurso(&conn, m + 1).expect("lido");
    assert_eq!(depois.len(), por_concurso(cfg));
    for l in &depois {
        // O alvo é exatamente "a dezena está entre as sorteadas do concurso".
        assert_eq!(
            l.features.saiu,
            Some(sorteadas.contains(&l.features.dezena)),
            "dezena {} janela {}",
            l.features.dezena,
            l.features.janela
        );
    }
    // Cada dezena sorteada aparece com saiu = true em todas as janelas.
    assert_eq!(
        depois
            .iter()
            .filter(|l| l.features.saiu == Some(true))
            .count(),
        cfg.qtd_dezenas * JANELAS.len()
    );
    // Surge o snapshot de m + 2 e só ele fica sem alvo.
    let novo_proximo = db::ler_features_do_concurso(&conn, m + 2).expect("lido");
    assert_eq!(novo_proximo.len(), por_concurso(cfg));
    assert!(novo_proximo.iter().all(|l| l.features.saiu.is_none()));
    assert_eq!(
        tabela(&conn)
            .iter()
            .filter(|l| l.features.saiu.is_none())
            .count(),
        por_concurso(cfg)
    );
    // As features de m + 1 (fora o alvo) não mudam entre as duas execuções.
    let sem_alvo = |linhas: &[db::LinhaFeatures]| -> Vec<FeaturesDezena> {
        linhas
            .iter()
            .map(|l| FeaturesDezena {
                saiu: None,
                ..l.features.clone()
            })
            .collect()
    };
    assert_eq!(sem_alvo(&antes), sem_alvo(&depois));
}

// ---------------------------------------------------------------------------
// 4. Resultado corrigido abaixo do último (ou nele) -> regeneração.
// ---------------------------------------------------------------------------

#[test]
fn dezenas_corrigidas_abaixo_do_ultimo_regeneram_e_igualam_a_referencia() {
    let cfg = quina();
    let mut hist = historico(cfg, 60, 51);
    let mut conn = calculado(cfg, &hist);
    let j = 20;
    hist[j - 1] = com_dezenas_trocadas(cfg, &hist[j - 1]);
    gravar(&mut conn, &hist[j - 1..j]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("após correção");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::ResultadosAlterados)
    );
    assert_eq!(resumo.concursos_incorporados, 60);
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

#[test]
fn so_a_data_corrigida_tambem_regenera() {
    let cfg = quina();
    let mut hist = historico(cfg, 60, 52);
    let mut conn = calculado(cfg, &hist);
    // Data que nunca coincide com as geradas por `res` (todas em 2020).
    hist[29].data_sorteio = "1999-01-01".to_string();
    gravar(&mut conn, &hist[29..30]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("após correção");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::ResultadosAlterados)
    );
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

#[test]
fn correcao_junto_com_concursos_novos_regenera_e_iguala_a_referencia() {
    let cfg = quina();
    let mut hist = historico(cfg, 70, 53);
    let mut conn = calculado(cfg, &hist[..50]);
    hist[19] = com_dezenas_trocadas(cfg, &hist[19]);
    gravar(&mut conn, &hist[19..20]);
    gravar(&mut conn, &hist[50..]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("correção e novos");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::ResultadosAlterados)
    );
    assert_eq!(resumo.concursos_incorporados, 70);
    assert_eq!(resumo.ultimo_concurso, 70);
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

#[test]
fn correcao_do_proprio_ultimo_concurso_regenera() {
    let cfg = quina();
    let mut hist = historico(cfg, 60, 54);
    let mut conn = calculado(cfg, &hist);
    hist[59] = com_dezenas_trocadas(cfg, &hist[59]);
    gravar(&mut conn, &hist[59..]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("após correção");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::ResultadosAlterados)
    );
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

#[test]
fn correcao_so_nos_concursos_novos_continua_incremental() {
    // Contraprova: mexer em concurso ainda não incorporado não é "alterado".
    let cfg = quina();
    let hist = historico(cfg, 60, 55);
    let mut conn = calculado(cfg, &hist[..50]);
    let mut corrigido = hist.clone();
    corrigido[52] = com_dezenas_trocadas(cfg, &corrigido[52]);
    gravar(&mut conn, &corrigido[50..]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("incremental");

    assert_eq!(resumo.modo, ModoFeatures::Incremental);
    assert_eq!(resumo.concursos_incorporados, 10);
    let (ref_tabela, _) = referencia(cfg, &corrigido);
    assert_eq!(tabela(&conn), ref_tabela);
}

// ---------------------------------------------------------------------------
// 5. Estado ou tabela inutilizáveis -> regeneração.
// ---------------------------------------------------------------------------

/// Calcula 40 concursos da Quina, executa `sql` para adulterar o banco e
/// sincroniza de novo. Confere que a tabela e o `motor_estado` finais são os
/// de um banco calculado do zero e devolve o resumo da segunda execução.
fn regenerado_apos(sql: &str) -> ResumoFeatures {
    let cfg = quina();
    let hist = historico(cfg, 40, 61);
    let mut conn = calculado(cfg, &hist);
    conn.execute_batch(sql).expect("adulteração aplicada");

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("sincronização");

    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
    resumo
}

#[test]
fn versao_das_formulas_diferente_regenera_com_a_versao_atual() {
    let sql = format!(
        "UPDATE motor_estado SET feature_version = {}",
        FEATURE_VERSION + 1
    );

    let resumo = regenerado_apos(&sql);

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::VersaoDiferente {
            salva: FEATURE_VERSION + 1,
            atual: FEATURE_VERSION
        })
    );
    assert_eq!(resumo.concursos_incorporados, 40);
}

#[test]
fn estado_com_json_invalido_regenera_como_ilegivel() {
    let resumo = regenerado_apos("UPDATE motor_estado SET estado = 'isto não é json'");

    assert!(
        matches!(
            resumo.modo,
            ModoFeatures::Completo(MotivoRegeneracao::EstadoIlegivel(_))
        ),
        "modo: {:?}",
        resumo.modo
    );
}

#[test]
fn estado_com_json_de_outro_formato_regenera_como_ilegivel() {
    let resumo = regenerado_apos("UPDATE motor_estado SET estado = '{\"campo\": 1}'");

    assert!(
        matches!(
            resumo.modo,
            ModoFeatures::Completo(MotivoRegeneracao::EstadoIlegivel(_))
        ),
        "modo: {:?}",
        resumo.modo
    );
}

#[test]
fn ultimo_concurso_da_linha_divergente_do_json_regenera_como_incompativel() {
    let resumo = regenerado_apos("UPDATE motor_estado SET ultimo_concurso = ultimo_concurso - 1");

    assert!(
        matches!(
            resumo.modo,
            ModoFeatures::Completo(MotivoRegeneracao::EstadoIncompativel(_))
        ),
        "modo: {:?}",
        resumo.modo
    );
}

#[test]
fn loteria_da_linha_diferente_regenera_como_incompativel() {
    let resumo = regenerado_apos("UPDATE motor_estado SET loteria = 'megasena'");

    assert!(
        matches!(
            resumo.modo,
            ModoFeatures::Completo(MotivoRegeneracao::EstadoIncompativel(_))
        ),
        "modo: {:?}",
        resumo.modo
    );
}

#[test]
fn linhas_apagadas_de_features_regeneram_como_incoerentes() {
    // 9 dezenas (1..=9) x 4 janelas = 36 linhas removidas do concurso 5.
    let resumo = regenerado_apos("DELETE FROM features_dezena WHERE concurso = 5 AND dezena < 10");

    let por = por_concurso(quina()) as u64;
    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::FeaturesIncoerentes {
            linhas: 41 * por - 36,
            esperadas: 41 * por,
            maior_concurso: Some(41),
        })
    );
}

#[test]
fn snapshot_do_proximo_concurso_apagado_regenera_como_incoerente() {
    let resumo = regenerado_apos("DELETE FROM features_dezena WHERE concurso = 41");

    let por = por_concurso(quina()) as u64;
    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::FeaturesIncoerentes {
            linhas: 40 * por,
            esperadas: 41 * por,
            maior_concurso: Some(40),
        })
    );
}

#[test]
fn features_totalmente_apagadas_regeneram_como_incoerentes() {
    let resumo = regenerado_apos("DELETE FROM features_dezena");

    let por = por_concurso(quina()) as u64;
    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::FeaturesIncoerentes {
            linhas: 0,
            esperadas: 41 * por,
            maior_concurso: None,
        })
    );
}

#[test]
fn linha_extra_em_features_regenera_como_incoerente() {
    // Linha a mais (concurso 50 não existe) também quebra a contagem.
    let resumo = regenerado_apos(
        "INSERT INTO features_dezena (
             concurso, dezena, janela, frequencia, repeticoes_total,
             repeticao_atual, feature_version
         ) VALUES (50, 1, 0, 0, 0, 0, 1)",
    );

    assert!(
        matches!(
            resumo.modo,
            ModoFeatures::Completo(MotivoRegeneracao::FeaturesIncoerentes { .. })
        ),
        "modo: {:?}",
        resumo.modo
    );
}

#[test]
fn motor_estado_apagado_regenera_como_sem_estado() {
    let resumo = regenerado_apos("DELETE FROM motor_estado");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::SemEstado)
    );
}

// ---------------------------------------------------------------------------
// 6. Histórico reprovado no `validar` -> recusa sem gravar.
// ---------------------------------------------------------------------------

#[test]
fn buraco_na_numeracao_recusa_e_nao_cria_tabelas_derivadas() {
    let cfg = quina();
    let hist = historico(cfg, 12, 71);
    let mut conn = banco();
    gravar(&mut conn, &hist[..10]);
    gravar(&mut conn, &hist[11..]); // pula o concurso 11

    let erro = sincronizar_features(&mut conn, cfg, false).expect_err("buraco");

    match erro {
        SpiderError::ResultadosReprovados { loteria, erros, .. } => {
            assert_eq!(loteria, "quina");
            assert!(
                erros.contains(&Problema::ConcursosFaltando {
                    inicio: 11,
                    fim: 11
                }),
                "erros: {erros:?}"
            );
        }
        outro => panic!("erro inesperado: {outro:?}"),
    }
    assert_eq!(tabelas_derivadas(&conn), 0);
}

#[test]
fn buraco_com_forcar_completo_tambem_recusa_sem_criar_tabelas() {
    let cfg = quina();
    let hist = historico(cfg, 12, 72);
    let mut conn = banco();
    gravar(&mut conn, &hist[..10]);
    gravar(&mut conn, &hist[11..]);

    let erro = sincronizar_features(&mut conn, cfg, true).expect_err("buraco");

    assert!(matches!(erro, SpiderError::ResultadosReprovados { .. }));
    assert_eq!(tabelas_derivadas(&conn), 0);
}

#[test]
fn buraco_depois_de_calculado_recusa_e_preserva_features_e_estado() {
    let cfg = quina();
    let m = 30;
    let hist = historico(cfg, m + 2, 73);
    let mut conn = calculado(cfg, &hist[..m as usize]);
    let tabela_antes = tabela(&conn);
    let estado_antes = estado_salvo(&conn);
    gravar(&mut conn, &hist[m as usize + 1..]); // grava m + 2, pulando m + 1

    let erro = sincronizar_features(&mut conn, cfg, false).expect_err("buraco");

    match erro {
        SpiderError::ResultadosReprovados { erros, .. } => assert!(
            erros.contains(&Problema::ConcursosFaltando {
                inicio: m + 1,
                fim: m + 1
            }),
            "erros: {erros:?}"
        ),
        outro => panic!("erro inesperado: {outro:?}"),
    }
    assert_eq!(tabela(&conn), tabela_antes);
    assert_eq!(estado_salvo(&conn), estado_antes);
}

#[test]
fn dezena_fora_da_faixa_recusa_e_nao_cria_tabelas_derivadas() {
    let cfg = quina();
    let mut hist = historico(cfg, 10, 74);
    hist[4].dezenas[0] = cfg.dezena_max + 1;
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let erro = sincronizar_features(&mut conn, cfg, false).expect_err("fora da faixa");

    match erro {
        SpiderError::ResultadosReprovados { erros, .. } => {
            assert!(
                erros.contains(&Problema::DezenaForaDaFaixa {
                    concurso: 5,
                    dezena: cfg.dezena_max + 1,
                    min: cfg.dezena_min,
                    max: cfg.dezena_max
                }),
                "erros: {erros:?}"
            );
        }
        outro => panic!("erro inesperado: {outro:?}"),
    }
    assert_eq!(tabelas_derivadas(&conn), 0);
}

#[test]
fn dezena_fora_da_faixa_depois_de_calculado_preserva_features_e_estado() {
    let cfg = quina();
    let mut hist = historico(cfg, 30, 75);
    let mut conn = calculado(cfg, &hist);
    let tabela_antes = tabela(&conn);
    let estado_antes = estado_salvo(&conn);
    hist[9].dezenas[0] = 0; // zero está fora da faixa 1..=80
    gravar(&mut conn, &hist[9..10]);

    let erro = sincronizar_features(&mut conn, cfg, false).expect_err("fora da faixa");

    assert!(matches!(erro, SpiderError::ResultadosReprovados { .. }));
    assert_eq!(tabela(&conn), tabela_antes);
    assert_eq!(estado_salvo(&conn), estado_antes);
}

#[test]
fn data_fora_de_ordem_e_so_aviso_e_nao_bloqueia() {
    let cfg = quina();
    let mut hist = historico(cfg, 10, 76);
    // Datas crescentes, exceto o concurso 6, anterior à do concurso 5.
    for r in &mut hist {
        r.data_sorteio = format!("2020-02-{:02}", r.concurso + 1);
    }
    hist[5].data_sorteio = "2020-01-01".to_string();
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("aviso não bloqueia");

    assert!(
        resumo
            .avisos
            .iter()
            .any(|p| matches!(p, Problema::DataForaDeOrdem { concurso: 6, .. })),
        "avisos: {:?}",
        resumo.avisos
    );
    assert_eq!(resumo.concursos_incorporados, 10);
    assert_eq!(tabela(&conn).len(), 11 * por_concurso(cfg));
}

#[test]
fn historico_sem_avisos_devolve_lista_de_avisos_vazia() {
    let cfg = quina();
    let mut hist = historico(cfg, 10, 77);
    for r in &mut hist {
        r.data_sorteio = format!("2020-02-{:02}", r.concurso + 1);
    }
    let mut conn = banco();
    gravar(&mut conn, &hist);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("calculado");

    assert!(resumo.avisos.is_empty(), "avisos: {:?}", resumo.avisos);
}

// ---------------------------------------------------------------------------
// 7. Atomicidade: falha no meio não deixa nada pela metade.
// ---------------------------------------------------------------------------

/// Calcula 30 concursos, grava mais 30, cria o gatilho `gatilho` e tenta
/// sincronizar. Deve falhar sem alterar `features_dezena` nem `motor_estado`;
/// removido o gatilho, a sincronização seguinte conclui e iguala a referência.
fn falha_simulada_nao_altera_nada(gatilho: &str, forcar_completo: bool) {
    let cfg = quina();
    let hist = historico(cfg, 60, 81);
    let mut conn = calculado(cfg, &hist[..30]);
    let tabela_antes = tabela(&conn);
    let estado_antes = estado_salvo(&conn);
    gravar(&mut conn, &hist[30..]);
    conn.execute_batch(gatilho).expect("gatilho criado");

    let erro = sincronizar_features(&mut conn, cfg, forcar_completo).expect_err("falha simulada");

    assert!(
        matches!(&erro, SpiderError::Db(e) if e.to_string().contains("falha simulada")),
        "erro inesperado: {erro:?}"
    );
    assert_eq!(tabela(&conn), tabela_antes, "features mudaram");
    assert_eq!(estado_salvo(&conn), estado_antes, "estado mudou");

    conn.execute_batch("DROP TRIGGER falha")
        .expect("gatilho removido");
    sincronizar_features(&mut conn, cfg, forcar_completo).expect("recuperação");
    let (ref_tabela, ref_estado) = referencia(cfg, &hist);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
}

const GATILHO_INSERT_ESTADO: &str = "CREATE TRIGGER falha BEFORE INSERT ON motor_estado
     BEGIN SELECT RAISE(ABORT, 'falha simulada'); END;";
const GATILHO_UPDATE_ESTADO: &str = "CREATE TRIGGER falha BEFORE UPDATE ON motor_estado
     BEGIN SELECT RAISE(ABORT, 'falha simulada'); END;";
// Falha no meio da gravação das features, depois de já terem sido gravados
// (e, no incremental, de o snapshot antigo ter sido apagado) vários concursos.
const GATILHO_INSERT_FEATURES: &str = "CREATE TRIGGER falha
     BEFORE INSERT ON features_dezena WHEN NEW.concurso = 45
     BEGIN SELECT RAISE(ABORT, 'falha simulada'); END;";

#[test]
fn falha_no_insert_do_estado_desfaz_o_incremental() {
    falha_simulada_nao_altera_nada(GATILHO_INSERT_ESTADO, false);
}

#[test]
fn falha_no_update_do_estado_desfaz_o_incremental() {
    falha_simulada_nao_altera_nada(GATILHO_UPDATE_ESTADO, false);
}

#[test]
fn falha_no_insert_do_estado_desfaz_o_completo_forcado() {
    falha_simulada_nao_altera_nada(GATILHO_INSERT_ESTADO, true);
}

#[test]
fn falha_no_update_do_estado_desfaz_o_completo_forcado() {
    falha_simulada_nao_altera_nada(GATILHO_UPDATE_ESTADO, true);
}

#[test]
fn falha_no_meio_das_features_desfaz_o_incremental() {
    falha_simulada_nao_altera_nada(GATILHO_INSERT_FEATURES, false);
}

#[test]
fn falha_no_meio_das_features_desfaz_o_completo_forcado() {
    falha_simulada_nao_altera_nada(GATILHO_INSERT_FEATURES, true);
}

#[test]
fn falha_na_primeira_execucao_nao_deixa_motor_estado_criada() {
    let cfg = quina();
    let hist = historico(cfg, 30, 82);
    let mut conn = banco();
    gravar(&mut conn, &hist);
    // `features_dezena` já existe com colunas a menos: a gravação das linhas
    // falha depois de `motor_estado` ter sido criada dentro da transação, e a
    // criação dessa tabela também tem de ser desfeita.
    conn.execute_batch("CREATE TABLE features_dezena (concurso INTEGER);")
        .expect("tabela incompatível criada");

    let resultado = sincronizar_features(&mut conn, cfg, false);

    assert!(resultado.is_err());
    let motor_estado: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'motor_estado'",
            [],
            |row| row.get(0),
        )
        .expect("sqlite_master");
    assert_eq!(motor_estado, 0, "motor_estado criada apesar da falha");
}

// ---------------------------------------------------------------------------
// 8. motor_estado coerente com o histórico.
// ---------------------------------------------------------------------------

#[test]
fn motor_estado_gravado_confere_com_o_historico_e_o_motor() {
    for slug in ["quina", "lotofacil", "megasena"] {
        let cfg = config::buscar(slug).expect("loteria configurada");
        let m = 35;
        let hist = historico(cfg, m, 91);
        let conn = calculado(cfg, &hist);

        let salvo = estado_salvo(&conn);

        assert_eq!(salvo.loteria, cfg.slug);
        assert_eq!(salvo.feature_version, FEATURE_VERSION);
        assert_eq!(salvo.ultimo_concurso, m);
        assert_eq!(salvo.impressao, impressao_resultados(&hist));
        let estado: EstadoMotor =
            serde_json::from_str(&salvo.estado_json).expect("estado desserializável");
        let (esperado, _) = motor::calcular(cfg, &hist).expect("motor");
        assert_eq!(estado, esperado, "{slug}");
    }
}

#[test]
fn motor_estado_apos_incremental_cobre_todo_o_historico() {
    let cfg = quina();
    let hist = historico(cfg, 60, 92);
    let mut conn = calculado(cfg, &hist[..25]);
    gravar(&mut conn, &hist[25..]);
    sincronizar_features(&mut conn, cfg, false).expect("incremental");

    let salvo = estado_salvo(&conn);

    assert_eq!(salvo.ultimo_concurso, 60);
    // A impressão cobre os 60 concursos, não só os 35 incorporados agora.
    assert_eq!(salvo.impressao, impressao_resultados(&hist));
    let estado: EstadoMotor = serde_json::from_str(&salvo.estado_json).expect("estado");
    let (esperado, _) = motor::calcular(cfg, &hist).expect("motor");
    assert_eq!(estado, esperado);
}

#[test]
fn motor_estado_tem_uma_unica_linha_apos_varias_execucoes() {
    let cfg = quina();
    let hist = historico(cfg, 30, 93);
    let mut conn = calculado(cfg, &hist[..10]);
    gravar(&mut conn, &hist[10..]);
    sincronizar_features(&mut conn, cfg, false).expect("incremental");
    sincronizar_features(&mut conn, cfg, true).expect("forçado");

    let linhas: i64 = conn
        .query_row("SELECT COUNT(*) FROM motor_estado", [], |row| row.get(0))
        .expect("contagem");

    assert_eq!(linhas, 1);
}

// ---------------------------------------------------------------------------
// 9. Contagem de linhas gravadas.
// ---------------------------------------------------------------------------

#[test]
fn linhas_gravadas_incremental_e_incorporados_mais_um_vezes_linhas_por_concurso() {
    let cfg = quina();
    let por = por_concurso(cfg);
    let hist = historico(cfg, 60, 101);
    let mut conn = banco();
    let mut feito = 0;
    for corte in [1, 4, 5, 30, 60] {
        gravar(&mut conn, &hist[feito..corte]);
        let resumo = sincronizar_features(&mut conn, cfg, false).expect("sincronizado");
        let incorporados = corte - feito;
        // Cada concurso novo mais o snapshot do próximo.
        assert_eq!(resumo.concursos_incorporados, incorporados);
        assert_eq!(resumo.linhas_gravadas, (incorporados + 1) * por, "{corte}");
        feito = corte;
    }
}

#[test]
fn linhas_gravadas_do_completo_e_todos_os_concursos_mais_um_vezes_linhas_por_concurso() {
    for slug in ["quina", "lotofacil", "megasena"] {
        let cfg = config::buscar(slug).expect("loteria configurada");
        let n = 30;
        let hist = historico(cfg, n, 102);
        let mut conn = banco();
        gravar(&mut conn, &hist);

        let primeira = sincronizar_features(&mut conn, cfg, false).expect("primeira");
        let forcada = sincronizar_features(&mut conn, cfg, true).expect("forçada");

        let esperado = (n as usize + 1) * por_concurso(cfg);
        assert_eq!(primeira.linhas_gravadas, esperado, "{slug}");
        assert_eq!(forcada.linhas_gravadas, esperado, "{slug}");
        assert_eq!(primeira.concursos_incorporados, n as usize);
        assert_eq!(forcada.concursos_incorporados, n as usize);
        assert_eq!(tabela(&conn).len(), esperado);
    }
}

#[test]
fn linhas_gravadas_na_regeneracao_por_correcao_cobrem_o_historico_inteiro() {
    let cfg = quina();
    let mut hist = historico(cfg, 40, 103);
    let mut conn = calculado(cfg, &hist);
    hist[9] = com_dezenas_trocadas(cfg, &hist[9]);
    gravar(&mut conn, &hist[9..10]);

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("regeneração");

    assert_eq!(resumo.linhas_gravadas, 41 * por_concurso(cfg));
}

// ---------------------------------------------------------------------------
// 10. Testes obrigatórios do plano (1a.3) sobre a tabela gravada: esquema,
//     golden, casos de borda com NULL de verdade, vazamento e incremental
//     concurso a concurso. Lê os valores crus do SQLite (`Value`), sem passar
//     pela conversão de `db::ler_features`, para comparar o que está gravado.
// ---------------------------------------------------------------------------

/// Colunas de `features_dezena`, na ordem do plano ("Esquema das tabelas
/// derivadas").
const COLUNAS_DO_PLANO: [&str; 14] = [
    "concurso",
    "dezena",
    "janela",
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
    "saiu",
    "feature_version",
];

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofacil configurada")
}

fn megasena() -> &'static LoteriaConfig {
    config::buscar("megasena").expect("megasena configurada")
}

/// Lotofácil 1..=n em que a dezena 25 sai exatamente nos concursos de
/// `aparicoes_25`; as outras 14 (ou 15) são sempre as menores.
fn lotofacil_com_25_em(n: u32, aparicoes_25: &[u32]) -> Vec<Resultado> {
    let cfg = lotofacil();
    (1..=n)
        .map(|c| {
            let dezenas: Vec<u32> = if aparicoes_25.contains(&c) {
                (1..=14).chain([25]).collect()
            } else {
                (1..=15).collect()
            };
            res(cfg, c, &dezenas)
        })
        .collect()
}

/// Linha esperada de `features_dezena`, na ordem de [`COLUNAS_DO_PLANO`]:
/// `None` é `NULL`, `Some(v)` é `INTEGER v`.
fn crua(valores: [Option<i64>; 14]) -> Vec<Value> {
    valores
        .into_iter()
        .map(|v| v.map_or(Value::Null, Value::Integer))
        .collect()
}

/// Linhas de `features_dezena` com `concurso <= ate`, todas as colunas na
/// ordem da tabela, ordenadas pela chave, com os valores como o SQLite os
/// guarda (classe de armazenamento incluída).
fn linhas_cruas(conn: &Connection, ate: u32) -> Vec<Vec<Value>> {
    let mut stmt = conn
        .prepare(
            "SELECT * FROM features_dezena WHERE concurso <= ?1
             ORDER BY concurso, dezena, janela",
        )
        .expect("consulta preparada");
    let colunas = stmt.column_count();
    let linhas = stmt
        .query_map([ate], |row| {
            (0..colunas)
                .map(|i| row.get::<_, Value>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .expect("consulta");
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("linhas lidas")
}

/// Uma linha de `features_dezena` pela chave, com as colunas do plano.
fn linha_crua(conn: &Connection, concurso: u32, dezena: u32, janela: u32) -> Vec<Value> {
    let sql = format!(
        "SELECT {} FROM features_dezena WHERE concurso = ?1 AND dezena = ?2 AND janela = ?3",
        COLUNAS_DO_PLANO.join(", ")
    );
    conn.query_row(&sql, [concurso, dezena, janela], |row| {
        (0..COLUNAS_DO_PLANO.len())
            .map(|i| row.get::<_, Value>(i))
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .expect("linha presente")
}

fn contar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0))
        .unwrap_or_else(|e| panic!("consulta `{sql}`: {e}"))
}

#[test]
fn features_dezena_tem_as_colunas_e_a_chave_do_plano() {
    let conn = calculado(quina(), &historico(quina(), 3, 111));

    let stmt = conn
        .prepare("SELECT * FROM features_dezena")
        .expect("consulta preparada");
    let colunas: Vec<&str> = stmt.column_names();
    assert_eq!(colunas, COLUNAS_DO_PLANO);

    // Chave primária (concurso, dezena, janela), nessa ordem.
    let mut stmt = conn
        .prepare("SELECT name, pk FROM pragma_table_info('features_dezena') WHERE pk > 0")
        .expect("pragma");
    let mut chave: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(1)?, row.get(0)?)))
        .expect("pragma")
        .collect::<rusqlite::Result<_>>()
        .expect("pragma lido");
    chave.sort();
    let chave: Vec<&str> = chave.iter().map(|(_, nome)| nome.as_str()).collect();
    assert_eq!(chave, ["concurso", "dezena", "janela"]);
}

#[test]
fn golden_do_plano_gravado_em_features_dezena() {
    // docs/PLANEJAMENTO.md, "Exemplo de referência": a dezena sai em 3, 4, 8,
    // 9, 10, 15; snapshot em t = 20, janela total (gravada como 0).
    let cfg = lotofacil();
    let hist = lotofacil_com_25_em(20, &[3, 4, 8, 9, 10, 15]);
    let fv = Some(i64::from(FEATURE_VERSION));
    let golden = |saiu: Option<i64>| {
        crua([
            Some(20), // concurso
            Some(25), // dezena
            Some(0),  // janela total
            Some(6),  // frequencia
            Some(4),  // maior_espaco
            Some(1),  // maior_espaco_ocorrencias
            Some(0),  // menor_espaco
            Some(3),  // menor_espaco_ocorrencias
            Some(10), // soma_variacoes
            Some(4),  // espaco_final
            Some(3),  // repeticoes_total
            Some(0),  // repeticao_atual
            saiu,
            fv,
        ])
    };
    assert_eq!(JANELA_TOTAL, 0);

    // Com o banco até o 19, a linha do 20 é o snapshot do próximo concurso,
    // ainda não sorteado: mesmos componentes, `saiu` NULL.
    let mut conn = banco();
    gravar(&mut conn, &hist[..19]);
    sincronizar_features(&mut conn, cfg, false).expect("até o 19");
    assert_eq!(linha_crua(&conn, 20, 25, JANELA_TOTAL), golden(None));

    // Chegou o 20, em que a 25 não saiu: mesma linha, alvo 0.
    gravar(&mut conn, &hist[19..]);
    let resumo = sincronizar_features(&mut conn, cfg, false).expect("o 20");
    assert_eq!(resumo.modo, ModoFeatures::Incremental);
    assert_eq!(linha_crua(&conn, 20, 25, JANELA_TOTAL), golden(Some(0)));

    // Calculado do zero, igual.
    let conn = calculado(cfg, &hist);
    assert_eq!(linha_crua(&conn, 20, 25, JANELA_TOTAL), golden(Some(0)));
}

#[test]
fn casos_de_borda_gravam_null_de_verdade() {
    // Mesmo histórico do golden: a 25 sai em 3, 4, 8, 9, 10, 15.
    let cfg = lotofacil();
    let conn = calculado(cfg, &lotofacil_com_25_em(20, &[3, 4, 8, 9, 10, 15]));
    let fv = Some(i64::from(FEATURE_VERSION));
    let casos = [
        // t = 3, total: nunca saiu → espaços, variações e espaco_final NULL.
        [
            Some(3),
            Some(25),
            Some(0),
            Some(0),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(0),
            Some(0),
            Some(1),
            fv,
        ],
        // t = 4, total: uma aparição (3) → sem espaços nem variações;
        // espaco_final = 3 − 3 = 0; sequência em curso de 1; saiu no 4.
        [
            Some(4),
            Some(25),
            Some(0),
            Some(1),
            None,
            None,
            None,
            None,
            None,
            Some(0),
            Some(0),
            Some(1),
            Some(1),
            fv,
        ],
        // t = 5, total: duas aparições (3, 4) → espaços [0], variações NULL;
        // sequência em curso de 2; não saiu no 5.
        [
            Some(5),
            Some(25),
            Some(0),
            Some(2),
            Some(0),
            Some(1),
            Some(0),
            Some(1),
            None,
            Some(0),
            Some(1),
            Some(2),
            Some(0),
            fv,
        ],
        // t = 9, total: três aparições (3, 4, 8) → espaços [0, 3], variação
        // |3 − 0| = 3; sequência em curso de 1 (o 8); saiu no 9.
        [
            Some(9),
            Some(25),
            Some(0),
            Some(3),
            Some(3),
            Some(1),
            Some(0),
            Some(1),
            Some(3),
            Some(0),
            Some(1),
            Some(1),
            Some(1),
            fv,
        ],
        // t = 20, janela 10 (10 … 19): aparições 10 e 15 → espaços [4],
        // variações NULL; espaco_final continua 4.
        [
            Some(20),
            Some(25),
            Some(10),
            Some(2),
            Some(4),
            Some(1),
            Some(4),
            Some(1),
            None,
            Some(4),
            Some(0),
            Some(0),
            Some(0),
            fv,
        ],
    ];
    for caso in casos {
        let (concurso, janela) = (caso[0].unwrap() as u32, caso[2].unwrap() as u32);
        assert_eq!(
            linha_crua(&conn, concurso, 25, janela),
            crua(caso),
            "concurso {concurso}, janela {janela}"
        );
    }
}

#[test]
fn nulls_da_tabela_seguem_as_regras_de_borda_do_plano() {
    // Invariantes sobre a tabela inteira, com históricos em que a janela de
    // 10 tem muitas dezenas com 0, 1 e 2 aparições (Quina, Mega-Sena).
    for (cfg, semente) in [(quina(), 121), (megasena(), 122), (lotofacil(), 123)] {
        let n = 130;
        let conn = calculado(cfg, &historico(cfg, n, semente));
        let slug = cfg.slug;
        let espacos = [
            "maior_espaco",
            "maior_espaco_ocorrencias",
            "menor_espaco",
            "menor_espaco_ocorrencias",
        ];

        for coluna in espacos {
            // Menos de 2 aparições na janela → colunas de espaço NULL; com 2
            // ou mais, preenchidas.
            assert_eq!(
                contar(
                    &conn,
                    &format!(
                        "SELECT COUNT(*) FROM features_dezena
                         WHERE (frequencia < 2) != ({coluna} IS NULL)"
                    )
                ),
                0,
                "{slug}: {coluna}"
            );
        }
        // Menos de 3 aparições → soma_variacoes NULL; com 3 ou mais, preenchida.
        assert_eq!(
            contar(
                &conn,
                "SELECT COUNT(*) FROM features_dezena
                 WHERE (frequencia < 3) != (soma_variacoes IS NULL)"
            ),
            0,
            "{slug}: soma_variacoes"
        );
        // espaco_final NULL só se a dezena nunca saiu antes de t (frequência
        // da janela total = 0), em qualquer janela.
        assert_eq!(
            contar(
                &conn,
                "SELECT COUNT(*) FROM features_dezena f
                 JOIN features_dezena t
                   ON t.concurso = f.concurso AND t.dezena = f.dezena AND t.janela = 0
                 WHERE (f.espaco_final IS NULL) != (t.frequencia = 0)"
            ),
            0,
            "{slug}: espaco_final"
        );
        // `saiu` NULL só no snapshot do próximo concurso.
        assert_eq!(
            contar(
                &conn,
                &format!(
                    "SELECT COUNT(*) FROM features_dezena
                     WHERE (saiu IS NULL) != (concurso = {})",
                    n + 1
                )
            ),
            0,
            "{slug}: saiu"
        );
        // Fora o NULL, tudo é INTEGER não negativo (nunca 0 ou −1 no lugar de
        // NULL, nunca texto ou real); `saiu` é 0 ou 1.
        for coluna in COLUNAS_DO_PLANO {
            assert_eq!(
                contar(
                    &conn,
                    &format!(
                        "SELECT COUNT(*) FROM features_dezena
                         WHERE typeof({coluna}) NOT IN ('integer', 'null') OR {coluna} < 0"
                    )
                ),
                0,
                "{slug}: {coluna}"
            );
        }
        assert_eq!(
            contar(
                &conn,
                "SELECT COUNT(*) FROM features_dezena WHERE saiu NOT IN (0, 1)"
            ),
            0,
            "{slug}: saiu"
        );

        // Os casos de borda acontecem de fato (o teste não é vazio): linhas
        // com 0, 1 e 2 aparições na janela e espaco_final NULL.
        for (descricao, filtro) in [
            ("sem aparições", "frequencia = 0"),
            ("uma aparição", "frequencia = 1"),
            ("duas aparições", "frequencia = 2"),
            ("três ou mais", "frequencia >= 3"),
            ("nunca saiu", "espaco_final IS NULL AND concurso > 1"),
        ] {
            assert!(
                contar(
                    &conn,
                    &format!("SELECT COUNT(*) FROM features_dezena WHERE {filtro}")
                ) > 0,
                "{slug}: nenhuma linha com {descricao}"
            );
        }
    }
}

/// Mesmo histórico até `t − 1`; de `t` em diante, outro sorteio (outra
/// semente), com `t` girado para garantir que o concurso `t` mudou.
fn divergente_a_partir_de(cfg: &LoteriaConfig, hist: &[Resultado], t: u32) -> Vec<Resultado> {
    let n = hist.len() as u32;
    let outro = historico(cfg, n, 0xd1fe_0000 + u64::from(t));
    let mut alterado = hist[..(t - 1) as usize].to_vec();
    alterado.push(com_dezenas_trocadas(cfg, &hist[(t - 1) as usize]));
    alterado.extend_from_slice(&outro[t as usize..]);
    alterado
}

#[test]
fn vazamento_no_banco_concurso_t_diferente_nao_muda_linhas_ate_t() {
    // Dois bancos com o mesmo histórico até t − 1 e concursos t … n
    // diferentes: as linhas gravadas de concursos < t são idênticas, coluna a
    // coluna (classe de armazenamento incluída); as de t também, menos `saiu`
    // (o alvo de t, que legitimamente muda). O banco B é calculado como em
    // produção: primeiro até t − 1 (gravando o snapshot de t com saiu NULL),
    // depois com os concursos divergentes, de forma incremental.
    for (cfg, n, semente) in [
        (quina(), 110u32, 131),
        (lotofacil(), 110, 132),
        (megasena(), 110, 133),
    ] {
        let hist = historico(cfg, n, semente);
        let a = calculado(cfg, &hist);
        let colunas: Vec<String> = a
            .prepare("SELECT * FROM features_dezena")
            .expect("consulta")
            .column_names()
            .into_iter()
            .map(String::from)
            .collect();
        let i_concurso = colunas.iter().position(|c| c == "concurso").unwrap();
        let i_saiu = colunas.iter().position(|c| c == "saiu").unwrap();

        for t in [1u32, 2, 11, 50, 100, 101, n] {
            let alterado = divergente_a_partir_de(cfg, &hist, t);
            assert_eq!(alterado.len(), hist.len());
            assert_ne!(
                alterado[(t - 1) as usize].dezenas,
                hist[(t - 1) as usize].dezenas
            );

            let mut b = banco();
            if t > 1 {
                gravar(&mut b, &alterado[..(t - 1) as usize]);
                sincronizar_features(&mut b, cfg, false).expect("B até t − 1");
            }
            gravar(&mut b, &alterado[(t - 1) as usize..]);
            sincronizar_features(&mut b, cfg, false).expect("B completo");

            let sem_alvo_de_t = |linhas: Vec<Vec<Value>>| -> Vec<Vec<Value>> {
                linhas
                    .into_iter()
                    .map(|mut l| {
                        if l[i_concurso] == Value::Integer(i64::from(t)) {
                            l[i_saiu] = Value::Null;
                        }
                        l
                    })
                    .collect()
            };
            let linhas_a = linhas_cruas(&a, t);
            let linhas_b = linhas_cruas(&b, t);
            assert_eq!(linhas_a.len(), t as usize * por_concurso(cfg));
            assert_eq!(
                sem_alvo_de_t(linhas_a.clone()),
                sem_alvo_de_t(linhas_b.clone()),
                "{} t = {t}",
                cfg.slug
            );

            // O alvo de t reflete as dezenas de t.
            let alvo_de_t = |linhas: &[Vec<Value>]| -> Vec<Value> {
                linhas
                    .iter()
                    .filter(|l| l[i_concurso] == Value::Integer(i64::from(t)))
                    .map(|l| l[i_saiu].clone())
                    .collect()
            };
            assert_ne!(
                alvo_de_t(&linhas_a),
                alvo_de_t(&linhas_b),
                "{} t = {t}",
                cfg.slug
            );

            // Contraprova: as linhas de t + 1 (que já veem t; para t = n, o
            // snapshot do próximo concurso), sem o alvo, mudam.
            let sem_alvo_de = |conn: &Connection, c: u32| -> Vec<Vec<Value>> {
                linhas_cruas(conn, c)
                    .into_iter()
                    .filter(|l| l[i_concurso] == Value::Integer(i64::from(c)))
                    .map(|mut l| {
                        l[i_saiu] = Value::Null;
                        l
                    })
                    .collect()
            };
            assert_ne!(
                sem_alvo_de(&a, t + 1),
                sem_alvo_de(&b, t + 1),
                "{} t = {t}",
                cfg.slug
            );
        }
    }
}

#[test]
fn incremental_concurso_a_concurso_igual_ao_completo_nas_tres_loterias() {
    // O plano: "calcular do zero e adicionar concurso a concurso produzem o
    // mesmo resultado" — um `sync features` por concurso gravado.
    let cortes: Vec<usize> = (1..=30).collect();
    for (slug, semente) in [("quina", 141), ("lotofacil", 142), ("megasena", 143)] {
        incremental_igual_ao_completo(slug, 30, &cortes, semente);
    }
}

// ---------------------------------------------------------------------------
// 11. Resultado apagado de `resultados` depois de calculado.
// ---------------------------------------------------------------------------

#[test]
fn resultado_apagado_abaixo_do_ultimo_incorporado_recusa_e_preserva_tudo() {
    // O plano: `sync features` recusa rodar enquanto o `validar` apontar
    // buraco. Apagar um concurso já incorporado (abaixo do último) abre um
    // buraco: recusa, com ou sem --completo, sem tocar features nem estado.
    let cfg = quina();
    let hist = historico(cfg, 30, 151);
    for apagado in [1u32, 12, 29] {
        let mut conn = calculado(cfg, &hist);
        let tabela_antes = tabela(&conn);
        let estado_antes = estado_salvo(&conn);
        conn.execute("DELETE FROM resultados WHERE concurso = ?1", [apagado])
            .expect("resultado apagado");

        for forcar_completo in [false, true] {
            let erro = sincronizar_features(&mut conn, cfg, forcar_completo).expect_err("buraco");
            match erro {
                SpiderError::ResultadosReprovados { erros, .. } => assert!(
                    erros.contains(&Problema::ConcursosFaltando {
                        inicio: apagado,
                        fim: apagado
                    }),
                    "apagado {apagado}: {erros:?}"
                ),
                outro => panic!("apagado {apagado}: erro inesperado: {outro:?}"),
            }
            assert_eq!(tabela(&conn), tabela_antes, "apagado {apagado}");
            assert_eq!(estado_salvo(&conn), estado_antes, "apagado {apagado}");
        }

        // Devolvido o mesmo resultado, a impressão volta a conferir: retoma
        // sem nada novo, sem regenerar.
        gravar(&mut conn, &hist[(apagado - 1) as usize..apagado as usize]);
        let resumo = sincronizar_features(&mut conn, cfg, false).expect("buraco fechado");
        assert_eq!(resumo.modo, ModoFeatures::Incremental, "apagado {apagado}");
        assert_eq!(resumo.concursos_incorporados, 0);
        assert_eq!(tabela(&conn), tabela_antes, "apagado {apagado}");
        assert_eq!(estado_salvo(&conn), estado_antes, "apagado {apagado}");
    }
}

#[test]
fn ultimo_concurso_apagado_regenera_sem_ele() {
    // Apagar o último não abre buraco (o `validar` passa), mas muda um
    // resultado já incorporado: regenera do zero, e a tabela fica igual à de
    // um banco que nunca teve o concurso — o snapshot dele volta a ter saiu
    // NULL e o do concurso seguinte some.
    let cfg = quina();
    let hist = historico(cfg, 30, 152);
    let mut conn = calculado(cfg, &hist);
    conn.execute("DELETE FROM resultados WHERE concurso = 30", [])
        .expect("resultado apagado");

    let resumo = sincronizar_features(&mut conn, cfg, false).expect("regeneração");

    assert_eq!(
        resumo.modo,
        ModoFeatures::Completo(MotivoRegeneracao::ResultadosAlterados)
    );
    assert_eq!(resumo.ultimo_concurso, 29);
    assert_eq!(resumo.proximo_concurso, 30);
    let (ref_tabela, ref_estado) = referencia(cfg, &hist[..29]);
    assert_eq!(tabela(&conn), ref_tabela);
    assert_eq!(estado_salvo(&conn), ref_estado);
    assert!(db::ler_features_do_concurso(&conn, 31)
        .expect("lido")
        .is_empty());
}
