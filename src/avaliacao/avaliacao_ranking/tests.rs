use super::*;
use crate::avaliacao::metricas::{brier, log_loss};
use crate::avaliacao::tests::{serie_aleatoria, Gerador, MINI};
use crate::avaliacao::{
    calcular_taxas, calibrar, ranquear, taxa_do_acaso, walk_forward, ObservacaoPosicoes,
    QTD_COMPONENTES,
};
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

// ------------------------------------------------------------------ à mão

/// Um concurso da MINI (K = 2 de N = 4) com as duas ordens dadas, por dezena.
fn concurso_ab(
    concurso: u32,
    pos_a: [u32; 4],
    pos_b: [u32; 4],
    saidas: Option<[u32; 2]>,
) -> Vec<LinhaRanking> {
    (1..=4_u32)
        .map(|d| {
            let i = (d - 1) as usize;
            LinhaRanking {
                concurso,
                dezena: d,
                posicao_indice: pos_a[i],
                saiu: saidas.map(|s| s.contains(&d)),
                nota_a: 0.0,
                nota_b: 0.0,
                posicao_ranking_a: pos_a[i],
                posicao_ranking_b: pos_b[i],
            }
        })
        .collect()
}

/// Série de referência (à mão), sem passar por `ranquear`: seis concursos da
/// MINI, K = 2 de N = 4, acaso 1/2, m = 100. Com 5 concursos avaliados (2 a 6)
/// as 5 dobras têm um concurso cada: dobra 1 = {2}, … , dobra 5 = {6}, e as
/// dobras de teste são os concursos 3, 4, 5 e 6.
///
/// | concurso | ordem A  | ordem B  | saíram |
/// |---|---|---|---|
/// | 1 | 1,2,3,4 | 1,2,3,4 | 1 e 2 |
/// | 2 | 1,2,3,4 | 2,1,3,4 | 2 e 3 |
/// | 3 | 2,1,3,4 | 1,2,3,4 | 1 e 4 |
/// | 4 | 1,2,3,4 | 3,1,2,4 | 1 e 2 |
/// | 5 | 4,3,2,1 | 1,2,4,3 | 3 e 4 |
/// | 6 | 1,2,3,4 | 2,1,3,4 | 1 e 3 |
fn serie_mao() -> Vec<LinhaRanking> {
    [
        concurso_ab(1, [1, 2, 3, 4], [1, 2, 3, 4], Some([1, 2])),
        concurso_ab(2, [1, 2, 3, 4], [2, 1, 3, 4], Some([2, 3])),
        concurso_ab(3, [2, 1, 3, 4], [1, 2, 3, 4], Some([1, 4])),
        concurso_ab(4, [1, 2, 3, 4], [3, 1, 2, 4], Some([1, 2])),
        concurso_ab(5, [4, 3, 2, 1], [1, 2, 4, 3], Some([3, 4])),
        concurso_ab(6, [1, 2, 3, 4], [2, 1, 3, 4], Some([1, 3])),
    ]
    .concat()
}

type Par = (f64, bool);

fn frac(num: f64, den: f64, saiu: bool) -> Par {
    (num / den, saiu)
}

/// Média, no concurso, do log loss e do Brier de pares (p, saiu) das 4 dezenas.
fn medias(pares: &[Par]) -> (f64, f64) {
    let n = pares.len() as f64;
    (
        pares.iter().map(|&(p, s)| log_loss(p, s)).sum::<f64>() / n,
        pares.iter().map(|&(p, s)| brier(p, s)).sum::<f64>() / n,
    )
}

/// Probabilidades da calibração pela posição (1c) sobre a ordem de B, dos
/// concursos 3 a 6, deduzidas à mão: `p(r) = (s_r + 50) / (c + 100)` (N = 4,
/// K = 2, m = 100), com `s_r` as saídas da posição `r` antes do concurso:
/// (2,1,1,0) com c = 2; (3,1,1,1) com c = 3; (4,1,2,1) com c = 4;
/// (4,1,3,2) com c = 5. Cada lista traz (p, saiu) das dezenas 1 a 4.
fn b_posicao_a_mao() -> [Vec<Par>; 4] {
    [
        vec![
            frac(52.0, 102.0, true),
            frac(51.0, 102.0, false),
            frac(51.0, 102.0, false),
            frac(50.0, 102.0, true),
        ],
        vec![
            frac(51.0, 103.0, true),
            frac(53.0, 103.0, true),
            frac(51.0, 103.0, false),
            frac(51.0, 103.0, false),
        ],
        vec![
            frac(54.0, 104.0, false),
            frac(51.0, 104.0, false),
            frac(51.0, 104.0, true),
            frac(52.0, 104.0, true),
        ],
        vec![
            frac(51.0, 105.0, true),
            frac(54.0, 105.0, false),
            frac(53.0, 105.0, true),
            frac(52.0, 105.0, false),
        ],
    ]
}

/// Calibração linear sobre a ordem de A, concursos 3 a 6, à mão:
/// `p = 1/2 + S·w(r)/D`, com `w(r) = 5 − 2r` (3, 1, −1, −3), `W2 = 20`,
/// `D = (c + 100)·20` e `S` a soma dos `w` das posições que saíram antes:
/// S = 4, 2, 6, 10 com c = 2, 3, 4, 5.
fn a_linear_a_mao() -> [Vec<Par>; 4] {
    let m = |d: f64, w: f64| 0.5 + w / d;
    [
        // S/D = 4/2040 = 1/510; ordem A = 2,1,3,4 → w = 1, 3, −1, −3.
        vec![
            (m(510.0, 1.0), true),
            (m(510.0, 3.0), false),
            (m(510.0, -1.0), false),
            (m(510.0, -3.0), true),
        ],
        // S/D = 2/2060 = 1/1030; ordem A = 1,2,3,4 → w = 3, 1, −1, −3.
        vec![
            (m(1030.0, 3.0), true),
            (m(1030.0, 1.0), true),
            (m(1030.0, -1.0), false),
            (m(1030.0, -3.0), false),
        ],
        // S/D = 6/2080 = 3/1040; ordem A = 4,3,2,1 → w = −3, −1, 1, 3.
        vec![
            (m(1040.0 / 3.0, -3.0), false),
            (m(1040.0 / 3.0, -1.0), false),
            (m(1040.0 / 3.0, 1.0), true),
            (m(1040.0 / 3.0, 3.0), true),
        ],
        // S/D = 10/2100 = 1/210; ordem A = 1,2,3,4 → w = 3, 1, −1, −3.
        vec![
            (m(210.0, 3.0), true),
            (m(210.0, 1.0), false),
            (m(210.0, -1.0), true),
            (m(210.0, -3.0), false),
        ],
    ]
}

