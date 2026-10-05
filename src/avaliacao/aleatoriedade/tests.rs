use super::*;
use crate::avaliacao::tests::{Gerador, MINI};
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn perto(a: f64, b: f64, tolerancia: f64) -> bool {
    (a - b).abs() <= tolerancia * a.abs().max(b.abs())
}

/// Tolerância **relativa** (sem piso absoluto: as caudas pequenas também
/// são conferidas em precisão relativa).
macro_rules! assert_perto {
    ($a:expr, $b:expr) => {
        assert_perto!($a, $b, 1e-12)
    };
    ($a:expr, $b:expr, $tol:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(
            perto(a, b, $tol),
            "{} = {a:?}, esperado {b:?} (erro relativo {:e})",
            stringify!($a),
            (a - b).abs() / a.abs().max(b.abs())
        );
    }};
}

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: "2020-01-01".to_string(),
        dezenas: dezenas.to_vec(),
    }
}

/// Histórico da MINI (2 de 1..=4) a partir do concurso 1.
fn historico_mini(sorteios: &[[u32; 2]]) -> Vec<Resultado> {
    sorteios
        .iter()
        .zip(1..)
        .map(|(d, c)| res(&MINI, c, d))
        .collect()
}

/// Histórico ao acaso: `concursos` sorteios de *K* de *N* (embaralhamento de
/// Fisher–Yates com o xorshift dos testes), concursos 1..=`concursos`.
fn historico_aleatorio(cfg: &LoteriaConfig, concursos: u32, g: &mut Gerador) -> Vec<Resultado> {
    (1..=concursos)
        .map(|c| {
            let mut dezenas: Vec<u32> = cfg.dezenas().collect();
            g.embaralhar(&mut dezenas);
            dezenas.truncate(cfg.qtd_dezenas);
            res(cfg, c, &dezenas)
        })
        .collect()
}

/// Detalhe do erro de um histórico `resultados` inválido.
fn erro_de(cfg: &LoteriaConfig, resultados: &[Resultado]) -> String {
    match testar_aleatoriedade(cfg, resultados) {
        Err(SpiderError::AleatoriedadeEntradaInvalida(m)) => m,
        outro => panic!("esperado AleatoriedadeEntradaInvalida, veio {outro:?}"),
    }
}

/// Detalhe do erro de configuração (ou argumento) inválido.
fn erro_de_config(cfg: &LoteriaConfig, resultados: &[Resultado]) -> String {
    match testar_aleatoriedade(cfg, resultados) {
        Err(SpiderError::AvaliacaoEntradaInvalida(m)) => m,
        outro => panic!("esperado AvaliacaoEntradaInvalida, veio {outro:?}"),
    }
}

// ------------------------------------------------------ funções especiais
//
// Referências independentes do algoritmo daqui (série + fração contínua):
// geradas em Python com `decimal` a 1200 dígitos pelas formas fechadas da
// gama incompleta com `a` inteiro ou semi-inteiro,
//   Q(m, x)       = e^{−x}·Σ_{j<m} x^j/j!,
//   Q(m + 1/2, x) = erfc(√x) + e^{−x}·Σ_{j<m} x^{j+1/2}/Γ(j + 3/2),
// com π por Machin e erfc = 1 − erf pela série de Taylor (a precisão alta
// absorve o cancelamento até |z| = 37). `P(χ²_gl ≥ x) = Q(gl/2, x/2)` e
// `2·Φ(−|z|) = erfc(|z|/√2)`. Os valores críticos tabelados (χ²: 3,841 com
// 1 gl, 7,815 com 3, 36,415 com 24, 77,931 com 59, 100,749 com 79;
// normal: 1,95996) conferem com 5% nessas referências.

#[test]
fn ln_gama_contra_fatoriais_e_raiz_de_pi() {
    // (2a, ln Γ(a)): ln √π, 0, ln(√π/2), ln 11!, ln Γ(29,5), ln Γ(39,5),
    // ln Γ(79,5), em `decimal` a 60 dígitos.
    let referencias = [
        (1, 5.723_649_429_247_001e-1),
        (2, 0.0),
        (3, -1.207_822_376_352_452_2e-1),
        (24, 1.750_230_784_587_388_7e1),
        (59, 6.956_908_092_082_364e1),
        (79, 1.047_967_743_971_583e2),
        (159, 2.671_047_914_568_685_5e2),
    ];
    for (dobro_a, esperado) in referencias {
        let obtido = ln_gama_meio(dobro_a);
        assert!(
            (obtido - esperado).abs() <= 1e-13 * (1.0 + esperado.abs()),
            "ln Γ({dobro_a}/2) = {obtido:?}, esperado {esperado:?}"
        );
    }
    assert_eq!(ln_gama_meio(4), 0.0); // Γ(2) = 1
}

