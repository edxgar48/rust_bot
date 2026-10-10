use super::*;
use crate::avaliacao::tests::MINI;
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

/// Erro relativo `|a − b| / max(|a|, |b|)`.
fn erro_relativo(a: f64, b: f64) -> f64 {
    (a - b).abs() / a.abs().max(b.abs()).max(f64::MIN_POSITIVE)
}

macro_rules! assert_perto {
    ($a:expr, $b:expr, $tol:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(
            erro_relativo(a, b) <= $tol,
            "{} = {a:?}, esperado {b:?} (erro relativo {:e})",
            stringify!($a),
            erro_relativo(a, b)
        );
    }};
}

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: Vec<u32>) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: "2020-01-01".to_string(),
        dezenas,
    }
}

/// Sorteio de *K* de *N* sem reposição (Fisher–Yates parcial).
fn sorteio(cfg: &LoteriaConfig, g: &mut SplitMix64) -> Vec<u32> {
    let mut pool: Vec<u32> = cfg.dezenas().collect();
    (0..cfg.qtd_dezenas)
        .map(|i| {
            let j = i + g.abaixo_de((pool.len() - i) as u64).unwrap() as usize;
            pool.swap(i, j);
            pool[i]
        })
        .collect()
}

/// Histórico sintético dos concursos `de..=ate`.
fn historico(cfg: &LoteriaConfig, de: u32, ate: u32, semente: u64) -> Vec<Resultado> {
    let mut g = SplitMix64::nova(semente);
    (de..=ate)
        .map(|c| {
            let dezenas = sorteio(cfg, &mut g);
            res(cfg, c, dezenas)
        })
        .collect()
}

fn priori(p: f64, s: f64) -> Priori {
    Priori::nova(p, s).expect("priori válida")
}

fn priori_ab(a: f64, b: f64) -> Priori {
    Priori { desvio: 0.0, a, b }
}

// ---------------------------------------------------------------------------
// ln Γ
// ---------------------------------------------------------------------------

/// ln Γ(n) = ln((n−1)!) = Σ_{k=1}^{n−1} ln k: soma independente da série de
/// Stirling (cobre os dois lados do limite de 15).
fn ln_fatorial_por_soma(n_menos_1: u32) -> f64 {
    (1..=n_menos_1).map(|k| f64::from(k).ln()).sum()
}

#[test]
fn ln_gamma_inteiros_contra_soma_de_logaritmos() {
    for n in [3u32, 4, 5, 10, 14, 15, 16, 17, 30, 100, 1000, 10_001] {
        let esperado = ln_fatorial_por_soma(n - 1);
        assert_perto!(ln_gamma(f64::from(n)), esperado, 5e-13);
    }
}

#[test]
fn ln_gamma_valores_conhecidos() {
    // Γ(1) = Γ(2) = 1.
    assert_eq!(ln_gamma(1.0), 0.0);
    assert_eq!(ln_gamma(2.0), 0.0);
    // Γ(1/2) = √π; Γ(3/2) = √π/2.
    let ln_raiz_pi = 0.5 * std::f64::consts::PI.ln();
    assert_perto!(ln_gamma(0.5), ln_raiz_pi, 1e-14);
    assert_perto!(ln_gamma(1.5), ln_raiz_pi - 2f64.ln(), 1e-13);
    // Γ(m + 1/2) = √π·Π_{j<m}(j + 1/2): m = 40 e m = 4000 (igual ao
    // `ln_gama_meio` da aleatoriedade, mas recalculado aqui).
    for m in [40u32, 4000] {
        let esperado = (0..m).fold(ln_raiz_pi, |s, j| s + (f64::from(j) + 0.5).ln());
        assert_perto!(ln_gamma(f64::from(m) + 0.5), esperado, 5e-13);
    }
    // Γ(0,1) = 9,513507698668732 (tabela).
    assert_perto!(ln_gamma(0.1), 9.513_507_698_668_732f64.ln(), 1e-14);
    // 170! = 7,257415615307994e306 (maior fatorial finito em f64).
    let ln_170_fatorial = 306.0 * 10f64.ln() + 7.257_415_615_307_994f64.ln();
    assert_perto!(ln_gamma(171.0), ln_170_fatorial, 1e-14);
}

#[test]
fn ln_gamma_recorrencia_e_continuidade() {
    // lnΓ(x + 1) = lnΓ(x) + ln x, inclusive atravessando o limite de 15.
    let mut g = SplitMix64::nova(7);
    for _ in 0..500 {
        let x = 0.05 + (g.proximo() >> 11) as f64 / (1u64 << 53) as f64 * 300.0;
        let esquerda = ln_gamma(x + 1.0);
        let direita = ln_gamma(x) + x.ln();
        assert!(
            (esquerda - direita).abs() <= 1e-12 * esquerda.abs().max(1.0),
            "x = {x}: {esquerda} contra {direita}"
        );
    }
    let abaixo = ln_gamma(LIMITE_STIRLING.next_down());
    let no_limite = ln_gamma(LIMITE_STIRLING);
    assert_perto!(abaixo, no_limite, 1e-14);
}

