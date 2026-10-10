use super::*;
use crate::avaliacao::tests::{concurso_mini, serie_aleatoria, MINI};
use crate::avaliacao::walk_forward;
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn mini_com_corte(corte: u32) -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: corte,
        ..MINI
    }
}

fn com_corte(slug: &str, corte: u32) -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: corte,
        ..*cfg(slug)
    }
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

macro_rules! assert_perto {
    ($a:expr, $b:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(perto(a, b), "{} = {a:?}, esperado {b:?}", stringify!($a));
    }};
}

/// Série da MINI (K = 2 de N = 4, acaso = 1/2) de 6 concursos sorteados e o
/// snapshot do 7º, conferida à mão abaixo. Com o corte em 6, o trecho avaliado
/// é 2..=6 (5 concursos): cada dobra tem um concurso e as de teste são 3..=6.
fn serie_a_mao() -> Vec<Observacao> {
    [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [2, 1, 3, 4], Some([2, 3])),
        concurso_mini(3, [1, 2, 3, 4], Some([1, 4])),
        concurso_mini(4, [3, 4, 1, 2], Some([3, 4])),
        concurso_mini(5, [1, 2, 3, 4], Some([2, 3])),
        concurso_mini(6, [4, 3, 2, 1], Some([1, 2])),
        concurso_mini(7, [1, 2, 3, 4], None),
    ]
    .concat()
}

/// Referência independente da `calibrar`: contagens por posição e a mesma
/// fórmula `p = (4·s + m·K) / (4·(c + m))` da MINI, sem o limite de 1e-15
/// (que nunca é atingido com m = 100). Soma `ln p_modelo − ln(1/2)` nos
/// concursos `de..=ate`.
fn referencia_mini(obs: &[Observacao], de: u32, ate: u32) -> f64 {
    let mut soma = 0.0;
    let mut saidas = [0_u64; 4];
    let ultimo = obs.iter().map(|o| o.concurso).max().unwrap();
    for c in 1..=ultimo {
        let do_concurso: Vec<&Observacao> = obs.iter().filter(|o| o.concurso == c).collect();
        if do_concurso[0].saiu.is_none() {
            break;
        }
        if c >= de && c <= ate {
            for o in &do_concurso {
                let s = saidas[(o.posicao - 1) as usize];
                let p = (4 * s + 100 * 2) as f64 / (4 * (u64::from(c - 1) + 100)) as f64;
                let (pm, pa): (f64, f64) = if o.saiu == Some(true) {
                    (p, 0.5)
                } else {
                    (1.0 - p, 0.5)
                };
                soma += pm.ln() - pa.ln();
            }
        }
        for o in &do_concurso {
            if o.saiu == Some(true) {
                saidas[(o.posicao - 1) as usize] += 1;
            }
        }
    }
    soma
}

// ---------------------------------------------------------- à mão