/// Calibração linear sobre a ordem de B, concursos 3 a 6, à mão: S = 6, 6, 8,
/// 4 com c = 2, 3, 4, 5.
fn b_linear_a_mao() -> [Vec<Par>; 4] {
    let m = |d: f64, w: f64| 0.5 + w / d;
    [
        // S/D = 6/2040 = 1/340; ordem B = 1,2,3,4 → w = 3, 1, −1, −3.
        vec![
            (m(340.0, 3.0), true),
            (m(340.0, 1.0), false),
            (m(340.0, -1.0), false),
            (m(340.0, -3.0), true),
        ],
        // S/D = 6/2060 = 3/1030; ordem B = 3,1,2,4 → w = −1, 3, 1, −3.
        vec![
            (m(1030.0 / 3.0, -1.0), true),
            (m(1030.0 / 3.0, 3.0), true),
            (m(1030.0 / 3.0, 1.0), false),
            (m(1030.0 / 3.0, -3.0), false),
        ],
        // S/D = 8/2080 = 1/260; ordem B = 1,2,4,3 → w = 3, 1, −3, −1.
        vec![
            (m(260.0, 3.0), false),
            (m(260.0, 1.0), false),
            (m(260.0, -3.0), true),
            (m(260.0, -1.0), true),
        ],
        // S/D = 4/2100 = 1/525; ordem B = 2,1,3,4 → w = 1, 3, −1, −3.
        vec![
            (m(525.0, 1.0), true),
            (m(525.0, 3.0), false),
            (m(525.0, -1.0), true),
            (m(525.0, -3.0), false),
        ],
    ]
}

/// Confere um `PorTrecho<ResumoAvaliacao>` contra as probabilidades à mão dos
/// concursos 3 a 6 (uma dobra por concurso): log loss ou Brier, perfil e acaso,
/// por dobra e no total (média dos quatro concursos).
fn confere_metricas(r: &PorTrecho<ResumoAvaliacao>, a_mao: &[Vec<Par>; 4], rotulo: &str) {
    assert_eq!(r.dobras.len(), 4, "{rotulo}");
    let acaso = medias(&[(0.5, true), (0.5, false), (0.5, true), (0.5, false)]);
    let mut soma = (0.0, 0.0);
    for (i, pares) in a_mao.iter().enumerate() {
        let esperado = medias(pares);
        let d = &r.dobras[i];
        let concurso = 3 + i as u32;
        assert_eq!(
            (
                d.concursos_avaliados,
                d.primeiro_concurso,
                d.ultimo_concurso
            ),
            (1, concurso, concurso),
            "{rotulo} dobra {}",
            i + 2
        );
        assert!(perto(d.log_loss.perfil, esperado.0), "{rotulo} ll {i}");
        assert!(perto(d.log_loss.acaso, acaso.0), "{rotulo} ll acaso {i}");
        assert!(
            perto(d.log_loss.diferenca, esperado.0 - acaso.0),
            "{rotulo} ll dif {i}"
        );
        assert!(perto(d.brier.perfil, esperado.1), "{rotulo} brier {i}");
        assert!(perto(d.brier.acaso, acaso.1), "{rotulo} brier acaso {i}");
        soma.0 += esperado.0;
        soma.1 += esperado.1;
    }
    let t = &r.total;
    assert_eq!(
        (
            t.concursos_avaliados,
            t.primeiro_concurso,
            t.ultimo_concurso
        ),
        (4, 3, 6),
        "{rotulo}"
    );
    assert!(perto(t.log_loss.perfil, soma.0 / 4.0), "{rotulo} ll total");
    assert!(perto(t.brier.perfil, soma.1 / 4.0), "{rotulo} brier total");
    assert!(
        perto(t.log_loss.diferenca, soma.0 / 4.0 - acaso.0),
        "{rotulo} dif total"
    );
}

#[test]
fn mini_dobras_de_teste_sao_os_concursos_3_a_6() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    assert_eq!(r.corte, MINI.ultimo_concurso_busca);
    assert_eq!(r.k, 2);
    // Dobras 2 a 5; a dobra 1 (concurso 2) fica fora, e o concurso 1 (o
    // primeiro da entrada) nem é avaliado.
    let numeros: Vec<u32> = r.dobras.iter().map(|d| d.dobra).collect();
    assert_eq!(numeros, [2, 3, 4, 5]);
    let faixas: Vec<(u32, u32)> = r
        .dobras
        .iter()
        .map(|d| (d.faixa.primeiro, d.faixa.ultimo))
        .collect();
    assert_eq!(faixas, [(3, 3), (4, 4), (5, 5), (6, 6)]);
    assert_eq!((r.faixa.primeiro, r.faixa.ultimo), (3, 6));
    assert_eq!(r.concursos, 4);
}

#[test]
fn mini_ordem_a_topk_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    // Top-2 pela ordem de A: acertos 1 (c3), 2 (c4), 2 (c5), 1 (c6).
    let acertos: Vec<u64> = r.a.topk.dobras.iter().map(|d| d.total_acertos).collect();
    assert_eq!(acertos, [1, 2, 2, 1]);
    let t = &r.a.topk.total;
    assert_eq!((t.k, t.concursos_avaliados, t.total_acertos), (2, 4, 6));
    assert_eq!((t.primeiro_concurso, t.ultimo_concurso), (3, 6));
    // Hipergeométrica (4, 2, 2): P = 1/6, 4/6, 1/6; média por concurso 1.
    assert_eq!(t.acertos.perfil, 1.5);
    assert_eq!(t.acertos.acaso, 1.0);
    assert_eq!(t.acertos.diferenca, 0.5);
    let esperados: Vec<f64> = t.distribuicao.iter().map(|f| f.esperados).collect();
    assert!(perto(esperados[0], 4.0 / 6.0));
    assert!(perto(esperados[1], 16.0 / 6.0));
    assert!(perto(esperados[2], 4.0 / 6.0));
    let observados: Vec<usize> = t.distribuicao.iter().map(|f| f.observados).collect();
    assert_eq!(observados, [0, 2, 2]);
    // P(S ≥ 6) com 4 concursos: (x⁻¹ + 4 + x)⁴ tem 117 dos 1296 pesos em
    // expoente ≥ 2 (1 + 16 + 100).
    assert!(
        perto(t.p_valor.probabilidade, 117.0 / 1296.0),
        "{:?}",
        t.p_valor
    );
}

#[test]
fn mini_ordem_a_linear_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    confere_metricas(&r.a.linear, &a_linear_a_mao(), "A linear");
}

#[test]
fn mini_ordem_b_posicao_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    confere_metricas(&r.b.posicao, &b_posicao_a_mao(), "B posição");
}

#[test]
fn mini_ordem_b_linear_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    confere_metricas(&r.b.linear, &b_linear_a_mao(), "B linear");
}

