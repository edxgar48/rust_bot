use super::*;
use crate::avaliacao::evidencia::esperado_exato;
use crate::avaliacao::tests::{serie_aleatoria, Gerador, MINI};
use crate::avaliacao::{
    calcular_taxas, calibrar, esperado_sob_o_acaso, Grau, Observacao, ObservacaoPosicoes,
    REPLICAS_ESPERADO, SEMENTE_ESPERADO,
};
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

// ------------------------------------------------------------ montagem

fn algum<const L: usize>(v: [i64; L]) -> [Option<i64>; L] {
    v.map(Some)
}

/// Um concurso da MINI com os componentes 0 e 1 dados (os 2 a 8 valem 0).
fn concurso_mini(
    concurso: u32,
    posicoes: [u32; 4],
    saidas: Option<[u32; 2]>,
    c0: [Option<i64>; 4],
    c1: [Option<i64>; 4],
) -> Vec<ObservacaoPosicoes> {
    (1..=4_u32)
        .map(|d| {
            let i = (d - 1) as usize;
            let mut componentes = [Some(0); QTD_COMPONENTES];
            componentes[0] = c0[i];
            componentes[1] = c1[i];
            ObservacaoPosicoes {
                concurso,
                dezena: d,
                posicao: posicoes[i],
                saiu: saidas.map(|s| s.contains(&d)),
                componentes,
            }
        })
        .collect()
}

/// Série de referência da MINI (K = 2 de N = 4, acaso 1/2, m = 100), com as
/// contas à mão em `mini_nota_b_a_mao`:
///
/// | concurso | posições | saíram | componente 0 | componente 1 |
/// |---|---|---|---|---|
/// | 1 | 1,2,3,4 | 1 e 2 | 10,20,30,40 | NULL,5,5,5 |
/// | 2 | 2,1,3,4 | 2 e 3 | 30,10,20,20 | NULL ×4 |
/// | 3 | 1,2,3,4 | — | 7,7,7,7 | 1,2,NULL,3 |
fn serie_mini() -> Vec<ObservacaoPosicoes> {
    [
        concurso_mini(
            1,
            [1, 2, 3, 4],
            Some([1, 2]),
            algum([10, 20, 30, 40]),
            [None, Some(5), Some(5), Some(5)],
        ),
        concurso_mini(
            2,
            [2, 1, 3, 4],
            Some([2, 3]),
            algum([30, 10, 20, 20]),
            [None; 4],
        ),
        concurso_mini(
            3,
            [1, 2, 3, 4],
            None,
            algum([7, 7, 7, 7]),
            [Some(1), Some(2), None, Some(3)],
        ),
    ]
    .concat()
}