#[test]
fn valores_conferidos_a_mao_na_mini() {
    // p_t(r) = (4·s_r + 200) / (4·(c + 100)), c = concursos antes de t,
    // s_r = quantas vezes a posição r saiu antes de t. Acaso = 1/2.
    //
    // c3 (c = 2, s = [2,1,1,0] → p = [208, 204, 204, 200]/408): saíram as
    //   posições 1 e 4; as posições 2 e 3 (p = 1/2) não saíram e valem 0:
    //   ln(416/408) + ln(400/408).
    // c4 (c = 3, s = [3,1,1,1] → p = [212, 204, 204, 204]/412): saíram as
    //   posições 1 e 2; não saíram as 3 e 4 (1 − p = 208/412):
    //   ln(424/412) + ln(408/412) + 2·ln(416/412).
    // c5 (c = 4, s = [4,2,1,1] → p = [216, 208, 204, 204]/416): saíram as
    //   posições 2 e 3; não saíram a 1 (200/416) e a 4 (212/416):
    //   ln(400/416) + ln(408/416) + ln(424/416) (a posição 2 vale 0).
    // c6 (c = 5, s = [4,3,2,1] → p = [216, 212, 208, 204]/420): saíram as
    //   posições 3 e 4; não saíram a 1 (204/420) e a 2 (208/420):
    //   ln(408/420) + ln(416/420) + ln(416/420) + ln(408/420).
    let esperado = [
        (416.0_f64 / 408.0).ln() + (400.0_f64 / 408.0).ln(),
        (424.0_f64 / 412.0).ln() + (408.0_f64 / 412.0).ln() + 2.0 * (416.0_f64 / 412.0).ln(),
        (400.0_f64 / 416.0).ln() + (408.0_f64 / 416.0).ln() + (424.0_f64 / 416.0).ln(),
        2.0 * (408.0_f64 / 420.0).ln() + 2.0 * (416.0_f64 / 420.0).ln(),
    ];
    let b = evidencia_do_indice(&mini_com_corte(6), &serie_a_mao()).expect("evidência");

    assert_eq!(
        b.faixa,
        FaixaConcursos {
            primeiro: 3,
            ultimo: 6
        }
    );
    assert_eq!(b.concursos, 4);
    assert_eq!(b.pares, 16);
    assert_eq!(b.por_dobra.len(), 4);
    for (i, d) in b.por_dobra.iter().enumerate() {
        assert_eq!(d.dobra, 2 + i as u32);
        assert_eq!(
            d.faixa,
            FaixaConcursos {
                primeiro: 3 + i as u32,
                ultimo: 3 + i as u32
            }
        );
        assert_eq!((d.concursos, d.pares), (1, 4));
        assert_perto!(d.ln_fb, esperado[i]);
        assert_perto!(d.log10_fb, esperado[i] / std::f64::consts::LN_10);
    }
    let total: f64 = esperado.iter().sum();
    assert_perto!(b.ln_fb, total);
    assert_perto!(b.log10_fb, total / std::f64::consts::LN_10);
    // Poucos pares e calibração puxada para o acaso: evidência muito fraca.
    assert_eq!(b.grau, Grau::MuitoFraca);
    assert_eq!(b.direcao, DirecaoIndice::Acaso);
}

#[test]
fn bate_com_a_referencia_independente() {
    for semente in [1_u64, 7, 99, 12345] {
        let obs = serie_aleatoria(&MINI, 61, semente, true);
        let c = mini_com_corte(60);
        let b = evidencia_do_indice(&c, &obs).expect("evidência");
        // Trecho avaliado 2..=60 (59 concursos): dobras de 11, a última com 15;
        // a primeira dobra de teste (a 2ª) começa no 13.
        assert_eq!(
            b.faixa,
            FaixaConcursos {
                primeiro: 13,
                ultimo: 60
            }
        );
        assert_eq!(b.concursos, 48);
        assert_eq!(b.pares, 48 * 4);
        assert_perto!(b.ln_fb, referencia_mini(&obs, 13, 60));
        // E cada dobra, sozinha.
        for d in &b.por_dobra {
            assert_perto!(
                d.ln_fb,
                referencia_mini(&obs, d.faixa.primeiro, d.faixa.ultimo)
            );
        }
    }
}

// ------------------------------------------- direção e graus

/// Posições fixas [1,2,3,4] e sempre as dezenas 1 e 2: o índice acerta sempre.
fn serie_informativa(concursos: u32) -> Vec<Observacao> {
    let mut v: Vec<Observacao> = (1..=concursos)
        .flat_map(|c| concurso_mini(c, [1, 2, 3, 4], Some([1, 2])))
        .collect();
    v.extend(concurso_mini(concursos + 1, [1, 2, 3, 4], None));
    v
}

#[test]
fn indice_que_acerta_sempre_da_evidencia_muito_forte_a_favor_do_indice() {
    // Com c concursos antes de t, p(posições 1 e 2) = (c + 50)/(c + 100) e
    // p(posições 3 e 4) = 50/(c + 100), e em todo par o termo é
    // ln(2·(c + 50)/(c + 100)) (saiu: p/(1/2); não saiu: (1 − p)/(1/2)):
    // ln FB do concurso t = 4·ln(2·(t + 49)/(t + 99)), c = t − 1.
    let obs = serie_informativa(100);
    let b = evidencia_do_indice(&mini_com_corte(100), &obs).expect("evidência");
    let esperado: f64 = (b.faixa.primeiro..=b.faixa.ultimo)
        .map(|t| 4.0 * (2.0 * (f64::from(t) + 49.0) / (f64::from(t) + 99.0)).ln())
        .sum();
    assert_perto!(b.ln_fb, esperado);
    assert!(b.ln_fb > 0.0);
    assert_eq!(b.direcao, DirecaoIndice::Indice);
    assert_eq!(b.grau, Grau::MuitoForte);
    assert!(b.log10_fb > 2.0);
}

