use super::*;

// Valores de referência calculados de forma independente em Python 3
// (inteiros de precisão arbitrária, máscara de 64 bits), com o trecho abaixo.
// Os de `SplitMix64::nova(1234567)` são também os publicados no Rosetta Code
// ("Pseudo-random numbers/Splitmix64"); o primeiro de `nova(0)`
// (0xe220a8397b1dcdaf) é o da implementação de referência de Vigna.
//
// ```python
// M = (1 << 64) - 1
// class SM:
//     def __init__(s, seed): s.x = seed & M
//     def next(s):
//         s.x = (s.x + 0x9E3779B97F4A7C15) & M
//         z = s.x
//         z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & M
//         z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & M
//         return z ^ (z >> 31)
//     def below(s, n):
//         lim = ((1 << 64) - n) % n
//         while True:
//             x = s.next()
//             if x >= lim: return x % n
//
// def boot(serie, b, B, seed):
//     T = len(serie); r = SM(seed); m = sum(serie) / T
//     medias = []
//     for _ in range(B):
//         s = 0.0; n = 0
//         for _ in range(-(-T // b)):
//             ini = r.below(T)
//             for j in range(b):
//                 if n == T: break
//                 s += serie[(ini + j) % T]; n += 1
//         medias.append(s / T)
//     p = (1 + sum(1 for x in medias if x - m <= m)) / (B + 1)
//     o = sorted(medias)
//     return m, p, o[(B * 25 + 999) // 1000 - 1], o[(B * 975 + 999) // 1000 - 1]
// ```

#[test]
fn splitmix64_valores_de_referencia() {
    let mut g = SplitMix64::nova(1_234_567);
    let v: Vec<u64> = (0..5).map(|_| g.proximo()).collect();
    assert_eq!(
        v,
        [
            6_457_827_717_110_365_317,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ]
    );
    let mut g = SplitMix64::nova(0);
    let v: Vec<u64> = (0..3).map(|_| g.proximo()).collect();
    assert_eq!(
        v,
        [
            0xe220_a839_7b1d_cdaf,
            7_960_286_522_194_355_700,
            487_617_019_471_545_679
        ]
    );
    // A semente de produção: se mudar, todos os vereditos mudam.
    let mut g = SplitMix64::nova(SEMENTE_BOOTSTRAP);
    let v: Vec<u64> = (0..3).map(|_| g.proximo()).collect();
    assert_eq!(
        v,
        [
            10_272_272_603_904_230_562,
            16_951_839_863_604_659_437,
            14_110_271_494_775_934_458
        ]
    );
}

#[test]
fn abaixo_de_valores_de_referencia() {
    let mut g = SplitMix64::nova(42);
    let v: Vec<u64> = (0..10).map(|_| g.abaixo_de(10).expect("n > 0")).collect();
    assert_eq!(v, [3, 1, 8, 4, 0, 2, 5, 8, 5, 4]);

    // n = 2^63 + 1: limiar 2^63 − 1, quase metade dos valores é descartada.
    let mut g = SplitMix64::nova(42);
    let v: Vec<u64> = (0..4)
        .map(|_| g.abaixo_de((1 << 63) + 1).expect("n > 0"))
        .collect();
    assert_eq!(
        v,
        [
            4_456_085_495_900_499_604,
            6_792_609_088_808_213_253,
            5_545_679_290_133_000_099,
            2_185_608_355_395_893_165
        ]
    );
}

#[test]
fn abaixo_de_bordas_e_uniformidade() {
    let mut g = SplitMix64::nova(7);
    assert!((0..100).all(|_| g.abaixo_de(1).expect("n = 1") == 0));
    assert!((0..100).all(|_| g.abaixo_de(u64::MAX).expect("máximo") < u64::MAX));
    assert!(matches!(
        g.abaixo_de(0),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));

    // 60 000 sorteios em 0..6: cada face perto de 10 000 (desvio-padrão ≈ 91;
    // a folga de 500 é > 5 desvios).
    let mut contagem = [0_u32; 6];
    for _ in 0..60_000 {
        contagem[g.abaixo_de(6).expect("n > 0") as usize] += 1;
    }
    assert!(
        contagem.iter().all(|&c| c.abs_diff(10_000) < 500),
        "{contagem:?}"
    );
}