#[test]
fn p_valor_qui_quadrado_contra_referencias() {
    // (x, gl, P(χ²_gl ≥ x))
    let referencias: [(f64, u32, f64); 22] = [
        (3.841_458_820_694_124, 1, 5.000_000_000_000_006e-2),
        (0.5, 1, 4.795_001_221_869_535e-1),
        (2.0, 2, 3.678_794_411_714_423_3e-1),
        (7.814_727_903_251_178, 3, 5.000_000_000_000_004_4e-2),
        (1.0, 3, 8.012_519_569_012_008e-1),
        (1.5, 3, 6.822_703_303_362_125e-1),
        (3.0, 3, 3.916_251_762_710_889_5e-1),
        (5.0, 3, 1.717_971_442_967_331_2e-1),
        (36.415_028_501_807_31, 24, 5.000_000_000_000_004e-2),
        (24.0, 24, 4.615_973_330_636_182e-1),
        (10.0, 24, 9.945_469_080_869_906e-1),
        (100.0, 24, 3.004_353_682_455_061e-11),
        (500.0, 24, 1.667_335_917_819_382_8e-90),
        (77.930_523_805_230_42, 59, 5.000_000_000_000_002e-2),
        (59.0, 59, 4.755_119_972_953_301e-1),
        (20.0, 59, 9.999_995_601_969_331e-1),
        (200.0, 59, 3.165_567_822_815_57e-17),
        (100.7486, 79, 5.000_013_091_587_873e-2),
        (79.0, 79, 4.788_384_304_231_576_6e-1),
        (0.5, 79, 1.0),
        (400.0, 79, 2.043_262_014_739_106_8e-44),
        (1500.0, 79, 3.014_323_086_908_191_8e-261),
    ];
    for (x, gl, esperado) in referencias {
        let obtido = p_valor_qui_quadrado(x, gl).expect("entrada válida");
        assert!(
            perto(obtido, esperado, 1e-12),
            "P(χ²_{gl} ≥ {x}) = {obtido:?}, esperado {esperado:?}"
        );
    }
}

#[test]
fn qui_quadrado_com_2_gl_e_a_exponencial() {
    // Q(1, x/2) = e^{−x/2}: forma fechada, inclusive na cauda.
    for x in [0.01, 0.5, 1.0, 3.9, 4.0, 4.1, 10.0, 50.0, 300.0, 1400.0] {
        assert_perto!(p_valor_qui_quadrado(x, 2).unwrap(), (-x / 2.0).exp());
    }
}

#[test]
fn p_valor_normal_bilateral_contra_referencias() {
    // (z, 2·Φ(−|z|))
    let referencias = [
        (0.0, 1.0),
        (0.5, 6.170_750_774_519_738e-1),
        (1.0, 3.173_105_078_629_141e-1),
        (1.959_963_984_540_054, 5.000_000_000_000_003e-2),
        (3.0, 2.699_796_063_260_189e-3),
        (5.0, 5.733_031_437_583_878e-7),
        (8.0, 1.244_192_114_854_356_8e-15),
        (20.0, 5.507_248_237_212_467_5e-89),
        (37.0, 1.145_114_244_504_915_4e-299),
        (2.0_f64.sqrt(), 1.572_992_070_502_851_3e-1), // erfc(1)
        (0.5_f64.sqrt(), 4.795_001_221_869_535e-1),   // erfc(1/2)
        (1.5_f64.sqrt(), 2.206_713_619_198_468e-1),
    ];
    for (z, esperado) in referencias {
        for sinal in [1.0, -1.0] {
            let obtido = p_valor_normal_bilateral(sinal * z).expect("z válido");
            assert!(
                perto(obtido, esperado, 1e-12),
                "2·Φ(−|{}|) = {obtido:?}, esperado {esperado:?}",
                sinal * z
            );
        }
    }
}

#[test]
fn caudas_infinitas_e_dominio() {
    assert_eq!(p_valor_normal_bilateral(f64::INFINITY).unwrap(), 0.0);
    assert_eq!(p_valor_normal_bilateral(f64::NEG_INFINITY).unwrap(), 0.0);
    assert_eq!(p_valor_normal_bilateral(1e200).unwrap(), 0.0);
    assert_eq!(p_valor_qui_quadrado(f64::INFINITY, 5).unwrap(), 0.0);
    assert_eq!(p_valor_qui_quadrado(0.0, 5).unwrap(), 1.0);
    for invalido in [
        p_valor_normal_bilateral(f64::NAN),
        p_valor_qui_quadrado(f64::NAN, 3),
        p_valor_qui_quadrado(-1e-300, 3),
        p_valor_qui_quadrado(1.0, 0),
    ] {
        assert!(
            matches!(invalido, Err(SpiderError::AvaliacaoEntradaInvalida(_))),
            "{invalido:?}"
        );
    }
}