/// Série sintética válida com componentes aleatórios (valores pequenos, para
/// dar empates; alguns `NULL`; alguns concursos com um componente constante).
fn serie_sintetica(
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
            for (c, valor) in componentes.iter_mut().enumerate() {
                let sorteio = g.proximo();
                *valor = if (o.concurso as usize + c).is_multiple_of(11) {
                    Some(3)
                } else if sorteio.is_multiple_of(7) {
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

fn ranking_da_serie(cfg: &LoteriaConfig, obs: &[ObservacaoPosicoes]) -> Vec<LinhaRanking> {
    let serie = calcular_taxas(cfg, obs).expect("série válida");
    ranquear(cfg, &serie.linhas).expect("linhas válidas")
}

fn linha_de(linhas: &[LinhaRanking], concurso: u32, dezena: u32) -> &LinhaRanking {
    linhas
        .iter()
        .find(|l| l.concurso == concurso && l.dezena == dezena)
        .expect("linha presente")
}

/// Linha de taxas feita à mão (MINI): faixas todas "médio", `saiu` irrelevante.
fn linha_taxas(
    concurso: u32,
    dezena: u32,
    taxa_posicao: f64,
    taxas_faixa: [f64; QTD_COMPONENTES],
) -> LinhaTaxas {
    LinhaTaxas {
        concurso,
        dezena,
        posicao: dezena,
        saiu: None,
        taxa_posicao,
        faixas: [Faixa::Medio; QTD_COMPONENTES],
        taxas_faixa,
    }
}

/// Um concurso da MINI só com a taxa da posição variando (faixas = acaso).
fn concurso_a(taxas_posicao: [f64; 4]) -> Vec<LinhaTaxas> {
    (1..=4_u32)
        .map(|d| {
            linha_taxas(
                1,
                d,
                taxas_posicao[(d - 1) as usize],
                [0.5; QTD_COMPONENTES],
            )
        })
        .collect()
}

fn posicoes_a(linhas: &[LinhaRanking]) -> Vec<u32> {
    linhas.iter().map(|l| l.posicao_ranking_a).collect()
}

fn posicoes_b(linhas: &[LinhaRanking]) -> Vec<u32> {
    linhas.iter().map(|l| l.posicao_ranking_b).collect()
}

// ------------------------------------------------------ arredondamento

#[test]
fn arredondar_espelha_o_indice() {
    assert_eq!(arredondar(0.5), 0.5);
    assert_eq!(arredondar(0.5 + 1e-12), 0.5);
    assert_eq!(arredondar(0.5 + 2e-9), 0.500_000_002);
    // 4e-9 e 6e-9 (fora do meio exato de 1e-9) vão para o múltiplo mais próximo.
    assert_eq!(arredondar(1.4e-9), 1e-9);
    assert_eq!(arredondar(1.6e-9), 2e-9);
    // Zero negativo vira zero positivo.
    for pequeno in [-1e-12, -4e-10, -0.0] {
        let r = arredondar(pequeno);
        assert_eq!(r, 0.0);
        assert!(r.is_sign_positive(), "{pequeno} -> {r:?}");
    }
    assert_eq!(arredondar(-2e-9), -2e-9);
}

// ----------------------------------------------------------- nota A e 1c

fn para_1c(obs: &[ObservacaoPosicoes]) -> Vec<Observacao> {
    obs.iter()
        .map(|o| Observacao {
            concurso: o.concurso,
            dezena: o.dezena,
            posicao: o.posicao,
            saiu: o.saiu,
        })
        .collect()
}

fn confere_nota_a_com_a_1c(cfg: &LoteriaConfig, obs: &[ObservacaoPosicoes], rotulo: &str) {
    let calibradas = calibrar(cfg, &para_1c(obs)).expect("1c");
    let ranking = ranking_da_serie(cfg, obs);
    assert_eq!(calibradas.len(), ranking.len(), "{rotulo}");
    for (c, r) in calibradas.iter().zip(&ranking) {
        assert_eq!(
            (c.concurso, c.dezena, c.posicao, c.saiu),
            (r.concurso, r.dezena, r.posicao_indice, r.saiu),
            "{rotulo}"
        );
        // Bit a bit, não só "perto".
        assert_eq!(
            c.probabilidade.to_bits(),
            r.nota_a.to_bits(),
            "{rotulo}: concurso {} dezena {}",
            c.concurso,
            c.dezena
        );
    }
}

#[test]
fn nota_a_e_a_taxa_da_1c_bit_a_bit_na_mini() {
    confere_nota_a_com_a_1c(&MINI, &serie_mini(), "mini à mão");
    for semente in [1_u64, 2, 3] {
        let s = serie_sintetica(&MINI, 40, semente, true);
        confere_nota_a_com_a_1c(&MINI, &s, &format!("mini semente {semente}"));
    }
}

#[test]
fn nota_a_e_a_taxa_da_1c_bit_a_bit_nas_tres_loterias() {
    for (slug, concursos) in [("lotofacil", 120_u32), ("megasena", 80), ("quina", 60)] {
        let cfg = cfg(slug);
        for (semente, snapshot) in [(5_u64, true), (9, false)] {
            let s = serie_sintetica(cfg, concursos, semente, snapshot);
            confere_nota_a_com_a_1c(cfg, &s, &format!("{slug} semente {semente}"));
        }
    }
}

// ----------------------------------------------------------- nota B

#[test]
fn mini_nota_b_a_mao() {
    let r = ranking_da_serie(&MINI, &serie_mini());
    assert_eq!(r.len(), 12);

    // Concurso 1: nada antes, todas as taxas = K/N = 1/2, então B = ln 1 = 0
    // exatamente e os dois rankings saem pela menor dezena.
    for d in 1..=4 {
        let l = linha_de(&r, 1, d);
        assert_eq!(l.nota_a, 0.5);
        assert_eq!(l.nota_b, 0.0);
        assert!(l.nota_b.is_sign_positive());
        assert_eq!((l.posicao_ranking_a, l.posicao_ranking_b), (d, d));
    }

    // Concurso 3, com o histórico dos concursos 1 e 2 (m = 100, K/N = 1/2):
    // posição: saídas (2,1,1,0) em 2 concursos -> (s + 50)/102;
    // c0 (constante no 3 -> médio): 0 saídas em 1 par -> 50/101;
    // c1: baixo 0 pares -> 1/2; médio 1 saída em 3 pares -> 51/103;
    //     nulo 3 saídas em 5 pares -> 53/105; alto 0 pares -> 1/2;
    // c2..c8 (médio sempre): 4 saídas em 8 pares -> 54/108 = 1/2.
    let ln = f64::ln;
    let c0 = ln(100.0 / 101.0);
    let esperado_b = [
        (1, ln(104.0 / 102.0) + c0 + ln(1.0)),
        (2, ln(1.0) + c0 + ln(102.0 / 103.0)),
        (3, ln(1.0) + c0 + ln(106.0 / 105.0)),
        (4, ln(100.0 / 102.0) + c0 + ln(1.0)),
    ];
    for (d, b) in esperado_b {
        let l = linha_de(&r, 3, d);
        assert!(perto(l.nota_b, b), "dezena {d}: {} x {b}", l.nota_b);
    }
    let a = [52.0 / 102.0, 51.0 / 102.0, 51.0 / 102.0, 50.0 / 102.0];
    for (d, taxa) in (1..=4).zip(a) {
        assert!(perto(linha_de(&r, 3, d).nota_a, taxa), "dezena {d}");
    }
    // A: 52/102 > 51/102 (dezenas 2 e 3 empatam, a 2 vem antes) > 50/102.
    assert_eq!(posicoes_a(&r[8..]), [1, 2, 3, 4]);
    // B: d1 (+0,0095) > d3 (−0,0005) > d2 (−0,0197) > d4 (−0,0298).
    assert_eq!(posicoes_b(&r[8..]), [1, 3, 2, 4]);
    // `saiu` do snapshot ausente; copiado nos demais.
    assert!(r[8..].iter().all(|l| l.saiu.is_none()));
    assert_eq!(linha_de(&r, 1, 1).saiu, Some(true));
    assert_eq!(linha_de(&r, 1, 3).saiu, Some(false));
}

#[test]
fn nota_b_com_todas_as_taxas_no_acaso_vale_zero() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let acaso = taxa_do_acaso(cfg);
        let l = linha_taxas(7, cfg.dezena_min, acaso, [acaso; QTD_COMPONENTES]);
        assert_eq!(nota_b(cfg, &l), 0.0, "{slug}");
        // Na primeira linha de uma série real (nenhum concurso antes) também:
        // as contas inteiras dão exatamente K/N.
        let s = serie_sintetica(cfg, 5, 3, false);
        let r = ranking_da_serie(cfg, &s);
        let n = cfg.qtd_dezenas_possiveis() as usize;
        for linha in &r[..n] {
            assert_eq!(linha.nota_b, 0.0, "{slug}");
            assert_eq!(linha.nota_a, acaso, "{slug}");
            // Tudo empatado: ranking = ordem das dezenas, A e B.
            assert_eq!(linha.posicao_ranking_a, linha.dezena - cfg.dezena_min + 1);
            assert_eq!(linha.posicao_ranking_b, linha.dezena - cfg.dezena_min + 1);
        }
    }
}

#[test]
fn nota_b_soma_posicao_e_nove_faixas_com_o_acaso_do_config() {
    let cfg = cfg("lotofacil");
    // K/N = 3/5 vem da config.
    let faixas = [0.5, 0.6, 0.7, 0.55, 0.65, 0.6, 0.4, 0.62, 0.58];
    let l = linha_taxas(2, 1, 0.66, faixas);
    let a_mao = (0.66_f64 / 0.6).ln() + faixas.iter().map(|t| (t / 0.6).ln()).sum::<f64>();
    assert!(perto(nota_b(cfg, &l), a_mao));
    // A faixa de cada componente pesa exatamente uma vez (derivada em cada
    // taxa): mexer só no componente 8 muda B só por ln da razão.
    let mut m = faixas;
    m[8] = 0.7;
    let depois = nota_b(cfg, &linha_taxas(2, 1, 0.66, m));
    assert!(perto(depois - nota_b(cfg, &l), (0.7_f64 / 0.58).ln()));
    // A taxa baixa (< K/N) puxa B para baixo e a alta para cima.
    assert!(nota_b(cfg, &linha_taxas(2, 1, 0.5, [0.6; 9])) < 0.0);
    assert!(nota_b(cfg, &linha_taxas(2, 1, 0.7, [0.6; 9])) > 0.0);
}

// ------------------------------------------------------------ ordenação

#[test]
fn ordena_por_nota_decrescente_e_desempata_pela_menor_dezena() {
    // Taxas 0,6 0,6 0,7 0,5: dezena 3 primeiro, 1 e 2 empatadas (1 antes), 4.
    let r = ranquear(&MINI, &concurso_a([0.6, 0.6, 0.7, 0.5])).unwrap();
    assert_eq!(posicoes_a(&r), [2, 3, 1, 4]);
    assert_eq!(posicoes_b(&r), [2, 3, 1, 4]);
    // Empate exato de todas.
    let r = ranquear(&MINI, &concurso_a([0.5; 4])).unwrap();
    assert_eq!(posicoes_a(&r), [1, 2, 3, 4]);
    assert_eq!(posicoes_b(&r), [1, 2, 3, 4]);
    // Ordem estritamente decrescente por dezena.
    let r = ranquear(&MINI, &concurso_a([0.4, 0.5, 0.6, 0.7])).unwrap();
    assert_eq!(posicoes_a(&r), [4, 3, 2, 1]);
}

#[test]
fn diferenca_abaixo_de_1e_9_e_empate_e_resolvida_pela_menor_dezena() {
    // Dezena 2 acima da 1 por 1e-12 (ruído): arredondadas empatam, a menor
    // dezena (1) fica à frente; só a 4, 2e-9 acima, passa na frente.
    let taxas = [0.5, 0.5 + 1e-12, 0.5, 0.5 + 2e-9];
    assert!(taxas[1] > taxas[0], "o ruído existe no valor cru");
    let r = ranquear(&MINI, &concurso_a(taxas)).unwrap();
    assert_eq!(posicoes_a(&r), [2, 3, 4, 1]);
    assert_eq!(posicoes_b(&r), [2, 3, 4, 1]);
    // As colunas de nota guardam o valor cru, não o arredondado.
    assert_eq!(r[1].nota_a, 0.5 + 1e-12);
    assert_eq!(r[1].nota_a, taxas[1]);
}

#[test]
fn zero_negativo_arredondado_nao_atrapalha_o_desempate() {
    // Dezena 1 com B = ln(1 − 1e-12) ≈ −1e-12, que arredonda para −0,0: sem a
    // troca por +0,0, ficaria atrás das dezenas 2 a 4 (B = 0).
    let taxas = [0.5 * (1.0 - 1e-12), 0.5, 0.5, 0.5];
    let r = ranquear(&MINI, &concurso_a(taxas)).unwrap();
    assert!(r[0].nota_b < 0.0 && r[0].nota_b > -1e-11);
    assert_eq!(posicoes_b(&r), [1, 2, 3, 4]);
    assert_eq!(posicoes_a(&r), [1, 2, 3, 4]);
    // E o contrário: positivo minúsculo na dezena 4 não passa as outras.
    let taxas = [0.5, 0.5, 0.5, 0.5 * (1.0 + 1e-12)];
    let r = ranquear(&MINI, &concurso_a(taxas)).unwrap();
    assert_eq!(posicoes_b(&r), [1, 2, 3, 4]);
}

/// Conferência independente (O(N²)) da ordem de um grupo de linhas.
fn confere_ordem(linhas: &[LinhaRanking], escolha: fn(&LinhaRanking) -> (f64, u32), n: u32) {
    let mut vistas: Vec<u32> = linhas.iter().map(|l| escolha(l).1).collect();
    vistas.sort_unstable();
    assert_eq!(vistas, (1..=n).collect::<Vec<_>>(), "permutação de 1..=N");
    for a in linhas {
        for b in linhas {
            let ((nota_a, rank_a), (nota_b, rank_b)) = (escolha(a), escolha(b));
            if rank_a < rank_b {
                let (ra, rb) = (arredondar(nota_a), arredondar(nota_b));
                assert!(
                    ra > rb || (ra == rb && a.dezena < b.dezena),
                    "concurso {}: dezena {} (nota {nota_a}, pos {rank_a}) antes da {} (nota {nota_b}, pos {rank_b})",
                    a.concurso, a.dezena, b.dezena
                );
            }
        }
    }
}

#[test]
fn posicoes_formam_permutacao_e_obedecem_a_ordem_nas_tres_loterias() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let n = cfg.qtd_dezenas_possiveis();
        let r = ranking_da_serie(cfg, &serie_sintetica(cfg, 30, 21, true));
        for grupo in r.chunk_by(|a, b| a.concurso == b.concurso) {
            assert_eq!(grupo.len(), n as usize);
            confere_ordem(grupo, |l| (l.nota_a, l.posicao_ranking_a), n);
            confere_ordem(grupo, |l| (l.nota_b, l.posicao_ranking_b), n);
        }
    }
}