#[test]
fn ln_gamma_fora_do_dominio_e_nan() {
    for x in [0.0, -1.0, -0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(ln_gamma(x).is_nan(), "ln_gamma({x}) deveria ser NaN");
    }
}

// ---------------------------------------------------------------------------
// Priori
// ---------------------------------------------------------------------------

#[test]
fn priori_tem_a_media_e_o_desvio_pedidos() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let p = c.qtd_dezenas as f64 / f64::from(c.qtd_dezenas_possiveis());
        for s in DESVIOS_PRIORI {
            let q = priori(p, s);
            let soma = q.a + q.b;
            assert_perto!(q.a / soma, p, 1e-13);
            // Variância da Beta: ab / ((a+b)²(a+b+1)) = (s·p)².
            let desvio = (q.a * q.b / (soma * soma * (soma + 1.0))).sqrt();
            assert_perto!(desvio, s * p, 1e-12);
        }
    }
}

#[test]
fn priori_valores_conferidos_a_mao() {
    // p = 1/2, s = 1/√3: a + b = 1/s² − 1 = 2 → Beta(1, 1) (uniforme).
    let q = priori(0.5, 1.0 / 3f64.sqrt());
    assert_perto!(q.a, 1.0, 1e-12);
    assert_perto!(q.b, 1.0, 1e-12);
    // Lotofácil, s = 5 %: a + b = 0,4/(0,0025·0,6) − 1 = 265,666…; a = 159,4.
    let q = priori(0.6, 0.05);
    assert_perto!(q.a + q.b, 265.0 + 2.0 / 3.0, 1e-12);
    assert_perto!(q.a, 159.4, 1e-12);
}

#[test]
fn priori_recusa_parametros_sem_sentido() {
    for (p, s) in [
        (0.0, 0.05),
        (1.0, 0.05),
        (-0.1, 0.05),
        (f64::NAN, 0.05),
        (0.5, 0.0),
        (0.5, -0.1),
        (0.5, f64::NAN),
        (0.5, f64::INFINITY),
        // s² ≥ (1−p)/p: a + b ≤ 0.
        (0.5, 1.0),
        (0.5, 2.0),
    ] {
        assert!(
            matches!(
                Priori::nova(p, s),
                Err(SpiderError::EvidenciaEntradaInvalida(_))
            ),
            "p = {p}, s = {s}"
        );
    }
}

// ---------------------------------------------------------------------------
// Fator de Bayes
// ---------------------------------------------------------------------------

/// Cada caso: (x, n, p, a, b, FB exato como fração, conta).
///
/// Marginal da sequência sob H1 = B(x+a, n−x+b)/B(a, b), sob H0 =
/// p^x (1−p)^(n−x); `B(u, v) = (u−1)!(v−1)!/(u+v−1)!` para inteiros.
///
/// - Beta(1,1), p = 1/2, n = 10:
///   - x = 5:  B(6,6) = 5!5!/11! = 1/2772;  FB = 2^10/2772 = 1024/2772;
///   - x = 8:  B(9,3) = 8!2!/11! = 1/495;   FB = 1024/495;
///   - x = 10: B(11,1) = 10!0!/11! = 1/11;  FB = 1024/11; x = 0 igual.
/// - Beta(1,3), p = 1/4, B(1,3) = 1/3, n = 3:
///   - x = 1: B(2,5)/B(1,3) = (1/30)·3 = 1/10; H0 = (1/4)(3/4)² = 9/64;
///     FB = 64/90 = 32/45;
///   - x = 0: B(1,6)/B(1,3) = (1/6)·3 = 1/2; H0 = 27/64; FB = 32/27;
///   - x = 3: B(4,3)/B(1,3) = (1/60)·3 = 1/20; H0 = 1/64; FB = 64/20 = 16/5;
///   - x = 2: B(3,4)/B(1,3) = (1/60)·3 = 1/20; H0 = (1/16)(3/4) = 3/64;
///     FB = 64/60 = 16/15.
///     (Conferência: Σ_x C(3,x)·marginal = 1/2 + 3/10 + 3/20 + 1/20 = 1.)
#[test]
fn fator_de_bayes_contra_contas_a_mao() {
    let casos: [(u64, u64, f64, f64, f64, f64); 8] = [
        (5, 10, 0.5, 1.0, 1.0, 1024.0 / 2772.0),
        (8, 10, 0.5, 1.0, 1.0, 1024.0 / 495.0),
        (10, 10, 0.5, 1.0, 1.0, 1024.0 / 11.0),
        (0, 10, 0.5, 1.0, 1.0, 1024.0 / 11.0),
        (1, 3, 0.25, 1.0, 3.0, 32.0 / 45.0),
        (0, 3, 0.25, 1.0, 3.0, 32.0 / 27.0),
        (3, 3, 0.25, 1.0, 3.0, 16.0 / 5.0),
        (2, 3, 0.25, 1.0, 3.0, 16.0 / 15.0),
    ];
    for (x, n, p, a, b, fb) in casos {
        let obtido = log10_fator_bayes(x, n, p, &priori_ab(a, b)).unwrap();
        assert_perto!(obtido, fb.log10(), 1e-12);
    }
}

