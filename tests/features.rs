//! Testes de `orquestracao::sincronizar_features` (comando `sync features`)
//! sobre bancos em memória: gravação em `features_dezena` e `motor_estado`,
//! retomada incremental, regeneração e recusa.

use rusqlite::Connection;
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db::{self, MotorEstadoSalvo};
use rust_spider::error::SpiderError;
use rust_spider::motor::{self, EstadoMotor, FeaturesDezena, FEATURE_VERSION, JANELAS};
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