#[test]
fn saida_ordenada_por_concurso_e_dezena_qualquer_que_seja_a_entrada() {
    let cfg = cfg("quina");
    let serie = calcular_taxas(cfg, &serie_sintetica(cfg, 12, 4, true)).unwrap();
    let normal = ranquear(cfg, &serie.linhas).unwrap();
    assert!(normal
        .windows(2)
        .all(|w| (w[0].concurso, w[0].dezena) < (w[1].concurso, w[1].dezena)));
    let mut invertida = serie.linhas.clone();
    invertida.reverse();
    assert_eq!(ranquear(cfg, &invertida).unwrap(), normal);
    assert!(ranquear(cfg, &[]).unwrap().is_empty());
}

fn detalhe(r: Result<Vec<LinhaRanking>>) -> String {
    match r {
        Err(SpiderError::AvaliacaoEntradaInvalida(d)) => d,
        outro => panic!("esperava AvaliacaoEntradaInvalida, veio {outro:?}"),
    }
}

#[test]
fn ranquear_recusa_entrada_invalida() {
    let boa = concurso_a([0.5, 0.6, 0.55, 0.45]);

    // Dezena faltando.
    assert!(detalhe(ranquear(&MINI, &boa[..3])).contains("tem 3 linhas"));
    // Dezena repetida no lugar de outra (ordenadas: 1, 2, 3, 3).
    let mut repetida = boa.clone();
    repetida[3].dezena = 3;
    assert!(detalhe(ranquear(&MINI, &repetida)).contains("esperada a dezena 4, veio a 3"));
    // Dezena fora da loteria.
    let mut fora = boa.clone();
    fora[3].dezena = 9;
    assert!(detalhe(ranquear(&MINI, &fora)).contains("esperada a dezena 4"));
    // Posição fora de 1..=N.
    for posicao in [0, 5] {
        let mut ruim = boa.clone();
        ruim[1].posicao = posicao;
        assert!(detalhe(ranquear(&MINI, &ruim)).contains("fora de 1..=4"));
    }
    // `saiu` misto no mesmo concurso.
    let mut misto = boa.clone();
    misto[2].saiu = Some(true);
    assert!(detalhe(ranquear(&MINI, &misto)).contains("`saiu`"));
    // Nota não finita: taxa 0 dá ln(0) = −∞ na B.
    let mut zero = boa.clone();
    zero[0].taxa_posicao = 0.0;
    assert!(detalhe(ranquear(&MINI, &zero)).contains("nota B não finita"));
    let mut nan = boa;
    nan[0].taxa_posicao = f64::NAN;
    assert!(detalhe(ranquear(&MINI, &nan)).contains("não finita"));
}