#[test]
fn fator_de_bayes_zero_concursos_e_exatamente_um() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let p = c.qtd_dezenas as f64 / f64::from(c.qtd_dezenas_possiveis());
        for s in DESVIOS_PRIORI {
            let v = log10_fator_bayes(0, 0, p, &priori(p, s)).unwrap();
            assert_eq!(v, 0.0, "{slug}, s = {s}");
            let c = classificar(v).unwrap();
            assert_eq!(c.direcao, Direcao::Acaso);
            assert_eq!(c.grau, Grau::MuitoFraca);
        }
    }
}

#[test]
fn fator_de_bayes_simetria() {
    // p = 1/2, Beta simétrica: FB(x) = FB(n − x).
    let n = 37u64;
    let q = priori(0.5, 0.05);
    assert_perto!(q.a, q.b, 1e-14);
    for x in 0..=n {
        let a = log10_fator_bayes(x, n, 0.5, &q).unwrap();
        let b = log10_fator_bayes(n - x, n, 0.5, &q).unwrap();
        assert!((a - b).abs() <= 1e-12, "x = {x}: {a} contra {b}");
    }
    // Em geral: trocar saiu/não saiu troca p por 1 − p e a por b.
    let p = 0.0625;
    let q = priori(p, 0.05);
    let espelho = Priori {
        desvio: q.desvio,
        a: q.b,
        b: q.a,
    };
    for x in [0u64, 1, 7, 20, 35, 50] {
        let a = log10_fator_bayes(x, 50, p, &q).unwrap();
        let b = log10_fator_bayes(50 - x, 50, 1.0 - p, &espelho).unwrap();
        assert!((a - b).abs() <= 1e-12, "x = {x}: {a} contra {b}");
    }
}

#[test]
fn fator_de_bayes_e_uma_razao_de_verossimilhancas_que_soma_um() {
    // Σ_x Binomial(x; n, p)·FB(x) = Σ_x BetaBinomial(x; n, a, b) = 1.
    for (p, n) in [(0.1, 50u64), (0.0625, 400), (0.6, 400)] {
        let q = priori(p, 0.05);
        let nf = n as f64;
        let ln_q = (-p).ln_1p();
        let soma: f64 = (0..=n)
            .map(|x| {
                let xf = x as f64;
                let ln_pmf = ln_gamma(nf + 1.0) - ln_gamma(xf + 1.0) - ln_gamma(nf - xf + 1.0)
                    + xf * p.ln()
                    + (nf - xf) * ln_q;
                let fb = 10f64.powf(log10_fator_bayes(x, n, p, &q).unwrap());
                ln_pmf.exp() * fb
            })
            .sum();
        assert_perto!(soma, 1.0, 1e-9);
    }
}

#[test]
fn fator_de_bayes_valores_extremos_de_loteria_sao_finitos() {
    // Quina, n = 7137, todas as dezenas possíveis: sempre finito, sempre
    // favorecendo H0 no centro e H1 nas pontas.
    let p = 5.0 / 80.0;
    for s in DESVIOS_PRIORI {
        let q = priori(p, s);
        let centro = log10_fator_bayes((7137.0 * p).round() as u64, 7137, p, &q).unwrap();
        assert!(centro < 0.0, "s = {s}: {centro}");
        let ponta = log10_fator_bayes(700, 7137, p, &q).unwrap();
        assert!(ponta > 2.0, "s = {s}: {ponta}");
    }
}

#[test]
fn fator_de_bayes_recusa_entrada_invalida() {
    let q = priori(0.5, 0.05);
    assert!(log10_fator_bayes(11, 10, 0.5, &q).is_err());
    assert!(log10_fator_bayes(1, 10, 0.0, &q).is_err());
    assert!(log10_fator_bayes(1, 10, 1.0, &q).is_err());
    assert!(log10_fator_bayes(1, 10, f64::NAN, &q).is_err());
    assert!(log10_fator_bayes(1, 10, 0.5, &priori_ab(0.0, 1.0)).is_err());
    assert!(log10_fator_bayes(1, 10, 0.5, &priori_ab(1.0, f64::NAN)).is_err());
}

// ---------------------------------------------------------------------------
// Graus
// ---------------------------------------------------------------------------

fn classe(v: f64) -> (Grau, Direcao) {
    let c = classificar(v).unwrap();
    (c.grau, c.direcao)
}

#[test]
fn graus_exatamente_nos_limites() {
    // Limites em log10, fechados à esquerda: no limite, o grau de cima; um
    // passo (1 ulp) abaixo, o de baixo. Vale nas duas direções.
    let limites = [
        (3f64.log10(), Grau::MuitoFraca, Grau::Fraca),
        (1.0, Grau::Fraca, Grau::Moderada),
        (30f64.log10(), Grau::Moderada, Grau::Forte),
        (2.0, Grau::Forte, Grau::MuitoForte),
    ];
    for (limite, abaixo, acima) in limites {
        assert_eq!(classe(limite), (acima, Direcao::Vies), "limite {limite}");
        assert_eq!(
            classe(limite.next_down()),
            (abaixo, Direcao::Vies),
            "logo abaixo de {limite}"
        );
        assert_eq!(classe(-limite), (acima, Direcao::Acaso), "limite -{limite}");
        assert_eq!(
            classe((-limite).next_up()),
            (abaixo, Direcao::Acaso),
            "logo acima de -{limite}"
        );
    }
    // Os valores dos limites (log10 3 = 0,4771…, log10 30 = 1,4771…).
    assert_perto!(3f64.log10(), 0.477_121_254_719_662_4, 1e-15);
    assert_perto!(30f64.log10(), 1.477_121_254_719_662_4, 1e-15);
}