#[test]
fn mudanca_de_regime_deixa_o_indice_pior_que_o_acaso() {
    // Os primeiros concursos ensinam "posições 1 e 2"; depois só saem a 3 e a
    // 4: a calibração acumulada erra e perde para o acaso nas dobras de teste.
    let mut obs: Vec<Observacao> = Vec::new();
    for c in 1..=80 {
        let saidas = if c <= 40 { [1, 2] } else { [3, 4] };
        obs.extend(concurso_mini(c, [1, 2, 3, 4], Some(saidas)));
    }
    obs.extend(concurso_mini(81, [1, 2, 3, 4], None));
    let b = evidencia_do_indice(&mini_com_corte(80), &obs).expect("evidência");
    assert_perto!(b.ln_fb, referencia_mini(&obs, b.faixa.primeiro, 80));
    assert!(b.ln_fb < 0.0, "ln FB = {}", b.ln_fb);
    assert_eq!(b.direcao, DirecaoIndice::Acaso);
}

#[test]
fn classificacao_do_indice_usa_a_escala_de_a() {
    let l3 = 3f64.log10();
    let casos: [(f64, Grau, DirecaoIndice); 9] = [
        // FB = 1 exato fica a favor do acaso, muito fraca.
        (0.0, Grau::MuitoFraca, DirecaoIndice::Acaso),
        (0.2, Grau::MuitoFraca, DirecaoIndice::Indice),
        (-0.2, Grau::MuitoFraca, DirecaoIndice::Acaso),
        // Limites fechados à esquerda, comparados em log10.
        (l3, Grau::Fraca, DirecaoIndice::Indice),
        (-l3, Grau::Fraca, DirecaoIndice::Acaso),
        (1.0, Grau::Moderada, DirecaoIndice::Indice),
        (30f64.log10(), Grau::Forte, DirecaoIndice::Indice),
        (2.0, Grau::MuitoForte, DirecaoIndice::Indice),
        (-7.5, Grau::MuitoForte, DirecaoIndice::Acaso),
    ];
    for (log10_fb, grau, direcao) in casos {
        assert_eq!(
            classificar_indice(log10_fb).expect("finito"),
            (grau, direcao),
            "log10 FB = {log10_fb}"
        );
    }
    assert!(classificar_indice(f64::NAN).is_err());
    assert!(classificar_indice(f64::INFINITY).is_err());
    assert_eq!(DirecaoIndice::Indice.nome(), "a favor do índice");
    assert_eq!(DirecaoIndice::Acaso.nome(), "a favor do acaso");
}

// --------------------- mesmo número da diferença de log loss (1c)