#[test]
fn mini_ordem_b_topk_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    // Top-2 pela ordem de B: acertos 1 (c3), 1 (c4), 0 (c5), 1 (c6).
    let acertos: Vec<u64> = r.b.topk.dobras.iter().map(|d| d.total_acertos).collect();
    assert_eq!(acertos, [1, 1, 0, 1]);
    let t = &r.b.topk.total;
    assert_eq!(t.total_acertos, 3);
    assert_eq!(t.acertos.perfil, 0.75);
    assert_eq!(t.acertos.diferenca, -0.25);
    // P(S ≥ 3): expoente ≥ −1, ou 1296 − (1 + 16 + 100) = 1179 dos 1296 pesos.
    assert!(
        perto(t.p_valor.probabilidade, 1179.0 / 1296.0),
        "{:?}",
        t.p_valor
    );
}

#[test]
fn mini_trecho_curto_nao_tem_bootstrap() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).expect("série válida");
    assert!(r.b.bootstrap.is_none());
    assert_eq!(r.b.concursos_minimos, 2 * BLOCO_BOOTSTRAP);
    assert_eq!(r.b.supera_o_acaso(), None);
}

#[test]
fn a_ordem_de_a_e_a_ordem_de_b_nao_se_misturam() {
    let linhas = serie_mao();
    let trocadas: Vec<LinhaRanking> = linhas
        .iter()
        .map(|l| LinhaRanking {
            posicao_ranking_a: l.posicao_ranking_b,
            posicao_ranking_b: l.posicao_ranking_a,
            ..l.clone()
        })
        .collect();
    let r = avaliar_ranking(&MINI, &linhas, 2).unwrap();
    let t = avaliar_ranking(&MINI, &trocadas, 2).unwrap();
    // O que A calcula sobre a ordem dela, B calcula sobre a ordem dela, e
    // trocar as colunas troca os papéis.
    assert_eq!(r.a.topk, t.b.topk);
    assert_eq!(r.a.linear, t.b.linear);
    assert_eq!(r.b.topk, t.a.topk);
    assert_eq!(r.b.linear, t.a.linear);
    assert_ne!(r.a.topk, r.b.topk);
}

#[test]
fn totais_batem_com_as_dobras() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 2).unwrap();
    for topk in [&r.a.topk, &r.b.topk] {
        let soma: u64 = topk.dobras.iter().map(|d| d.total_acertos).sum();
        assert_eq!(soma, topk.total.total_acertos);
        let concursos: usize = topk.dobras.iter().map(|d| d.concursos_avaliados).sum();
        assert_eq!(concursos, topk.total.concursos_avaliados);
        assert_eq!(concursos, r.concursos);
    }
    for m in [&r.a.linear, &r.b.posicao, &r.b.linear] {
        let concursos: usize = m.dobras.iter().map(|d| d.concursos_avaliados).sum();
        assert_eq!(concursos, m.total.concursos_avaliados);
        assert_eq!(concursos, r.concursos);
    }
}

// ---------------------------------------------------- séries sintéticas

/// Série sintética válida com componentes aleatórios (valores pequenos, para
/// dar empates; alguns `NULL`): a entrada de `calcular_taxas`.
fn serie_com_componentes(
    cfg: &LoteriaConfig,
    concursos: u32,
    semente: u64,
    com_snapshot: bool,
) -> Vec<ObservacaoPosicoes> {
    let mut g = Gerador(semente.wrapping_mul(2).wrapping_add(1));
    serie_aleatoria(cfg, concursos, semente, com_snapshot)
        .into_iter()
        .map(|o| {
            let mut componentes = [None; QTD_COMPONENTES];
            for valor in componentes.iter_mut() {
                let sorteio = g.proximo();
                *valor = if sorteio.is_multiple_of(7) {
                    None
                } else {
                    Some((sorteio >> 8) as i64 % 5 - 2)
                };
            }
            ObservacaoPosicoes {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                saiu: o.saiu,
                componentes,
            }
        })
        .collect()
}

fn ranking_sintetico(
    cfg: &LoteriaConfig,
    concursos: u32,
    semente: u64,
    com_snapshot: bool,
) -> Vec<LinhaRanking> {
    let serie = serie_com_componentes(cfg, concursos, semente, com_snapshot);
    let taxas = calcular_taxas(cfg, &serie).expect("série válida");
    ranquear(cfg, &taxas.linhas).expect("linhas válidas")
}

/// Mesma série, as linhas com concurso > `a_partir_de` alteradas por `muda`.
fn alterar(
    linhas: &[LinhaRanking],
    a_partir_de: u32,
    muda: impl Fn(&mut LinhaRanking),
) -> Vec<LinhaRanking> {
    let mut v = linhas.to_vec();
    for l in v.iter_mut().filter(|l| l.concurso > a_partir_de) {
        muda(l);
    }
    v
}

fn k_da(cfg: &LoteriaConfig) -> u32 {
    cfg.qtd_dezenas as u32
}

#[test]
fn nada_acima_do_corte_entra_nem_para_validar() {
    // MINI: corte = 20. Série de 40 concursos; acima do 20 vira lixo (posição
    // repetida, `saiu` trocado, até concurso faltando): se alguma coisa fosse
    // lida, a validação recusaria ou o resultado mudaria.
    let base = ranking_sintetico(&MINI, 40, 3, true);
    let limpa = avaliar_ranking(&MINI, &base, 2).expect("série válida");
    assert_eq!(limpa.faixa.ultimo, 20, "o último avaliado é o corte");

    let lixo = alterar(&base, 20, |l| {
        l.posicao_ranking_a = 0;
        l.posicao_ranking_b = 99;
        l.saiu = Some(l.dezena == 1);
    });
    let sujo = avaliar_ranking(&MINI, &lixo, 2).expect("o trancado nem é validado");
    assert_eq!(limpa, sujo);

    // Também com buraco: sem os concursos 25 a 30.
    let buraco: Vec<LinhaRanking> = base
        .iter()
        .filter(|l| !(25..=30).contains(&l.concurso))
        .cloned()
        .collect();
    assert_eq!(avaliar_ranking(&MINI, &buraco, 2).unwrap(), limpa);

    // E um corte que cai antes do fim da série: só até o 20 vale.
    let curta: Vec<LinhaRanking> = base.iter().filter(|l| l.concurso <= 20).cloned().collect();
    assert_eq!(avaliar_ranking(&MINI, &curta, 2).unwrap(), limpa);
}