#[test]
fn bootstrap_pequeno_igual_a_referencia_em_python() {
    // Valores diádicos: todas as somas são exatas, então a comparação é bit a
    // bit. Python: boot([-0.5, 0.25, -1.0, 0.75, -0.125, 0.5], 2, 7, 99).
    let serie = [-0.5, 0.25, -1.0, 0.75, -0.125, 0.5];
    let b = bootstrap_circular(&serie, 2, 7, 99).expect("bootstrap");
    assert_eq!(
        b,
        Bootstrap {
            concursos: 6,
            bloco: 2,
            reamostragens: 7,
            semente: 99,
            media: -0.125 / 6.0,
            ic95: Intervalo {
                inferior: -1.0 / 6.0,
                superior: 0.125 / 6.0,
            },
            // Médias reamostradas: −1/6, −0,0625, −0,0625, 1/48, 1/48, 0,
            // −1/6; centradas (m* − m) ≤ m = −1/48 em 4 delas: (1+4)/(7+1).
            p_valor: 0.625,
        }
    );
}

#[test]
fn blocos_dao_a_volta_no_fim_da_serie() {
    assert_eq!(indices_do_bloco(5, 3, 7).collect::<Vec<_>>(), [5, 6, 0]);
    assert_eq!(indices_do_bloco(6, 3, 7).collect::<Vec<_>>(), [6, 0, 1]);
    assert_eq!(indices_do_bloco(0, 3, 7).collect::<Vec<_>>(), [0, 1, 2]);
    // Bloco cortado (a última parte da reamostra).
    assert_eq!(indices_do_bloco(6, 1, 7).collect::<Vec<_>>(), [6]);

    // Na referência em Python (semente 99, T = 6, b = 2) os três primeiros
    // inícios são 5, 0 e 1: a primeira reamostra é [s5, s0, s0, s1, s1, s2],
    // com o bloco que começa no último índice dando a volta para o 0.
    let mut g = SplitMix64::nova(99);
    let inicios: Vec<u64> = (0..3).map(|_| g.abaixo_de(6).expect("n > 0")).collect();
    assert_eq!(inicios, [5, 0, 1]);
    let s = [-0.5, 0.25, -1.0, 0.75, -0.125, 0.5];
    let primeira = (s[5] + s[0] + s[0] + s[1] + s[1] + s[2]) / 6.0;
    assert_eq!(primeira, -1.0 / 6.0);

    // Uma série que só tem valor no último concurso: sem a volta, os blocos
    // que começam perto do fim teriam menos chance de pegá-lo. Com a volta,
    // a média das médias reamostradas é a média da série (cada índice entra
    // com a mesma chance).
    let mut serie = vec![0.0; 200];
    serie[199] = 200.0;
    let b = bootstrap_circular(&serie, 50, 4000, 3).expect("bootstrap");
    assert_eq!(b.media, 1.0);
    let mut g = SplitMix64::nova(3);
    let mut soma = 0.0;
    for _ in 0..4000 {
        let mut vezes = 0;
        for _ in 0..4 {
            let inicio = g.abaixo_de(200).expect("n > 0") as usize;
            vezes += indices_do_bloco(inicio, 50, 200)
                .filter(|&i| i == 199)
                .count();
        }
        soma += vezes as f64;
    }
    // Cada bloco pega o índice 199 com chance 50/200: 4 blocos → 1 vez em média.
    let media_das_vezes = soma / 4000.0;
    assert!((media_das_vezes - 1.0).abs() < 0.05, "{media_das_vezes}");
}