#[test]
fn ln_fb_e_menos_pares_vezes_a_diferenca_de_log_loss_do_walk_forward() {
    // Conferência exigida pelo plano: o FB do índice é a diferença de log loss
    // da 1c em outra escala. Por dobra, `média por par × pares` do
    // walk-forward, somada nas dobras 2 a 5, tem de dar o mesmo ln FB.
    // Tolerância: 1e-13 × pares × log loss do acaso (a escala da soma, não do
    // resultado). O lado do walk-forward subtrai duas médias de ~0,2–0,7 e
    // multiplica por pares, então carrega ~1e-15 de erro absoluto por par; o
    // ln FB pode ser pequeno por cancelamento (medido: erro de 3e-13 a 4e-12
    // para ln FB de 0,4 a 12, isto é, até ~3e-12 relativo ao próprio ln FB,
    // e ~2e-15 relativo a pares × log loss). Passa-se com folga de 50 vezes.
    for (slug, seed) in [("lotofacil", 11_u64), ("megasena", 22), ("quina", 33)] {
        let c = com_corte(slug, 400);
        let obs = serie_aleatoria(&c, 430, seed, true);
        let b = evidencia_do_indice(&c, &obs).expect("evidência");
        let wf = walk_forward(&c, &obs, c.qtd_dezenas as u32).expect("walk-forward");
        assert_eq!(wf.dobras.len(), 5);
        assert_eq!(b.por_dobra.len(), 4);

        let mut total_wf = 0.0;
        let mut escala = 0.0;
        for d in &b.por_dobra {
            let a = &wf.dobras[d.dobra as usize - 1];
            assert_eq!(a.faixa, d.faixa, "{slug}: mesma dobra");
            assert_eq!(a.metricas.concursos_avaliados, d.concursos);
            let ll = &a.metricas.log_loss;
            let de_wf = -(d.pares as f64) * (ll.perfil - ll.acaso);
            assert!(
                (d.ln_fb - de_wf).abs() <= 1e-13 * d.pares as f64 * ll.acaso,
                "{slug} dobra {}: ln FB {} contra {}",
                d.dobra,
                d.ln_fb,
                de_wf
            );
            // `diferenca` (média de d_t) é o mesmo número, a menos de arredondamento.
            let de_diferenca = -(d.pares as f64) * ll.diferenca;
            assert!((d.ln_fb - de_diferenca).abs() <= 1e-13 * d.pares as f64 * ll.acaso);
            total_wf += de_wf;
            escala += d.pares as f64 * ll.acaso;
        }
        assert!(
            (b.ln_fb - total_wf).abs() <= 1e-13 * escala,
            "{slug}: ln FB {} contra {total_wf}",
            b.ln_fb
        );
        // Não é um zero trivial: o histórico aleatório dá FB diferente de 1.
        assert!(b.ln_fb != 0.0);
    }
}

// ------------------------------------------------ ponto no tempo

#[test]
fn nada_acima_do_corte_entra_no_calculo() {
    let c = com_corte("lotofacil", 300);
    let base = serie_aleatoria(&c, 340, 5, true);
    let original = evidencia_do_indice(&c, &base).expect("evidência");

    // Trocar tudo o que está acima do corte (saiu e posições) não muda nada.
    let outra = serie_aleatoria(&c, 340, 77, true);
    let mut trocada = base.clone();
    for o in trocada.iter_mut().filter(|o| o.concurso > 300) {
        *o = *outra
            .iter()
            .find(|x| x.concurso == o.concurso && x.dezena == o.dezena)
            .unwrap();
    }
    assert_eq!(evidencia_do_indice(&c, &trocada).unwrap(), original);

    // Nem sequer é validado: um buraco no trancado não atrapalha.
    let com_buraco: Vec<Observacao> = base.iter().copied().filter(|o| o.concurso != 320).collect();
    assert_eq!(evidencia_do_indice(&c, &com_buraco).unwrap(), original);

    // E o snapshot do próximo concurso (sem resultado) também não conta.
    let sem_trancado: Vec<Observacao> =
        base.iter().copied().filter(|o| o.concurso <= 300).collect();
    assert_eq!(evidencia_do_indice(&c, &sem_trancado).unwrap(), original);
}

#[test]
fn alterar_um_concurso_nao_muda_os_anteriores() {
    let c = com_corte("megasena", 250);
    let base = serie_aleatoria(&c, 260, 9, true);
    let original = evidencia_do_indice(&c, &base).expect("evidência");

    // O último concurso do trecho: `saiu` e posições novos mudam só o dele.
    let mut mudada = base.clone();
    let outra = serie_aleatoria(&c, 260, 1234, true);
    for o in mudada.iter_mut().filter(|o| o.concurso == 250) {
        *o = *outra
            .iter()
            .find(|x| x.concurso == 250 && x.dezena == o.dezena)
            .unwrap();
    }
    let depois = evidencia_do_indice(&c, &mudada).expect("evidência");
    assert_eq!(depois.por_dobra[..3], original.por_dobra[..3]);
    assert_ne!(depois.por_dobra[3], original.por_dobra[3]);
}