#[test]
fn dobra_1_nao_e_avaliada_mas_treina_a_calibracao() {
    let base = ranking_sintetico(&MINI, 30, 5, false);
    let r = avaliar_ranking(&MINI, &base, 2).expect("série válida");
    // Concursos avaliados 2..=20 (19): dobras de 3 e a última de 7; a dobra 1 é
    // 2..=4, então o trecho de teste começa no 5.
    assert_eq!((r.faixa.primeiro, r.faixa.ultimo), (5, 20));
    assert_eq!(r.concursos, 16);
    assert_eq!(r.dobras.len(), 4);
    assert_eq!(r.dobras[0].dobra, 2);
    assert_eq!(r.dobras[0].faixa.primeiro, 5);
    for topk in [&r.a.topk, &r.b.topk] {
        assert_eq!(topk.total.primeiro_concurso, 5);
        assert!(topk.dobras.iter().all(|d| d.primeiro_concurso >= 5));
    }

    // Trocar o resultado da dobra 1 (concursos 2 a 4) não mexe nos acertos do
    // top-k das dobras de teste (a ordem vem pronta e o acerto lê só o concurso
    // avaliado), mas muda a calibração de B e a linear, que aprendem com ele.
    let outra = alterar(&base, 1, |l| {
        if l.concurso <= 4 {
            l.saiu = Some(l.dezena <= 2);
        }
    });
    let o = avaliar_ranking(&MINI, &outra, 2).expect("série válida");
    assert_eq!(o.a.topk, r.a.topk);
    assert_eq!(o.b.topk, r.b.topk);
    assert_ne!(o.b.posicao, r.b.posicao);
    assert_ne!(o.a.linear, r.a.linear);
}

#[test]
fn ponto_no_tempo_mudar_o_ultimo_concurso_so_mexe_na_ultima_dobra() {
    let base = ranking_sintetico(&MINI, 30, 11, false);
    let r = avaliar_ranking(&MINI, &base, 2).expect("série válida");
    // Troca o resultado e as ordens do concurso 20 (o último avaliado).
    let mudada = alterar(&base, 19, |l| {
        if l.concurso == 20 {
            l.saiu = Some(l.dezena >= 3);
            l.posicao_ranking_a = 5 - l.dezena;
            l.posicao_ranking_b = 5 - l.dezena;
        }
    });
    let m = avaliar_ranking(&MINI, &mudada, 2).expect("série válida");
    // As dobras 2, 3 e 4 (índices 0..3) acabam antes do 20: nada muda nelas,
    // em nenhuma das medidas.
    for (a, b) in [
        (&r.a.topk.dobras, &m.a.topk.dobras),
        (&r.b.topk.dobras, &m.b.topk.dobras),
    ] {
        assert_eq!(a[..3], b[..3]);
    }
    for (a, b) in [
        (&r.a.linear.dobras, &m.a.linear.dobras),
        (&r.b.posicao.dobras, &m.b.posicao.dobras),
        (&r.b.linear.dobras, &m.b.linear.dobras),
    ] {
        assert_eq!(a[..3], b[..3]);
        assert_ne!(a[3], b[3], "a última dobra tem o concurso 20");
    }
}

#[test]
fn determinismo_duas_execucoes_dao_o_mesmo_resultado() {
    let linhas = ranking_sintetico(cfg("quina"), 220, 7, true);
    let k = k_da(cfg("quina"));
    let a = avaliar_ranking(cfg("quina"), &linhas, k).unwrap();
    let b = avaliar_ranking(cfg("quina"), &linhas, k).unwrap();
    assert_eq!(a, b);
    // A ordem das linhas na entrada não importa.
    let mut embaralhadas = linhas.clone();
    embaralhadas.reverse();
    assert_eq!(avaliar_ranking(cfg("quina"), &embaralhadas, k).unwrap(), a);
}

// ------------------------------------------------- A é a calibração da 1c

/// Média por concurso do log loss e do Brier de usar `nota_a` como
/// probabilidade, nos concursos de `faixa` (a conta da 1c, feita aqui à parte).
fn metricas_da_nota_a(
    cfg: &LoteriaConfig,
    linhas: &[LinhaRanking],
    faixa: FaixaConcursos,
) -> Vec<MetricasConcurso> {
    let acaso = taxa_do_acaso(cfg);
    let mut ordenadas: Vec<&LinhaRanking> = linhas.iter().collect();
    ordenadas.sort_by_key(|l| (l.concurso, l.dezena));
    ordenadas
        .chunk_by(|a, b| a.concurso == b.concurso)
        .filter(|g| faixa.contem(g[0].concurso))
        .map(|g| {
            let mut soma = [0.0_f64; 4];
            for l in g {
                let saiu = l.saiu.expect("concurso sorteado");
                soma[0] += log_loss(l.nota_a, saiu);
                soma[1] += log_loss(acaso, saiu);
                soma[2] += brier(l.nota_a, saiu);
                soma[3] += brier(acaso, saiu);
            }
            let n = g.len() as f64;
            MetricasConcurso {
                concurso: g[0].concurso,
                log_loss_perfil: soma[0] / n,
                log_loss_acaso: soma[1] / n,
                brier_perfil: soma[2] / n,
                brier_acaso: soma[3] / n,
            }
        })
        .collect()
}

fn confere_a_igual_a_1c(cfg: &LoteriaConfig, linhas: &[LinhaRanking], rotulo: &str) {
    let k = k_da(cfg);
    let r = avaliar_ranking(cfg, linhas, k).expect("avaliação");
    // A série da 1c: a posição do índice e o `saiu`, como o `sync avaliar`.
    let obs_1c: Vec<Observacao> = linhas
        .iter()
        .map(|l| Observacao {
            concurso: l.concurso,
            dezena: l.dezena,
            posicao: l.posicao_indice,
            saiu: l.saiu,
        })
        .collect();

    // 1) As probabilidades de A são a saída de `calibrar` sobre a posição do
    // índice, bit a bit.
    let calibradas = calibrar(cfg, &obs_1c).expect("1c");
    assert_eq!(calibradas.len(), linhas.len(), "{rotulo}");
    for (c, l) in calibradas.iter().zip(linhas) {
        assert_eq!((c.concurso, c.dezena), (l.concurso, l.dezena), "{rotulo}");
        assert_eq!(
            c.probabilidade.to_bits(),
            l.nota_a.to_bits(),
            "{rotulo}: concurso {} dezena {}",
            c.concurso,
            c.dezena
        );
    }

    // 2) Por isso o log loss e o Brier de A, nas dobras de teste, são os do
    // walk-forward do `avaliar` (mesmos bits), e não precisam ser repetidos.
    let wf = walk_forward(cfg, &obs_1c, k).expect("walk-forward");
    for (d, dobra) in r.dobras.iter().enumerate() {
        let via_nota = metricas_da_nota_a(cfg, linhas, dobra.faixa);
        let resumo = resumir(&via_nota).expect("dobra com concursos");
        let da_1c = &wf.dobras[dobra.dobra as usize - 1].metricas;
        assert_eq!(resumo, *da_1c, "{rotulo}: dobra {}", dobra.dobra);
        assert_eq!(
            da_1c.log_loss.perfil.to_bits(),
            resumo.log_loss.perfil.to_bits()
        );
        assert_eq!(d + 2, dobra.dobra as usize);
    }
    let total = resumir(&metricas_da_nota_a(cfg, linhas, r.faixa)).expect("trecho");
    let total_1c = resumir(
        &metricas_por_concurso_entre(cfg, &obs_1c, r.faixa.primeiro, r.faixa.ultimo).expect("1c"),
    )
    .expect("1c");
    assert_eq!(total, total_1c, "{rotulo}: trecho inteiro");
}