// ------------------------------------------------------ ponto no tempo

/// Troca quem saiu no concurso `t` por uma rotação das dezenas (continuam
/// *K* saídas), sem tocar em mais nada.
fn muda_o_resultado(
    cfg: &LoteriaConfig,
    obs: &[ObservacaoPosicoes],
    t: u32,
) -> Vec<ObservacaoPosicoes> {
    let n = cfg.qtd_dezenas_possiveis() as usize;
    let do_t: Vec<&ObservacaoPosicoes> = obs.iter().filter(|o| o.concurso == t).collect();
    let originais: Vec<Option<bool>> = do_t.iter().map(|o| o.saiu).collect();
    obs.iter()
        .map(|o| {
            let mut novo = *o;
            if o.concurso == t {
                let i = (o.dezena - cfg.dezena_min) as usize;
                novo.saiu = originais[(i + 1) % n];
            }
            novo
        })
        .collect()
}

#[test]
fn mudar_o_resultado_de_t_nao_muda_as_notas_nem_os_rankings_ate_t() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let concursos = 25_u32;
        let base = serie_sintetica(cfg, concursos, 17, true);
        let ranking = ranking_da_serie(cfg, &base);
        for t in [1_u32, 2, 10, concursos - 1] {
            let mudada = muda_o_resultado(cfg, &base, t);
            assert_ne!(mudada, base, "{slug}: a mutação tem de mudar algo");
            let outro = ranking_da_serie(cfg, &mudada);
            assert_eq!(outro.len(), ranking.len());
            // Até t, só `saiu` das linhas de t difere (é o resultado em si).
            for (a, b) in ranking.iter().zip(&outro).filter(|(a, _)| a.concurso <= t) {
                assert_eq!(
                    (a.nota_a.to_bits(), a.nota_b.to_bits()),
                    (b.nota_a.to_bits(), b.nota_b.to_bits()),
                    "{slug}: t = {t}, concurso {} dezena {}",
                    a.concurso,
                    a.dezena
                );
                assert_eq!(
                    (a.posicao_ranking_a, a.posicao_ranking_b, a.posicao_indice),
                    (b.posicao_ranking_a, b.posicao_ranking_b, b.posicao_indice)
                );
            }
            // De t+1 em diante a nota A muda (a posição de quem mudou de
            // resultado tem outra taxa).
            let diferentes = ranking
                .iter()
                .zip(&outro)
                .filter(|(a, b)| a.concurso == t + 1 && a.nota_a != b.nota_a)
                .count();
            assert!(diferentes > 0, "{slug}: t = {t}: t+1 deveria mudar");
        }
    }
}

#[test]
fn mudar_um_concurso_posterior_nao_muda_os_anteriores() {
    let cfg = cfg("lotofacil");
    let base = serie_sintetica(cfg, 20, 6, true);
    let ranking = ranking_da_serie(cfg, &base);
    let mudada = muda_o_resultado(cfg, &base, 12);
    let outro = ranking_da_serie(cfg, &mudada);
    assert!(ranking
        .iter()
        .zip(&outro)
        .filter(|(a, _)| a.concurso <= 12)
        .all(|(a, b)| a.nota_a == b.nota_a
            && a.nota_b == b.nota_b
            && a.posicao_ranking_a == b.posicao_ranking_a
            && a.posicao_ranking_b == b.posicao_ranking_b));
}

#[test]
fn mudar_posicao_e_componentes_de_t_mais_1_nao_muda_t() {
    // Variante da entrada: o que vem depois de t (posições e valores dos
    // componentes, não só o resultado) não pode mexer em t nem antes.
    let cfg = cfg("lotofacil");
    let base = serie_sintetica(cfg, 20, 9, true);
    let ranking = ranking_da_serie(cfg, &base);
    let t = 10;
    let mut mudada = base.clone();
    let n = cfg.qtd_dezenas_possiveis();
    for o in mudada.iter_mut().filter(|o| o.concurso > t) {
        o.posicao = n + 1 - o.posicao;
        o.componentes = o.componentes.map(|v| v.map(|x| 4 - x));
    }
    assert_ne!(mudada, base);
    let outro = ranking_da_serie(cfg, &mudada);
    assert!(ranking
        .iter()
        .zip(&outro)
        .filter(|(a, _)| a.concurso <= t)
        .all(|(a, b)| a == b));
    assert!(ranking.iter().zip(&outro).any(|(a, b)| a != b));
}