#[test]
fn serie_curta_nao_tem_veredito() {
    // T < 50 com os blocos de produção, T entre um e dois blocos e T menor
    // que o bloco: todos recusados (com um bloco só, toda reamostra é uma
    // rotação da série e o bootstrap não vê incerteza nenhuma).
    for t in [1, 2, 49, 50, 51, 99] {
        let serie = vec![-0.01; t];
        assert!(
            matches!(
                bootstrap_circular(&serie, BLOCO_BOOTSTRAP, 100, SEMENTE_BOOTSTRAP),
                Err(SpiderError::AvaliacaoSemConcursos(_))
            ),
            "T = {t}"
        );
    }
    assert_eq!(concursos_minimos(BLOCO_BOOTSTRAP), 100);
    assert!(bootstrap_circular(&[0.0; 100], BLOCO_BOOTSTRAP, 100, 1).is_ok());
    assert!(matches!(
        bootstrap_circular(&[0.1, -0.2, 0.3], 5, 100, 1),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        bootstrap_circular(&[], 5, 100, 1),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    // Exatamente dois blocos: aceito.
    assert!(bootstrap_circular(
        &[0.1, -0.2, 0.3, 0.0, -0.1, 0.2, 0.4, -0.3, 0.0, 0.1],
        5,
        100,
        1
    )
    .is_ok());

    // Por que a recusa: com T ≤ b as médias reamostradas são todas iguais à
    // observada (a menos de arredondamento), e o p-valor sairia mínimo.
    let serie: Vec<f64> = (0..30).map(|i| f64::from(i % 7) - 3.2).collect();
    let mut g = SplitMix64::nova(5);
    for _ in 0..50 {
        let inicio = g.abaixo_de(30).expect("n > 0") as usize;
        let rotacao: f64 = indices_do_bloco(inicio, 30, 30).map(|i| serie[i]).sum();
        let media: f64 = serie.iter().sum();
        assert!((rotacao - media).abs() < 1e-12);
    }
}

#[test]
fn entradas_invalidas() {
    let serie = vec![0.0; 200];
    for (bloco, b) in [(0, 10), (10, 0)] {
        assert!(matches!(
            bootstrap_circular(&serie, bloco, b, 1),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
    for ruim in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut s = serie.clone();
        s[17] = ruim;
        assert!(matches!(
            bootstrap_circular(&s, 50, 10, 1),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

/// U(−1, 1) com o próprio gerador (53 bits de mantissa).
fn uniforme(g: &mut SplitMix64) -> f64 {
    (g.proximo() >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
}

fn ruido(t: usize, semente: u64) -> Vec<f64> {
    let mut g = SplitMix64::nova(semente);
    (0..t).map(|_| uniforme(&mut g)).collect()
}

/// AR(1) `x_t = φ·x_{t−1} + e_t` com média zero, começando em 0.
fn ar1(t: usize, phi: f64, semente: u64) -> Vec<f64> {
    let mut g = SplitMix64::nova(semente);
    let mut x = 0.0;
    (0..t)
        .map(|_| {
            x = phi * x + uniforme(&mut g);
            x
        })
        .collect()
}

#[test]
fn deterministico_e_dependente_da_semente() {
    let serie = ruido(500, 11);
    let a = bootstrap_circular(&serie, BLOCO_BOOTSTRAP, 2000, SEMENTE_BOOTSTRAP).expect("a");
    let b = bootstrap_circular(&serie, BLOCO_BOOTSTRAP, 2000, SEMENTE_BOOTSTRAP).expect("b");
    assert_eq!(a.media.to_bits(), b.media.to_bits());
    assert_eq!(a.p_valor.to_bits(), b.p_valor.to_bits());
    assert_eq!(a.ic95.inferior.to_bits(), b.ic95.inferior.to_bits());
    assert_eq!(a.ic95.superior.to_bits(), b.ic95.superior.to_bits());

    let c = bootstrap_circular(&serie, BLOCO_BOOTSTRAP, 2000, SEMENTE_BOOTSTRAP + 1).expect("c");
    assert_eq!(c.media, a.media);
    assert_ne!(
        (c.ic95.inferior, c.ic95.superior, c.p_valor),
        (a.ic95.inferior, a.ic95.superior, a.p_valor)
    );
}

#[test]
fn mesma_semente_reamostra_igual_series_do_mesmo_tamanho() {
    // Os inícios dos blocos dependem só de T, b, B e da semente: a série
    // dobrada (×2 é exato em f64) tem médias reamostradas dobradas, logo o
    // mesmo p-valor e o intervalo dobrado. É o que deixa log loss e Brier
    // pareados no veredito.
    let serie = ruido(300, 21);
    let dobro: Vec<f64> = serie.iter().map(|x| 2.0 * x).collect();
    let a = bootstrap_circular(&serie, 50, 1000, 9).expect("a");
    let b = bootstrap_circular(&dobro, 50, 1000, 9).expect("b");
    assert_eq!(b.p_valor, a.p_valor);
    assert_eq!(b.media, 2.0 * a.media);
    assert_eq!(b.ic95.inferior, 2.0 * a.ic95.inferior);
    assert_eq!(b.ic95.superior, 2.0 * a.ic95.superior);
}

#[test]
fn serie_constante() {
    // Sem variação: toda reamostra tem a média observada (soma de T cópias do
    // mesmo valor, na mesma ordem: mesmos bits), centrada = 0.
    let b = bootstrap_circular(&[-0.01; 120], 50, 999, 1).expect("negativa");
    // A soma em sequência arredonda (−0,010000000000000007), mas toda
    // reamostra faz exatamente as mesmas somas: mesmos bits da observada.
    assert!((b.media + 0.01).abs() < 1e-15, "{}", b.media);
    assert_eq!((b.ic95.inferior, b.ic95.superior), (b.media, b.media));
    // 0 ≤ −0,01 nunca: só a própria amostra conta. p = 1/(B+1).
    assert_eq!(b.p_valor, 1.0 / 1000.0);

    let b = bootstrap_circular(&[0.0; 120], 50, 999, 1).expect("zero");
    assert_eq!(b.p_valor, 1.0, "empate exato não supera o acaso");
    let b = bootstrap_circular(&[0.01; 120], 50, 999, 1).expect("positiva");
    assert_eq!(b.p_valor, 1.0);
}

#[test]
fn efeito_claro_e_detectado() {
    // Média −0,3 com ruído U(−1, 1) (desvio ≈ 0,58): em 500 concursos o erro
    // padrão da média é ≈ 0,026, mais de 10 erros padrão abaixo de zero.
    let serie: Vec<f64> = ruido(500, 31).into_iter().map(|x| x - 0.3).collect();
    let b =
        bootstrap_circular(&serie, BLOCO_BOOTSTRAP, 2000, SEMENTE_BOOTSTRAP).expect("bootstrap");
    assert_eq!(b.p_valor, 1.0 / 2001.0);
    assert!(b.ic95.superior < -0.2 && b.ic95.inferior > -0.4, "{b:?}");
    assert!(b.ic95.inferior < b.media && b.media < b.ic95.superior);

    // O efeito ao contrário (perfil pior) dá p ≈ 1: o teste é unilateral.
    let pior: Vec<f64> = serie.iter().map(|x| -x).collect();
    let b = bootstrap_circular(&pior, BLOCO_BOOTSTRAP, 2000, SEMENTE_BOOTSTRAP).expect("pior");
    assert_eq!(b.p_valor, 1.0);
}

#[test]
fn p_valor_cai_quando_a_serie_fica_mais_negativa() {
    // m* − m não muda ao somar uma constante (mesmas reamostras); o limiar m
    // cai, então o p-valor não pode subir.
    let serie = ruido(400, 41);
    let mut anterior = 1.0;
    for passo in 0..8 {
        let delta = 0.01 * f64::from(passo);
        let s: Vec<f64> = serie.iter().map(|x| x - delta).collect();
        let p = bootstrap_circular(&s, 50, 1000, 2)
            .expect("bootstrap")
            .p_valor;
        assert!(p <= anterior, "delta {delta}: {p} > {anterior}");
        anterior = p;
    }
    assert!(anterior < 0.01, "{anterior}");
}

/// Fração de séries rejeitadas a 5% entre `n` séries nulas.
fn taxa_de_rejeicao(series: impl Iterator<Item = Vec<f64>>, bloco: usize) -> f64 {
    let mut n = 0;
    let mut rejeitadas = 0;
    for (i, s) in series.enumerate() {
        let b = bootstrap_circular(&s, bloco, 400, 1000 + i as u64).expect("bootstrap");
        n += 1;
        if b.p_valor <= super::super::ALFA {
            rejeitadas += 1;
        }
    }
    f64::from(rejeitadas) / f64::from(n)
}

#[test]
fn caso_nulo_rejeita_perto_de_alfa() {
    // Série sem efeito (média verdadeira 0), independente: o teste rejeita
    // perto de 5%. 300 séries: desvio-padrão da taxa ≈ 1,3 ponto.
    let iid = taxa_de_rejeicao((0..300).map(|i| ruido(300, 500 + i)), BLOCO_BOOTSTRAP);
    assert!(iid <= 0.10, "i.i.d.: {iid}");

    // Série nula com memória longa (AR(1), φ = 0,9), como a calibração
    // acumulada: com blocos de 50 a taxa fica perto do nominal; com blocos
    // de 1 (bootstrap i.i.d.) o erro padrão sai estreito demais e o teste
    // rejeita muito mais. É o motivo dos blocos fixos de 50.
    let com_blocos = taxa_de_rejeicao((0..300).map(|i| ar1(600, 0.9, 900 + i)), BLOCO_BOOTSTRAP);
    let sem_blocos = taxa_de_rejeicao((0..300).map(|i| ar1(600, 0.9, 900 + i)), 1);
    assert!(com_blocos <= 0.12, "AR(1) com blocos de 50: {com_blocos}");
    assert!(
        sem_blocos >= 2.0 * com_blocos && sem_blocos >= 0.15,
        "AR(1) sem blocos: {sem_blocos} (com blocos: {com_blocos})"
    );
}