#[test]
fn gama_q_e_continua_e_decrescente_na_troca_de_serie_para_fracao() {
    // A série vale para x < a + 1 e a fração contínua do ponto em diante:
    // os dois lados da fronteira têm de concordar, e Q tem de cair com x.
    for dobro_a in [1, 2, 3, 24, 59, 79] {
        let a = f64::from(dobro_a) / 2.0;
        let fronteira = a + 1.0;
        let antes = gama_q(dobro_a, fronteira * (1.0 - 1e-12)).unwrap();
        let depois = gama_q(dobro_a, fronteira).unwrap();
        assert_perto!(antes, depois, 1e-10);
        let mut anterior = 1.0;
        for i in 1..=400 {
            let x = f64::from(i) * (a + 10.0) / 100.0;
            let q = gama_q(dobro_a, x).unwrap();
            assert!(
                q <= anterior && (0.0..=1.0).contains(&q),
                "Q({a}, {x}) = {q}, anterior {anterior}"
            );
            anterior = q;
        }
    }
}

// ------------------------------------------------------------------ Holm

#[test]
fn holm_exemplo_classico() {
    // Mesmo resultado de `p.adjust(c(0.01, 0.04, 0.03, 0.005), "holm")` no
    // R. Ordenados: 0,005·4 = 0,02; 0,01·3 = 0,03; 0,03·2 = 0,06;
    // 0,04·1 = 0,04 → sobe para 0,06 (monotonicidade).
    let ajustados = holm(&[0.01, 0.04, 0.03, 0.005]).unwrap();
    let esperados = [0.03, 0.06, 0.06, 0.02];
    for (a, e) in ajustados.iter().zip(esperados) {
        assert_perto!(*a, e, 1e-15);
    }
}

#[test]
fn holm_monotonicidade_e_teto() {
    // 2·0,011 = 0,022 < 3·0,01 = 0,03: o segundo sobe para 0,03.
    let ajustados = holm(&[0.011, 0.5, 0.01]).unwrap();
    for (a, e) in ajustados.iter().zip([0.03, 0.5, 0.03]) {
        assert_perto!(*a, e, 1e-15);
    }
    // Teto 1: 3·0,4 e 2·0,5 passam de 1; 0,9 sobe para 1 pelo máximo.
    assert_eq!(holm(&[0.4, 0.5, 0.9]).unwrap(), vec![1.0, 1.0, 1.0]);
    // p-valor 1 e 0.
    assert_eq!(holm(&[1.0, 0.0]).unwrap(), vec![1.0, 0.0]);
}