#[test]
fn snapshot_do_proximo_concurso_usa_todo_o_historico() {
    // O snapshot (sem `saiu`) é ranqueado como qualquer concurso e traz as
    // taxas com todos os concursos sorteados.
    let cfg = cfg("megasena");
    let obs = serie_sintetica(cfg, 15, 8, true);
    let serie = calcular_taxas(cfg, &obs).unwrap();
    let r = ranquear(cfg, &serie.linhas).unwrap();
    let ultimo: Vec<&LinhaRanking> = r.iter().filter(|l| l.concurso == 15).collect();
    assert_eq!(ultimo.len(), 60);
    assert!(ultimo.iter().all(|l| l.saiu.is_none()));
    for l in ultimo {
        let taxa = serie.acumuladas.taxa_posicao(l.posicao_indice).unwrap();
        assert_eq!(l.nota_a, taxa);
    }
}

// ------------------------------------------------------------ evidência

fn prioris_de(cfg: &LoteriaConfig) -> (f64, [Priori; QTD_PRIORIS]) {
    let p = taxa_do_acaso(cfg);
    (
        p,
        DESVIOS_PRIORI.map(|s| Priori::nova(p, s).expect("priori válida")),
    )
}

/// `x` saídas em `n` com `log10_fator_bayes` direto, para comparar.
fn direto(x: u64, n: u64, p: f64, prioris: &[Priori; QTD_PRIORIS]) -> AvaliacaoFb {
    let log10_fb = prioris.map(|pr| log10_fator_bayes(x, n, p, &pr).unwrap());
    AvaliacaoFb {
        log10_fb,
        classificacao: log10_fb.map(|v| classificar(v).unwrap()),
    }
}

/// Quantas das classificações caem em (grau, direção), por priori.
fn tabular(avaliacoes: &[&AvaliacaoFb], k: usize) -> Grade<usize> {
    let mut g = Grade::<usize>::default();
    for a in avaliacoes {
        let c = a.classificacao[k];
        match c.direcao {
            Direcao::Vies => g.vies[c.grau.indice()] += 1,
            Direcao::Acaso => g.acaso[c.grau.indice()] += 1,
        }
    }
    g
}

#[test]
fn evidencia_da_mini_a_mao() {
    let cfg = &MINI;
    let serie = calcular_taxas(cfg, &serie_mini()).unwrap();
    let ev = evidencia_das_taxas(cfg, &serie.acumuladas).unwrap();
    let (p, prioris) = prioris_de(cfg);
    assert_eq!(ev.p, 0.5);
    assert_eq!(ev.prioris, prioris);
    assert_eq!(ev.concursos, 2);

    // Posições: saídas (2,1,1,0) em 2 concursos.
    assert_eq!(ev.posicoes.len(), 4);
    for (e, x) in ev.posicoes.iter().zip([2_u64, 1, 1, 0]) {
        assert_eq!((e.saidas, e.total), (x, 2), "posição {}", e.posicao);
        assert_eq!(e.avaliacao, Some(direto(x, 2, p, &prioris)));
    }
    assert_eq!(
        ev.posicoes.iter().map(|e| e.posicao).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );

    // Faixas, na ordem (componente, baixo/médio/alto/nulo).
    assert_eq!(ev.faixas.len(), 36);
    let da = |c: usize, f: Faixa| {
        ev.faixas
            .iter()
            .find(|e| e.componente == c && e.faixa == f)
            .unwrap()
    };
    // c0: baixo 4 de 5, médio 0 de 1, alto 0 de 2, nulo nunca apareceu.
    for (f, x, n) in [
        (Faixa::Baixo, 4_u64, 5_u64),
        (Faixa::Medio, 0, 1),
        (Faixa::Alto, 0, 2),
    ] {
        let e = da(0, f);
        assert_eq!((e.saidas, e.total), (x, n));
        assert_eq!(e.avaliacao, Some(direto(x, n, p, &prioris)));
    }
    let vazia = da(0, Faixa::Nulo);
    assert_eq!((vazia.saidas, vazia.total), (0, 0));
    assert_eq!(vazia.avaliacao, None, "n = 0 fica ausente");
    // c1: nulo 3 de 5 (a faixa nula é uma faixa como as outras), médio 1 de 3.
    assert_eq!(
        da(1, Faixa::Nulo).avaliacao,
        Some(direto(3, 5, p, &prioris))
    );
    assert_eq!(
        da(1, Faixa::Medio).avaliacao,
        Some(direto(1, 3, p, &prioris))
    );
    assert_eq!(da(1, Faixa::Baixo).avaliacao, None);
    assert_eq!(da(1, Faixa::Alto).avaliacao, None);
    // c2: sempre médio, 4 de 8.
    assert_eq!(
        da(2, Faixa::Medio).avaliacao,
        Some(direto(4, 8, p, &prioris))
    );

    // Células com n > 0: 4 posições; 3 + 2 + 7 = 12 faixas.
    for k in 0..QTD_PRIORIS {
        assert_eq!(ev.observada_posicoes[k].total(), 4);
        assert_eq!(ev.observada_faixas[k].total(), 12);
    }
    // Poucos dados: tudo "muito fraca".
    for k in 0..QTD_PRIORIS {
        assert_eq!(
            ev.observada_posicoes[k].vies[0] + ev.observada_posicoes[k].acaso[0],
            4
        );
    }
}