#[test]
fn graus_no_meio_de_cada_faixa() {
    for (v, grau) in [
        (0.1, Grau::MuitoFraca),
        (0.7, Grau::Fraca),
        (1.2, Grau::Moderada),
        (1.7, Grau::Forte),
        (5.0, Grau::MuitoForte),
        (300.0, Grau::MuitoForte),
    ] {
        assert_eq!(classe(v), (grau, Direcao::Vies));
        assert_eq!(classe(-v), (grau, Direcao::Acaso));
    }
}

#[test]
fn fb_igual_a_um_e_a_favor_do_acaso() {
    assert_eq!(classe(0.0), (Grau::MuitoFraca, Direcao::Acaso));
    assert_eq!(classe(-0.0), (Grau::MuitoFraca, Direcao::Acaso));
    // Um passo acima de FB = 1 já é "a favor de viés" (muito fraca).
    assert_eq!(classe(f64::MIN_POSITIVE), (Grau::MuitoFraca, Direcao::Vies));
    assert_eq!(
        classe(-f64::MIN_POSITIVE),
        (Grau::MuitoFraca, Direcao::Acaso)
    );
}

#[test]
fn classificar_recusa_valor_nao_finito() {
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            classificar(v),
            Err(SpiderError::EvidenciaEntradaInvalida(_))
        ));
    }
}

#[test]
fn nomes_e_ordem_dos_graus() {
    assert!(Grau::TODOS.windows(2).all(|w| w[0] < w[1]));
    for (i, g) in Grau::TODOS.iter().enumerate() {
        assert_eq!(g.indice(), i);
    }
    assert_eq!(Grau::MuitoFraca.nome(), "muito fraca");
    assert_eq!(Grau::MuitoForte.nome(), "muito forte");
    assert_eq!(Direcao::Vies.nome(), "a favor de viés");
    assert_eq!(Direcao::Acaso.nome(), "a favor do acaso");
}