#[test]
fn a_e_a_calibracao_da_1c_bit_a_bit_na_mini() {
    confere_a_igual_a_1c(&MINI, &ranking_sintetico(&MINI, 30, 1, true), "mini 1");
    confere_a_igual_a_1c(&MINI, &ranking_sintetico(&MINI, 25, 2, false), "mini 2");
}

#[test]
fn a_e_a_calibracao_da_1c_bit_a_bit_nas_tres_loterias() {
    for (slug, concursos) in [("lotofacil", 150_u32), ("megasena", 120), ("quina", 90)] {
        let c = cfg(slug);
        confere_a_igual_a_1c(c, &ranking_sintetico(c, concursos, 9, true), slug);
    }
}

// ---------------------------------------------------------- bootstrap

#[test]
fn bootstrap_de_b_e_o_da_1c_sobre_as_dobras_2_a_5() {
    let c = cfg("lotofacil");
    let linhas = ranking_sintetico(c, 240, 4, true);
    let k = k_da(c);
    let r = avaliar_ranking(c, &linhas, k).expect("avaliação");
    assert!(r.concursos >= 2 * BLOCO_BOOTSTRAP, "{}", r.concursos);
    let boot = r.b.bootstrap.as_ref().expect("trecho longo o bastante");

    // Referência independente: a diferença por concurso de B (calibrar sobre a
    // ordem de B) nos concursos do trecho, e o bootstrap da 1c sobre ela.
    let obs_b: Vec<Observacao> = linhas
        .iter()
        .map(|l| Observacao {
            concurso: l.concurso,
            dezena: l.dezena,
            posicao: l.posicao_ranking_b,
            saiu: l.saiu,
        })
        .collect();
    let probs = calibrar(c, &obs_b).expect("1c");
    let acaso = taxa_do_acaso(c);
    let mut diferencas = Vec::new();
    let mut briers = Vec::new();
    for g in probs.chunk_by(|a, b| a.concurso == b.concurso) {
        if !r.faixa.contem(g[0].concurso) {
            continue;
        }
        let n = g.len() as f64;
        let (mut ll, mut br) = (0.0, 0.0);
        for p in g {
            let saiu = p.saiu.expect("sorteado");
            ll += log_loss(p.probabilidade, saiu) - log_loss(acaso, saiu);
            br += brier(p.probabilidade, saiu) - brier(acaso, saiu);
        }
        // O mesmo `d_t` da 1c: média do perfil − média do acaso, aqui como
        // média das diferenças (iguais a menos de arredondamento).
        diferencas.push(ll / n);
        briers.push(br / n);
    }
    assert_eq!(diferencas.len(), r.concursos);
    let esperado = bootstrap_circular(
        &diferencas,
        BLOCO_BOOTSTRAP,
        REAMOSTRAGENS_BOOTSTRAP,
        SEMENTE_BOOTSTRAP,
    )
    .unwrap();
    assert_eq!(boot.log_loss.concursos, r.concursos);
    assert_eq!(boot.log_loss.bloco, 50);
    assert_eq!(boot.log_loss.reamostragens, 10_000);
    assert_eq!(boot.log_loss.semente, SEMENTE_BOOTSTRAP);
    // Média e IC no mesmo valor, a menos de arredondamento da soma.
    assert!(perto(boot.log_loss.media, esperado.media));
    assert!(perto(boot.log_loss.ic95.inferior, esperado.ic95.inferior));
    assert!(perto(boot.log_loss.ic95.superior, esperado.ic95.superior));
    assert!((boot.log_loss.p_valor - esperado.p_valor).abs() < 1e-3);
    assert!(boot.brier.concursos == briers.len());
    assert_eq!(r.b.supera_o_acaso(), Some(boot.log_loss.p_valor <= ALFA));
    assert_eq!(r.b.concursos_minimos, 2 * BLOCO_BOOTSTRAP);
}

// ------------------------------------------------------------- erros

#[test]
fn k_fora_do_intervalo_e_recusado() {
    let linhas = serie_mao();
    for k in [0, 4, 9] {
        assert!(
            matches!(
                avaliar_ranking(&MINI, &linhas, k),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ),
            "k = {k}"
        );
    }
    assert!(avaliar_ranking(&MINI, &linhas, 3).is_ok());
}