#[test]
fn evidencia_confere_com_o_fator_de_bayes_direto_e_a_contagem_observada() {
    for (slug, concursos) in [("lotofacil", 150_u32), ("megasena", 120), ("quina", 100)] {
        let cfg = cfg(slug);
        let serie = calcular_taxas(cfg, &serie_sintetica(cfg, concursos, 33, true)).expect("série");
        let ev = evidencia_das_taxas(cfg, &serie.acumuladas).unwrap();
        let (p, prioris) = prioris_de(cfg);
        let n = u64::from(concursos) - 1;
        let k = cfg.qtd_dezenas as u64;
        let tamanho = u64::from(cfg.qtd_dezenas_possiveis());
        assert_eq!(ev.concursos, n);
        assert_eq!(ev.p, p);

        let mut soma_saidas = 0;
        for e in &ev.posicoes {
            assert_eq!(e.total, n);
            assert_eq!(e.avaliacao, Some(direto(e.saidas, e.total, p, &prioris)));
            soma_saidas += e.saidas;
        }
        assert_eq!(soma_saidas, k * n, "{slug}: K saídas por concurso");

        let mut com_dados = 0;
        let mut pares = [0_u64; QTD_COMPONENTES];
        let mut sai = [0_u64; QTD_COMPONENTES];
        for e in &ev.faixas {
            let c = serie
                .acumuladas
                .contagem_faixa(e.componente, e.faixa)
                .unwrap();
            assert_eq!((e.saidas, e.total), (c.saidas, c.total));
            if e.total == 0 {
                assert_eq!(e.avaliacao, None);
            } else {
                com_dados += 1;
                assert_eq!(e.avaliacao, Some(direto(e.saidas, e.total, p, &prioris)));
            }
            pares[e.componente] += e.total;
            sai[e.componente] += e.saidas;
        }
        for c in 0..QTD_COMPONENTES {
            assert_eq!(pares[c], tamanho * n, "{slug} componente {c}");
            assert_eq!(sai[c], k * n, "{slug} componente {c}");
        }

        for kk in 0..QTD_PRIORIS {
            // As contagens observadas somam o número de células com n > 0 e
            // batem com uma tabulação independente.
            assert_eq!(ev.observada_posicoes[kk].total(), tamanho as usize);
            assert_eq!(ev.observada_faixas[kk].total(), com_dados);
            let pos: Vec<&AvaliacaoFb> = ev
                .posicoes
                .iter()
                .filter_map(|e| e.avaliacao.as_ref())
                .collect();
            let fx: Vec<&AvaliacaoFb> = ev
                .faixas
                .iter()
                .filter_map(|e| e.avaliacao.as_ref())
                .collect();
            assert_eq!(ev.observada_posicoes[kk], tabular(&pos, kk));
            assert_eq!(ev.observada_faixas[kk], tabular(&fx, kk));
        }
    }
}

/// Série em que a posição 1 sempre sai (as outras *K*−1 saídas são sorteadas).
fn serie_viesada(cfg: &LoteriaConfig, concursos: u32, semente: u64) -> Vec<ObservacaoPosicoes> {
    let mut obs = serie_sintetica(cfg, concursos, semente, false);
    let mut g = Gerador(semente.wrapping_mul(7).wrapping_add(3));
    for grupo in obs.chunks_mut(cfg.qtd_dezenas_possiveis() as usize) {
        let mut resto: Vec<usize> = (0..grupo.len())
            .filter(|&i| grupo[i].posicao != 1)
            .collect();
        g.embaralhar(&mut resto);
        let saem: Vec<usize> = (0..grupo.len())
            .filter(|&i| grupo[i].posicao == 1)
            .chain(resto.into_iter().take(cfg.qtd_dezenas - 1))
            .collect();
        for (i, o) in grupo.iter_mut().enumerate() {
            o.saiu = Some(saem.contains(&i));
        }
    }
    obs
}

#[test]
fn posicao_sempre_sorteada_aparece_como_vies_forte() {
    let cfg = cfg("lotofacil");
    let serie = calcular_taxas(cfg, &serie_viesada(cfg, 300, 2)).unwrap();
    let ev = evidencia_das_taxas(cfg, &serie.acumuladas).unwrap();
    let primeira = &ev.posicoes[0];
    assert_eq!((primeira.saidas, primeira.total), (300, 300));
    for k in 0..QTD_PRIORIS {
        let c = primeira.avaliacao.as_ref().unwrap().classificacao[k];
        assert_eq!(c.direcao, Direcao::Vies);
        assert_eq!(c.grau, Grau::MuitoForte, "priori {k}");
        // E a contagem observada registra esse caso.
        assert!(ev.observada_posicoes[k].vies[Grau::MuitoForte.indice()] >= 1);
    }
}

#[test]
fn evidencia_sem_concursos_deixa_tudo_ausente() {
    let cfg = cfg("quina");
    let ev = evidencia_das_taxas(cfg, &TaxasAcumuladas::novas(cfg)).unwrap();
    assert_eq!(ev.concursos, 0);
    assert_eq!(ev.posicoes.len(), 80);
    assert_eq!(ev.faixas.len(), 36);
    assert!(ev.posicoes.iter().all(|e| e.avaliacao.is_none()));
    assert!(ev.faixas.iter().all(|e| e.avaliacao.is_none()));
    for k in 0..QTD_PRIORIS {
        assert_eq!(ev.observada_posicoes[k].total(), 0);
        assert_eq!(ev.observada_faixas[k].total(), 0);
    }
}