#[test]
fn holm_bordas_e_empates() {
    assert!(holm(&[]).unwrap().is_empty());
    assert_eq!(holm(&[0.2]).unwrap(), vec![0.2]);
    // Empate: todos recebem (m − j + 1)·p do primeiro do grupo.
    let empate = holm(&[0.02, 0.02, 0.02]).unwrap();
    for a in &empate {
        assert_perto!(*a, 0.06, 1e-15);
    }
    for invalido in [f64::NAN, -0.1, 1.000_001, f64::INFINITY] {
        assert!(matches!(
            holm(&[0.5, invalido]),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

#[test]
fn holm_propriedades_em_p_valores_aleatorios() {
    let mut g = Gerador(0x9e37_79b9_7f4a_7c15);
    for m in [1usize, 2, 5, 25, 80] {
        let p: Vec<f64> = (0..m)
            .map(|_| (g.proximo() >> 11) as f64 / (1u64 << 53) as f64)
            .collect();
        let ajustados = holm(&p).unwrap();
        let menor = p.iter().copied().fold(1.0, f64::min);
        assert_eq!(
            ajustados.iter().copied().fold(1.0, f64::min),
            (m as f64 * menor).min(1.0),
            "o menor ajustado é o Bonferroni do menor p"
        );
        for i in 0..m {
            assert!(ajustados[i] >= p[i] && ajustados[i] <= 1.0);
            for j in 0..m {
                if p[i] <= p[j] {
                    assert!(ajustados[i] <= ajustados[j], "ordem preservada");
                }
            }
        }
        // Mesma família em outra ordem: mesmos ajustados por p-valor.
        let mut invertido = p.clone();
        invertido.reverse();
        let mut de_novo = holm(&invertido).unwrap();
        de_novo.reverse();
        assert_eq!(de_novo, ajustados);
    }
}

// ------------------------------------------------- qui-quadrado (global)

#[test]
fn qui_quadrado_a_mao_na_mini() {
    // T = 3 concursos {1,2}, {2,3}, {1,2}: O = 2, 3, 1, 0 e E = 3·2/4 = 1,5.
    // Σ(O − E)² = 0,25 + 2,25 + 0,25 + 2,25 = 5; Pearson = 5/1,5 = 10/3;
    // corrigida = (N−1)/(N−K)·10/3 = 3/2·10/3 = 5, com 3 gl.
    let q = qui_quadrado_frequencia(&MINI, &[2, 3, 1, 0], 3).unwrap();
    assert_eq!(q.concursos, 3);
    assert_eq!(q.esperado, 1.5);
    assert_perto!(q.pearson, 10.0 / 3.0, 1e-15);
    assert_eq!(q.estatistica, 5.0);
    assert_eq!(q.graus_liberdade, 3);
    assert_perto!(q.p_valor, 1.717_971_442_967_331_2e-1); // P(χ²_3 ≥ 5)
    assert!(!q.rejeita);
}

#[test]
fn qui_quadrado_com_um_concurso_da_n_menos_1_em_toda_loteria() {
    // Com T = 1, K dezenas têm O = 1 e N−K têm O = 0: Pearson = N−K e a
    // corrigida = N−1, exatas (Σ(N·O − K)² = K·(N−K)·N).
    for l in config::LOTERIAS {
        let n = l.qtd_dezenas_possiveis() as usize;
        let mut freq = vec![0u64; n];
        freq[..l.qtd_dezenas].fill(1);
        let q = qui_quadrado_frequencia(l, &freq, 1).unwrap();
        assert_eq!(q.pearson, (n - l.qtd_dezenas) as f64, "{}", l.slug);
        assert_eq!(q.estatistica, (n - 1) as f64, "{}", l.slug);
        assert_eq!(q.graus_liberdade as usize, n - 1);
    }
}

#[test]
fn qui_quadrado_com_frequencias_iguais_da_zero() {
    let l = cfg("lotofacil");
    // 5 concursos de 15 em 25: 75 saídas, 3 por dezena = E.
    let q = qui_quadrado_frequencia(l, &[3; 25], 5).unwrap();
    assert_eq!((q.esperado, q.pearson, q.estatistica), (3.0, 0.0, 0.0));
    assert_eq!(q.p_valor, 1.0);
}

#[test]
fn qui_quadrado_corrigido_tem_media_exata_n_menos_1() {
    // Enumeração exata de todos os históricos da MINI com T = 2 e T = 3
    // (6^T sorteios igualmente prováveis): a média da corrigida é N−1 = 3
    // e a de Pearson é N−K = 2 — por isso Pearson puro não serve contra
    // χ²(N−1).
    let pares: Vec<[usize; 2]> = (0..4)
        .flat_map(|a| (a + 1..4).map(move |b| [a, b]))
        .collect();
    assert_eq!(pares.len(), 6);
    for t in [2u32, 3] {
        let (mut soma_corrigida, mut soma_pearson, mut casos) = (0.0, 0.0, 0u32);
        for indice in 0..6usize.pow(t) {
            let mut freq = [0u64; 4];
            let mut resto = indice;
            for _ in 0..t {
                for d in pares[resto % 6] {
                    freq[d] += 1;
                }
                resto /= 6;
            }
            let q = qui_quadrado_frequencia(&MINI, &freq, u64::from(t)).unwrap();
            soma_corrigida += q.estatistica;
            soma_pearson += q.pearson;
            casos += 1;
        }
        assert_perto!(soma_corrigida / f64::from(casos), 3.0, 1e-13);
        assert_perto!(soma_pearson / f64::from(casos), 2.0, 1e-13);
    }
}

#[test]
fn qui_quadrado_recusa_frequencias_incoerentes() {
    for (freq, t) in [
        (&[2u64, 3, 1][..], 3),    // faltou uma dezena
        (&[2, 3, 1, 1][..], 3),    // soma 7 ≠ 3·2
        (&[4, 1, 1, 0][..], 3),    // 4 saídas em 3 concursos
        (&[1, 1, 1, 1, 0][..], 2), // dezena a mais
    ] {
        assert!(
            matches!(
                qui_quadrado_frequencia(&MINI, freq, t),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ),
            "{freq:?}"
        );
    }
    assert!(matches!(
        qui_quadrado_frequencia(&MINI, &[0, 0, 0, 0], 0),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

// ------------------------------------------------------------- runs test

#[test]
fn runs_a_mao() {
    // 1 1 0 0 1 0: n1 = n0 = 3, R = 4, μ = 2·9/6 + 1 = 4 → z = 0, p = 1.
    // σ² = 2·9·(18 − 6)/(36·5) = 6/5.
    let t = teste_de_runs(&[true, true, false, false, true, false]).unwrap();
    assert_eq!((t.saidas, t.nao_saidas, t.sequencias), (3, 3, 4));
    assert_eq!(t.media, 4.0);
    assert_perto!(t.desvio_padrao, 1.2_f64.sqrt(), 1e-15);
    assert_eq!(t.z, Some(0.0));
    assert_eq!(t.p_valor, Some(1.0));

    // 1 0 1 0 1 0 1 0: n1 = n0 = 4, R = 8, μ = 5, σ² = 2·16·24/(64·7) =
    // 12/7, z = 3/√(12/7) = 3·√(7/12). Alternância demais: z > 0.
    let alterna = [true, false, true, false, true, false, true, false];
    let t = teste_de_runs(&alterna).unwrap();
    assert_eq!((t.saidas, t.sequencias), (4, 8));
    assert_eq!(t.media, 5.0);
    let z = 3.0 * (7.0_f64 / 12.0).sqrt();
    assert_perto!(t.z.unwrap(), z, 1e-15);
    assert_perto!(t.p_valor.unwrap(), 2.194_677_100_324_686e-2);

    // 1 1 1 1 0 0 0 0: R = 2, blocos longos: z = −3·√(7/12), mesmo p.
    let blocos = [true, true, true, true, false, false, false, false];
    let t = teste_de_runs(&blocos).unwrap();
    assert_eq!(t.sequencias, 2);
    assert_perto!(t.z.unwrap(), -z, 1e-15);
    assert_perto!(t.p_valor.unwrap(), 2.194_677_100_324_686e-2);
}

#[test]
fn runs_media_e_desvio_conferem_com_a_distribuicao_exata() {
    // Sob o acaso, dado n1, toda ordem dos uns é igualmente provável:
    // enumerando todas as C(n, n1) ordens, a média e a variância de R têm de
    // bater com μ e σ² das fórmulas.
    for (n, n1) in [(3u32, 1u32), (7, 1), (8, 3), (10, 5), (12, 4), (13, 6)] {
        let (mut soma, mut soma_quadrados, mut casos) = (0.0, 0.0, 0.0);
        for mascara in 0u32..(1 << n) {
            if mascara.count_ones() != n1 {
                continue;
            }
            let seq: Vec<bool> = (0..n).map(|i| mascara >> i & 1 == 1).collect();
            let r = teste_de_runs(&seq).unwrap().sequencias as f64;
            soma += r;
            soma_quadrados += r * r;
            casos += 1.0;
        }
        let media = soma / casos;
        let variancia = soma_quadrados / casos - media * media;
        let seq: Vec<bool> = (0..n).map(|i| i < n1).collect();
        let t = teste_de_runs(&seq).unwrap();
        assert_perto!(t.media, media, 1e-13);
        assert_perto!(t.desvio_padrao * t.desvio_padrao, variancia, 1e-12);
    }
}

#[test]
fn runs_nao_testavel() {
    for seq in [
        &[true][..],
        &[false][..],
        &[true, true, true][..],
        &[false, false][..],
        &[true, false][..], // n1 = n0 = 1: σ² = 0
        &[false, true][..],
    ] {
        let t = teste_de_runs(seq).unwrap();
        assert!(!t.testavel(), "{seq:?}");
        assert_eq!((t.z, t.p_valor, t.desvio_padrao), (None, None, 0.0));
    }
    let t = teste_de_runs(&[false; 5]).unwrap();
    assert_eq!(
        (t.saidas, t.nao_saidas, t.sequencias, t.media),
        (0, 5, 1, 1.0)
    );
    // n1 = 1, n0 = 2 já é testável.
    assert!(teste_de_runs(&[true, false, false]).unwrap().testavel());
    assert!(matches!(
        teste_de_runs(&[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

// ------------------------------------------------------- função completa

#[test]
fn resumo_a_mao_com_dezena_nao_testavel() {
    // {1,2}, {1,3}, {1,4}: a dezena 1 sai sempre (não testável, fora do Holm).
    // Dezena 2: 1 0 0 → n1 = 1, R = 2, μ = 7/3, σ² = 2·2·1/(9·2) = 2/9,
    //   z = (−1/3)/(√2/3) = −1/√2, p = erfc(1/2).
    // Dezena 3: 0 1 0 → R = 3, z = (2/3)/(√2/3) = √2, p = erfc(1).
    // Dezena 4: 0 0 1 → como a 2.
    // Holm com m = 3: erfc(1)·3 = 0,4719 (dezena 3); erfc(1/2)·2 = 0,9590
    // (dezenas 2 e 4, empatadas).
    // Qui-quadrado: O = 3, 1, 1, 1; 4·O − 6 = 6, −2, −2, −2 → Σ² = 48;
    // Pearson = 48/(3·2·4) = 2; corrigida = 3/2·2 = 3.
    let r = testar_aleatoriedade(&MINI, &historico_mini(&[[1, 2], [1, 3], [1, 4]])).unwrap();
    assert_eq!(
        (r.primeiro_concurso, r.ultimo_concurso, r.concursos),
        (1, 3, 3)
    );
    assert_eq!(r.alfa, 0.05);
    assert_eq!(r.qui_quadrado.pearson, 2.0);
    assert_eq!(r.qui_quadrado.estatistica, 3.0);
    assert_perto!(r.qui_quadrado.p_valor, 3.916_251_762_710_889_5e-1);

    let erfc_meio = 4.795_001_221_869_535e-1;
    let erfc_um = 1.572_992_070_502_851_3e-1;
    let dezenas: Vec<u32> = r.runs.iter().map(|x| x.dezena).collect();
    assert_eq!(dezenas, [1, 2, 3, 4]);

    let d1 = &r.runs[0];
    assert_eq!((d1.teste.saidas, d1.teste.sequencias), (3, 1));
    assert_eq!(
        (d1.teste.p_valor, d1.p_valor_holm, d1.rejeita),
        (None, None, false)
    );

    for (i, z, p, p_holm) in [
        (1, -0.5_f64.sqrt(), erfc_meio, 2.0 * erfc_meio),
        (2, 2.0_f64.sqrt(), erfc_um, 3.0 * erfc_um),
        (3, -0.5_f64.sqrt(), erfc_meio, 2.0 * erfc_meio),
    ] {
        let d = &r.runs[i];
        assert_perto!(d.teste.media, 7.0 / 3.0, 1e-15);
        assert_perto!(d.teste.z.unwrap(), z, 1e-15);
        assert_perto!(d.teste.p_valor.unwrap(), p);
        assert_perto!(d.p_valor_holm.unwrap(), p_holm);
        assert!(!d.rejeita);
    }
    assert_eq!(r.dezenas_testadas_runs, 3);
    assert_eq!((r.rejeicoes_runs_sem_correcao, r.rejeicoes_runs), (0, 0));
}

#[test]
fn resumo_a_mao_com_todas_testaveis() {
    // {1,2}, {2,3}, {1,2}, {3,4}: O = 2, 3, 2, 1, E = 2 → 4·O − 8 = 0, 4,
    // 0, −4 → Pearson = 32/32 = 1, corrigida 1,5.
    // Dezenas 1 (1010) e 3 (0101): R = 4, μ = 3, σ² = 2/3, z = √1,5.
    // Dezenas 2 (1110) e 4 (0001): R = 2, μ = 2,5, σ = 1/2, z = −1.
    // Holm (m = 4): o menor p, p(√1,5) = 0,2207, vira 4·0,2207 = 0,8827, e
    // os outros sobem para ele (3·0,2207 e 2·0,3173, 1·0,3173 são menores).
    let r =
        testar_aleatoriedade(&MINI, &historico_mini(&[[1, 2], [2, 3], [1, 2], [3, 4]])).unwrap();
    assert_eq!(r.qui_quadrado.pearson, 1.0);
    assert_eq!(r.qui_quadrado.estatistica, 1.5);
    assert_perto!(r.qui_quadrado.p_valor, 6.822_703_303_362_125e-1);
    let p_z_raiz = 2.206_713_619_198_468e-1;
    let p_z_um = 3.173_105_078_629_141e-1;
    for (d, sequencias, media, z, p) in [
        (0, 4, 3.0, 1.5_f64.sqrt(), p_z_raiz),
        (1, 2, 2.5, -1.0, p_z_um),
        (2, 4, 3.0, 1.5_f64.sqrt(), p_z_raiz),
        (3, 2, 2.5, -1.0, p_z_um),
    ] {
        let x = &r.runs[d];
        assert_eq!(x.teste.sequencias, sequencias);
        assert_eq!(x.teste.media, media);
        assert_perto!(x.teste.z.unwrap(), z, 1e-15);
        assert_perto!(x.teste.p_valor.unwrap(), p);
        assert_perto!(x.p_valor_holm.unwrap(), 4.0 * p_z_raiz);
    }
    assert_eq!(r.dezenas_testadas_runs, 4);
}

#[test]
fn holm_do_resumo_e_o_holm_dos_p_valores_testaveis() {
    let mut g = Gerador(2026);
    let l = cfg("quina");
    let r = testar_aleatoriedade(l, &historico_aleatorio(l, 300, &mut g)).unwrap();
    let p: Vec<f64> = r.runs.iter().filter_map(|x| x.teste.p_valor).collect();
    let ajustados: Vec<f64> = r.runs.iter().filter_map(|x| x.p_valor_holm).collect();
    assert_eq!(ajustados, holm(&p).unwrap());
    assert_eq!(r.dezenas_testadas_runs, p.len());
    assert_eq!(
        r.rejeicoes_runs_sem_correcao,
        p.iter().filter(|&&p| p <= ALFA).count()
    );
    // Frequências do qui-quadrado = saídas do runs test.
    let freq: Vec<u64> = r.runs.iter().map(|x| x.teste.saidas).collect();
    assert_eq!(
        qui_quadrado_frequencia(l, &freq, 300).unwrap(),
        r.qui_quadrado
    );
}

#[test]
fn nao_depende_da_ordem_da_entrada_e_e_deterministico() {
    let mut g = Gerador(42);
    for slug in ["lotofacil", "megasena", "quina"] {
        let l = cfg(slug);
        let historico = historico_aleatorio(l, 120, &mut g);
        let base = testar_aleatoriedade(l, &historico).unwrap();
        assert_eq!(testar_aleatoriedade(l, &historico).unwrap(), base);
        let mut embaralhado = historico.clone();
        g.embaralhar(&mut embaralhado);
        // Ordem das dezenas dentro do concurso também não importa.
        for r in &mut embaralhado {
            r.dezenas.reverse();
        }
        let outro = testar_aleatoriedade(l, &embaralhado).unwrap();
        assert_eq!(outro, base, "{slug}");
        assert_eq!(
            outro.qui_quadrado.estatistica.to_bits(),
            base.qui_quadrado.estatistica.to_bits()
        );
    }
}

#[test]
fn serie_pode_comecar_depois_do_concurso_1() {
    let historico: Vec<Resultado> = [[1, 2], [2, 3], [1, 2], [3, 4]]
        .iter()
        .zip(50..)
        .map(|(d, c)| res(&MINI, c, d))
        .collect();
    let r = testar_aleatoriedade(&MINI, &historico).unwrap();
    assert_eq!(
        (r.primeiro_concurso, r.ultimo_concurso, r.concursos),
        (50, 53, 4)
    );
    let do_um =
        testar_aleatoriedade(&MINI, &historico_mini(&[[1, 2], [2, 3], [1, 2], [3, 4]])).unwrap();
    assert_eq!(r.qui_quadrado, do_um.qui_quadrado);
    assert_eq!(r.runs, do_um.runs);
}

#[test]
fn um_concurso_so_nao_testa_runs() {
    let r = testar_aleatoriedade(&MINI, &historico_mini(&[[2, 4]])).unwrap();
    assert_eq!(r.qui_quadrado.estatistica, 3.0);
    assert_eq!(r.dezenas_testadas_runs, 0);
    assert!(r
        .runs
        .iter()
        .all(|x| x.p_valor_holm.is_none() && !x.rejeita));
}

#[test]
fn erros_de_entrada() {
    assert!(matches!(
        testar_aleatoriedade(&MINI, &[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));

    let mut repetido = historico_mini(&[[1, 2], [2, 3], [3, 4]]);
    repetido[2].concurso = 2;
    assert!(erro_de(&MINI, &repetido).contains("concurso 2 repetido"));

    let mut buraco = historico_mini(&[[1, 2], [2, 3], [3, 4]]);
    buraco[2].concurso = 6;
    assert!(erro_de(&MINI, &buraco).contains("3..=5"));

    let curto = vec![res(&MINI, 1, &[1, 2]), res(&MINI, 2, &[3])];
    assert!(erro_de(&MINI, &curto).contains("concurso 2: 1 dezena(s), esperadas 2"));

    let longo = vec![res(&MINI, 1, &[1, 2, 3])];
    assert!(erro_de(&MINI, &longo).contains("3 dezena"));

    let fora = vec![res(&MINI, 1, &[1, 5])];
    assert!(erro_de(&MINI, &fora).contains("dezena 5 fora"));

    let zero = vec![res(&MINI, 1, &[0, 1])];
    assert!(erro_de(&MINI, &zero).contains("dezena 0 fora"));

    let dobrada = vec![res(&MINI, 1, &[3, 3])];
    assert!(erro_de(&MINI, &dobrada).contains("dezena 3 repetida"));

    let mut outra = historico_mini(&[[1, 2]]);
    outra[0].loteria = "quina".to_string();
    assert!(erro_de(&MINI, &outra).contains("`quina`"));

    // Regras de `validacao::validar_resultado` que o laço antigo não tinha:
    // concurso 0 e data inválida também reprovam (como no motor).
    let zero_concurso = vec![res(&MINI, 0, &[1, 2])];
    assert!(erro_de(&MINI, &zero_concurso).contains("numeração começa em 1"));
    let mut sem_data = historico_mini(&[[1, 2], [3, 4]]);
    sem_data[1].data_sorteio = "2020-02-30".to_string();
    assert!(erro_de(&MINI, &sem_data).contains("concurso 2: data `2020-02-30`"));

    // Vários problemas no mesmo concurso vão juntos.
    let dois = vec![res(&MINI, 1, &[5, 5])];
    let detalhe = erro_de(&MINI, &dois);
    assert!(
        detalhe.contains("dezena 5 fora") && detalhe.contains("dezena 5 repetida"),
        "{detalhe}"
    );
}

#[test]
fn erro_de_resultados_orienta_para_validar_e_buracos() {
    let mut buraco = historico_mini(&[[1, 2], [2, 3], [3, 4]]);
    buraco[2].concurso = 6;
    let mensagem = testar_aleatoriedade(&MINI, &buraco)
        .unwrap_err()
        .to_string();
    for trecho in ["sync validar", "sync buracos", "sync inserir", "3..=5"] {
        assert!(mensagem.contains(trecho), "{mensagem}");
    }
    // Nada de features/índice: os testes leem `resultados` direto.
    assert!(!mensagem.contains("sync features"), "{mensagem}");
    assert!(!mensagem.contains("sync indice"), "{mensagem}");
}

#[test]
fn recusa_configuracao_sem_nada_a_testar() {
    for k in [0, 4, 5] {
        let c = LoteriaConfig {
            qtd_dezenas: k,
            ..MINI
        };
        let dezenas: Vec<u32> = (1..=k as u32).collect();
        let historico = vec![res(&c, 1, &dezenas)];
        assert!(
            erro_de_config(&c, &historico).contains("1 ≤ K < N"),
            "K = {k}"
        );
        assert!(matches!(
            qui_quadrado_frequencia(&c, &[1, 1, 1, 1], 1),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

// ------------------------------------------------ poder e tamanho do teste

#[test]
fn alternancia_perfeita_e_pega_pelo_runs_mas_nao_pelo_qui_quadrado() {
    // {1,2} nos ímpares, {3,4} nos pares, 200 concursos: frequências iguais
    // (qui-quadrado zero), mas cada dezena alterna sempre (R = n).
    let sorteios: Vec<[u32; 2]> = (0..200)
        .map(|i| if i % 2 == 0 { [1, 2] } else { [3, 4] })
        .collect();
    let r = testar_aleatoriedade(&MINI, &historico_mini(&sorteios)).unwrap();
    assert_eq!(r.qui_quadrado.estatistica, 0.0);
    assert!(!r.qui_quadrado.rejeita);
    assert_eq!(r.rejeicoes_runs, 4);
    for x in &r.runs {
        assert_eq!(x.teste.sequencias, 200);
        assert!(x.teste.z.unwrap() > 13.0);
    }
}

#[test]
fn dezena_viciada_e_pega_pelo_qui_quadrado() {
    // Mega-Sena com a dezena 7 em 30% dos concursos (acaso: 10%).
    let l = cfg("megasena");
    let mut g = Gerador(7);
    let mut historico = historico_aleatorio(l, 1000, &mut g);
    for r in historico.iter_mut() {
        if g.proximo() % 10 < 3 && !r.dezenas.contains(&7) {
            r.dezenas[0] = 7;
        }
    }
    let r = testar_aleatoriedade(l, &historico).unwrap();
    assert!(r.qui_quadrado.rejeita, "{:?}", r.qui_quadrado);
    assert!(r.qui_quadrado.p_valor < 1e-20);
}

#[test]
fn sob_o_acaso_as_estatisticas_tem_o_tamanho_certo() {
    // Simulação do nulo (Mega-Sena, 400 históricos de 500 concursos, semente
    // fixa). Esperados: média da corrigida = 59 (exata, desvio-padrão da
    // média ≈ √(2·59/400) ≈ 0,54) e de Pearson = 54; rejeição do
    // qui-quadrado ≈ 5% (desvio ≈ 1,1 pp); rejeição dos runs por dezena,
    // sem correção, ≈ 5%. Com Holm, o nominal seria alguma rejeição em
    // ≤ 5% dos históricos, mas aqui sai ~9%: com n1 ≈ 50 de 500, a normal
    // subestima a cauda esquerda de R no limiar α/60 (limitação documentada
    // no módulo). O teto de 10% só pega erro grosseiro; se o p-valor do runs
    // passar a ser exato, apertar para ~7%.
    let l = cfg("megasena");
    let mut g = Gerador(0x5eed_1c04);
    let repeticoes = 400;
    let (mut soma_corrigida, mut soma_pearson) = (0.0, 0.0);
    let (mut rejeicoes_qui, mut rejeicoes_runs, mut testes_runs, mut familias_rejeitadas) =
        (0, 0, 0, 0);
    for _ in 0..repeticoes {
        let r = testar_aleatoriedade(l, &historico_aleatorio(l, 500, &mut g)).unwrap();
        soma_corrigida += r.qui_quadrado.estatistica;
        soma_pearson += r.qui_quadrado.pearson;
        rejeicoes_qui += usize::from(r.qui_quadrado.rejeita);
        rejeicoes_runs += r.rejeicoes_runs_sem_correcao;
        testes_runs += r.dezenas_testadas_runs;
        familias_rejeitadas += usize::from(r.rejeicoes_runs > 0);
    }
    let media_corrigida = soma_corrigida / f64::from(repeticoes);
    let media_pearson = soma_pearson / f64::from(repeticoes);
    let taxa_qui = rejeicoes_qui as f64 / f64::from(repeticoes);
    let taxa_runs = rejeicoes_runs as f64 / testes_runs as f64;
    let taxa_familia = familias_rejeitadas as f64 / f64::from(repeticoes);
    let resumo = format!(
        "corrigida {media_corrigida:.3}, Pearson {media_pearson:.3}, rejeição qui {taxa_qui:.4}, runs {taxa_runs:.4}, família Holm {taxa_familia:.4}"
    );
    assert!((media_corrigida - 59.0).abs() < 2.5, "{resumo}");
    assert!((media_pearson - 54.0).abs() < 2.5, "{resumo}");
    assert!((0.015..=0.095).contains(&taxa_qui), "{resumo}");
    assert!((0.035..=0.065).contains(&taxa_runs), "{resumo}");
    assert!(taxa_familia <= 0.10, "{resumo}");
}