#[test]
fn menos_concursos_que_dobras_e_sem_concursos() {
    // Seis concursos dão 5 avaliados (ok); cinco dão 4 (< 5 dobras).
    let cinco: Vec<LinhaRanking> = serie_mao()
        .into_iter()
        .filter(|l| l.concurso <= 5)
        .collect();
    assert!(matches!(
        avaliar_ranking(&MINI, &cinco, 2),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        avaliar_ranking(&MINI, &[], 2),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

#[test]
fn ordem_que_nao_e_permutacao_e_recusada() {
    let mut linhas = serie_mao();
    // Posição repetida no concurso 4 (ordem de B).
    for l in linhas.iter_mut().filter(|l| l.concurso == 4) {
        l.posicao_ranking_b = 1;
    }
    assert!(matches!(
        avaliar_ranking(&MINI, &linhas, 2),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
    // Posição fora de 1..=N na ordem de A.
    let mut fora = serie_mao();
    fora[9].posicao_ranking_a = 7;
    assert!(matches!(
        avaliar_ranking(&MINI, &fora, 2),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

// ------------------------------------------------- bordas adicionais

fn faixas_das_dobras(r: &AvaliacaoRanking) -> Vec<(u32, u32)> {
    r.dobras
        .iter()
        .map(|d| (d.faixa.primeiro, d.faixa.ultimo))
        .collect()
}

fn acertos_das_dobras(t: &PorTrecho<ResumoTopK>) -> Vec<u64> {
    t.dobras.iter().map(|d| d.total_acertos).collect()
}

fn p_valores_das_dobras(t: &PorTrecho<ResumoTopK>) -> Vec<f64> {
    t.dobras.iter().map(|d| d.p_valor.probabilidade).collect()
}

/// Linhas sem as do concurso `concurso`.
fn sem_o_concurso(linhas: &[LinhaRanking], concurso: u32) -> Vec<LinhaRanking> {
    linhas
        .iter()
        .filter(|l| l.concurso != concurso)
        .cloned()
        .collect()
}

// 1) k nas pontas ------------------------------------------------------

#[test]
fn k_igual_a_1_na_serie_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 1).expect("série válida");
    assert_eq!(r.k, 1);
    // Top-1 pela ordem de A: dezena na posição 1 de cada concurso 3 a 6 = d2
    // (saíram 1 e 4: 0), d1 (1 e 2: 1), d4 (3 e 4: 1), d1 (1 e 3: 1).
    assert_eq!(acertos_das_dobras(&r.a.topk), [0, 1, 1, 1]);
    // Top-1 pela ordem de B: d1 (1 e 4: 1), d2 (1 e 2: 1), d1 (3 e 4: 0),
    // d2 (1 e 3: 0).
    assert_eq!(acertos_das_dobras(&r.b.topk), [1, 1, 0, 0]);

    // Hipergeométrica (N = 4, K = 2, k = 1): P(0) = P(1) = 1/2; acaso = 0,5
    // acerto por concurso. S ~ Binomial(4, 1/2).
    let a = &r.a.topk.total;
    assert_eq!((a.k, a.concursos_avaliados, a.total_acertos), (1, 4, 3));
    assert_eq!(a.acertos.perfil, 0.75);
    assert_eq!(a.acertos.acaso, 0.5);
    assert!(perto(a.acertos.diferenca, 0.25));
    let observados: Vec<usize> = a.distribuicao.iter().map(|f| f.observados).collect();
    assert_eq!(observados, [1, 3]);
    let esperados: Vec<f64> = a.distribuicao.iter().map(|f| f.esperados).collect();
    assert!(perto(esperados[0], 2.0) && perto(esperados[1], 2.0));
    // P(S >= 3) = (C(4,3) + C(4,4)) / 16 = 5/16.
    assert!(
        perto(a.p_valor.probabilidade, 5.0 / 16.0),
        "{:?}",
        a.p_valor
    );

    let b = &r.b.topk.total;
    assert_eq!(b.total_acertos, 2);
    assert_eq!(b.acertos.perfil, 0.5);
    assert!(perto(b.acertos.diferenca, 0.0));
    // P(S >= 2) = (6 + 4 + 1) / 16 = 11/16.
    assert!(
        perto(b.p_valor.probabilidade, 11.0 / 16.0),
        "{:?}",
        b.p_valor
    );

    // Por dobra (um concurso cada): 0 acerto dá P = 1; 1 acerto dá P = 1/2.
    let ps = p_valores_das_dobras(&r.a.topk);
    assert!(perto(ps[0], 1.0), "{ps:?}");
    assert!(ps[1..].iter().all(|&p| perto(p, 0.5)), "{ps:?}");
}

#[test]
fn k_igual_a_n_menos_1_na_serie_a_mao() {
    let r = avaliar_ranking(&MINI, &serie_mao(), 3).expect("série válida");
    assert_eq!(r.k, 3);
    // Top-3 pela ordem de A: concurso 3 = {d2, d1, d3} (saíram 1 e 4: 1);
    // 4 = {d1, d2, d3} (1 e 2: 2); 5 = {d4, d3, d2} (3 e 4: 2);
    // 6 = {d1, d2, d3} (1 e 3: 2).
    assert_eq!(acertos_das_dobras(&r.a.topk), [1, 2, 2, 2]);
    // Top-3 pela ordem de B: {d1, d2, d3} → 1; {d2, d3, d1} → 2;
    // {d1, d2, d4} → 1; {d2, d1, d3} → 2.
    assert_eq!(acertos_das_dobras(&r.b.topk), [1, 2, 1, 2]);

    // Hipergeométrica (N = 4, K = 2, k = 3): só 1 ou 2 acertos, P = 1/2 cada;
    // acaso = 3·2/4 = 1,5 por concurso. S = 4 + Binomial(4, 1/2).
    let a = &r.a.topk.total;
    assert_eq!((a.k, a.total_acertos), (3, 7));
    assert_eq!(a.acertos.perfil, 1.75);
    assert_eq!(a.acertos.acaso, 1.5);
    let observados: Vec<usize> = a.distribuicao.iter().map(|f| f.observados).collect();
    assert_eq!(observados, [0, 1, 3]);
    let esperados: Vec<f64> = a.distribuicao.iter().map(|f| f.esperados).collect();
    assert!(esperados[0] == 0.0 && perto(esperados[1], 2.0) && perto(esperados[2], 2.0));
    // P(S >= 7) = P(Binomial >= 3) = 5/16.
    assert!(
        perto(a.p_valor.probabilidade, 5.0 / 16.0),
        "{:?}",
        a.p_valor
    );

    let b = &r.b.topk.total;
    assert_eq!(b.total_acertos, 6);
    assert_eq!(b.acertos.perfil, 1.5);
    // P(S >= 6) = P(Binomial >= 2) = 11/16.
    assert!(
        perto(b.p_valor.probabilidade, 11.0 / 16.0),
        "{:?}",
        b.p_valor
    );

    // Por dobra: 1 acerto (o mínimo possível) dá P = 1; 2 acertos dão 1/2.
    let ps = p_valores_das_dobras(&r.a.topk);
    assert!(perto(ps[0], 1.0), "{ps:?}");
    assert!(ps[1..].iter().all(|&p| perto(p, 0.5)), "{ps:?}");
}

#[test]
fn k_so_muda_o_topk_e_nao_as_medidas_de_probabilidade() {
    let base = avaliar_ranking(&MINI, &serie_mao(), 2).unwrap();
    for k in [1, 3] {
        let r = avaliar_ranking(&MINI, &serie_mao(), k).unwrap();
        assert_eq!(r.a.linear, base.a.linear, "k = {k}");
        assert_eq!(r.b.posicao, base.b.posicao, "k = {k}");
        assert_eq!(r.b.linear, base.b.linear, "k = {k}");
        assert_eq!(r.dobras, base.dobras, "k = {k}");
        assert_eq!(r.faixa, base.faixa, "k = {k}");
    }
}

// 2) sobra da divisão nas dobras --------------------------------------

#[test]
fn mini_com_24_concursos_corta_no_20_e_a_sobra_vai_para_a_ultima_dobra() {
    let linhas = ranking_sintetico(&MINI, 24, 13, false);
    let r = avaliar_ranking(&MINI, &linhas, 2).expect("série válida");
    // Avaliados 2..=20 (19, o corte corta os concursos 21 a 24): 19 / 5 = 3 e
    // sobram 4 na última. Dobras: 2–4 (fora), 5–7, 8–10, 11–13 e 14–20.
    assert_eq!(faixas_das_dobras(&r), [(5, 7), (8, 10), (11, 13), (14, 20)]);
    assert_eq!((r.faixa.primeiro, r.faixa.ultimo), (5, 20));
    // 3 + 3 + 3 + 7 = 16.
    assert_eq!(r.concursos, 16);
    let tamanhos: Vec<usize> =
        r.a.topk
            .dobras
            .iter()
            .map(|d| d.concursos_avaliados)
            .collect();
    assert_eq!(tamanhos, [3, 3, 3, 7]);
    for m in [&r.a.linear, &r.b.posicao, &r.b.linear] {
        let t: Vec<usize> = m.dobras.iter().map(|d| d.concursos_avaliados).collect();
        assert_eq!(t, [3, 3, 3, 7]);
        assert_eq!(m.total.concursos_avaliados, 16);
    }
    // Só 16 concursos no trecho: abaixo dos 100 do bootstrap.
    assert!(r.b.bootstrap.is_none());

    // Os concursos 21 a 24 não entram em nada.
    let ate_20: Vec<LinhaRanking> = linhas
        .iter()
        .filter(|l| l.concurso <= 20)
        .cloned()
        .collect();
    assert_eq!(avaliar_ranking(&MINI, &ate_20, 2).unwrap(), r);
}

#[test]
fn loterias_reais_com_31_concursos_tem_dobras_exatas_de_6() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let linhas = ranking_sintetico(c, 31, 17, false);
        let r = avaliar_ranking(c, &linhas, k_da(c)).expect(slug);
        // Avaliados 2..=31 (30): 5 dobras de 6, sem sobra. A dobra 1 é 2–7.
        assert_eq!(
            faixas_das_dobras(&r),
            [(8, 13), (14, 19), (20, 25), (26, 31)],
            "{slug}"
        );
        assert_eq!(r.concursos, 24, "{slug}");
        assert_eq!(r.corte, c.ultimo_concurso_busca, "{slug}");
        for d in &r.a.topk.dobras {
            assert_eq!(d.concursos_avaliados, 6, "{slug}");
        }
        for d in &r.b.posicao.dobras {
            assert_eq!(d.concursos_avaliados, 6, "{slug}");
        }
        assert!(r.b.bootstrap.is_none(), "{slug}: 24 < 100");
    }
}

// 3) totais coerentes com as dobras nas loterias reais ----------------

/// Média ponderada por concursos avaliados do que há em cada dobra contra o
/// total do trecho (as dobras particionam o trecho e cada concurso pesa igual).
fn confere_media_ponderada(m: &PorTrecho<ResumoAvaliacao>, rotulo: &str) {
    let peso: f64 = m.dobras.iter().map(|d| d.concursos_avaliados as f64).sum();
    assert_eq!(peso, m.total.concursos_avaliados as f64, "{rotulo}");
    type Campo = fn(&ResumoAvaliacao) -> f64;
    let campos: [(&str, Campo); 4] = [
        ("ll perfil", |r| r.log_loss.perfil),
        ("ll acaso", |r| r.log_loss.acaso),
        ("brier perfil", |r| r.brier.perfil),
        ("brier acaso", |r| r.brier.acaso),
    ];
    for (nome, f) in campos {
        let ponderada = m
            .dobras
            .iter()
            .map(|d| f(d) * d.concursos_avaliados as f64)
            .sum::<f64>()
            / peso;
        let total = f(&m.total);
        assert!(
            perto(ponderada, total),
            "{rotulo} {nome}: {ponderada:?} contra {total:?}"
        );
    }
}

#[test]
fn totais_batem_com_as_dobras_nas_tres_loterias_reais() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let c = cfg(slug);
        let linhas = ranking_sintetico(c, 200, 21, false);
        let r = avaliar_ranking(c, &linhas, k_da(c)).expect(slug);
        // Avaliados 2..=200 (199): dobras de 39 e a última de 43; a dobra 1 é
        // 2–40, então o trecho de teste tem 39·3 + 43 = 160 concursos.
        assert_eq!(
            faixas_das_dobras(&r),
            [(41, 79), (80, 118), (119, 157), (158, 200)],
            "{slug}"
        );
        assert_eq!(r.concursos, 160, "{slug}");

        for (rotulo, topk) in [("A", &r.a.topk), ("B", &r.b.topk)] {
            let soma: u64 = topk.dobras.iter().map(|d| d.total_acertos).sum();
            assert_eq!(soma, topk.total.total_acertos, "{slug} {rotulo}");
            let concursos: usize = topk.dobras.iter().map(|d| d.concursos_avaliados).sum();
            assert_eq!(concursos, 160, "{slug} {rotulo}");
            // Média de acertos por concurso do total = soma / T.
            assert!(
                perto(topk.total.acertos.perfil, soma as f64 / 160.0),
                "{slug} {rotulo}"
            );
        }
        confere_media_ponderada(&r.a.linear, &format!("{slug} A linear"));
        confere_media_ponderada(&r.b.posicao, &format!("{slug} B posição"));
        confere_media_ponderada(&r.b.linear, &format!("{slug} B linear"));
        // 160 >= 2 blocos de 50: com bootstrap.
        assert!(r.b.bootstrap.is_some(), "{slug}");
    }
}

// 4) B igual a A, em identidade ---------------------------------------

/// `saiu` ao acaso (série aleatória) e as duas ordens iguais à identidade:
/// a dezena `i` na posição `i`.
fn ranking_identidade(cfg: &LoteriaConfig, concursos: u32, semente: u64) -> Vec<LinhaRanking> {
    serie_aleatoria(cfg, concursos, semente, false)
        .into_iter()
        .map(|o| {
            let posicao = o.dezena - cfg.dezena_min + 1;
            LinhaRanking {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao_indice: posicao,
                saiu: o.saiu,
                nota_a: 0.0,
                nota_b: 0.0,
                posicao_ranking_a: posicao,
                posicao_ranking_b: posicao,
            }
        })
        .collect()
}

#[test]
fn mesma_ordem_em_a_e_b_da_as_mesmas_medidas() {
    for (c, concursos) in [(&MINI, 30_u32), (cfg("lotofacil"), 150), (cfg("quina"), 90)] {
        let linhas = ranking_identidade(c, concursos, 31);
        let r = avaliar_ranking(c, &linhas, k_da(c)).expect(c.slug);
        assert!(r.concursos > 0, "{}", c.slug);
        assert_eq!(r.a.topk, r.b.topk, "{}", c.slug);
        assert_eq!(r.a.linear, r.b.linear, "{}", c.slug);
    }
}

#[test]
fn ordem_identidade_em_b_e_a_calibracao_da_1c_sobre_a_posicao_igual_a_dezena() {
    // Referência independente: `metricas_por_concurso_entre` da 1c sobre a
    // série com `posicao = dezena`, no trecho de teste, contra o resumo de B.
    let c = &MINI;
    let linhas = ranking_identidade(c, 30, 33);
    let r = avaliar_ranking(c, &linhas, 2).expect("série válida");
    let obs: Vec<Observacao> = linhas
        .iter()
        .map(|l| Observacao {
            concurso: l.concurso,
            dezena: l.dezena,
            posicao: l.dezena,
            saiu: l.saiu,
        })
        .collect();
    let esperado =
        metricas_por_concurso_entre(c, &obs, r.faixa.primeiro, r.faixa.ultimo).expect("1c");
    assert_eq!(r.b.posicao.total, resumir(&esperado).expect("trecho"));
}

// 5) linhas duplicadas e faltando -------------------------------------

#[test]
fn linha_duplicada_ate_o_corte_e_recusada() {
    let base = ranking_sintetico(&MINI, 30, 41, false);
    // Concursos no começo, no meio e no próprio corte (20).
    for concurso in [1, 10, 20] {
        let copia = base
            .iter()
            .find(|l| l.concurso == concurso && l.dezena == 3)
            .cloned()
            .unwrap();

        let mut exata = base.clone();
        exata.push(copia.clone());
        assert!(
            matches!(
                avaliar_ranking(&MINI, &exata, 2),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ),
            "cópia exata no concurso {concurso}"
        );

        // Duplicada com outro `saiu`.
        let mut contraditoria = base.clone();
        contraditoria.push(LinhaRanking {
            saiu: copia.saiu.map(|s| !s),
            ..copia
        });
        assert!(
            matches!(
                avaliar_ranking(&MINI, &contraditoria, 2),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ),
            "cópia contraditória no concurso {concurso}"
        );
    }
}

#[test]
fn dezena_faltando_ate_o_corte_e_recusada() {
    let base = ranking_sintetico(&MINI, 30, 43, false);
    for concurso in [1, 10, 20] {
        let linhas: Vec<LinhaRanking> = base
            .iter()
            .filter(|l| !(l.concurso == concurso && l.dezena == 2))
            .cloned()
            .collect();
        assert!(
            matches!(
                avaliar_ranking(&MINI, &linhas, 2),
                Err(SpiderError::AvaliacaoEntradaInvalida(_))
            ),
            "dezena 2 faltando no concurso {concurso}"
        );
    }
}

#[test]
fn linha_duplicada_ou_faltando_acima_do_corte_e_ignorada() {
    let base = ranking_sintetico(&MINI, 30, 45, false);
    // Referência: a série cortada no 20, sem nada acima.
    let ate_20: Vec<LinhaRanking> = base.iter().filter(|l| l.concurso <= 20).cloned().collect();
    let esperado = avaliar_ranking(&MINI, &ate_20, 2).expect("série válida");

    for concurso in [21, 25, 30] {
        let mut duplicada = base.clone();
        let copia = duplicada
            .iter()
            .find(|l| l.concurso == concurso && l.dezena == 3)
            .cloned()
            .unwrap();
        duplicada.push(copia);
        assert_eq!(
            avaliar_ranking(&MINI, &duplicada, 2).expect("acima do corte não vale"),
            esperado,
            "duplicada no concurso {concurso}"
        );

        let faltando: Vec<LinhaRanking> = base
            .iter()
            .filter(|l| !(l.concurso == concurso && l.dezena == 2))
            .cloned()
            .collect();
        assert_eq!(
            avaliar_ranking(&MINI, &faltando, 2).expect("acima do corte não vale"),
            esperado,
            "dezena faltando no concurso {concurso}"
        );
    }
}

// 6) snapshot sem resultado -------------------------------------------

#[test]
fn snapshot_acima_do_corte_nao_muda_o_resultado() {
    // 25 concursos, o 25 é o snapshot do próximo sorteio; o corte é 20.
    let com = ranking_sintetico(&MINI, 25, 51, true);
    assert!(com
        .iter()
        .filter(|l| l.concurso == 25)
        .all(|l| l.saiu.is_none()));
    let sem = sem_o_concurso(&com, 25);
    let r = avaliar_ranking(&MINI, &com, 2).expect("série válida");
    assert_eq!(r, avaliar_ranking(&MINI, &sem, 2).unwrap());
    assert_eq!(r.faixa.ultimo, 20);
    assert_eq!(r.concursos, 16);
}

#[test]
fn snapshot_no_ultimo_concurso_antes_do_corte_nao_e_avaliado() {
    // 12 concursos, o 12 é snapshot: avaliados 2..=11 (10), duas por dobra. A
    // dobra 1 é 2–3.
    let com = ranking_sintetico(&MINI, 12, 53, true);
    let sem = sem_o_concurso(&com, 12);
    let r = avaliar_ranking(&MINI, &com, 2).expect("série válida");
    assert_eq!(faixas_das_dobras(&r), [(4, 5), (6, 7), (8, 9), (10, 11)]);
    assert_eq!(r.concursos, 8);
    assert_eq!(r.faixa.ultimo, 11);
    assert_eq!(r, avaliar_ranking(&MINI, &sem, 2).unwrap());
}

#[test]
fn snapshot_nao_conta_para_o_minimo_de_dobras() {
    // A série à mão (6 concursos, 5 avaliados: o mínimo) com um snapshot 7
    // continua valendo e dá o resultado de antes.
    let sem = serie_mao();
    let mut com = sem.clone();
    com.extend(concurso_ab(7, [1, 2, 3, 4], [4, 3, 2, 1], None));
    assert_eq!(
        avaliar_ranking(&MINI, &com, 2).unwrap(),
        avaliar_ranking(&MINI, &sem, 2).unwrap()
    );

    // Com 5 concursos mais um snapshot sobram 4 avaliados (< 5 dobras): o
    // snapshot não completa o que falta.
    let mut cinco: Vec<LinhaRanking> = sem.into_iter().filter(|l| l.concurso <= 5).collect();
    cinco.extend(concurso_ab(6, [1, 2, 3, 4], [1, 2, 3, 4], None));
    assert!(matches!(
        avaliar_ranking(&MINI, &cinco, 2),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

#[test]
fn concurso_sem_resultado_no_meio_ate_o_corte_e_recusado() {
    // Só o último concurso pode estar sem `saiu`.
    let mut linhas = serie_mao();
    for l in linhas.iter_mut().filter(|l| l.concurso == 3) {
        l.saiu = None;
    }
    assert!(matches!(
        avaliar_ranking(&MINI, &linhas, 2),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

// 7) independência da ordem de entrada --------------------------------

/// Misturador de 64 bits (splitmix64): bijeção, então as chaves não colidem.
fn mistura(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Mesmas linhas em ordem pseudoaleatória, derivada de (concurso, dezena).
fn embaralhadas(linhas: &[LinhaRanking], semente: u64) -> Vec<LinhaRanking> {
    let mut v = linhas.to_vec();
    v.sort_by_key(|l| mistura(((u64::from(l.concurso) << 32) | u64::from(l.dezena)) ^ semente));
    v
}

#[test]
fn embaralhar_as_linhas_nao_muda_o_resultado() {
    let casos = [
        ("mini à mão", &MINI, serie_mao(), 2),
        ("mini 30", &MINI, ranking_sintetico(&MINI, 30, 61, true), 2),
        (
            "lotofacil",
            cfg("lotofacil"),
            ranking_sintetico(cfg("lotofacil"), 150, 62, true),
            15,
        ),
        (
            "megasena",
            cfg("megasena"),
            ranking_sintetico(cfg("megasena"), 120, 63, false),
            6,
        ),
    ];
    for (rotulo, c, linhas, k) in casos {
        let esperado = avaliar_ranking(c, &linhas, k).expect(rotulo);
        for semente in [1_u64, 2, 3] {
            let misturadas = embaralhadas(&linhas, semente);
            assert_ne!(misturadas, linhas, "{rotulo}: a mistura não mexeu na ordem");
            assert_eq!(
                avaliar_ranking(c, &misturadas, k).expect(rotulo),
                esperado,
                "{rotulo}, semente {semente}"
            );
        }
    }
}
