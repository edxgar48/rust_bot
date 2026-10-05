use super::*;
use crate::avaliacao::tests::{concurso_mini, obs, serie_aleatoria, serie_mini, Gerador, MINI};
use crate::avaliacao::Intervalo;
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn perto(a: f64, b: f64, tolerancia: f64) -> bool {
    (a - b).abs() <= tolerancia * (1.0 + a.abs().max(b.abs()))
}

macro_rules! assert_perto {
    ($a:expr, $b:expr) => {
        assert_perto!($a, $b, 1e-12)
    };
    ($a:expr, $b:expr, $tol:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(
            perto(a, b, $tol),
            "{} = {a:?}, esperado {b:?}",
            stringify!($a)
        );
    }};
}

/// p-valor de `t` concursos da hipergeométrica (n, sorteadas, escolhidas).
fn p(n: u32, sorteadas: u32, escolhidas: u32, t: usize, total: u64) -> Cauda {
    let h = Hipergeometrica::nova(n, sorteadas, escolhidas).expect("parâmetros válidos");
    p_valor_soma(&h, t, total).expect("total possível")
}

// ----------------------------------------------------------- binomial

#[test]
fn binomial_contra_valores_tabelados() {
    // Totais de combinações das apostas mínimas, como publicados pela Caixa.
    assert_eq!(binomial(25, 15), Some(3_268_760));
    assert_eq!(binomial(60, 6), Some(50_063_860));
    assert_eq!(binomial(80, 5), Some(24_040_016));
    // O maior das loterias reais (Quina, top-40).
    assert_eq!(binomial(80, 40), Some(107_507_208_733_336_176_461_620));
    assert_eq!(binomial(0, 0), Some(1));
    assert_eq!(binomial(7, 0), Some(1));
    assert_eq!(binomial(7, 7), Some(1));
    assert_eq!(binomial(5, 6), Some(0));
    assert_eq!(binomial(10, 3), binomial(10, 7));
}

