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
// absorve o cancelamento até |z| = 37). `P(χ²_gl ≥ x) = Q(gl/2, x/2)`; com
// 1 gl, `P(χ²_1 ≥ z²) = 2·Φ(−|z|) = erfc(|z|/√2)`, que dá as caudas de
// a = 1/2 (z = 5, 8, 20, 37). Os valores críticos tabelados (χ²: 3,841 com
// 1 gl, 7,815 com 3, 36,415 com 24, 77,931 com 59, 100,749 com 79) conferem
// com 5% nessas referências.

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
    let referencias: [(f64, u32, f64); 26] = [
        (3.841_458_820_694_124, 1, 5.000_000_000_000_006e-2),
        (0.5, 1, 4.795_001_221_869_535e-1),
        (25.0, 1, 5.733_031_437_583_878e-7),       // 2·Φ(−5)
        (64.0, 1, 1.244_192_114_854_356_8e-15),    // 2·Φ(−8)
        (400.0, 1, 5.507_248_237_212_467_5e-89),   // 2·Φ(−20)
        (1369.0, 1, 1.145_114_244_504_915_4e-299), // 2·Φ(−37)
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
fn caudas_infinitas_e_dominio() {
    assert_eq!(p_valor_qui_quadrado(f64::INFINITY, 1).unwrap(), 0.0);
    assert_eq!(p_valor_qui_quadrado(1e200, 1).unwrap(), 0.0);
    assert_eq!(p_valor_qui_quadrado(f64::INFINITY, 5).unwrap(), 0.0);
    assert_eq!(p_valor_qui_quadrado(0.0, 5).unwrap(), 1.0);
    for invalido in [
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

/// Contagem exata, por enumeração, dos arranjos de `n1` uns entre `n`
/// posições por número de sequências: `contagens[r]` (índice = r). Método
/// independente da recorrência do módulo (só conta blocos).
fn contagens_por_enumeracao(n: u32, n1: u32) -> Vec<u64> {
    let mut contagens = vec![0u64; n as usize + 2];
    for mascara in 0u32..(1 << n) {
        if mascara.count_ones() != n1 {
            continue;
        }
        let seq: Vec<bool> = (0..n).map(|i| mascara >> i & 1 == 1).collect();
        let r = 1 + seq.windows(2).filter(|w| w[0] != w[1]).count();
        contagens[r] += 1;
    }
    contagens
}

/// p-valor pela regra do plano em inteiros: `min(total, 2·min(Σ_{r ≤ obs} c,
/// Σ_{r ≥ obs} c)) / total` — uma divisão só, arredondada corretamente.
fn p_por_contagens(contagens: &[u64], r_obs: usize) -> f64 {
    let total: u64 = contagens.iter().sum();
    let esquerda: u64 = contagens[..=r_obs].iter().sum();
    let direita: u64 = contagens[r_obs..].iter().sum();
    (2 * esquerda.min(direita)).min(total) as f64 / total as f64
}

#[test]
fn runs_a_mao() {
    // 1 1 0 0 1 0: n1 = n0 = 3, R = 4, μ = 2·9/6 + 1 = 4, σ² = 2·9·(18 − 6)/
    // (36·5) = 6/5. Contagens de R = 2..6 em C(6,3) = 20: 2, 4, 8, 4, 2;
    // P(R ≤ 4) = P(R ≥ 4) = 14/20 → p = min(1, 28/20) = 1.
    let t = teste_de_runs(&[true, true, false, false, true, false]).unwrap();
    assert_eq!((t.saidas, t.nao_saidas, t.sequencias), (3, 3, 4));
    assert_eq!(t.media, 4.0);
    assert_perto!(t.desvio_padrao, 1.2_f64.sqrt(), 1e-15);
    assert_eq!(t.p_valor, 1.0);
    assert!(!t.fixa());

    // 1 0 1 0 1 0 1 0: n1 = n0 = 4, R = 8 (máximo), μ = 5. Contagens de
    // R = 2..8 em C(8,4) = 70: 2, 6, 18, 18, 18, 6, 2. P(R ≥ 8) = 2/70 →
    // p = 2·2/70 = 2/35.
    let alterna = [true, false, true, false, true, false, true, false];
    let t = teste_de_runs(&alterna).unwrap();
    assert_eq!((t.saidas, t.sequencias), (4, 8));
    assert_eq!(t.media, 5.0);
    assert!(t.sequencias as f64 > t.media);
    assert_perto!(t.p_valor, 2.0 / 35.0, 1e-14);

    // 1 1 1 1 0 0 0 0: R = 2 (mínimo), blocos longos: P(R ≤ 2) = 2/70, mesmo
    // p.
    let blocos = [true, true, true, true, false, false, false, false];
    let t = teste_de_runs(&blocos).unwrap();
    assert_eq!(t.sequencias, 2);
    assert!((t.sequencias as f64) < t.media);
    assert_perto!(t.p_valor, 2.0 / 35.0, 1e-14);
}

#[test]
fn runs_media_e_desvio_conferem_com_a_distribuicao_exata() {
    // Sob o acaso, dado n1, toda ordem dos uns é igualmente provável:
    // enumerando todas as C(n, n1) ordens, a média e a variância de R têm de
    // bater com μ e σ² das fórmulas.
    for (n, n1) in [(3u32, 1u32), (7, 1), (8, 3), (10, 5), (12, 4), (13, 6)] {
        let contagens = contagens_por_enumeracao(n, n1);
        let casos: u64 = contagens.iter().sum();
        let (mut soma, mut soma_quadrados) = (0.0, 0.0);
        for (r, &c) in contagens.iter().enumerate() {
            soma += (r as f64) * c as f64;
            soma_quadrados += (r * r) as f64 * c as f64;
        }
        let media = soma / casos as f64;
        let variancia = soma_quadrados / casos as f64 - media * media;
        let seq: Vec<bool> = (0..n).map(|i| i < n1).collect();
        let t = teste_de_runs(&seq).unwrap();
        assert_perto!(t.media, media, 1e-13);
        assert_perto!(t.desvio_padrao * t.desvio_padrao, variancia, 1e-12);
    }
}

#[test]
fn pesos_do_runs_conferem_com_a_enumeracao() {
    // A recorrência em log (com o modo = 0) normalizada tem de dar a
    // distribuição exata de R para todo n ≤ 14 e todo 1 ≤ n1 < n, e somar 1.
    for n in 2u32..=14 {
        for n1 in 1..n {
            let contagens = contagens_por_enumeracao(n, n1);
            let total: u64 = contagens.iter().sum();
            let pesos = ln_pesos_runs(u64::from(n1), u64::from(n - n1));
            let r_max = contagens.iter().rposition(|&c| c > 0).unwrap();
            assert_eq!(pesos.len(), r_max - 1, "n = {n}, n1 = {n1}");
            // O modo vale 0; outro r empatado com ele pode sair 1 eps acima.
            assert!(pesos.contains(&0.0));
            let maior = pesos.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!((0.0..=1e-15).contains(&maior), "{maior:e}");
            let soma: f64 = pesos.iter().map(|w| w.exp()).sum();
            let mut soma_p = 0.0;
            for (i, w) in pesos.iter().enumerate() {
                let p = w.exp() / soma;
                soma_p += p;
                assert_perto!(p, contagens[i + 2] as f64 / total as f64, 1e-13);
            }
            assert_perto!(soma_p, 1.0, 1e-14);
        }
    }
}

#[test]
fn p_valor_exato_confere_com_a_enumeracao() {
    // Todo R possível de todo (n, n1) com n ≤ 14: o p-valor da recorrência
    // em log contra a soma das contagens inteiras pela mesma regra.
    for n in 2u32..=14 {
        for n1 in 1..n {
            let contagens = contagens_por_enumeracao(n, n1);
            for r in (2..contagens.len()).filter(|&r| contagens[r] > 0) {
                let p = p_valor_runs_exato(u64::from(n1), u64::from(n - n1), r as u64).unwrap();
                assert_perto!(p, p_por_contagens(&contagens, r), 1e-13);
                assert!(p > 0.0 && p <= 1.0);
            }
        }
    }
}

// Referências do p-valor exato do runs test: contagem exata em inteiros
// grandes pela fórmula clássica de dois termos (`math.comb`, independente da
// recorrência em log do módulo), as duas caudas em inteiros e uma única
// divisão em `Fraction`; `float(Fraction)` arredonda corretamente (inclusive
// para subnormal e 0). Para refazer a conta (Python 3.8+; os casos de
// n = 7000 levam ~1 min):
//
//     from fractions import Fraction
//     from math import comb
//
//     def contagens(n1, n0):  # arranjos de n1 uns e n0 zeros com R sequências
//         c = {}
//         for r in range(2, n1 + n0 + 1):
//             k = r // 2
//             if r % 2 == 0:
//                 v = 2 * comb(n1 - 1, k - 1) * comb(n0 - 1, k - 1)
//             else:
//                 v = comb(n1 - 1, k - 1) * comb(n0 - 1, k) + comb(n1 - 1, k) * comb(n0 - 1, k - 1)
//             if v:
//                 c[r] = v
//         return c
//
//     def p_runs(n1, n0, r):  # min(1, 2·min(P(R ≤ r), P(R ≥ r)))
//         c = contagens(n1, n0)
//         assert r in c and sum(c.values()) == comb(n1 + n0, n1)
//         esquerda = sum(v for s, v in c.items() if s <= r)
//         direita = sum(v for s, v in c.items() if s >= r)
//         return float(min(Fraction(1), Fraction(2 * min(esquerda, direita), comb(n1 + n0, n1))))
//
//     print(repr(p_runs(440, 6560, 780)))  # 2.364965762982194e-05

#[test]
fn p_valor_exato_contra_referencias_em_python() {
    // (n1, n0, R, p), saídas de `p_runs` acima. Cobrem: n1 = 1 (R = 2 e 3),
    // R mínimo e máximo, n1 = n0 (2, 3, 4, 10: caudas iguais), os dois lados
    // do modo (μ = 2·n1·n0/n + 1) nos tamanhos de 500 concursos com p = 10%
    // (Mega-Sena), 3000 com p = 60% (Lotofácil) e 7000 com n1 = 440 (Quina),
    // inclusive pares e ímpares vizinhos (780/781, 825/826, 870/871,
    // 880/881) e caudas minúsculas.
    //
    // Tolerância relativa 1e-12: o erro esperado é de algumas dezenas de
    // `eps` — logaritmos acumulados com soma compensada e o modo subtraído
    // parcela a parcela, mas `ln p` passa por `exp` com |ln p| até ~650, o
    // que multiplica o erro absoluto do log (~1e-16·650) — e o teto de 1e-12
    // deixa duas ordens de folga sem esconder erro de fórmula (≥ 1e-3).
    let referencias: [(u64, u64, u64, f64); 59] = [
        (1, 1, 2, 1.0),
        (1, 2, 2, 1.0),
        (1, 2, 3, 0.666_666_666_666_666_6),
        (2, 1, 3, 0.666_666_666_666_666_6),
        (1, 5, 2, 0.666_666_666_666_666_6),
        (1, 5, 3, 1.0),
        (1, 9, 2, 0.4),
        (1, 9, 3, 1.0),
        (2, 2, 2, 0.666_666_666_666_666_6),
        (2, 2, 3, 1.0),
        (2, 2, 4, 0.666_666_666_666_666_6),
        (3, 3, 2, 0.2),
        (3, 3, 3, 0.6),
        (3, 3, 4, 1.0),
        (3, 3, 5, 0.6),
        (3, 3, 6, 0.2),
        (4, 4, 2, 0.057_142_857_142_857_14),
        (4, 4, 3, 0.228_571_428_571_428_56),
        (4, 4, 4, 0.742_857_142_857_142_9),
        (4, 4, 5, 1.0),
        (4, 4, 6, 0.742_857_142_857_142_9),
        (4, 4, 7, 0.228_571_428_571_428_56),
        (4, 4, 8, 0.057_142_857_142_857_14),
        (3, 7, 2, 0.033_333_333_333_333_33),
        (3, 7, 3, 0.166_666_666_666_666_66),
        (3, 7, 4, 0.566_666_666_666_666_7),
        (3, 7, 5, 1.0),
        (3, 7, 6, 0.833_333_333_333_333_4),
        (3, 7, 7, 0.333_333_333_333_333_3),
        (10, 10, 2, 2.165_017_644_893_806e-5),
        (10, 10, 6, 0.037_043_451_904_133_017),
        (10, 10, 11, 1.0),
        (10, 10, 15, 0.102_513_585_485_721_7),
        (10, 10, 20, 2.165_017_644_893_806e-5),
        (50, 450, 2, 1.728_292_666_160_625e-69),
        (50, 450, 75, 7.589_006_764_899_597e-4),
        (50, 450, 80, 0.014_014_115_258_015_896),
        (50, 450, 90, 0.800_561_831_724_244_1),
        (50, 450, 91, 1.0),
        (50, 450, 92, 0.879_816_680_682_364_5),
        (50, 450, 100, 0.008_545_681_257_173_155),
        (50, 450, 101, 0.006_836_545_005_738_524),
        (1800, 1200, 1300, 9.447_139_783_615_626e-8),
        (1800, 1200, 1400, 0.123_360_134_130_413_34),
        (1800, 1200, 1441, 1.0),
        (1800, 1200, 1500, 0.025_836_777_331_213_555),
        (1800, 1200, 1600, 1.386_607_773_595_064_3e-9),
        (440, 6560, 760, 3.892_001_365_973_738e-9),
        (440, 6560, 780, 2.364_965_762_982_194e-5),
        (440, 6560, 781, 4.453_785_532_834_404_4e-5),
        (440, 6560, 825, 0.995_161_551_154_769_7),
        (440, 6560, 826, 1.0),
        (440, 6560, 870, 9.393_494_730_671_855e-8),
        (440, 6560, 871, 8.407_746_497_169_47e-8),
        (440, 6560, 880, 3.188_542_666_685_532e-13),
        (440, 6560, 881, 2.787_697_302_873_636_4e-13),
        // Caudas extremas, ainda em f64 normal.
        (440, 6560, 600, 1.567_716_085_168_069_2e-67),
        (440, 6560, 300, 7.538_835_726_635_354e-284),
        (1, 7000, 2, 5.713_469_504_356_521e-4),
    ];
    for (n1, n0, r, esperado) in referencias {
        let obtido = p_valor_runs_exato(n1, n0, r).unwrap();
        assert!(
            perto(obtido, esperado, 1e-12),
            "p(n1 = {n1}, n0 = {n0}, R = {r}) = {obtido:?}, esperado {esperado:?}"
        );
    }
    // Subnormais (1,59e-313 ≈ 2^−1039, 35 bits de mantissa; 9,83e-322 =
    // 199·2^−1074, 8 bits): ainda positivos e com a precisão que os bits
    // restantes permitem.
    let subnormal = p_valor_runs_exato(440, 6560, 270).unwrap();
    assert_perto!(subnormal, 1.591_645_249_97e-313, 1e-9);
    let quase_zero = p_valor_runs_exato(440, 6560, 262).unwrap();
    assert_perto!(quase_zero, 9.83e-322, 1e-2);
    // O valor exato abaixo de 2^−1075 (metade do menor subnormal) não cabe
    // em f64: 0, como o `float(Fraction)` do Python.
    for (n1, n0, r) in [
        (440, 6560, 255),
        (440, 6560, 3),
        (440, 6560, 2),
        (1800, 1200, 2),
        (1800, 1200, 2401),
    ] {
        assert_eq!(
            p_valor_runs_exato(n1, n0, r).unwrap(),
            0.0,
            "{n1}, {n0}, R = {r}"
        );
    }
}

#[test]
fn p_valor_exato_nao_depende_de_qual_lado_e_o_um() {
    // Trocar saiu por não saiu não muda a distribuição de R.
    for (n1, n0, r) in [
        (3, 7, 4),
        (50, 450, 75),
        (440, 6560, 780),
        (1800, 1200, 1300),
    ] {
        assert_perto!(
            p_valor_runs_exato(n1, n0, r).unwrap(),
            p_valor_runs_exato(n0, n1, r).unwrap(),
            1e-13
        );
    }
}

#[test]
fn distribuicao_exata_soma_1_em_tamanhos_reais() {
    // Propriedade com fórmula independente da recorrência: P(r) pela forma
    // clássica de dois termos, com ln C por ln-fatoriais (soma de logs
    // acumulada em f64 simples; erro absoluto ~1e-10 em ln C de ordem 1e3,
    // daí a tolerância 1e-8), tem de somar 1 e bater com os pesos do módulo
    // normalizados.
    let maior_n = 7200usize;
    let mut ln_fat = vec![0.0f64; maior_n + 1];
    for i in 1..=maior_n {
        ln_fat[i] = ln_fat[i - 1] + (i as f64).ln();
    }
    let ln_c = |a: u64, b: u64| -> Option<f64> {
        (b <= a).then(|| ln_fat[a as usize] - ln_fat[b as usize] - ln_fat[(a - b) as usize])
    };
    for (n1, n0) in [
        (1u64, 7000u64),
        (50, 450),
        (1800, 1200),
        (440, 6560),
        (3500, 3500),
        (7, 7000),
    ] {
        let n = n1 + n0;
        let ln_total = ln_c(n, n1).unwrap();
        let pesos = ln_pesos_runs(n1, n0);
        let soma_w: f64 = pesos.iter().map(|w| w.exp()).sum();
        let mut soma_p = 0.0;
        for (i, w) in pesos.iter().enumerate() {
            let r = i as u64 + 2;
            let k = r / 2;
            let ln_contagem = if r.is_multiple_of(2) {
                std::f64::consts::LN_2 + ln_c(n1 - 1, k - 1).unwrap() + ln_c(n0 - 1, k - 1).unwrap()
            } else {
                // C(n1−1,k−1)·C(n0−1,k) + C(n1−1,k)·C(n0−1,k−1), com termo
                // inexistente (C(a, b) com b > a) valendo 0, em log-soma-exp.
                let termos: Vec<f64> = [
                    ln_c(n1 - 1, k - 1).zip(ln_c(n0 - 1, k)),
                    ln_c(n1 - 1, k).zip(ln_c(n0 - 1, k - 1)),
                ]
                .into_iter()
                .flatten()
                .map(|(a, b)| a + b)
                .collect();
                let maior = termos.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                maior + termos.iter().map(|t| (t - maior).exp()).sum::<f64>().ln()
            };
            let p = (ln_contagem - ln_total).exp();
            soma_p += p;
            if p > 1e-250 {
                assert_perto!(w.exp() / soma_w, p, 1e-8);
            }
        }
        assert_perto!(soma_p, 1.0, 1e-8);
        // Na mediana as duas caudas passam de 1/2: p-valor 1.
        let mut acumulado = 0.0;
        let mediana = (2u64..)
            .zip(&pesos)
            .find(|(_, w)| {
                acumulado += w.exp() / soma_w;
                acumulado >= 0.5
            })
            .unwrap()
            .0;
        assert_perto!(p_valor_runs_exato(n1, n0, mediana).unwrap(), 1.0, 1e-12);
    }
}

/// Distribuição exata de R normalizada a partir dos pesos do módulo
/// (`prob[i]` é de r = i + 2) e o p-valor de cada r.
fn distribuicao_e_p_valores(n1: u64, n0: u64) -> (Vec<f64>, Vec<f64>) {
    let pesos = ln_pesos_runs(n1, n0);
    let soma: f64 = pesos.iter().map(|w| w.exp()).sum();
    let prob = pesos.iter().map(|w| w.exp() / soma).collect();
    let ps = (2..pesos.len() as u64 + 2)
        .map(|r| p_valor_runs_exato(n1, n0, r).unwrap())
        .collect();
    (prob, ps)
}

#[test]
fn p_valor_exato_e_o_dobro_da_menor_cauda_e_unimodal_em_r() {
    // Contra caudas acumuladas aqui por fora (soma simples das
    // probabilidades, cada cauda a partir da sua ponta): p(r) = min(1,
    // 2·min(F(r), S(r))) onde representável, p ≥ min(1, 2·P(r)), p ∈ [0, 1].
    // E a propriedade que a regra antiga (soma das probabilidades até a
    // observada) não tinha: p **não decresce até a mediana e não cresce
    // depois dela** — afastar R do centro, para qualquer lado, nunca aumenta
    // o p-valor, seja R par ou ímpar. O centro é a mediana, não o modo: na
    // Quina (446, 6688) o modo é 839 e a mediana 837, e p(839) < 1.
    for (n1, n0) in [
        (1u64, 9u64),
        (5, 5),
        (6, 25),
        (60, 440),
        (250, 2750),
        (446, 6688),
        (2278, 1518),
    ] {
        let (prob, ps) = distribuicao_e_p_valores(n1, n0);
        let m = prob.len();
        let mut esquerda = vec![0.0; m];
        let mut direita = vec![0.0; m];
        let mut acumulado = 0.0;
        for i in 0..m {
            acumulado += prob[i];
            esquerda[i] = acumulado;
        }
        acumulado = 0.0;
        for i in (0..m).rev() {
            acumulado += prob[i];
            direita[i] = acumulado;
        }
        for i in 0..m {
            let r = i + 2;
            assert!((0.0..=1.0).contains(&ps[i]), "n1 = {n1}, R = {r}");
            let esperado = (2.0 * esquerda[i].min(direita[i])).min(1.0);
            if esperado > 1e-250 {
                assert!(ps[i] > 0.0, "n1 = {n1}, R = {r}");
                assert_perto!(ps[i], esperado, 1e-9);
            }
            assert!(ps[i] >= (2.0 * prob[i]).min(1.0) * (1.0 - 1e-12));
        }
        let mediana = esquerda.iter().position(|&f| f >= 0.5).unwrap();
        assert_perto!(ps[mediana], 1.0, 1e-12);
        for i in 1..=mediana {
            assert!(
                ps[i - 1] <= ps[i] * (1.0 + 1e-12),
                "n1 = {n1}: p({}) = {} > p({}) = {}",
                i + 1,
                ps[i - 1],
                i + 2,
                ps[i]
            );
        }
        for i in mediana + 1..m {
            assert!(
                ps[i] <= ps[i - 1] * (1.0 + 1e-12),
                "n1 = {n1}: p({}) = {} > p({}) = {}",
                i + 2,
                ps[i],
                i + 1,
                ps[i - 1]
            );
        }
    }
}

#[test]
fn rejeicao_nao_depende_da_paridade_de_r() {
    // Quina com 7134 concursos e n1 = 446 (≈ 7134·5/80). A paridade de R só
    // diz se o primeiro e o último concurso têm o mesmo estado; sob o acaso,
    // dado n1, a chance de rejeitar tem de ser parecida com R par e com R
    // ímpar. Taxas exatas P(p ≤ α | R par) e P(p ≤ α | R ímpar) pela
    // distribuição exata (pesos do módulo normalizados), conferidas com a
    // contagem em `Fraction` no Python (mesmas `contagens`/`p_runs` do
    // comentário acima, somando as contagens de cada paridade com p ≤ α; o
    // p-valor mais perto de cada α fica a ≥ 1% dele, então nenhum r muda de
    // lado por arredondamento). P(R par) ≈ 0,117.
    //
    // Para contraste, a regra antiga (soma das P(r) ≤ P(obs)·(1 + 10⁻⁷))
    // nas mesmas contas: 26,6% contra 1,95% a 5%, e 0,22% contra 0,035% no
    // limiar do Holm (α/80) — o teste pegaria a volta dela.
    let (n1, n0) = (446u64, 6688u64);
    let (prob, ps) = distribuicao_e_p_valores(n1, n0);
    let antiga: Vec<f64> = prob
        .iter()
        .map(|&q| {
            prob.iter()
                .filter(|&&s| s <= q * (1.0 + 1e-7))
                .sum::<f64>()
                .min(1.0)
        })
        .collect();
    let taxas = |p_valores: &[f64], alfa: f64| {
        let (mut par, mut rejeita_par, mut impar, mut rejeita_impar) = (0.0, 0.0, 0.0, 0.0);
        for (r, (&q, &p)) in (2u64..).zip(prob.iter().zip(p_valores)) {
            let rejeita = if p <= alfa { q } else { 0.0 };
            if r % 2 == 0 {
                par += q;
                rejeita_par += rejeita;
            } else {
                impar += q;
                rejeita_impar += rejeita;
            }
        }
        (rejeita_par / par, rejeita_impar / impar)
    };
    for (alfa, par_esperado, impar_esperado) in [
        (ALFA, 0.053_593_506_759_916_976, 0.043_480_139_151_463_71),
        (
            ALFA / 80.0,
            5.374_258_948_608_887e-4,
            5.561_397_948_389_85e-4,
        ),
    ] {
        let (par, impar) = taxas(&ps, alfa);
        assert_perto!(par, par_esperado, 1e-9);
        assert_perto!(impar, impar_esperado, 1e-9);
        assert!(
            (2.0 / 3.0..=1.5).contains(&(par / impar)),
            "α = {alfa}: par {par}, ímpar {impar}"
        );
        let (par_antiga, impar_antiga) = taxas(&antiga, alfa);
        assert!(
            par_antiga / impar_antiga > 5.0,
            "α = {alfa}: regra antiga par {par_antiga}, ímpar {impar_antiga}"
        );
    }
}

#[test]
fn p_valor_exato_recusa_r_impossivel() {
    for (n1, n0, r, possiveis) in [
        (3, 3, 1, "2..=6"), // R = 1 exige dezena fixa
        (3, 3, 0, "2..=6"),
        (3, 3, 7, "2..=6"), // máximo com n1 = n0 = 3 é 6
        (3, 4, 8, "2..=7"), // máximo com n1 ≠ n0 é 2·3 + 1 = 7
        (1, 1, 3, "2..=2"),
        (1, 5, 4, "2..=3"),
        (0, 5, 2, "1..=1"), // dezena fixa: só R = 1
        (5, 0, 0, "1..=1"),
        // Recusados antes de montar a distribuição (2^41 termos, que nem
        // caberiam na memória).
        (1 << 40, 1 << 40, 1, "2..=2199023255552"),
        (1 << 40, 1 << 40, (1 << 41) + 1, "2..=2199023255552"),
        (1 << 40, (1 << 40) + 1, (1 << 41) + 2, "2..=2199023255553"),
    ] {
        match p_valor_runs_exato(n1, n0, r) {
            Err(SpiderError::AvaliacaoEntradaInvalida(m)) => assert!(
                m.contains("impossível") && m.contains(&format!("(possíveis: {possiveis})")),
                "{m}"
            ),
            outro => panic!("n1 = {n1}, n0 = {n0}, R = {r}: {outro:?}"),
        }
    }
    for (n1, n0, trecho) in [
        (0, 0, "sequência vazia"),
        (u64::MAX, 1, "não cabe em 64 bits"),
        (1 << 63, 1 << 63, "não cabe em 64 bits"),
    ] {
        match p_valor_runs_exato(n1, n0, 2) {
            Err(SpiderError::AvaliacaoEntradaInvalida(m)) => assert!(m.contains(trecho), "{m}"),
            outro => panic!("n1 = {n1}, n0 = {n0}: {outro:?}"),
        }
    }
    // Extremos válidos com n1 = 3, n0 = 4: R = 2 e R = 7.
    assert!(p_valor_runs_exato(3, 4, 2).unwrap() > 0.0);
    assert!(p_valor_runs_exato(3, 4, 7).unwrap() > 0.0);
    // Dezena fixa, inclusive com n no limite de u64: p = 1 sem distribuição.
    assert_eq!(p_valor_runs_exato(0, 5, 1).unwrap(), 1.0);
    assert_eq!(p_valor_runs_exato(5, 0, 1).unwrap(), 1.0);
    assert_eq!(p_valor_runs_exato(u64::MAX, 0, 1).unwrap(), 1.0);
}

#[test]
fn runs_dezena_fixa() {
    for seq in [
        &[true][..],
        &[false][..],
        &[true, true, true][..],
        &[false, false][..],
    ] {
        let t = teste_de_runs(seq).unwrap();
        assert!(t.fixa(), "{seq:?}");
        assert_eq!((t.sequencias, t.p_valor, t.desvio_padrao), (1, 1.0, 0.0));
    }
    let t = teste_de_runs(&[false; 5]).unwrap();
    assert_eq!(
        (t.saidas, t.nao_saidas, t.sequencias, t.media),
        (0, 5, 1, 1.0)
    );
    // n1 = n0 = 1: R = 2 sempre (σ = 0), p = 1 pela própria distribuição,
    // mas não é fixa.
    for seq in [&[true, false][..], &[false, true][..]] {
        let t = teste_de_runs(seq).unwrap();
        assert!(!t.fixa());
        assert_eq!((t.p_valor, t.desvio_padrao), (1.0, 0.0));
    }
    // n1 = 1, n0 = 2: R = 3 (P = 1/3, a cauda de cima) → p = 2/3.
    assert_perto!(
        teste_de_runs(&[false, true, false]).unwrap().p_valor,
        2.0 / 3.0,
        1e-15
    );
    assert!(matches!(
        teste_de_runs(&[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

// ------------------------------------------------------- função completa

#[test]
fn resumo_a_mao_com_dezena_fixa() {
    // {1,2}, {1,3}, {1,4}: a dezena 1 sai sempre (fixa: p = 1, entra no Holm).
    // Dezena 2: 1 0 0 → n1 = 1, n0 = 2, R = 2: P(R ≤ 2) = 2/3 → p = 1.
    // Dezena 3: 0 1 0 → R = 3: P(R ≥ 3) = 1/3 → p = 2/3.
    // Dezena 4: 0 0 1 → como a 2.
    // Holm com m = 4: 4·(2/3) passa de 1 → todos os ajustados = 1.
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

    let dezenas: Vec<u32> = r.runs.iter().map(|x| x.dezena).collect();
    assert_eq!(dezenas, [1, 2, 3, 4]);

    let d1 = &r.runs[0];
    assert!(d1.teste.fixa());
    assert_eq!((d1.teste.saidas, d1.teste.sequencias), (3, 1));
    assert_eq!(
        (d1.teste.p_valor, d1.p_valor_holm, d1.rejeita),
        (1.0, 1.0, false)
    );
    for (i, sequencias, p) in [(1, 2, 1.0), (2, 3, 2.0 / 3.0), (3, 2, 1.0)] {
        let d = &r.runs[i];
        assert!(!d.teste.fixa());
        assert_eq!(d.teste.sequencias, sequencias);
        assert_perto!(d.teste.media, 7.0 / 3.0, 1e-15);
        assert_perto!(d.teste.p_valor, p, 1e-15);
        assert_eq!(d.p_valor_holm, 1.0);
        assert!(!d.rejeita);
    }
    assert_eq!((r.dezenas_testadas_runs, r.dezenas_fixas_runs), (4, 1));
    assert_eq!((r.rejeicoes_runs_sem_correcao, r.rejeicoes_runs), (0, 0));
}

#[test]
fn dezena_fixa_conta_no_m_do_holm() {
    // 12 concursos da MINI com a dezena 1 em todos (fixa) e a outra vaga
    // em blocos: 2 nos concursos 1–4, 3 nos 5–8, 4 nos 9–12.
    // Com n1 = 4, n0 = 8 (C(12,4) = 495), as contagens de R = 2..9 são
    // 2, 10, 42, 84, 126, 126, 70, 35:
    //   dezenas 2 (111100000000) e 4 (000000001111): R = 2, P(R ≤ 2) =
    //   2/495 → p = 4/495;
    //   dezena 3 (000011110000): R = 3, P(R ≤ 3) = 12/495 → p = 24/495
    //   ≈ 0,0485.
    // Holm com m = 4 (a fixa conta, com p = 1): 4·4/495 = 16/495 para as
    // dezenas 2 e 4 (a segunda sobe pelo máximo: 3·4/495 < 16/495) e
    // 2·24/495 = 48/495 ≈ 0,097 para a 3 — que **não** é rejeitada. Se a
    // fixa ficasse de fora (m = 3), seriam 12/495 e 24/495 e as três seriam
    // rejeitadas: a decisão muda.
    let mut sorteios = Vec::new();
    for outra in [2, 3, 4] {
        sorteios.extend([[1, outra]; 4]);
    }
    let r = testar_aleatoriedade(&MINI, &historico_mini(&sorteios)).unwrap();
    assert_eq!((r.dezenas_testadas_runs, r.dezenas_fixas_runs), (4, 1));
    let p: Vec<f64> = r.runs.iter().map(|x| x.teste.p_valor).collect();
    let esperados = [1.0, 4.0 / 495.0, 24.0 / 495.0, 4.0 / 495.0];
    for (obtido, esperado) in p.iter().zip(esperados) {
        assert_perto!(*obtido, esperado, 1e-13);
    }
    let holm_esperado = [1.0, 16.0 / 495.0, 48.0 / 495.0, 16.0 / 495.0];
    for (x, esperado) in r.runs.iter().zip(holm_esperado) {
        assert_perto!(x.p_valor_holm, esperado, 1e-13);
    }
    assert_eq!(
        r.runs.iter().map(|x| x.rejeita).collect::<Vec<_>>(),
        [false, true, false, true]
    );
    assert_eq!((r.rejeicoes_runs_sem_correcao, r.rejeicoes_runs), (3, 2));
    // Sem a fixa na família (m = 3), o Holm rejeitaria as três.
    let sem_fixa = holm(&p[1..]).unwrap();
    for (obtido, esperado) in sem_fixa
        .iter()
        .zip([12.0 / 495.0, 24.0 / 495.0, 12.0 / 495.0])
    {
        assert_perto!(*obtido, esperado, 1e-13);
    }
    assert!(sem_fixa.iter().all(|&x| x <= ALFA));
}

#[test]
fn resumo_a_mao_sem_dezena_fixa() {
    // {1,2}, {2,3}, {1,2}, {3,4}: O = 2, 3, 2, 1, E = 2 → 4·O − 8 = 0, 4,
    // 0, −4 → Pearson = 32/32 = 1, corrigida 1,5.
    // Dezenas 1 (1010) e 3 (0101): n1 = n0 = 2, R = 4 (máximo), μ = 3; com
    // n1 = n0 = 2 as três contagens (R = 2, 3, 4) empatam em 2 de 6 →
    // P(R ≥ 4) = 1/3, p = 2/3.
    // Dezenas 2 (1110) e 4 (0001): n = 4 com n1 = 3 ou 1, R = 2, μ = 2,5;
    // contagens de R = 2, 3: 2 e 2 → P(R ≤ 2) = 1/2, p = 1.
    // Holm com m = 4: 4·2/3 passa de 1 → todos os ajustados = 1.
    let r =
        testar_aleatoriedade(&MINI, &historico_mini(&[[1, 2], [2, 3], [1, 2], [3, 4]])).unwrap();
    assert_eq!(r.qui_quadrado.pearson, 1.0);
    assert_eq!(r.qui_quadrado.estatistica, 1.5);
    assert_perto!(r.qui_quadrado.p_valor, 6.822_703_303_362_125e-1);
    for (d, sequencias, media, p) in [
        (0, 4, 3.0, 2.0 / 3.0),
        (1, 2, 2.5, 1.0),
        (2, 4, 3.0, 2.0 / 3.0),
        (3, 2, 2.5, 1.0),
    ] {
        let x = &r.runs[d];
        assert_eq!(x.teste.sequencias, sequencias);
        assert_eq!(x.teste.media, media);
        assert_perto!(x.teste.p_valor, p, 1e-15);
        assert_eq!(x.p_valor_holm, 1.0);
    }
    assert_eq!((r.dezenas_testadas_runs, r.dezenas_fixas_runs), (4, 0));
}

#[test]
fn holm_do_resumo_e_o_holm_de_todas_as_dezenas() {
    let mut g = Gerador(2026);
    let l = cfg("quina");
    let r = testar_aleatoriedade(l, &historico_aleatorio(l, 300, &mut g)).unwrap();
    let p: Vec<f64> = r.runs.iter().map(|x| x.teste.p_valor).collect();
    let ajustados: Vec<f64> = r.runs.iter().map(|x| x.p_valor_holm).collect();
    assert_eq!(ajustados, holm(&p).unwrap());
    assert_eq!(r.dezenas_testadas_runs, 80);
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
fn um_concurso_so_deixa_todas_as_dezenas_fixas() {
    // Com T = 1 toda dezena é fixa: todas entram no Holm com p = 1.
    let r = testar_aleatoriedade(&MINI, &historico_mini(&[[2, 4]])).unwrap();
    assert_eq!(r.qui_quadrado.estatistica, 3.0);
    assert_eq!((r.dezenas_testadas_runs, r.dezenas_fixas_runs), (4, 4));
    assert!(r
        .runs
        .iter()
        .all(|x| x.teste.fixa() && x.teste.p_valor == 1.0 && x.p_valor_holm == 1.0 && !x.rejeita));
    assert_eq!((r.rejeicoes_runs_sem_correcao, r.rejeicoes_runs), (0, 0));
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
        // R = 200 é o máximo com n1 = n0 = 100: p = 2·2/C(200, 100) ≈ 4,4e-59.
        assert_eq!(x.teste.sequencias, 200);
        assert!(x.teste.sequencias as f64 > x.teste.media);
        assert!(x.teste.p_valor < 1e-50, "{}", x.teste.p_valor);
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
    // qui-quadrado ≈ 5% (desvio ≈ 1,1 pp; sai 5,25%). Rejeição dos runs por
    // dezena, sem correção: o teste exato é discreto e o dobro da menor
    // cauda é conservador; o tamanho exato, com n1 ~ Binomial(500; 0,1) e a
    // distribuição de R contada em inteiros no Python (`contagens` do
    // comentário das referências), é 3,06% — sai 3,13%, e a janela
    // [2,5%; 3,7%] (~±5 desvios-padrão de ~0,11 pp em 24 000 testes) pega
    // tanto a volta da regra antiga (~4,3%) quanto um teste conservador
    // demais. Com Holm, alguma rejeição em ≤ 5% dos históricos: sai 2,25%
    // (com a aproximação normal antiga, ~9%). O teto de 7% fica ~6 desvios-
    // padrão acima da taxa medida (desvio ≈ 0,7 pp com 400 históricos) e
    // pegaria a volta da normal.
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
    assert!((0.025..=0.037).contains(&taxa_runs), "{resumo}");
    assert!(taxa_familia <= 0.07, "{resumo}");
}