#[test]
fn evidencia_recusa_contagens_de_outra_loteria() {
    let mini = TaxasAcumuladas::novas(&MINI);
    let lotofacil = TaxasAcumuladas::novas(cfg("lotofacil"));
    assert!(matches!(
        evidencia_das_taxas(cfg("lotofacil"), &mini),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
    assert!(matches!(
        evidencia_das_taxas(&MINI, &lotofacil),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

// ------------------------------------------------------ esperado (acaso)

fn plano(g: &Grade<f64>) -> [f64; 10] {
    let mut v = [0.0; 10];
    v[..5].copy_from_slice(&g.vies);
    v[5..].copy_from_slice(&g.acaso);
    v
}

#[test]
fn esperado_e_deterministico_e_depende_da_semente() {
    let cfg = cfg("lotofacil");
    let serie = calcular_taxas(cfg, &serie_sintetica(cfg, 60, 5, true)).unwrap();
    let a = esperado_das_taxas(cfg, &serie.linhas, 20, SEMENTE_ESPERADO).unwrap();
    let b = esperado_das_taxas(cfg, &serie.linhas, 20, SEMENTE_ESPERADO).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        (a.concursos, a.replicas, a.semente),
        (59, 20, SEMENTE_ESPERADO)
    );
    assert_eq!(a.posicoes.len(), QTD_PRIORIS);
    assert_eq!(a.faixas.len(), QTD_PRIORIS);
    for (esp, &s) in a.posicoes.iter().zip(&DESVIOS_PRIORI) {
        assert_eq!(esp.desvio, s);
    }
    // Outra semente: outro resultado (com 59 concursos de 25 dezenas, as
    // contagens de graus mudam entre sementes).
    let outra = esperado_das_taxas(cfg, &serie.linhas, 20, SEMENTE_ESPERADO + 1).unwrap();
    assert_ne!((&a.posicoes, &a.faixas), (&outra.posicoes, &outra.faixas));
    // A ordem das linhas de entrada não importa.
    let mut invertidas = serie.linhas.clone();
    invertidas.reverse();
    assert_eq!(
        esperado_das_taxas(cfg, &invertidas, 20, SEMENTE_ESPERADO).unwrap(),
        a
    );
    // O snapshot não sorteado é ignorado: tirá-lo não muda nada.
    let sem_snapshot: Vec<LinhaTaxas> = serie
        .linhas
        .iter()
        .filter(|l| l.saiu.is_some())
        .cloned()
        .collect();
    assert_eq!(
        esperado_das_taxas(cfg, &sem_snapshot, 20, SEMENTE_ESPERADO).unwrap(),
        a
    );
}

#[test]
fn esperado_soma_o_numero_de_celulas_por_priori() {
    for (slug, concursos) in [("lotofacil", 80_u32), ("megasena", 60), ("quina", 50)] {
        let cfg = cfg(slug);
        let serie = calcular_taxas(cfg, &serie_sintetica(cfg, concursos, 11, true)).unwrap();
        let esp = esperado_das_taxas(cfg, &serie.linhas, 15, 99).unwrap();
        let ev = evidencia_das_taxas(cfg, &serie.acumuladas).unwrap();
        assert_eq!(esp.concursos, concursos as usize - 1);
        assert_eq!(esp.celulas_posicoes, cfg.qtd_dezenas_possiveis() as usize);
        // As mesmas células com n > 0 que a evidência observada conta.
        let com_dados = ev.faixas.iter().filter(|e| e.avaliacao.is_some()).count();
        assert_eq!(esp.celulas_faixas, com_dados);
        for k in 0..QTD_PRIORIS {
            assert_eq!(esp.posicoes[k].media.total(), esp.celulas_posicoes as f64);
            assert_eq!(esp.faixas[k].media.total(), esp.celulas_faixas as f64);
            assert_eq!(
                esp.posicoes[k].media.total(),
                ev.observada_posicoes[k].total() as f64
            );
            assert_eq!(
                esp.faixas[k].media.total(),
                ev.observada_faixas[k].total() as f64
            );
            assert!(plano(&esp.posicoes[k].erro_padrao)
                .iter()
                .all(|&e| e >= 0.0));
        }
    }
}

#[test]
fn esperado_mini_conta_as_celulas_da_evidencia_a_mao() {
    // Só os concursos 1 e 2 são sorteados: 4 posições e 12 (componente, faixa)
    // com algum par, como na evidência da MINI.
    let serie = calcular_taxas(&MINI, &serie_mini()).unwrap();
    let esp = esperado_das_taxas(&MINI, &serie.linhas, 10, 1).unwrap();
    assert_eq!(esp.concursos, 2);
    assert_eq!((esp.celulas_posicoes, esp.celulas_faixas), (4, 12));
    // Com tão poucos dados tudo cai em "muito fraca" (a direção varia).
    for k in 0..QTD_PRIORIS {
        let g = &esp.faixas[k].media;
        assert_eq!(g.vies[0] + g.acaso[0], 12.0);
    }
}

#[test]
fn esperado_das_posicoes_e_igual_ao_da_1d_quando_a_posicao_nao_muda() {
    // Com a posição = dezena em todos os concursos, a sequência de sorteios é
    // a mesma da 1d, então o resultado é igual bit a bit.
    let cfg = cfg("lotofacil");
    let mut obs = serie_sintetica(cfg, 51, 14, true);
    for o in &mut obs {
        o.posicao = o.dezena - cfg.dezena_min + 1;
    }
    let serie = calcular_taxas(cfg, &obs).unwrap();
    let mine = esperado_das_taxas(cfg, &serie.linhas, 30, 7).unwrap();
    let d1 = esperado_sob_o_acaso(cfg, 50, &DESVIOS_PRIORI, 30, 7).unwrap();
    assert_eq!(mine.concursos, d1.concursos);
    assert_eq!(mine.posicoes, d1.por_priori);
}

#[test]
fn esperado_das_posicoes_confere_com_o_exato_da_1d_com_posicoes_embaralhadas() {
    // As posições são uma permutação das dezenas em cada concurso, então a
    // contagem esperada por posição é a mesma binomial da 1d.
    for (slug, concursos) in [("lotofacil", 300_u32), ("quina", 300)] {
        let cfg = cfg(slug);
        let serie = calcular_taxas(cfg, &serie_sintetica(cfg, concursos + 1, 19, true)).unwrap();
        let esp = esperado_das_taxas(cfg, &serie.linhas, 300, 5).unwrap();
        assert_eq!(esp.concursos, concursos as usize);
        let mut celulas_relevantes = 0;
        for (e, &s) in esp.posicoes.iter().zip(&DESVIOS_PRIORI) {
            let exato = esperado_exato(cfg, u64::from(concursos), s).unwrap();
            for (m, (ex, ep)) in plano(&e.media)
                .into_iter()
                .zip(plano(&exato).into_iter().zip(plano(&e.erro_padrao)))
            {
                assert!(
                    (m - ex).abs() <= (5.0 * ep).max(0.05),
                    "{slug}, s = {s}: simulado {m}, exato {ex} (±{ep})"
                );
                if ex > 0.1 {
                    celulas_relevantes += 1;
                }
            }
        }
        assert!(
            celulas_relevantes >= 1,
            "{slug}: o teste não pode passar vazio"
        );
    }
}

#[test]
fn faixa_constante_em_todo_concurso_e_deterministica() {
    // Todos os componentes constantes: toda dezena em "médio" em todo
    // concurso. Cada faixa "médio" tem n = N·C pares e x = K·C saídas **em
    // toda réplica** (a soma por concurso é fixa: sorteio sem reposição, não
    // binomial), então o esperado das faixas é determinístico e o erro padrão
    // é zero. Uma simulação de pares independentes teria variância.
    let cfg = cfg("lotofacil");
    let c = 200_u32;
    let mut obs = serie_sintetica(cfg, c, 3, false);
    for o in &mut obs {
        o.componentes = [Some(0); QTD_COMPONENTES];
    }
    let serie = calcular_taxas(cfg, &obs).unwrap();
    let esp = esperado_das_taxas(cfg, &serie.linhas, 12, 4).unwrap();
    assert_eq!(esp.celulas_faixas, QTD_COMPONENTES);
    let (p, prioris) = prioris_de(cfg);
    let (x, n) = (15 * u64::from(c), 25 * u64::from(c));
    let esperada = direto(x, n, p, &prioris);
    for k in 0..QTD_PRIORIS {
        let mut esperado_plano = [0.0; 10];
        esperado_plano[celula(esperada.classificacao[k])] = QTD_COMPONENTES as f64;
        assert_eq!(plano(&esp.faixas[k].media), esperado_plano, "priori {k}");
        assert_eq!(plano(&esp.faixas[k].erro_padrao), [0.0; 10], "priori {k}");
    }
    // O mesmo vale para a evidência observada: as 9 faixas "médio" caem na
    // mesma célula.
    let ev = evidencia_das_taxas(cfg, &serie.acumuladas).unwrap();
    assert_eq!(ev.observada_faixas[0].total(), QTD_COMPONENTES);
}

fn binomial_f64(n: u32, k: u32) -> f64 {
    (0..k).fold(1.0, |acc, i| acc * f64::from(n - i) / f64::from(i + 1))
}

#[test]
fn faixa_dividida_confere_com_a_distribuicao_exata_hipergeometrica() {
    // Lotofácil; o componente 0 vale 1 nas dezenas 1 a 10 ("baixo") e 2 nas
    // outras 15 ("médio") em todo concurso; os componentes 1 a 8 são
    // constantes ("médio"). Em cada concurso o número de saídas entre as
    // dezenas 1 a 10 é Hipergeométrica(25, 15, 10); sobre C concursos,
    // X_baixo é a convolução dessas C distribuições e X_médio = 15·C − X_baixo.
    // O esperado exato por (grau, direção) sai da distribuição de X_baixo e dos
    // fatores de Bayes diretos, sem nada da simulação.
    let cfg = cfg("lotofacil");
    let c = 400_u32;
    let mut obs = serie_sintetica(cfg, c, 23, false);
    for o in &mut obs {
        o.componentes = [Some(0); QTD_COMPONENTES];
        o.componentes[0] = Some(if o.dezena <= 10 { 1 } else { 2 });
    }
    let serie = calcular_taxas(cfg, &obs).unwrap();
    let esp = esperado_das_taxas(cfg, &serie.linhas, REPLICAS_ESPERADO, SEMENTE_ESPERADO).unwrap();
    assert_eq!(esp.celulas_faixas, 10);

    let pmf: Vec<f64> = (0..=10)
        .map(|h| binomial_f64(10, h) * binomial_f64(15, 15 - h) / binomial_f64(25, 15))
        .collect();
    assert!(perto(pmf.iter().sum::<f64>(), 1.0));
    let mut dist = vec![1.0_f64];
    for _ in 0..c {
        let mut nova = vec![0.0; dist.len() + 10];
        for (i, &pi) in dist.iter().enumerate() {
            for (h, &ph) in pmf.iter().enumerate() {
                nova[i + h] += pi * ph;
            }
        }
        dist = nova;
    }
    assert_eq!(dist.len(), 10 * c as usize + 1);
    assert!(perto(dist.iter().sum::<f64>(), 1.0));

    let (p, prioris) = prioris_de(cfg);
    let (n_baixo, n_medio, n_resto) = (10 * u64::from(c), 15 * u64::from(c), 25 * u64::from(c));
    #[allow(clippy::needless_range_loop)] // `k` indexa `prioris` e `esp.faixas`
    for k in 0..QTD_PRIORIS {
        let mut exato = [0.0_f64; 10];
        let mut massa_baixo = [0.0_f64; 10];
        for (x, &pr) in dist.iter().enumerate() {
            let x = x as u64;
            let cb = celula(
                classificar(log10_fator_bayes(x, n_baixo, p, &prioris[k]).unwrap()).unwrap(),
            );
            let cm = celula(
                classificar(log10_fator_bayes(n_medio - x, n_medio, p, &prioris[k]).unwrap())
                    .unwrap(),
            );
            exato[cb] += pr;
            exato[cm] += pr;
            massa_baixo[cb] += pr;
        }
        // Os 8 componentes constantes: x = K·C, n = N·C, sempre.
        let fixa = celula(
            classificar(log10_fator_bayes(15 * u64::from(c), n_resto, p, &prioris[k]).unwrap())
                .unwrap(),
        );
        exato[fixa] += 8.0;
        // O teste só vale se a distribuição do "baixo" não for trivial.
        assert!(
            massa_baixo.iter().all(|&m| m < 0.95),
            "priori {k}: distribuição trivial {massa_baixo:?}"
        );
        let sim = &esp.faixas[k];
        for (cel, ((&m, &ep), &ex)) in plano(&sim.media)
            .iter()
            .zip(&plano(&sim.erro_padrao))
            .zip(&exato)
            .enumerate()
        {
            assert!(
                (m - ex).abs() <= (5.0 * ep).max(0.02),
                "priori {k}, célula {cel}: simulado {m}, exato {ex} (±{ep})"
            );
        }
        assert!(perto(plano(&sim.media).iter().sum::<f64>(), 10.0));
    }
}

#[test]
fn esperado_recusa_entrada_invalida() {
    let cfg = cfg("lotofacil");
    let serie = calcular_taxas(cfg, &serie_sintetica(cfg, 5, 1, true)).unwrap();
    // Réplicas demais poucas.
    for r in [0, 1] {
        assert!(matches!(
            esperado_das_taxas(cfg, &serie.linhas, r, 1),
            Err(SpiderError::EvidenciaEntradaInvalida(_))
        ));
    }
    // Linha faltando num concurso.
    assert!(matches!(
        esperado_das_taxas(cfg, &serie.linhas[1..], 5, 1),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
    // Sem nenhum concurso sorteado (só o snapshot).
    let so_snapshot: Vec<LinhaTaxas> = serie
        .linhas
        .iter()
        .filter(|l| l.saiu.is_none())
        .cloned()
        .collect();
    assert_eq!(so_snapshot.len(), 25);
    assert!(matches!(
        esperado_das_taxas(cfg, &so_snapshot, 5, 1),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        esperado_das_taxas(cfg, &[], 5, 1),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

// -------------------------------------------------------- determinismo

#[test]
fn saida_inteira_e_deterministica() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let obs = serie_sintetica(cfg, 40, 12, true);
        let rodar = || {
            let serie = calcular_taxas(cfg, &obs).unwrap();
            (
                ranquear(cfg, &serie.linhas).unwrap(),
                evidencia_das_taxas(cfg, &serie.acumuladas).unwrap(),
                esperado_das_taxas(cfg, &serie.linhas, 10, SEMENTE_ESPERADO).unwrap(),
            )
        };
        assert_eq!(rodar(), rodar(), "{slug}");
    }
}