#[test]
fn binomial_so_estoura_quando_o_resultado_nao_cabe() {
    // C(131, 65) ≈ 1,9e38 cabe em u128 (máx. ≈ 3,4e38), mesmo com o produto
    // intermediário ingênuo C(131, 64)·67 passando do limite; C(132, 66) não
    // cabe. Valor de `math.comb(131, 65)` do Python.
    assert_eq!(
        binomial(131, 65),
        Some(188_694_833_082_770_476_622_296_176_145_946_360_850)
    );
    assert_eq!(binomial(132, 66), None);
    assert!(matches!(
        Hipergeometrica::nova(140, 5, 70),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

// ------------------------------------------------------ hipergeométrica

#[test]
fn hipergeometrica_lotofacil_contra_a_tabela_da_caixa() {
    // 15 de 25 contra 15 sorteadas. Probabilidades publicadas pela Caixa
    // (aposta de 15): 11 pontos 1 em 11, 12 em 59, 13 em 691, 14 em 21.791,
    // 15 em 3.268.760 (a Caixa trunca: 3268760/286650 = 11,40…).
    let h = Hipergeometrica::da_loteria(cfg("lotofacil"), 15).expect("k válido");
    assert_eq!(h.total(), 3_268_760);
    assert_eq!((h.minimo(), h.maximo()), (5, 15));
    assert_eq!(h.casos(15), 1);
    assert_eq!(h.casos(14), 150); // C(15,14)·C(10,1)
    assert_eq!(h.casos(13), 4_725); // C(15,13)·C(10,2)
    assert_eq!(h.casos(12), 54_600); // C(15,12)·C(10,3)
    assert_eq!(h.casos(11), 286_650); // C(15,11)·C(10,4)
    assert_eq!(h.casos(5), 3_003); // C(15,5)·C(10,10)
    assert_eq!(h.casos(4), 0);
    assert_eq!(h.casos(0), 0);
    assert_eq!(h.casos(16), 0);
    for (pontos, um_em) in [(11, 11), (12, 59), (13, 691), (14, 21_791), (15, 3_268_760)] {
        assert_eq!(h.total() / h.casos(pontos), um_em, "{pontos} pontos");
    }
    assert_eq!(h.pmf(15), 1.0 / 3_268_760.0);
    assert_eq!(h.pmf(11), 286_650.0 / 3_268_760.0);
    assert_eq!(h.media(), 9.0);
}

#[test]
fn hipergeometrica_megasena_contra_a_tabela_da_caixa() {
    // Caixa (aposta de 6): sena 1 em 50.063.860, quina 1 em 154.518, quadra
    // 1 em 2.332.
    let h = Hipergeometrica::da_loteria(cfg("megasena"), 6).expect("k válido");
    assert_eq!(h.total(), 50_063_860);
    assert_eq!((h.minimo(), h.maximo()), (0, 6));
    assert_eq!(h.casos(6), 1);
    assert_eq!(h.casos(5), 324); // C(6,5)·C(54,1)
    assert_eq!(h.casos(4), 21_465); // C(6,4)·C(54,2)
    assert_eq!(h.casos(0), 25_827_165); // C(54,6)
    for (acertos, um_em) in [(6, 50_063_860), (5, 154_518), (4, 2_332)] {
        assert_eq!(h.total() / h.casos(acertos), um_em, "{acertos} acertos");
    }
    assert_eq!(h.pmf(6), 1.0 / 50_063_860.0);
}

#[test]
fn hipergeometrica_quina_contra_a_tabela_da_caixa() {
    // Caixa (aposta de 5): quina 1 em 24.040.016, quadra 1 em 64.106, terno
    // 1 em 866, duque 1 em 36 (a Caixa arredonda 35,6).
    let h = Hipergeometrica::da_loteria(cfg("quina"), 5).expect("k válido");
    assert_eq!(h.total(), 24_040_016);
    assert_eq!(h.casos(5), 1);
    assert_eq!(h.casos(4), 375); // C(5,4)·C(75,1)
    assert_eq!(h.casos(3), 27_750); // C(5,3)·C(75,2)
    assert_eq!(h.casos(2), 675_250); // C(5,2)·C(75,3)
    for (acertos, um_em) in [(5, 24_040_016), (4, 64_106), (3, 866)] {
        assert_eq!(h.total() / h.casos(acertos), um_em, "{acertos} acertos");
    }
    assert_eq!((h.total() as f64 / h.casos(2) as f64).round(), 36.0);
    assert_eq!(h.pmf(5), 1.0 / 24_040_016.0);
    assert_eq!(h.media(), 0.3125);
}

#[test]
fn casos_somam_o_total_e_media_e_variancia_batem_em_todo_k() {
    // Vandermonde: Σ casos = C(N, k), exato em inteiros, para todo k das
    // três loterias (inclui a Quina com k = 40, o maior binomial).
    for cfg in config::LOTERIAS {
        let n = cfg.qtd_dezenas_possiveis();
        for k in 1..n {
            let h = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
            let soma: u128 = (0..=h.maximo()).map(|i| h.casos(i)).sum();
            assert_eq!(soma, h.total(), "{} k = {k}", cfg.slug);
            assert!((h.minimo()..=h.maximo()).all(|i| h.casos(i) > 0));
            let media: f64 = (0..=h.maximo()).map(|i| f64::from(i) * h.pmf(i)).sum();
            let segundo: f64 = (0..=h.maximo()).map(|i| f64::from(i * i) * h.pmf(i)).sum();
            assert_perto!(media, h.media());
            assert_perto!(segundo - media * media, h.variancia(), 1e-10);
            assert_perto!(
                h.media(),
                f64::from(k) * cfg.qtd_dezenas as f64 / f64::from(n)
            );
        }
    }
}

#[test]
fn k_fora_de_um_a_n_menos_um_e_erro() {
    for cfg in config::LOTERIAS {
        let n = cfg.qtd_dezenas_possiveis();
        for k in [0, n, n + 1] {
            assert!(
                matches!(
                    Hipergeometrica::da_loteria(cfg, k),
                    Err(SpiderError::AvaliacaoEntradaInvalida(_))
                ),
                "{} k = {k}",
                cfg.slug
            );
            let serie = serie_aleatoria(cfg, 5, 1, false);
            assert!(matches!(
                avaliar_topk(cfg, &serie, k, u32::MAX),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ));
            assert!(matches!(
                acertos_por_concurso(cfg, &serie, k, u32::MAX),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ));
            let um = [AcertosConcurso {
                concurso: 2,
                acertos: 0,
            }];
            assert!(matches!(
                resumir_topk(cfg, k, &um),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ));
        }
        for k in [1, n - 1] {
            assert!(Hipergeometrica::da_loteria(cfg, k).is_ok());
        }
    }
    assert!(matches!(
        Hipergeometrica::nova(4, 5, 2),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

// ------------------------------------------------- soma de T e p-valor

#[test]
fn soma_pequena_confere_com_fracoes_a_mao() {
    // N = 4, K = 2, k = 1: acertos 0 ou 1 com 1/2 cada.
    // T = 2: S = 0, 1, 2 com 1/4, 1/2, 1/4.
    let h = Hipergeometrica::nova(4, 2, 1).expect("válida");
    assert_eq!(distribuicao_soma(&h, 2), [0.25, 0.5, 0.25]);
    assert_eq!(p(4, 2, 1, 2, 0).probabilidade, 1.0);
    assert_perto!(p(4, 2, 1, 2, 1).probabilidade, 0.75);
    assert_perto!(p(4, 2, 1, 2, 2).probabilidade, 0.25);

    // k = 2: acertos 0, 1, 2 com 1/6, 4/6, 1/6.
    let h = Hipergeometrica::nova(4, 2, 2).expect("válida");
    assert_eq!(
        (h.casos(0), h.casos(1), h.casos(2), h.total()),
        (1, 4, 1, 6)
    );
    // T = 1: a própria pmf.
    for (s, esperado) in [(0, 1.0), (1, 5.0 / 6.0), (2, 1.0 / 6.0)] {
        assert_perto!(p(4, 2, 2, 1, s).probabilidade, esperado);
    }
    // T = 2: (1 + 4x + x²)²/36 = (1, 8, 18, 8, 1)/36.
    let d2 = distribuicao_soma(&h, 2);
    for (s, num) in [1.0, 8.0, 18.0, 8.0, 1.0].iter().enumerate() {
        assert_perto!(d2[s], num / 36.0);
    }
    for (s, cauda) in [(0, 36.0), (1, 35.0), (2, 27.0), (3, 9.0), (4, 1.0)] {
        assert_perto!(p(4, 2, 2, 2, s).probabilidade, cauda / 36.0);
    }
    // T = 3: (1 + 4x + x²)³/216 = (1, 12, 51, 88, 51, 12, 1)/216; caudas
    // P(S ≥ s) = 216, 215, 203, 152, 64, 13, 1 (/216).
    let d3 = distribuicao_soma(&h, 3);
    for (s, num) in [1.0, 12.0, 51.0, 88.0, 51.0, 12.0, 1.0].iter().enumerate() {
        assert_perto!(d3[s], num / 216.0);
    }
    for (s, cauda) in [216.0, 215.0, 203.0, 152.0, 64.0, 13.0, 1.0]
        .iter()
        .enumerate()
    {
        let c = p(4, 2, 2, 3, s as u64);
        assert_perto!(c.probabilidade, cauda / 216.0);
        assert_perto!(c.log10, (cauda / 216.0).log10());
    }

    // k = 3: mínimo 3 + 2 − 4 = 1 acerto; 1 ou 2 com 1/2 cada.
    // T = 2: S = 2, 3, 4 com 1/4, 1/2, 1/4; S < 2 é impossível.
    let h = Hipergeometrica::nova(4, 2, 3).expect("válida");
    assert_eq!((h.minimo(), h.maximo()), (1, 2));
    assert_eq!(distribuicao_soma(&h, 2), [0.0, 0.0, 0.25, 0.5, 0.25]);
    for s in 0..=2 {
        assert_eq!(p(4, 2, 3, 2, s).probabilidade, 1.0, "s = {s}");
        assert_eq!(p(4, 2, 3, 2, s).log10, 0.0, "s = {s}");
    }
    assert_perto!(p(4, 2, 3, 2, 3).probabilidade, 0.75);
    assert_perto!(p(4, 2, 3, 2, 4).probabilidade, 0.25);
}

#[test]
fn total_impossivel_e_erro_e_t_zero_da_um() {
    let h = Hipergeometrica::nova(4, 2, 2).expect("válida");
    assert!(matches!(
        p_valor_soma(&h, 3, 7),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
    assert!(matches!(
        p_valor_soma(&h, 0, 1),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
    assert_eq!(p_valor_soma(&h, 0, 0).expect("possível").probabilidade, 1.0);
    assert_eq!(distribuicao_soma(&h, 0), [1.0]);
}

#[test]
fn distribuicao_da_soma_tem_media_e_variancia_teoricas() {
    // Média T·k·K/N e variância T·k·(K/N)·((N−K)/N)·((N−k)/(N−1)), e soma 1.
    for (slug, k, t) in [
        ("lotofacil", 15, 60),
        ("lotofacil", 3, 40),
        ("megasena", 6, 80),
        ("megasena", 30, 25),
        ("quina", 5, 100),
        ("quina", 79, 30),
    ] {
        let cfg = cfg(slug);
        let h = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
        let d = distribuicao_soma(&h, t);
        assert_eq!(d.len(), t * h.maximo() as usize + 1);
        assert!(d.iter().all(|x| x.is_finite() && *x >= 0.0));
        let soma: f64 = d.iter().sum();
        let media: f64 = d.iter().enumerate().map(|(s, x)| s as f64 * x).sum();
        let segundo: f64 = d
            .iter()
            .enumerate()
            .map(|(s, x)| (s as f64) * (s as f64) * x)
            .sum();
        let (n, kk, kf) = (
            f64::from(cfg.qtd_dezenas_possiveis()),
            cfg.qtd_dezenas as f64,
            f64::from(k),
        );
        let tf = t as f64;
        let media_teorica = tf * kf * kk / n;
        let variancia_teorica = tf * kf * (kk / n) * ((n - kk) / n) * ((n - kf) / (n - 1.0));
        assert_perto!(soma, 1.0, 1e-12);
        assert_perto!(media, media_teorica, 1e-11);
        assert_perto!(segundo - media * media, variancia_teorica, 1e-8);
    }
}

/// Convolução ingênua, sem inclinação nem poda, para comparar.
fn cauda_ingenua(h: &Hipergeometrica, t: usize) -> Vec<f64> {
    let maximo = h.maximo() as usize;
    let mut dist = vec![1.0_f64];
    for _ in 0..t {
        let mut nova = vec![0.0; dist.len() + maximo];
        for (s, a) in dist.iter().enumerate() {
            for i in 0..=maximo {
                nova[s + i] += a * h.pmf(i as u32);
            }
        }
        dist = nova;
    }
    let mut cauda = vec![0.0; dist.len() + 1];
    for s in (0..dist.len()).rev() {
        cauda[s] = cauda[s + 1] + dist[s];
    }
    cauda
}

#[test]
fn p_valor_bate_com_a_convolucao_ingenua_onde_ela_nao_some() {
    for (slug, k, t) in [
        ("megasena", 6, 60),
        ("lotofacil", 15, 40),
        ("quina", 5, 120),
        ("quina", 20, 30),
    ] {
        let h = Hipergeometrica::da_loteria(cfg(slug), k).expect("k válido");
        let ingenua = cauda_ingenua(&h, t);
        let maximo = t as u64 * u64::from(h.maximo());
        for total in 0..=maximo {
            let esperado = ingenua[total as usize].min(1.0);
            let c = p_valor_soma(&h, t, total).expect("possível");
            if esperado > 1e-250 {
                assert_perto!(c.probabilidade, esperado, 1e-10);
                assert_perto!(c.log10, esperado.log10(), 1e-10);
            }
        }
    }
}

/// (loteria, k, T, [(total, log10 P(S ≥ total))]).
type CasoExato = (&'static str, u32, usize, &'static [(u64, f64)]);

#[test]
fn p_valor_contra_caudas_exatas_em_fracoes() {
    // log10 de P(S ≥ s) calculado à parte em Python com inteiros exatos
    // (convolução de `math.comb` em big int, sem ponto flutuante até o log
    // final):
    //   c = [comb(K,i)*comb(N-K,k-i) for i in range(min(k,K)+1)]
    //   d = convolução de T cópias de c; cauda = sum(d[s:]); den = comb(N,k)**T
    //   log10(cauda) − log10(den)
    let casos: [CasoExato; 4] = [
        (
            "megasena",
            6,
            200,
            &[
                (0, 0.0),
                (100, -0.007798809710493515),
                (119, -0.25518738103210126),
                (120, -0.28765847266890887),
                (121, -0.3228174926214251),
                (150, -2.7145940852044532),
                (200, -13.337088863810777),
                (300, -55.90700025048477),
                (600, -318.4971349961054),
                (1199, -1535.0932845581842),
                (1200, -1539.9048662660784),
            ],
        ),
        (
            "lotofacil",
            15,
            100,
            &[
                (500, 0.0),
                (900, -0.28730894122543305),
                (901, -0.31562152115918707),
                (950, -4.5657843774895355),
                (1000, -15.59626883104454),
                (1200, -134.56711217512634),
                (1499, -647.2621832724323),
                (1500, -651.4383034834883),
            ],
        ),
        (
            "quina",
            5,
            300,
            &[
                (93, -0.2605555643926891),
                (94, -0.29654010181866397),
                (95, -0.3357078201056538),
                (120, -2.5000139693788697),
                (200, -24.25974411275365),
                (1500, -2214.280425713554),
            ],
        ),
        (
            "quina",
            79,
            50,
            &[
                (200, 0.0),
                (201, 0.0),
                (230, -2.5579538487363607e-13),
                (249, -0.7646140824249983),
                (250, -1.401436180012169),
            ],
        ),
    ];
    for (slug, k, t, valores) in casos {
        let h = Hipergeometrica::da_loteria(cfg(slug), k).expect("k válido");
        for &(total, log10) in valores {
            let c = p_valor_soma(&h, t, total).expect("possível");
            assert_perto!(c.log10, log10, 1e-10);
            assert!(
                perto(c.probabilidade, 10f64.powf(log10), 1e-9) || log10 < -300.0,
                "{slug} k = {k}, T = {t}, total {total}: {c:?}"
            );
        }
    }
}

#[test]
fn cauda_extrema_nao_vira_nan_nem_negativa() {
    // Lotofácil, T = 3400 (o tamanho real do trecho de busca), k = 15:
    // P(S = 15T) = p15^T e P(S ≥ 15T − 1) = p15^(T−1)·(p15 + T·p14), com
    // p15 = 1/3268760 e p14 = 150/3268760: ~10^−22000, muito abaixo do
    // menor f64. Também no menor f64 em diante na Mega-Sena.
    let h = Hipergeometrica::da_loteria(cfg("lotofacil"), 15).expect("k válido");
    let t = 3400_usize;
    let tf = t as f64;
    let (p15, p14) = (1.0 / 3_268_760.0_f64, 150.0 / 3_268_760.0_f64);
    let topo = 15 * t as u64;

    let c = p_valor_soma(&h, t, topo).expect("possível");
    assert_eq!(c.probabilidade, 0.0);
    assert_perto!(c.log10, tf * p15.log10(), 1e-12);

    let c = p_valor_soma(&h, t, topo - 1).expect("possível");
    assert_eq!(c.probabilidade, 0.0);
    assert_perto!(
        c.log10,
        (tf - 1.0) * p15.log10() + (p15 + tf * p14).log10(),
        1e-10
    );
}

#[test]
fn p_valor_em_zero_um_monotonico_e_um_no_minimo() {
    for (slug, k, t) in [
        ("megasena", 6, 50),
        ("lotofacil", 15, 30),
        ("lotofacil", 24, 20),
        ("quina", 79, 40),
        ("quina", 5, 60),
    ] {
        let h = Hipergeometrica::da_loteria(cfg(slug), k).expect("k válido");
        let minimo = t as u64 * u64::from(h.minimo());
        let maximo = t as u64 * u64::from(h.maximo());
        let mut anterior = Cauda {
            probabilidade: 1.0,
            log10: 0.0,
        };
        for total in 0..=maximo {
            let c = p_valor_soma(&h, t, total).expect("possível");
            assert!(
                (0.0..=1.0).contains(&c.probabilidade) && c.log10.is_finite() && c.log10 <= 0.0,
                "{slug} k = {k}, total {total}: {c:?}"
            );
            if total <= minimo {
                assert_eq!(c.probabilidade, 1.0);
                assert_eq!(c.log10, 0.0);
            }
            assert!(
                c.probabilidade <= anterior.probabilidade && c.log10 <= anterior.log10,
                "{slug} k = {k}: não monotônico em {total}: {anterior:?} → {c:?}"
            );
            anterior = c;
        }
    }
}

/// Mega-Sena, k = 6, `t` concursos, totais de ⌊média⌋−2 a ⌊média⌋+3 (média
/// T·k·K/N = 0,6·T): até ⌊média⌋ a conta vai sem inclinação, de ⌊média⌋+1
/// em diante, com. O p-valor tem de ser não crescente, em [0, 1], perto de
/// 1/2 e sem salto na troca de caminho: cada passo tira só P(S = total),
/// que perto da média é ~1/(√(2π)·σ), com σ² = T·0,4942 (variância da
/// hipergeométrica 6·(6/60)·(54/60)·(54/59)); aceita-se de 0,5 a 1,5 vez isso.
fn conferir_troca_de_caminho(t: usize) {
    let h = Hipergeometrica::da_loteria(cfg("megasena"), 6).expect("k válido");
    let media_vezes_n = t as u64 * 6 * 6;
    let piso_media = media_vezes_n / 60;
    // A troca (sem → com inclinação) cai entre piso_media e piso_media + 1.
    assert!(piso_media * 60 <= media_vezes_n && (piso_media + 1) * 60 > media_vezes_n);
    let densidade = 1.0 / ((2.0 * std::f64::consts::PI * t as f64 * h.variancia()).sqrt());

    let mut anterior: Option<Cauda> = None;
    for total in piso_media - 2..=piso_media + 3 {
        let c = p_valor_soma(&h, t, total).expect("possível");
        assert!(
            (0.0..=1.0).contains(&c.probabilidade) && c.log10.is_finite() && c.log10 <= 0.0,
            "T = {t}, total {total}: {c:?}"
        );
        assert!(
            c.probabilidade > 0.4 && c.probabilidade < 0.6,
            "T = {t}, total {total}: {c:?}"
        );
        if let Some(a) = anterior {
            assert!(
                c.probabilidade <= a.probabilidade && c.log10 <= a.log10,
                "T = {t}: não monotônico em {total}: {a:?} → {c:?}"
            );
            let passo = a.probabilidade - c.probabilidade;
            assert!(
                passo >= 0.5 * densidade && passo <= 1.5 * densidade,
                "T = {t}: salto de {passo} em {total} (esperado ~{densidade}): {a:?} → {c:?}"
            );
        }
        anterior = Some(c);
    }
}

#[test]
fn p_valor_monotonico_na_troca_de_caminho() {
    // T = 1000 (média 600, exata: 600 vai sem inclinação, 601 com) para
    // caber no tempo da suíte em debug; o T real fica no teste ignorado.
    conferir_troca_de_caminho(1000);
}

/// O mesmo com o T real da Mega-Sena (média 1654,8). ~20 s em debug:
/// `cargo test --release troca_de_caminho_com_t_real -- --ignored`.
#[test]
#[ignore = "lento em debug; rodar em release"]
fn p_valor_monotonico_na_troca_de_caminho_com_t_real() {
    conferir_troca_de_caminho(2758);
}

#[test]
fn p_valor_e_deterministico() {
    let h = Hipergeometrica::da_loteria(cfg("megasena"), 6).expect("k válido");
    for total in [0_u64, 120, 150, 600, 1199] {
        let a = p_valor_soma(&h, 200, total).expect("possível");
        let b = p_valor_soma(&h, 200, total).expect("possível");
        assert_eq!(a.probabilidade.to_bits(), b.probabilidade.to_bits());
        assert_eq!(a.log10.to_bits(), b.log10.to_bits());
    }
}

/// Custo no pior caso realista. Rode em release:
/// `cargo test --release custo_do_p_valor -- --ignored --nocapture`.
#[test]
#[ignore = "medição de tempo; rodar em release"]
fn custo_do_p_valor_no_pior_caso_realista() {
    for (slug, k, t) in [
        ("lotofacil", 15, 3415),
        ("lotofacil", 10, 3415),
        ("megasena", 6, 2758),
        ("megasena", 30, 2758),
        ("quina", 5, 6420),
        ("quina", 40, 6420),
        ("quina", 79, 6420),
    ] {
        let h = Hipergeometrica::da_loteria(cfg(slug), k).expect("k válido");
        let media = (t as f64 * h.media()).round() as u64;
        let maximo = t as u64 * u64::from(h.maximo());
        for total in [media - 1, media + 1, media + media / 20, maximo - 1] {
            let total = total.min(maximo);
            let inicio = std::time::Instant::now();
            let c = p_valor_soma(&h, t, total).expect("possível");
            println!(
                "{slug} k = {k} T = {t} total = {total}: {:?} em {:?}",
                c,
                inicio.elapsed()
            );
        }
    }
}

// ------------------------------------------------ acertos e resumo

#[test]
fn acertos_e_resumo_conferem_com_a_conta_a_mao() {
    // serie_mini (N = 4, K = 2): avaliados os concursos 2 e 3.
    // - 2: posições (dezena→posição) 1→2, 2→1, 3→3, 4→4; saíram 2 e 3;
    // - 3: posições 1→1, 2→2, 3→3, 4→4; saíram 1 e 4;
    // - 1 fica de fora (primeiro) e 4 (snapshot) também.
    let serie = serie_mini();
    let acertos = |k| {
        acertos_por_concurso(&MINI, &serie, k, u32::MAX)
            .expect("série válida")
            .iter()
            .map(|a| (a.concurso, a.acertos))
            .collect::<Vec<_>>()
    };
    // top-1: dezena 2 no 2 (saiu), dezena 1 no 3 (saiu).
    assert_eq!(acertos(1), [(2, 1), (3, 1)]);
    // top-2: {1, 2} nos dois; saiu uma em cada.
    assert_eq!(acertos(2), [(2, 1), (3, 1)]);
    // top-3: {1, 2, 3}; no 2 saíram 2 e 3, no 3 só a 1.
    assert_eq!(acertos(3), [(2, 2), (3, 1)]);

    // k = 1: média 1 contra 1·2/4 = 0,5; d_t = (0,5, 0,5), EP 0;
    // P(S ≥ 2) com T = 2 e p = 1/2: 1/4.
    let r = avaliar_topk(&MINI, &serie, 1, u32::MAX).expect("série válida");
    assert_eq!(r.k, 1);
    assert_eq!(r.concursos_avaliados, 2);
    assert_eq!((r.primeiro_concurso, r.ultimo_concurso), (2, 3));
    assert_eq!(r.total_acertos, 2);
    assert_eq!(r.acertos.perfil, 1.0);
    assert_eq!(r.acertos.acaso, 0.5);
    assert_eq!(r.acertos.diferenca, 0.5);
    assert_eq!(r.acertos.erro_padrao, Some(0.0));
    assert_eq!(
        r.acertos.ic95,
        Some(Intervalo {
            inferior: 0.5,
            superior: 0.5
        })
    );
    assert_eq!(
        r.distribuicao,
        [
            FaixaAcertos {
                acertos: 0,
                observados: 0,
                esperados: 1.0
            },
            FaixaAcertos {
                acertos: 1,
                observados: 2,
                esperados: 1.0
            },
        ]
    );
    assert_perto!(r.p_valor.probabilidade, 0.25);

    // k = 2: média 1 contra 1; acertos 0, 1, 2 esperados 2·(1, 4, 1)/6;
    // S = 0..=4 com (1, 8, 18, 8, 1)/36, então P(S ≥ 2) = 27/36.
    let r = avaliar_topk(&MINI, &serie, 2, u32::MAX).expect("série válida");
    assert_eq!(r.acertos.perfil, 1.0);
    assert_eq!(r.acertos.acaso, 1.0);
    assert_eq!(r.acertos.diferenca, 0.0);
    let observados: Vec<usize> = r.distribuicao.iter().map(|f| f.observados).collect();
    assert_eq!(observados, [0, 2, 0]);
    for (f, num) in r.distribuicao.iter().zip([2.0, 8.0, 2.0]) {
        assert_perto!(f.esperados, num / 6.0);
    }
    assert_perto!(r.p_valor.probabilidade, 27.0 / 36.0);

    // k = 3: acertos (2, 1), média 1,5 contra 3·2/4 = 1,5; d_t = (0,5, −0,5),
    // desvio √0,5, EP √0,5/√2 = 0,5, IC ±0,98. Mínimo 1 acerto: a linha do
    // 0 fica com 0 esperados. P(S ≥ 3) com S = 2, 3, 4 (1/4, 1/2, 1/4): 3/4.
    let r = avaliar_topk(&MINI, &serie, 3, u32::MAX).expect("série válida");
    assert_eq!(r.total_acertos, 3);
    assert_eq!(r.acertos.perfil, 1.5);
    assert_eq!(r.acertos.diferenca, 0.0);
    assert_perto!(r.acertos.erro_padrao.expect("T = 2"), 0.5);
    let ic = r.acertos.ic95.expect("T = 2");
    assert_perto!(ic.inferior, -0.98);
    assert_perto!(ic.superior, 0.98);
    let linhas: Vec<(u32, usize, f64)> = r
        .distribuicao
        .iter()
        .map(|f| (f.acertos, f.observados, f.esperados))
        .collect();
    assert_eq!(linhas, [(0, 0, 0.0), (1, 1, 1.0), (2, 1, 1.0)]);
    assert_perto!(r.p_valor.probabilidade, 0.75);
}

#[test]
fn um_concurso_avaliado_fica_sem_erro_padrao() {
    let r = avaliar_topk(&MINI, &serie_mini(), 1, 2).expect("série válida");
    assert_eq!(r.concursos_avaliados, 1);
    assert_eq!(r.acertos.erro_padrao, None);
    assert_eq!(r.acertos.ic95, None);
    // Um concurso, 1 acerto no top-1: P = 1/2.
    assert_perto!(r.p_valor.probabilidade, 0.5);
}

#[test]
fn sem_concursos_para_avaliar_e_erro() {
    for corte in [0_u32, 1] {
        assert!(matches!(
            avaliar_topk(&MINI, &serie_mini(), 2, corte),
            Err(SpiderError::AvaliacaoSemConcursos(_))
        ));
    }
    let curta: Vec<Observacao> = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [1, 2, 3, 4], None),
    ]
    .concat();
    assert!(matches!(
        avaliar_topk(&MINI, &curta, 2, u32::MAX),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        resumir_topk(&MINI, 2, &[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

#[test]
fn resumir_recusa_acertos_impossiveis() {
    // MINI com k = 3: só 1 ou 2 acertos são possíveis.
    for acertos in [0, 3] {
        let serie = [AcertosConcurso {
            concurso: 2,
            acertos,
        }];
        assert!(matches!(
            resumir_topk(&MINI, 3, &serie),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

#[test]
fn entrada_invalida_e_recusada_como_na_1c1() {
    // Buraco, `saiu` NULL no meio e posições repetidas: mesma validação de
    // `calibrar`.
    let buraco: Vec<Observacao> = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(3, [1, 2, 3, 4], Some([1, 2])),
    ]
    .concat();
    let nulo_no_meio: Vec<Observacao> = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [1, 2, 3, 4], None),
        concurso_mini(3, [1, 2, 3, 4], Some([1, 2])),
    ]
    .concat();
    let posicao_repetida: Vec<Observacao> = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [1, 1, 3, 4], Some([1, 2])),
    ]
    .concat();
    for serie in [buraco, nulo_no_meio, posicao_repetida] {
        assert!(matches!(
            avaliar_topk(&MINI, &serie, 2, u32::MAX),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

// ------------------------------------------------------ ponto no tempo

#[test]
fn mudar_o_resultado_de_t_so_muda_os_acertos_de_t() {
    let cfg = cfg("megasena");
    let serie = serie_aleatoria(cfg, 60, 5, true);
    let base = acertos_por_concurso(cfg, &serie, 6, u32::MAX).expect("série válida");
    for t in [2_u32, 30, 59] {
        // Outro sorteio válido em t: as 6 dezenas de posição 1..=6 saem.
        let alterada: Vec<Observacao> = serie
            .iter()
            .map(|o| {
                if o.concurso == t {
                    obs(o.concurso, o.dezena, o.posicao, Some(o.posicao <= 6))
                } else {
                    *o
                }
            })
            .collect();
        let nova = acertos_por_concurso(cfg, &alterada, 6, u32::MAX).expect("série válida");
        assert_eq!(nova.len(), base.len());
        for (a, b) in base.iter().zip(&nova) {
            if a.concurso == t {
                assert_eq!(b.acertos, 6, "concurso {t}");
            } else {
                assert_eq!(a, b, "alterar {t} mudou {}", a.concurso);
            }
        }
    }
}

#[test]
fn nada_acima_do_corte_e_lido() {
    let cfg = cfg("lotofacil");
    let serie = serie_aleatoria(cfg, 40, 8, true);
    let corte = 25;
    let base = avaliar_topk(cfg, &serie, 15, corte).expect("série válida");
    assert_eq!(base.concursos_avaliados, 24);
    assert_eq!((base.primeiro_concurso, base.ultimo_concurso), (2, 25));

    let mut alterada: Vec<Observacao> = serie
        .iter()
        .filter(|o| o.concurso <= corte)
        .copied()
        .collect();
    alterada.extend(
        serie_aleatoria(cfg, 40, 1234, false)
            .into_iter()
            .filter(|o| o.concurso > corte),
    );
    // Concurso acima do corte inválido: todas as dezenas "saíram".
    alterada.extend((1..=25).map(|d| obs(50, d, d, Some(true))));
    assert_eq!(
        avaliar_topk(cfg, &alterada, 15, corte).expect("até o corte é válido"),
        base
    );
}

#[test]
fn acertos_com_corte_menor_sao_prefixo_dos_com_corte_maior() {
    for (slug, semente) in [("quina", 3_u64), ("lotofacil", 11), ("megasena", 29)] {
        let cfg = cfg(slug);
        let k = cfg.qtd_dezenas as u32;
        let serie = serie_aleatoria(cfg, 80, semente, true);
        let maior = acertos_por_concurso(cfg, &serie, k, u32::MAX).expect("série válida");
        assert_eq!(maior.len(), 78, "{slug}: do 2 ao 79");
        for corte in [2_u32, 3, 40, 79, 80] {
            let menor = acertos_por_concurso(cfg, &serie, k, corte).expect("série válida");
            assert_eq!(&maior[..menor.len()], &menor[..], "{slug}, corte {corte}");
            assert_eq!(menor.last().map(|a| a.concurso), Some(corte.min(79)));
        }
    }
}

#[test]
fn mesmo_trecho_da_avaliacao_da_1c1() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let serie = serie_aleatoria(cfg, 50, 40 + i as u64, true);
        for corte in [10_u32, 49, 50, u32::MAX] {
            let topk = acertos_por_concurso(cfg, &serie, 1, corte).expect("série válida");
            let metricas =
                crate::avaliacao::metricas_por_concurso(cfg, &serie, corte).expect("série válida");
            assert_eq!(
                topk.iter().map(|a| a.concurso).collect::<Vec<_>>(),
                metricas.iter().map(|m| m.concurso).collect::<Vec<_>>(),
                "{} corte {corte}",
                cfg.slug
            );
        }
    }
}

#[test]
fn avaliacao_topk_e_deterministica_e_nao_depende_da_ordem() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let k = cfg.qtd_dezenas as u32;
        let serie = serie_aleatoria(cfg, 120, 31 + i as u64, true);
        let a = avaliar_topk(cfg, &serie, k, 100).expect("série válida");
        let b = avaliar_topk(cfg, &serie, k, 100).expect("série válida");
        assert_eq!(a, b, "{}", cfg.slug);
        assert_eq!(
            serde_json::to_string(&a).expect("serializa"),
            serde_json::to_string(&b).expect("serializa")
        );
        let mut embaralhada = serie.clone();
        Gerador(77).embaralhar(&mut embaralhada);
        assert_eq!(
            avaliar_topk(cfg, &embaralhada, k, 100).expect("série válida"),
            a,
            "{}",
            cfg.slug
        );
    }
}

#[test]
fn tres_loterias_com_k_principal_derivado_do_config() {
    // Esperado k·K/N com k = K: Lotofácil 9, Mega-Sena 0,6, Quina 0,3125.
    for (slug, esperado) in [("lotofacil", 9.0), ("megasena", 0.6), ("quina", 0.3125)] {
        let cfg = cfg(slug);
        let k = cfg.qtd_dezenas as u32;
        let serie = serie_aleatoria(cfg, 150, 17, true);
        let r = avaliar_topk(cfg, &serie, k, u32::MAX).expect("série válida");
        assert_eq!(r.concursos_avaliados, 148, "{slug}");
        assert_perto!(r.acertos.acaso, esperado, 1e-13);
        assert_eq!(r.distribuicao.len(), k as usize + 1);
        let observados: usize = r.distribuicao.iter().map(|f| f.observados).sum();
        assert_eq!(observados, 148);
        let esperados: f64 = r.distribuicao.iter().map(|f| f.esperados).sum();
        assert_perto!(esperados, 148.0, 1e-12);
        let total: u64 = r
            .distribuicao
            .iter()
            .map(|f| u64::from(f.acertos) * f.observados as u64)
            .sum();
        assert_eq!(total, r.total_acertos);
        assert_perto!(r.acertos.perfil, r.total_acertos as f64 / 148.0);
        // Posições ao acaso: o perfil não deve se afastar muito do acaso.
        assert!(r.p_valor.probabilidade > 1e-4, "{slug}: {:?}", r.p_valor);
    }
}

#[test]
fn ranking_que_sempre_acerta_tem_p_valor_minusculo() {
    // Top-K sempre igual ao sorteio: p-valor = p(máx)^T, em log.
    let cfg = cfg("quina");
    let mut serie = Vec::new();
    for c in 1..=200_u32 {
        for d in cfg.dezenas() {
            // Saem as dezenas 1..=5, que estão sempre nas posições 1..=5.
            serie.push(obs(c, d, d, Some(d <= 5)));
        }
    }
    let r = avaliar_topk(cfg, &serie, 5, u32::MAX).expect("série válida");
    assert_eq!(r.total_acertos, 5 * 199);
    assert_eq!(r.p_valor.probabilidade, 0.0);
    assert_perto!(
        r.p_valor.log10,
        199.0 * (1.0 / 24_040_016.0_f64).log10(),
        1e-12
    );
    assert!(r.acertos.diferenca > 4.0);
}

// ======================================================================
// Casos de referência e bordas adicionais (1c.2)
// ======================================================================

// ------------------------------------- pmf: referência independente

#[test]
fn hipergeometrica_n10_k3_escolhidas4_contra_math_comb() {
    // Python: casos[i] = comb(3, i)·comb(7, 4 − i), total = comb(10, 4):
    //   i = 0: 1·35 = 35; i = 1: 3·35 = 105; i = 2: 3·21 = 63; i = 3: 1·7 = 7;
    //   soma 210 = comb(10, 4). Mínimo max(0, 4 + 3 − 10) = 0, máximo 3.
    let h = Hipergeometrica::nova(10, 3, 4).expect("válida");
    assert_eq!(h.total(), 210);
    assert_eq!((h.minimo(), h.maximo()), (0, 3));
    assert_eq!(
        [h.casos(0), h.casos(1), h.casos(2), h.casos(3)],
        [35, 105, 63, 7]
    );
    assert_eq!(h.casos(4), 0);
    assert_eq!(h.media(), 1.2); // 4·3/10
}

#[test]
fn hipergeometrica_n25_k15_escolhidas20_tem_minimo_dez() {
    // Python: comb(15, i)·comb(10, 20 − i) para i = 10..=15 e comb(25, 20):
    //   [3003, 13650, 20475, 12600, 3150, 252], total 53130 (a soma bate).
    // Mínimo 20 + 15 − 25 = 10: abaixo disso `comb(10, 20 − i)` é zero.
    let h = Hipergeometrica::nova(25, 15, 20).expect("válida");
    assert_eq!(h.total(), 53_130);
    assert_eq!((h.minimo(), h.maximo()), (10, 15));
    for i in 0..10 {
        assert_eq!(h.casos(i), 0, "i = {i}");
    }
    let esperados = [3_003_u128, 13_650, 20_475, 12_600, 3_150, 252];
    for (i, esperado) in (10..=15).zip(esperados) {
        assert_eq!(h.casos(i), esperado, "i = {i}");
    }
    assert_eq!(h.casos(16), 0);
    assert_eq!(h.media(), 12.0); // 20·15/25
}

#[test]
fn hipergeometrica_e_simetrica_em_escolhidas_e_sorteadas_para_todo_k() {
    // P(i; N, K, k) = P(i; N, k, K): C(K,i)·C(N−K,k−i)/C(N,k) é simétrica em
    // K e k. Os denominadores C(N,k) e C(N,K) diferem, então a igualdade é de
    // frações: comparadas reduzidas pelo mdc, exatas em inteiros (o produto
    // cruzado estouraria u128 na Quina). `minimo`, `maximo` e `media` coincidem.
    for cfg in config::LOTERIAS {
        let n = cfg.qtd_dezenas_possiveis();
        let kk = cfg.qtd_dezenas as u32;
        for k in 1..n {
            let a = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
            let b = Hipergeometrica::nova(n, k, kk).expect("válida");
            assert_eq!(a.minimo(), b.minimo(), "{} k = {k}", cfg.slug);
            assert_eq!(a.maximo(), b.maximo(), "{} k = {k}", cfg.slug);
            assert_eq!(a.media(), b.media(), "{} k = {k}", cfg.slug);
            let reduzida = |h: &Hipergeometrica, i: u32| {
                let g = mdc(h.casos(i), h.total());
                (h.casos(i) / g, h.total() / g)
            };
            for i in 0..=a.maximo() + 1 {
                assert_eq!(
                    reduzida(&a, i),
                    reduzida(&b, i),
                    "{} k = {k}, i = {i}",
                    cfg.slug
                );
            }
        }
    }
}

// ------------------------------------- p-valor em frações exatas

/// Confere P(S ≥ T·mínimo + j) = `numeradores[j] / denominador` para todo `j`
/// e que um acima do máximo é recusado (`numeradores` cobre `0..=T·(máx−mín)`).
fn confere_caudas_inteiras(
    n: u32,
    sorteadas: u32,
    escolhidas: u32,
    t: usize,
    numeradores: &[u64],
    denominador: f64,
) {
    let h = Hipergeometrica::nova(n, sorteadas, escolhidas).expect("válida");
    let base = t as u64 * u64::from(h.minimo());
    for (j, &num) in numeradores.iter().enumerate() {
        let total = base + j as u64;
        let esperado = num as f64 / denominador;
        let c = p_valor_soma(&h, t, total).expect("possível");
        assert_perto!(c.probabilidade, esperado, 1e-12);
        assert_perto!(c.log10, esperado.log10(), 1e-12);
        assert!(c.probabilidade <= 1.0, "total {total}: {c:?}");
    }
    let acima = base + numeradores.len() as u64;
    assert!(matches!(
        p_valor_soma(&h, t, acima),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

#[test]
fn p_valor_t4_com_media_nao_inteira_em_fracoes_exatas() {
    // N = 5, K = 2, k = 2: casos (3, 6, 1) de 10 (C(2,i)·C(3,2−i)), média
    // 0,8. T = 4: média da soma 3,2 (não inteira), então totais ≤ 3 usam o
    // ramo sem inclinação (total·5 ≤ 4·2·2 = 16 vale até 3) e ≥ 4 o inclinado.
    // Python: (3 + 6x + x²)^4 = [81, 648, 2052, 3240, 2646, 1080, 228, 24, 1],
    // com denominador 10^4; caudas P(S ≥ s) em 10^4, s = 0..=8:
    let caudas = [10_000, 9_919, 9_271, 7_219, 3_979, 1_333, 253, 25, 1];
    confere_caudas_inteiras(5, 2, 2, 4, &caudas, 10_000.0);

    let h = Hipergeometrica::nova(5, 2, 2).expect("válida");
    let dist = distribuicao_soma(&h, 4);
    let pontuais = [
        81.0, 648.0, 2052.0, 3240.0, 2646.0, 1080.0, 228.0, 24.0, 1.0,
    ];
    assert_eq!(dist.len(), pontuais.len());
    for (s, num) in pontuais.iter().enumerate() {
        assert_perto!(dist[s], num / 10_000.0);
    }
}

#[test]
fn p_valor_t5_com_total_igual_a_media_em_fracoes_exatas() {
    // Mesma hipergeométrica, T = 5: média da soma 4 exata. Total 4 cai no
    // ramo sem inclinação (4·5 = 20 ≤ 5·2·2 = 20, a fronteira) e total 5 no
    // inclinado. Python: (3 + 6x + x²)^5, denominador 10^5; caudas s = 0..=10:
    let caudas = [
        100_000, 99_757, 97_327, 87_202, 64_522, 35_092, 12_736, 2_926, 406, 31, 1,
    ];
    confere_caudas_inteiras(5, 2, 2, 5, &caudas, 100_000.0);
}

#[test]
fn p_valor_com_minimo_positivo_em_fracoes_exatas() {
    // N = 5, K = 3, k = 4: mínimo 4 + 3 − 5 = 2, máximo 3; casos (3, 2) de 5
    // (C(3,2)·C(2,2) e C(3,3)·C(2,1)). T = 4: o excedente sobre o mínimo é
    // Binomial(4, 2/5); média da soma 4·4·3/5 = 9,6 (totais 8..=12).
    // Python: (3 + 2x)^4 = [81, 216, 216, 96, 16] sobre 5^4 = 625; caudas
    // [625, 544, 328, 112, 16]: total 8 (= T·mín) vale 1, total 9 usa o ramo
    // sem inclinação (45 ≤ 48), 10 a 12 o inclinado (50 > 48); 12 é o máximo.
    confere_caudas_inteiras(5, 3, 4, 4, &[625, 544, 328, 112, 16], 625.0);
    let h = Hipergeometrica::nova(5, 3, 4).expect("válida");
    assert_eq!(p_valor_soma(&h, 4, 8).expect("possível").probabilidade, 1.0);
    assert_eq!(p_valor_soma(&h, 4, 0).expect("possível").probabilidade, 1.0);
}

// ------------------------------------------------------- invariâncias

#[test]
fn cauda_mais_cumulativa_da_distribuicao_soma_vale_um() {
    // P(S ≥ s) + P(S ≤ s − 1) = 1; o segundo termo vem de `distribuicao_soma`
    // (convolução sem inclinação), o primeiro de `p_valor_soma`.
    for (slug, k, t) in [
        ("megasena", 6, 30),
        ("lotofacil", 15, 20),
        ("lotofacil", 9, 25),
        ("quina", 10, 25),
    ] {
        let h = Hipergeometrica::da_loteria(cfg(slug), k).expect("k válido");
        let dist = distribuicao_soma(&h, t);
        for s in 0..dist.len() {
            let cauda = p_valor_soma(&h, t, s as u64).expect("possível");
            let abaixo: f64 = dist[..s].iter().sum();
            assert!(
                (cauda.probabilidade + abaixo - 1.0).abs() <= 1e-10,
                "{slug} k = {k}, T = {t}, s = {s}: {} + {abaixo}",
                cauda.probabilidade
            );
        }
    }
}

#[test]
fn distribuicao_soma_com_t_um_e_a_propria_pmf() {
    for cfg in config::LOTERIAS {
        let n = cfg.qtd_dezenas_possiveis();
        let kk = cfg.qtd_dezenas as u32;
        for k in [1, 2, kk, n / 2, n - kk, n - 1] {
            let h = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
            let d = distribuicao_soma(&h, 1);
            assert_eq!(d.len(), h.maximo() as usize + 1, "{} k = {k}", cfg.slug);
            for (i, x) in d.iter().enumerate() {
                if (i as u32) < h.minimo() {
                    assert_eq!(*x, 0.0, "{} k = {k}, i = {i}", cfg.slug);
                } else {
                    // t = 1: cada posição é `1.0 · pmf(i)`, exata.
                    assert_eq!(*x, h.pmf(i as u32), "{} k = {k}, i = {i}", cfg.slug);
                }
            }
        }
    }
}

// ----------------------------------------------- bordas de acertos/avaliar

/// MINI a partir do concurso 3 (o 1 e o 2 não estão na entrada):
/// - 3: posições 1,2,3,4; saíram 1 e 2 (se fosse avaliado, top-1 = dezena 1
///   acertaria);
/// - 4: posições 2,1,3,4; saíram 1 e 3;
/// - 5: posições 3,1,2,4; saíram 2 e 4.
fn serie_mini_a_partir_do_3() -> Vec<Observacao> {
    [
        concurso_mini(3, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(4, [2, 1, 3, 4], Some([1, 3])),
        concurso_mini(5, [3, 1, 2, 4], Some([2, 4])),
    ]
    .concat()
}

#[test]
fn entrada_a_partir_do_concurso_3_nao_avalia_o_3() {
    let serie = serie_mini_a_partir_do_3();
    let acertos = |k, corte| {
        acertos_por_concurso(&MINI, &serie, k, corte)
            .expect("série válida")
            .iter()
            .map(|a| (a.concurso, a.acertos))
            .collect::<Vec<_>>()
    };
    // top-1 (dezena de posição 1): no 4 é a dezena 2, que não saiu (0); no 5
    // é a dezena 2, que saiu (1). O concurso 3 ficaria com 1 se fosse lido.
    assert_eq!(acertos(1, u32::MAX), [(4, 0), (5, 1)]);
    // top-3 (posições ≤ 3): no 4 são as dezenas 1, 2, 3 e saíram 1 e 3 (2); no
    // 5 são 1, 2, 3 e saiu só a 2 (1).
    assert_eq!(acertos(3, u32::MAX), [(4, 2), (5, 1)]);
    assert_eq!(acertos(1, 4), [(4, 0)]);

    let r = avaliar_topk(&MINI, &serie, 1, u32::MAX).expect("série válida");
    assert_eq!(r.concursos_avaliados, 2);
    assert_eq!((r.primeiro_concurso, r.ultimo_concurso), (4, 5));
    assert_eq!(r.total_acertos, 1);
    // P(S ≥ 1) com T = 2 e p = 1/2 por concurso: 3/4.
    assert_perto!(r.p_valor.probabilidade, 0.75);
}

#[test]
fn corte_no_primeiro_concurso_da_entrada_nao_deixa_nada_para_avaliar() {
    // O primeiro concurso da entrada (3) é o corte: `acertos_por_concurso`
    // devolve a série vazia (a entrada é válida) e `avaliar_topk` recusa com
    // AvaliacaoSemConcursos. Corte abaixo do primeiro: mesma coisa.
    let serie = serie_mini_a_partir_do_3();
    for corte in [3_u32, 2, 0] {
        assert_eq!(
            acertos_por_concurso(&MINI, &serie, 2, corte).expect("entrada válida"),
            []
        );
        assert!(
            matches!(
                avaliar_topk(&MINI, &serie, 2, corte),
                Err(SpiderError::AvaliacaoSemConcursos(_))
            ),
            "corte {corte}"
        );
    }
}

#[test]
fn entrada_vazia_da_serie_vazia_e_avaliar_recusa_sem_concursos() {
    // Comportamento real: sem observações não há o que validar, então
    // `acertos_por_concurso` devolve Ok(vazio); `avaliar_topk` transforma isso
    // em AvaliacaoSemConcursos (nada a avaliar).
    assert_eq!(
        acertos_por_concurso(&MINI, &[], 2, u32::MAX).expect("vazio é válido"),
        []
    );
    assert!(matches!(
        avaliar_topk(&MINI, &[], 2, u32::MAX),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    // k inválido continua tendo prioridade sobre a série vazia.
    assert!(matches!(
        avaliar_topk(&MINI, &[], 0, u32::MAX),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

#[test]
fn k_um_e_k_n_menos_um_na_megasena_contra_a_conta_independente() {
    // top-1: acerta 1 se a dezena de posição 1 saiu. top-59: o único de fora
    // é a dezena de posição 60, então acertos = 6 − [ela saiu].
    let cfg = cfg("megasena");
    let serie = serie_aleatoria(cfg, 40, 21, true);
    let saiu_na_posicao = |concurso: u32, posicao: u32| {
        let o = serie
            .iter()
            .find(|o| o.concurso == concurso && o.posicao == posicao)
            .expect("posição presente");
        u32::from(o.saiu == Some(true))
    };
    let topo1 = acertos_por_concurso(cfg, &serie, 1, u32::MAX).expect("série válida");
    let topo59 = acertos_por_concurso(cfg, &serie, 59, u32::MAX).expect("série válida");
    assert_eq!(topo1.len(), 38, "do 2 ao 39; o 40 é snapshot");
    assert_eq!(topo59.len(), 38);
    for (a, b) in topo1.iter().zip(&topo59) {
        assert_eq!(a.concurso, b.concurso);
        assert_eq!(
            a.acertos,
            saiu_na_posicao(a.concurso, 1),
            "top-1, {}",
            a.concurso
        );
        assert_eq!(
            b.acertos,
            6 - saiu_na_posicao(a.concurso, 60),
            "top-59, {}",
            a.concurso
        );
    }

    // Resumo do top-59: mínimo 59 + 6 − 60 = 5, máximo 6; as linhas 0..=4
    // ficam com 0 esperados e 0 observados; média 59·6/60 = 5,9.
    let r = resumir_topk(cfg, 59, &topo59).expect("acertos possíveis");
    assert_eq!(r.distribuicao.len(), 7);
    for f in &r.distribuicao[..5] {
        assert_eq!(
            (f.observados, f.esperados),
            (0, 0.0),
            "acertos {}",
            f.acertos
        );
    }
    // 5 acertos: C(6,5)·C(54,54) = 6 casos de C(60,59) = 60; 6 acertos:
    // C(6,6)·C(54,53) = 54 casos de 60.
    assert_perto!(r.distribuicao[5].esperados, 38.0 * 6.0 / 60.0);
    assert_perto!(r.distribuicao[6].esperados, 38.0 * 54.0 / 60.0);
    assert_perto!(r.acertos.acaso, 5.9);
}

#[test]
fn snapshot_nao_sorteado_no_fim_e_ignorado() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let k = cfg.qtd_dezenas as u32;
        let com = serie_aleatoria(cfg, 30, 90 + i as u64, true);
        let sem: Vec<Observacao> = com.iter().filter(|o| o.concurso < 30).copied().collect();
        assert_eq!(
            avaliar_topk(cfg, &com, k, u32::MAX).expect("série válida"),
            avaliar_topk(cfg, &sem, k, u32::MAX).expect("série válida"),
            "{}",
            cfg.slug
        );
    }
}

#[test]
fn dezena_fora_da_faixa_e_recusada_mesmo_no_concurso_nao_avaliado() {
    // MINI aceita 1..=4. Dezena 5 (acima) ou 0 (abaixo), no concurso 2
    // (avaliado) ou no 1 (o primeiro, que não entra na avaliação): recusada.
    for fora in [5_u32, 0] {
        let invalido = |concurso: u32| {
            // O grupo do concurso inválido troca a dezena 4 por `fora`.
            let linhas = |c: u32| {
                let dezenas = if c == concurso {
                    [1, 2, 3, fora]
                } else {
                    [1, 2, 3, 4]
                };
                dezenas
                    .iter()
                    .zip([2, 1, 3, 4])
                    .map(|(&d, p)| obs(c, d, p, Some(d == 1 || d == 2)))
                    .collect::<Vec<_>>()
            };
            [linhas(1), linhas(2), linhas(3)].concat()
        };
        for concurso in [1_u32, 2] {
            assert!(
                matches!(
                    avaliar_topk(&MINI, &invalido(concurso), 2, u32::MAX),
                    Err(SpiderError::AvaliacaoEntradaInvalida(_))
                ),
                "dezena {fora} no concurso {concurso}"
            );
        }
        // Acima do corte não é lida: com corte 1 o concurso 2 inválido passa
        // a validação (e não há o que avaliar).
        assert!(matches!(
            avaliar_topk(&MINI, &invalido(2), 2, 1),
            Err(SpiderError::AvaliacaoSemConcursos(_))
        ));
    }
}

// ------------------------------------------------------------- resumo

#[test]
fn distribuicao_do_resumo_cobre_zero_ate_o_maximo_e_zera_abaixo_do_minimo() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let n = cfg.qtd_dezenas_possiveis();
        let kk = cfg.qtd_dezenas as u32;
        let serie = serie_aleatoria(cfg, 40, 300 + i as u64, false);
        for k in [1, 3, kk, n / 2, n - kk, n - 1] {
            let r = avaliar_topk(cfg, &serie, k, u32::MAX).expect("série válida");
            let h = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
            let t = r.concursos_avaliados;
            let rotulo = format!("{} k = {k}", cfg.slug);
            assert_eq!(t, 39, "{rotulo}: do 2 ao 40");
            assert_eq!(r.distribuicao.len(), k.min(kk) as usize + 1, "{rotulo}");
            for (j, f) in r.distribuicao.iter().enumerate() {
                assert_eq!(f.acertos, j as u32, "{rotulo}");
                if f.acertos < h.minimo() {
                    assert_eq!((f.observados, f.esperados), (0, 0.0), "{rotulo}, {j}");
                }
            }
            let esperados: f64 = r.distribuicao.iter().map(|f| f.esperados).sum();
            assert_perto!(esperados, t as f64, 1e-12);
            let observados: usize = r.distribuicao.iter().map(|f| f.observados).sum();
            assert_eq!(observados, t, "{rotulo}");

            let acertos = acertos_por_concurso(cfg, &serie, k, u32::MAX).expect("série válida");
            let soma: u64 = acertos.iter().map(|a| u64::from(a.acertos)).sum();
            assert_eq!(r.total_acertos, soma, "{rotulo}");
            let ponderado: u64 = r
                .distribuicao
                .iter()
                .map(|f| u64::from(f.acertos) * f.observados as u64)
                .sum();
            assert_eq!(ponderado, soma, "{rotulo}");
            assert!(
                (0.0..=1.0).contains(&r.p_valor.probabilidade)
                    && r.p_valor.log10.is_finite()
                    && r.p_valor.log10 <= 0.0,
                "{rotulo}: {:?}",
                r.p_valor
            );
        }
    }
}

#[test]
fn ic95_do_resumo_e_diferenca_mais_menos_z_vezes_o_erro_padrao() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let k = cfg.qtd_dezenas as u32;
        let serie = serie_aleatoria(cfg, 61, 500 + i as u64, true);
        let r = avaliar_topk(cfg, &serie, k, u32::MAX).expect("série válida");
        let h = Hipergeometrica::da_loteria(cfg, k).expect("k válido");
        let acertos = acertos_por_concurso(cfg, &serie, k, u32::MAX).expect("série válida");
        let t = acertos.len() as f64;
        assert_eq!(acertos.len(), 59, "do 2 ao 60");

        // Erro padrão por conta própria: d_t = acertos_t − k·K/N.
        let d: Vec<f64> = acertos
            .iter()
            .map(|a| f64::from(a.acertos) - h.media())
            .collect();
        let media = d.iter().sum::<f64>() / t;
        let var = d.iter().map(|x| (x - media) * (x - media)).sum::<f64>() / (t - 1.0);
        let c = r.acertos;
        assert_perto!(c.diferenca, media);
        let ep = c.erro_padrao.expect("T ≥ 2");
        assert_perto!(ep, (var / t).sqrt());
        // A relação do IC é exata (mesma aritmética, sem arredondamento extra).
        assert_eq!(
            c.ic95,
            Some(Intervalo {
                inferior: c.diferenca - crate::avaliacao::Z_95 * ep,
                superior: c.diferenca + crate::avaliacao::Z_95 * ep,
            }),
            "{}",
            cfg.slug
        );
        assert_perto!(c.acaso, h.media());
    }
}

#[test]
fn resumir_serie_fora_de_ordem_usa_menor_e_maior_concurso() {
    // Lotofácil, k = 15 (acertos possíveis 5..=15), concursos fora de ordem.
    // T = 3, total 27 (= média 9 por concurso); d_t = (0, 1, −1) na ordem
    // dada: média 0, variância amostral 2/2 = 1, EP = 1/√3.
    let serie =
        [(7, 9), (3, 10), (5, 8)].map(|(concurso, acertos)| AcertosConcurso { concurso, acertos });
    let r = resumir_topk(cfg("lotofacil"), 15, &serie).expect("acertos possíveis");
    assert_eq!((r.primeiro_concurso, r.ultimo_concurso), (3, 7));
    assert_eq!(r.concursos_avaliados, 3);
    assert_eq!(r.total_acertos, 27);
    assert_eq!(r.acertos.perfil, 9.0);
    assert_eq!(r.acertos.diferenca, 0.0);
    assert_perto!(r.acertos.erro_padrao.expect("T = 3"), 1.0 / 3.0_f64.sqrt());
    let observados: Vec<usize> = r.distribuicao.iter().map(|f| f.observados).collect();
    assert_eq!(observados, [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0]);
    // Total na média: a cauda é a de um total típico, não a de um extremo.
    assert!(r.p_valor.probabilidade > 0.3 && r.p_valor.probabilidade < 0.8);
}

// ------------------------------------------------ caudas extremas (Mega)

#[test]
fn megasena_t2758_no_maximo_e_perto_do_maximo_em_formula_fechada() {
    // T = 2758 (concursos reais da Mega-Sena), k = 6, p6 = 1/50063860,
    // p5 = 324/50063860, p4 = 21465/50063860.
    // - total = máx (16548): p6^T, ~10^−21234;
    // - total = máx − 1: p6^(T−1)·(p6 + T·p5);
    // - total = máx − 2: ou um concurso com 4 (T·p4) ou dois com 5
    //   (C(T,2)·p5²), além de todos com 6 (relativo a p6^T: 1, T·324,
    //   C(T,2)·324², T·21465).
    let h = Hipergeometrica::da_loteria(cfg("megasena"), 6).expect("k válido");
    let t = 2758_usize;
    let tf = t as f64;
    let lp6 = (1.0_f64 / 50_063_860.0).log10();
    let topo = 6 * t as u64;

    let c = p_valor_soma(&h, t, topo).expect("possível");
    assert_eq!(c.probabilidade, 0.0);
    assert_perto!(c.log10, tf * lp6, 1e-12);

    let c = p_valor_soma(&h, t, topo - 1).expect("possível");
    assert_eq!(c.probabilidade, 0.0);
    let (p6, p5) = (1.0_f64 / 50_063_860.0, 324.0_f64 / 50_063_860.0);
    assert_perto!(c.log10, (tf - 1.0) * lp6 + (p6 + tf * p5).log10(), 1e-10);

    let c = p_valor_soma(&h, t, topo - 2).expect("possível");
    assert_eq!(c.probabilidade, 0.0);
    let relativo = 1.0 + tf * 324.0 + tf * (tf - 1.0) / 2.0 * 324.0 * 324.0 + tf * 21_465.0;
    assert_perto!(c.log10, tf * lp6 + relativo.log10(), 1e-10);
}

#[test]
fn megasena_cauda_no_limite_do_f64_nunca_e_nan_nem_negativa() {
    // p6^T cruza o menor normal (~2,2e−308) em T ≈ 40 e o menor subnormal
    // (~4,9e−324) em T ≈ 42. Nessa faixa `probabilidade` é normal, subnormal
    // ou 0, mas nunca NaN nem negativa, e `log10` segue finito e exato.
    let h = Hipergeometrica::da_loteria(cfg("megasena"), 6).expect("k válido");
    let (p6, p5) = (1.0_f64 / 50_063_860.0, 324.0_f64 / 50_063_860.0);
    let lp6 = p6.log10();
    for t in [30_usize, 38, 39, 40, 41, 42, 43, 60] {
        let tf = t as f64;
        let topo = 6 * t as u64;
        for (total, log10) in [
            (topo, tf * lp6),
            (topo - 1, (tf - 1.0) * lp6 + (p6 + tf * p5).log10()),
        ] {
            let c = p_valor_soma(&h, t, total).expect("possível");
            let rotulo = format!("T = {t}, total {total}: {c:?}");
            assert!(
                !c.probabilidade.is_nan() && c.probabilidade >= 0.0,
                "{rotulo}"
            );
            assert!(c.probabilidade <= 1.0, "{rotulo}");
            assert!(c.log10.is_finite() && c.log10 < 0.0, "{rotulo}");
            assert_perto!(c.log10, log10, 1e-10);
            if log10 > -300.0 {
                assert_perto!(c.probabilidade, 10f64.powf(log10), 1e-9);
            } else if log10 > -322.0 {
                // Normal baixo ou subnormal (pode perder precisão): positiva.
                assert!(c.probabilidade > 0.0, "{rotulo}");
            } else if log10 < -325.0 {
                assert_eq!(c.probabilidade, 0.0, "{rotulo}");
            }
        }
    }
}