#[test]
fn a_dobra_1_fica_fora() {
    let c = com_corte("quina", 300);
    let obs = serie_aleatoria(&c, 310, 3, true);
    let dobras = dobras_da_busca(&c, &obs).expect("dobras");
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    assert_eq!(b.faixa.primeiro, dobras[1].primeiro);
    assert_eq!(b.faixa.ultimo, 300);
    let da_dobra_1 = dobras[0].tamanho() as usize;
    let todos = (dobras.last().unwrap().ultimo - dobras[0].primeiro + 1) as usize;
    assert_eq!(b.concursos, todos - da_dobra_1);
    assert_eq!(
        b.por_dobra.iter().map(|d| d.dobra).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    // Total = soma das dobras, em ordem; concursos e pares idem.
    let soma: f64 = b.por_dobra.iter().map(|d| d.ln_fb).sum();
    assert_eq!(b.ln_fb, soma);
    assert_eq!(
        b.concursos,
        b.por_dobra.iter().map(|d| d.concursos).sum::<usize>()
    );
    assert_eq!(b.pares, b.por_dobra.iter().map(|d| d.pares).sum::<u64>());
    assert_eq!(b.pares, b.concursos as u64 * 80);
    assert_eq!(b.log10_fb, b.ln_fb * std::f64::consts::LOG10_E);
}

// -------------------------------------------------------- bordas

#[test]
fn menos_concursos_que_dobras_e_sem_concursos() {
    // 5 concursos avaliáveis (2..=6) é o mínimo; com 5 sorteados (4 avaliáveis)
    // não há como dividir em 5 dobras.
    let c = mini_com_corte(5);
    let curta = serie_aleatoria(&c, 6, 1, true);
    assert!(matches!(
        evidencia_do_indice(&c, &curta),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        evidencia_do_indice(&c, &[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    // Com 6 sorteados funciona (a série à mão).
    assert!(evidencia_do_indice(&mini_com_corte(6), &serie_a_mao()).is_ok());
}

#[test]
fn corte_acima_do_ultimo_sorteado_usa_so_o_que_foi_sorteado() {
    // Banco menor que o corte da busca (histórico curto): as dobras cobrem o
    // que existe, e o snapshot do próximo não entra.
    let c = mini_com_corte(500);
    let obs = serie_aleatoria(&c, 41, 4, true);
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    assert_eq!(b.faixa.ultimo, 40);
    assert_perto!(b.ln_fb, referencia_mini(&obs, b.faixa.primeiro, 40));
}

#[test]
fn serie_invalida_ate_o_corte_e_recusada() {
    let c = mini_com_corte(60);
    let mut obs = serie_aleatoria(&c, 61, 2, true);
    obs.retain(|o| o.concurso != 30); // buraco dentro da busca
    assert!(matches!(
        evidencia_do_indice(&c, &obs),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

#[test]
fn determinismo_bit_a_bit() {
    let c = com_corte("lotofacil", 200);
    let obs = serie_aleatoria(&c, 210, 8, true);
    let a = evidencia_do_indice(&c, &obs).expect("evidência");
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    assert_eq!(a, b);
    assert_eq!(a.ln_fb.to_bits(), b.ln_fb.to_bits());
    // A ordem da entrada não importa.
    let mut invertida = obs.clone();
    invertida.reverse();
    assert_eq!(evidencia_do_indice(&c, &invertida).unwrap(), a);
}

// ------------------------------------------- índice inerte

#[test]
fn inerte_e_deterministico_e_nao_depende_da_ordem_da_entrada() {
    let c = com_corte("lotofacil", 200);
    let obs = serie_aleatoria(&c, 210, 8, true);
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    let x = esperado_indice_inerte(&c, &obs, &b, 20, SEMENTE_INERTE).expect("inerte");
    let y = esperado_indice_inerte(&c, &obs, &b, 20, SEMENTE_INERTE).expect("inerte");
    assert_eq!(x, y);
    assert_eq!(x.media_log10.to_bits(), y.media_log10.to_bits());
    let mut invertida = obs.clone();
    invertida.reverse();
    assert_eq!(
        esperado_indice_inerte(&c, &invertida, &b, 20, SEMENTE_INERTE).unwrap(),
        x
    );
    // Outra semente, outras réplicas.
    let z = esperado_indice_inerte(&c, &obs, &b, 20, SEMENTE_INERTE + 1).unwrap();
    assert_ne!(z.media_log10.to_bits(), x.media_log10.to_bits());
    assert_eq!((x.replicas, x.semente), (20, SEMENTE_INERTE));
}

#[test]
fn inerte_custa_evidencia_e_o_indice_que_acerta_fica_acima_de_todas_as_replicas() {
    // Sob o acaso a calibração ainda paga para estimar a taxa por posição:
    // a média do inerte sai negativa (a favor do acaso), e a faixa a contém.
    let c = mini_com_corte(100);
    let obs = serie_informativa(100);
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    let inerte = esperado_indice_inerte(&c, &obs, &b, 50, SEMENTE_INERTE).expect("inerte");
    assert!(inerte.media_log10 < 0.0, "{inerte:?}");
    assert!(inerte.p025_log10 <= inerte.media_log10, "{inerte:?}");
    assert!(inerte.media_log10 <= inerte.p975_log10, "{inerte:?}");
    assert_eq!(inerte.replicas_iguais_ou_acima, 0);
}

#[test]
fn inerte_nao_usa_nada_acima_do_fim_da_ultima_dobra() {
    let c = mini_com_corte(100);
    let mut obs = serie_informativa(110);
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    let antes = esperado_indice_inerte(&c, &obs, &b, 10, SEMENTE_INERTE).unwrap();
    // Posições e resultados do trecho trancado trocados: nada muda.
    obs.retain(|o| o.concurso <= 100);
    for t in 101..=110 {
        obs.extend(concurso_mini(t, [4, 3, 2, 1], Some([3, 4])));
    }
    obs.extend(concurso_mini(111, [2, 1, 4, 3], None));
    assert_eq!(
        esperado_indice_inerte(&c, &obs, &b, 10, SEMENTE_INERTE).unwrap(),
        antes
    );
}

#[test]
fn inerte_exige_ao_menos_duas_replicas() {
    let c = mini_com_corte(100);
    let obs = serie_informativa(100);
    let b = evidencia_do_indice(&c, &obs).expect("evidência");
    assert!(matches!(
        esperado_indice_inerte(&c, &obs, &b, 1, SEMENTE_INERTE),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

#[test]
fn replica_do_inerte_e_a_mesma_conta_de_evidencia_do_indice() {
    // A réplica rápida (laço único) contra o caminho completo: refaz os mesmos
    // sorteios (mesma semente, mesmo baralho, concursos na ordem e dezenas em
    // ordem crescente), grava-os em `saiu` e roda `evidencia_do_indice`.
    for (c, concursos) in [
        (com_corte("lotofacil", 200), 210),
        (com_corte("quina", 150), 160),
        (mini_com_corte(60), 70),
    ] {
        let obs = serie_aleatoria(&c, concursos, 8, true);
        let b = evidencia_do_indice(&c, &obs).expect("evidência");
        let serie = SerieInerte::nova(&c, &obs, b.faixa).expect("série");
        for semente in [1, SEMENTE_INERTE] {
            let rapido = serie
                .ln_fb_de_uma_replica(&c, &mut SplitMix64::nova(semente))
                .expect("réplica");

            let mut simuladas = ate_o_corte(&obs, b.faixa.ultimo);
            simuladas.sort_unstable_by_key(|o| (o.concurso, o.dezena));
            let n = c.qtd_dezenas_possiveis() as usize;
            let mut gerador = SplitMix64::nova(semente);
            let mut baralho: Vec<usize> = (0..n).collect();
            for grupo in simuladas.chunk_by_mut(|a, b| a.concurso == b.concurso) {
                if grupo[0].saiu.is_none() {
                    continue;
                }
                grupo.iter_mut().for_each(|o| o.saiu = Some(false));
                for i in 0..c.qtd_dezenas {
                    let j = i + gerador.abaixo_de((n - i) as u64).unwrap() as usize;
                    baralho.swap(i, j);
                    grupo[baralho[i]].saiu = Some(true);
                }
            }
            let completo = evidencia_do_indice(&c, &simuladas).expect("evidência");
            assert!(
                (rapido - completo.ln_fb).abs() <= 1e-9 * (1.0 + completo.ln_fb.abs()),
                "{}: {rapido} contra {}",
                c.slug,
                completo.ln_fb
            );
        }
    }
}