#[test]
fn grade_indexa_por_direcao_e_grau() {
    let g = Grade::de_celulas([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(g.do_grau(Grau::MuitoFraca, Direcao::Vies), 1);
    assert_eq!(g.do_grau(Grau::MuitoForte, Direcao::Vies), 5);
    assert_eq!(g.do_grau(Grau::MuitoFraca, Direcao::Acaso), 6);
    assert_eq!(g.do_grau(Grau::MuitoForte, Direcao::Acaso), 10);
    assert_eq!(g.total(), 55);
    assert_eq!(g.a_partir_de(Grau::Moderada), 3 + 4 + 5 + 8 + 9 + 10);
    let f = Grade::de_celulas([0.5; 10]);
    assert_eq!(f.total(), 5.0);
    assert_eq!(f.a_partir_de(Grau::Forte), 2.0);
}

// ---------------------------------------------------------------------------
// Evidência por dezena
// ---------------------------------------------------------------------------

#[test]
fn evidencia_confere_contagem_e_formula() {
    let c = cfg("lotofacil");
    let h = historico(c, 1, 120, 11);
    let e = evidencia_por_dezena(c, &h, 121, Janela::Inteira).unwrap();
    assert_eq!(
        (e.t, e.concursos, e.primeiro_concurso, e.ultimo_concurso),
        (121, 120, 1, 120)
    );
    assert_eq!(e.dezenas.len(), 25);
    assert_perto!(e.p, 0.6, 1e-15);
    let mut total_saidas = 0;
    for (i, d) in e.dezenas.iter().enumerate() {
        assert_eq!(d.dezena, 1 + i as u32);
        assert_eq!(d.concursos, 120);
        let x = h.iter().filter(|r| r.dezenas.contains(&d.dezena)).count() as u64;
        assert_eq!(d.saidas, x);
        total_saidas += x;
        for (k, &s) in DESVIOS_PRIORI.iter().enumerate() {
            let q = priori(0.6, s);
            let fb = log10_fator_bayes(x, 120, 0.6, &q).unwrap();
            assert_eq!(d.log10_fb[k], fb);
            assert_eq!(d.classificacao[k], classificar(fb).unwrap());
        }
    }
    // Cada concurso soma K saídas.
    assert_eq!(total_saidas, 120 * 15);
    for g in &e.observada {
        assert_eq!(g.total(), 25);
    }
}

#[test]
fn evidencia_de_dezena_muito_enviesada_e_muito_forte() {
    // Dezena 1 sempre fora e dezena 2 sempre dentro, resto sorteado: viés
    // enorme, os dois muito fortes a favor de viés; demais dezenas, não.
    let c = cfg("lotofacil");
    let mut g = SplitMix64::nova(5);
    let h: Vec<Resultado> = (1..=200)
        .map(|n| {
            let mut pool: Vec<u32> = (3..=25).collect();
            for i in 0..14 {
                let j = i + g.abaixo_de((pool.len() - i) as u64).unwrap() as usize;
                pool.swap(i, j);
            }
            let mut dezenas = vec![2];
            dezenas.extend_from_slice(&pool[..14]);
            res(c, n, dezenas)
        })
        .collect();
    let e = evidencia_por_dezena(c, &h, 201, Janela::Inteira).unwrap();
    for d in &e.dezenas[..2] {
        assert_eq!(d.classificacao[0].grau, Grau::MuitoForte, "{d:?}");
        assert_eq!(d.classificacao[0].direcao, Direcao::Vies);
    }
    assert_eq!(e.dezenas[0].saidas, 0);
    assert_eq!(e.dezenas[1].saidas, 200);
}

#[test]
fn evidencia_de_t_nao_depende_de_concursos_a_partir_de_t() {
    let c = cfg("megasena");
    let h = historico(c, 1, 200, 3);
    let t = 101;
    for janela in [Janela::Inteira, Janela::Ultimos(30), Janela::Ultimos(500)] {
        let base = evidencia_por_dezena(c, &h, t, janela).unwrap();

        // Só os concursos < t.
        let so_passado: Vec<Resultado> = h.iter().filter(|r| r.concurso < t).cloned().collect();
        assert_eq!(
            base,
            evidencia_por_dezena(c, &so_passado, t, janela).unwrap()
        );

        // Concursos ≥ t alterados, apagados, repetidos e até inválidos
        // (dezena fora da faixa, buraco, duplicata) não mudam nada.
        let mut alterado = so_passado.clone();
        alterado.extend(historico(c, t, 200, 999));
        alterado.push(res(c, 250, vec![1, 2, 3])); // buraco e dezenas a menos
        alterado.push(res(c, t, vec![99; 6])); // repetido e fora da faixa
        alterado.reverse(); // fora de ordem
        assert_eq!(base, evidencia_por_dezena(c, &alterado, t, janela).unwrap());
    }
}

#[test]
fn evidencia_usa_exatamente_ate_t_menos_1() {
    let c = cfg("quina");
    let h = historico(c, 1, 60, 8);
    let e = evidencia_por_dezena(c, &h, 31, Janela::Inteira).unwrap();
    assert_eq!(
        (e.concursos, e.primeiro_concurso, e.ultimo_concurso),
        (30, 1, 30)
    );
    assert!(e.dezenas.iter().all(|d| d.concursos == 30));
    // O concurso 31 existe em `h`, mas não entra: com t = 32 entra.
    let seguinte = evidencia_por_dezena(c, &h, 32, Janela::Inteira).unwrap();
    assert_eq!(seguinte.concursos, 31);
    assert_ne!(e.dezenas, seguinte.dezenas);
}

#[test]
fn janela_secundaria_pega_os_ultimos_concursos_antes_de_t() {
    let c = cfg("lotofacil");
    let h = historico(c, 1, 80, 21);
    let e = evidencia_por_dezena(c, &h, 61, Janela::Ultimos(10)).unwrap();
    assert_eq!(
        (e.concursos, e.primeiro_concurso, e.ultimo_concurso),
        (10, 51, 60)
    );
    for d in &e.dezenas {
        let x = h
            .iter()
            .filter(|r| (51..=60).contains(&r.concurso) && r.dezenas.contains(&d.dezena))
            .count() as u64;
        assert_eq!((d.saidas, d.concursos), (x, 10));
    }
    // Janela maior que o histórico: min(J, t − 1), igual à inteira.
    let curta = evidencia_por_dezena(c, &h, 21, Janela::Ultimos(JANELA_SECUNDARIA)).unwrap();
    let inteira = evidencia_por_dezena(c, &h, 21, Janela::Inteira).unwrap();
    assert_eq!(curta.concursos, 20);
    assert_eq!(curta.dezenas, inteira.dezenas);
    assert_eq!(curta.janela, Janela::Ultimos(JANELA_SECUNDARIA));
}

#[test]
fn evidencia_primeiro_concurso_sem_historico_e_erro() {
    let c = cfg("lotofacil");
    let h = historico(c, 1, 10, 1);
    assert!(matches!(
        evidencia_por_dezena(c, &h, 1, Janela::Inteira),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        evidencia_por_dezena(c, &[], 5, Janela::Inteira),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    // Um único concurso de histórico (t = 2) já calcula.
    let e = evidencia_por_dezena(c, &h, 2, Janela::Inteira).unwrap();
    assert_eq!(e.concursos, 1);
}

fn detalhe(r: Result<EvidenciaJanela>) -> String {
    match r {
        Err(SpiderError::EvidenciaEntradaInvalida(m)) => m,
        outro => panic!("esperado EvidenciaEntradaInvalida, veio {outro:?}"),
    }
}

#[test]
fn evidencia_recusa_historico_invalido() {
    let c = cfg("lotofacil");
    let h = historico(c, 1, 30, 4);

    // Buraco dentro da janela.
    let mut com_buraco = h.clone();
    com_buraco.remove(14); // sem o concurso 15
    let m = detalhe(evidencia_por_dezena(c, &com_buraco, 31, Janela::Inteira));
    assert!(m.contains("faltam os concursos 15..=15"), "{m}");

    // Concurso repetido.
    let mut repetido = h.clone();
    repetido.push(h[9].clone());
    let m = detalhe(evidencia_por_dezena(c, &repetido, 31, Janela::Inteira));
    assert!(m.contains("concurso 10 repetido"), "{m}");

    // Dezenas inválidas (quantidade errada).
    let mut curto = h.clone();
    curto[4].dezenas.pop();
    let m = detalhe(evidencia_por_dezena(c, &curto, 31, Janela::Inteira));
    assert!(m.contains("concurso 5"), "{m}");

    // Janela de 0 concursos.
    assert!(evidencia_por_dezena(c, &h, 31, Janela::Ultimos(0)).is_err());

    // Buraco antes da janela secundária não é visto por ela (a orquestração
    // valida o histórico todo antes); dentro dela, sim.
    let mut velho = h.clone();
    velho.remove(2);
    assert!(evidencia_por_dezena(c, &velho, 31, Janela::Ultimos(10)).is_ok());
    assert!(evidencia_por_dezena(c, &velho, 31, Janela::Ultimos(29)).is_err());
}

#[test]
fn evidencia_exige_k_menor_que_n() {
    let todas = LoteriaConfig {
        qtd_dezenas: 4,
        ..MINI
    };
    let h = vec![res(&todas, 1, vec![1, 2, 3, 4])];
    assert!(matches!(
        evidencia_por_dezena(&todas, &h, 2, Janela::Inteira),
        Err(SpiderError::EvidenciaEntradaInvalida(_))
    ));
}

#[test]
fn evidencia_e_deterministica_e_independe_da_ordem_da_entrada() {
    let c = cfg("megasena");
    let h = historico(c, 1, 150, 17);
    let a = evidencia_por_dezena(c, &h, 151, Janela::Inteira).unwrap();
    let b = evidencia_por_dezena(c, &h, 151, Janela::Inteira).unwrap();
    assert_eq!(a, b);
    let mut invertido = h.clone();
    invertido.reverse();
    assert_eq!(
        a,
        evidencia_por_dezena(c, &invertido, 151, Janela::Inteira).unwrap()
    );
    for (x, y) in a.dezenas.iter().zip(&b.dezenas) {
        for k in 0..QTD_PRIORIS {
            assert_eq!(x.log10_fb[k].to_bits(), y.log10_fb[k].to_bits());
        }
    }
}

#[test]
fn baseline_vem_da_config() {
    let mini = historico(&MINI, 1, 10, 2);
    let e = evidencia_por_dezena(&MINI, &mini, 11, Janela::Inteira).unwrap();
    assert_perto!(e.p, 0.5, 1e-15);
    assert_eq!(e.dezenas.len(), 4);
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let h = historico(c, 1, 5, 2);
        let e = evidencia_por_dezena(c, &h, 6, Janela::Inteira).unwrap();
        assert_perto!(
            e.p,
            c.qtd_dezenas as f64 / f64::from(c.qtd_dezenas_possiveis()),
            1e-15
        );
        for (q, s) in e.prioris.iter().zip(DESVIOS_PRIORI) {
            assert_eq!(q.desvio, s);
            assert_perto!(q.a / (q.a + q.b), e.p, 1e-13);
        }
    }
}

// ---------------------------------------------------------------------------
// Esperado sob o acaso
// ---------------------------------------------------------------------------

#[test]
fn esperado_exato_soma_o_numero_de_dezenas() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        for s in DESVIOS_PRIORI {
            for n in [1u64, 50, 500, 3799] {
                let g = esperado_exato(c, n, s).unwrap();
                assert_perto!(g.total(), f64::from(c.qtd_dezenas_possiveis()), 1e-9);
            }
        }
    }
}

#[test]
fn esperado_exato_com_um_concurso_so_cai_em_muito_fraca() {
    // n = 1: x ∈ {0, 1}; FB perto de 1 nos dois casos.
    let c = cfg("quina");
    let g = esperado_exato(c, 1, 0.05).unwrap();
    assert_perto!(g.vies[0] + g.acaso[0], 80.0, 1e-9);
}

#[test]
fn simulacao_confere_com_o_valor_exato_da_binomial() {
    // Esperança da contagem de dezenas por (direção, grau): linear, então a
    // média das réplicas converge para N·P(x na célula), x ~ Binomial(n, p),
    // mesmo com as dezenas do concurso dependentes.
    for (slug, n) in [("lotofacil", 400usize), ("megasena", 300), ("quina", 400)] {
        let c = cfg(slug);
        let sim = esperado_sob_o_acaso(c, n, &DESVIOS_PRIORI, REPLICAS_ESPERADO, SEMENTE_ESPERADO)
            .unwrap();
        assert_eq!(
            (sim.concursos, sim.replicas, sim.semente),
            (n, 1000, 20_261_008)
        );
        assert_eq!(sim.por_priori.len(), QTD_PRIORIS);
        for (esp, &s) in sim.por_priori.iter().zip(&DESVIOS_PRIORI) {
            assert_eq!(esp.desvio, s);
            let exato = esperado_exato(c, n as u64, s).unwrap();
            // Cada réplica contribui N dezenas.
            assert_perto!(
                esp.media.total(),
                f64::from(c.qtd_dezenas_possiveis()),
                1e-12
            );
            let mut conferidas_acima_de_um_decimo = 0;
            for grau in Grau::TODOS {
                for direcao in [Direcao::Vies, Direcao::Acaso] {
                    let (m, e, ep) = (
                        esp.media.do_grau(grau, direcao),
                        exato.do_grau(grau, direcao),
                        esp.erro_padrao.do_grau(grau, direcao),
                    );
                    let tolerancia = (5.0 * ep).max(0.02);
                    assert!(
                        (m - e).abs() <= tolerancia,
                        "{slug}, s = {s}, {grau:?}/{direcao:?}: simulado {m}, exato {e} (±{ep})"
                    );
                    if e > 0.1 {
                        conferidas_acima_de_um_decimo += 1;
                    }
                }
            }
            // O teste não pode passar vazio: ao menos uma célula relevante.
            assert!(conferidas_acima_de_um_decimo >= 1, "{slug}, s = {s}");
        }
    }
}

#[test]
fn simulacao_e_deterministica_e_depende_da_semente() {
    let c = cfg("lotofacil");
    let a = esperado_sob_o_acaso(c, 100, &DESVIOS_PRIORI, 50, SEMENTE_ESPERADO).unwrap();
    let b = esperado_sob_o_acaso(c, 100, &DESVIOS_PRIORI, 50, SEMENTE_ESPERADO).unwrap();
    assert_eq!(a, b);
    let outra = esperado_sob_o_acaso(c, 100, &DESVIOS_PRIORI, 50, SEMENTE_ESPERADO + 1).unwrap();
    assert_ne!(a.por_priori, outra.por_priori);
}

#[test]
fn simulacao_recusa_parametros_invalidos() {
    let c = cfg("lotofacil");
    for r in [
        esperado_sob_o_acaso(c, 0, &DESVIOS_PRIORI, 10, 1),
        esperado_sob_o_acaso(c, 10, &DESVIOS_PRIORI, 1, 1),
        esperado_sob_o_acaso(c, 10, &[], 10, 1),
        esperado_sob_o_acaso(c, 10, &[0.0], 10, 1),
    ] {
        assert!(
            matches!(r, Err(SpiderError::EvidenciaEntradaInvalida(_))),
            "{r:?}"
        );
    }
}

#[test]
fn simulacao_sorteia_k_distintas_por_concurso() {
    // Com n = 1 por réplica, a soma das saídas é K: a contagem de dezenas
    // em cada célula é consistente com x ∈ {0, 1} e N dezenas.
    let c = cfg("megasena");
    let s = esperado_sob_o_acaso(c, 1, &[0.05], 200, 3).unwrap();
    assert_perto!(s.por_priori[0].media.total(), 60.0, 1e-12);
    // Com n = 1, 6 das 60 dezenas têm x = 1 em cada réplica: as duas
    // células ocupadas são fixas e a variância entre réplicas é zero.
    let ocupadas = s.por_priori[0]
        .erro_padrao
        .vies
        .iter()
        .chain(&s.por_priori[0].erro_padrao.acaso)
        .filter(|&&e| e > 0.0)
        .count();
    assert_eq!(ocupadas, 0);
}

/// O esperado exato da Mega-Sena com n = 3067 e o simulado da mesma corrida
/// do relatório concordam (usa as constantes de produção; mais lento).
#[test]
fn simulacao_de_producao_confere_com_o_exato_na_megasena() {
    let c = cfg("megasena");
    let n = 3067usize;
    let sim =
        esperado_sob_o_acaso(c, n, &DESVIOS_PRIORI, REPLICAS_ESPERADO, SEMENTE_ESPERADO).unwrap();
    let esp = &sim.por_priori[0];
    let exato = esperado_exato(c, n as u64, DESVIOS_PRIORI[0]).unwrap();
    for grau in Grau::TODOS {
        for direcao in [Direcao::Vies, Direcao::Acaso] {
            let (m, e, ep) = (
                esp.media.do_grau(grau, direcao),
                exato.do_grau(grau, direcao),
                esp.erro_padrao.do_grau(grau, direcao),
            );
            assert!(
                (m - e).abs() <= (5.0 * ep).max(0.02),
                "{grau:?}/{direcao:?}: {m} contra {e}"
            );
        }
    }
}

// ------------------------------------------- teto a favor do acaso

#[test]
fn teto_conferido_a_mao() {
    // p = 1/2, s = 10 %: Var_priori/Var_amostral = n·s²·p/(1−p) = 0,01·n.
    // n = 9900 → 99 → √(1 + 99) = 10 → log10 = 1.
    assert_perto!(teto_a_favor_do_acaso(9_900, 0.5, 0.10).unwrap(), 1.0, 1e-14);
    // n = 99 → razão 0,99 → √1,99.
    assert_perto!(
        teto_a_favor_do_acaso(99, 0.5, 0.10).unwrap(),
        0.5 * 1.99_f64.log10(),
        1e-14
    );
    // Sem concursos não há o que acumular: FB = 1.
    assert_eq!(teto_a_favor_do_acaso(0, 0.6, 0.05).unwrap(), 0.0);
}

#[test]
fn teto_reproduz_os_numeros_do_plano() {
    // Plano (decisão de 2026-10-10): priori de 5 %, histórico inteiro de
    // 2026-10: ~0,13 na Mega-Sena (n ≈ 3070), ~0,17 na Quina (n ≈ 7150) e
    // ~0,59 na Lotofácil (n ≈ 3800). Na janela de 500 (a mão: 0,5·log10(1 +
    // 500·0,0025·p/(1−p))): 0,028 na Mega-Sena, 0,017 na Quina e 0,229 na
    // Lotofácil (o "~0,03" do plano vale para a Mega-Sena).
    let casos = [
        ("megasena", 3_070_u64, 0.13, 0.028),
        ("quina", 7_150, 0.17, 0.017),
        ("lotofacil", 3_800, 0.59, 0.229),
    ];
    for (slug, n, esperado, na_janela) in casos {
        let c = cfg(slug);
        let p = c.qtd_dezenas as f64 / f64::from(c.qtd_dezenas_possiveis());
        let teto = teto_a_favor_do_acaso(n, p, 0.05).unwrap();
        assert!((teto - esperado).abs() < 0.01, "{slug}: {teto}");
        let pequeno = teto_a_favor_do_acaso(500, p, 0.05).unwrap();
        assert!((pequeno - na_janela).abs() < 0.001, "{slug}: {pequeno}");
    }
}

#[test]
fn teto_cresce_com_n_e_com_a_largura_da_priori() {
    let p = 0.1;
    let mut anterior = 0.0;
    for n in [1_u64, 10, 100, 1_000, 10_000, 100_000] {
        let t = teto_a_favor_do_acaso(n, p, 0.05).unwrap();
        assert!(t > anterior, "n = {n}");
        anterior = t;
    }
    let mut anterior = 0.0;
    for s in [0.025, 0.05, 0.10] {
        let t = teto_a_favor_do_acaso(3_000, p, s).unwrap();
        assert!(t > anterior, "s = {s}");
        anterior = t;
    }
}

#[test]
fn teto_confere_com_o_fator_de_bayes_exato_em_x_igual_a_n_p() {
    // O máximo do FB a favor de H0 é atingido em x = n·p; a fórmula é a
    // aproximação normal desse ponto, então o FB exato de `log10_fator_bayes`
    // em x = n·p (n·p inteiro) tem de ficar perto de −teto. Mede-se o erro:
    // abaixo de 1 % do teto a partir de n = 1000.
    let casos = [
        (0.6, 1_000_u64),
        (0.6, 3_800),
        (0.1, 3_000),
        (0.1, 1_000),
        (0.0625, 3_200),
        (0.0625, 16_000),
        (0.5, 10_000),
    ];
    for (p, n) in casos {
        let x = (n as f64 * p).round() as u64;
        assert_eq!(x as f64, n as f64 * p, "n·p inteiro: n = {n}, p = {p}");
        for s in DESVIOS_PRIORI {
            let priori = Priori::nova(p, s).unwrap();
            let exato = -log10_fator_bayes(x, n, p, &priori).unwrap();
            let teto = teto_a_favor_do_acaso(n, p, s).unwrap();
            assert!(exato > 0.0, "a favor do acaso: p = {p}, n = {n}, s = {s}");
            assert!(
                (exato - teto).abs() <= 0.01 * teto,
                "p = {p}, n = {n}, s = {s}: exato {exato}, teto {teto}"
            );
        }
    }
}

#[test]
fn teto_vale_como_maximo_no_entorno_de_x_igual_a_n_p() {
    // Nenhum x vizinho dá mais evidência a favor de H0 que o ponto n·p (a
    // menos do erro da aproximação).
    let (p, n) = (0.6, 1_000_u64);
    let priori = Priori::nova(p, 0.05).unwrap();
    let no_pico = -log10_fator_bayes(600, n, p, &priori).unwrap();
    for x in 560..=640 {
        let a_favor_do_acaso = -log10_fator_bayes(x, n, p, &priori).unwrap();
        assert!(a_favor_do_acaso <= no_pico + 1e-9, "x = {x}");
    }
}

#[test]
fn teto_recusa_entradas_invalidas() {
    for (p, s) in [
        (0.0, 0.05),
        (1.0, 0.05),
        (-0.1, 0.05),
        (f64::NAN, 0.05),
        (0.5, 0.0),
        (0.5, -0.05),
        (0.5, f64::NAN),
        (0.5, f64::INFINITY),
    ] {
        assert!(
            matches!(
                teto_a_favor_do_acaso(100, p, s),
                Err(SpiderError::EvidenciaEntradaInvalida(_))
            ),
            "p = {p}, s = {s}"
        );
    }
}
