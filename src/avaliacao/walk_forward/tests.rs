use super::*;
use crate::avaliacao::tests::{
    concurso_mini, metricas_mini_a_mao, obs, serie_aleatoria, serie_mini, Gerador, MINI,
};
use crate::avaliacao::{
    acertos_por_concurso, acertos_por_concurso_entre, avaliar, avaliar_entre, avaliar_topk,
    avaliar_topk_entre, calibrar, metricas_por_concurso, p_valor_soma, Hipergeometrica,
};
use crate::config;

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn faixa(primeiro: u32, ultimo: u32) -> FaixaConcursos {
    FaixaConcursos { primeiro, ultimo }
}

fn pares(dobras: &[FaixaConcursos]) -> Vec<(u32, u32)> {
    dobras.iter().map(|d| (d.primeiro, d.ultimo)).collect()
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

/// A MINI com outro corte da busca.
fn mini_com_corte(corte: u32) -> LoteriaConfig {
    LoteriaConfig {
        ultimo_concurso_busca: corte,
        ..MINI
    }
}

/// Mesmos bits: `{:?}` de `f64` é a representação mais curta que volta ao
/// mesmo valor, então a comparação do texto confere bit a bit.
fn bits<T: std::fmt::Debug + ?Sized>(x: &T) -> String {
    format!("{x:?}")
}

// ------------------------------------------------------------ dobras

#[test]
fn cinco_dobras_iguais_sem_sobra() {
    let dobras = dividir_em_dobras(faixa(2, 11), 5).expect("10 concursos");
    assert_eq!(pares(&dobras), [(2, 3), (4, 5), (6, 7), (8, 9), (10, 11)]);
}

#[test]
fn sobra_da_divisao_vai_para_a_ultima_dobra() {
    // T = 14: ⌊14/5⌋ = 2, sobra 4 → última com 6.
    let dobras = dividir_em_dobras(faixa(2, 15), 5).expect("14 concursos");
    assert_eq!(pares(&dobras), [(2, 3), (4, 5), (6, 7), (8, 9), (10, 15)]);
    // T = 9: ⌊9/5⌋ = 1, sobra 4 → última com 5.
    let dobras = dividir_em_dobras(faixa(1, 9), 5).expect("9 concursos");
    assert_eq!(pares(&dobras), [(1, 1), (2, 2), (3, 3), (4, 4), (5, 9)]);
}

#[test]
fn exatamente_cinco_concursos_da_um_por_dobra() {
    let dobras = dividir_em_dobras(faixa(7, 11), 5).expect("5 concursos");
    assert_eq!(pares(&dobras), [(7, 7), (8, 8), (9, 9), (10, 10), (11, 11)]);
}

#[test]
fn uma_dobra_e_a_faixa_inteira() {
    let dobras = dividir_em_dobras(faixa(2, 2759), 1).expect("1 dobra");
    assert_eq!(pares(&dobras), [(2, 2759)]);
}

#[test]
fn menos_concursos_que_dobras_e_erro() {
    // T = 4, 1 e 0 (faixa vazia, primeiro > ultimo).
    for f in [faixa(2, 5), faixa(9, 9), faixa(10, 9)] {
        let erro = dividir_em_dobras(f, 5).expect_err("menos de 5 concursos");
        assert!(
            matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
            "{f:?}: {erro}"
        );
    }
    let erro = dividir_em_dobras(faixa(1, 100), 0).expect_err("0 dobras");
    assert!(
        matches!(erro, SpiderError::AvaliacaoEntradaInvalida(_)),
        "{erro}"
    );
}

#[test]
fn dobras_reais_das_tres_loterias() {
    // Do segundo concurso do índice (o 2) até o corte de 2026-10-05.
    let esperado: [(&str, [(u32, u32); 5]); 3] = [
        // T = 3415 = 5·683.
        (
            "lotofacil",
            [
                (2, 684),
                (685, 1367),
                (1368, 2050),
                (2051, 2733),
                (2734, 3416),
            ],
        ),
        // T = 2758 = 5·551 + 3: a última fica com 554.
        (
            "megasena",
            [
                (2, 552),
                (553, 1103),
                (1104, 1654),
                (1655, 2205),
                (2206, 2759),
            ],
        ),
        // T = 6420 = 5·1284.
        (
            "quina",
            [
                (2, 1285),
                (1286, 2569),
                (2570, 3853),
                (3854, 5137),
                (5138, 6421),
            ],
        ),
    ];
    for (slug, limites) in esperado {
        let c = cfg(slug);
        let dobras =
            dividir_em_dobras(faixa(2, c.ultimo_concurso_busca), QTD_DOBRAS).expect("dobras");
        assert_eq!(pares(&dobras), limites, "{slug}");
    }
}

#[test]
fn dobras_sao_consecutivas_e_cobrem_a_faixa_em_muitos_tamanhos() {
    for primeiro in [0_u32, 1, 2, 100] {
        for t in 1_u32..=60 {
            for qtd in 1_u32..=7 {
                let f = faixa(primeiro, primeiro + t - 1);
                let Ok(dobras) = dividir_em_dobras(f, qtd) else {
                    assert!(t < qtd, "{f:?} em {qtd}");
                    continue;
                };
                assert!(t >= qtd, "{f:?} em {qtd}");
                assert_eq!(dobras.len() as u32, qtd);
                assert_eq!(dobras[0].primeiro, f.primeiro);
                assert_eq!(dobras.last().map(|d| d.ultimo), Some(f.ultimo));
                for par in dobras.windows(2) {
                    assert_eq!(par[1].primeiro, par[0].ultimo + 1, "{f:?} em {qtd}");
                }
                let base = u64::from(t / qtd);
                for d in &dobras[..dobras.len() - 1] {
                    assert_eq!(d.tamanho(), base, "{f:?} em {qtd}");
                }
                assert_eq!(
                    dobras.last().map(FaixaConcursos::tamanho),
                    Some(base + u64::from(t % qtd))
                );
            }
        }
    }
}

#[test]
fn faixa_enorme_nao_estoura() {
    let f = faixa(0, u32::MAX);
    assert_eq!(f.tamanho(), 1 << 32);
    let dobras = dividir_em_dobras(f, 5).expect("cabe");
    assert_eq!(dobras[0].primeiro, 0);
    assert_eq!(dobras[4].ultimo, u32::MAX);
    assert_eq!(dobras[0].tamanho(), (1 << 32) / 5);
}

#[test]
fn tamanho_e_contem_da_faixa() {
    let f = faixa(3, 7);
    assert_eq!(f.tamanho(), 5);
    assert!(!f.contem(2));
    assert!(f.contem(3) && f.contem(7));
    assert!(!f.contem(8));
    assert_eq!(faixa(8, 7).tamanho(), 0);
    assert!(!faixa(8, 7).contem(8));
}

// ------------------------------------------- trecho da busca e corte do config

#[test]
fn faixa_da_busca_vai_do_segundo_concurso_ao_corte_do_config() {
    assert_eq!(MINI.ultimo_concurso_busca, 20);
    // Histórico além do corte, com snapshot: termina no corte.
    let longa = serie_aleatoria(&MINI, 31, 11, true);
    assert_eq!(faixa_da_busca(&MINI, &longa).expect("faixa"), faixa(2, 20));
    // Histórico que ainda não chegou ao corte: termina no último sorteado.
    let curta = serie_aleatoria(&MINI, 13, 11, true);
    assert_eq!(faixa_da_busca(&MINI, &curta).expect("faixa"), faixa(2, 12));
    // O corte é o snapshot ainda não sorteado: termina antes dele.
    let no_corte = serie_aleatoria(&MINI, 20, 11, true);
    assert_eq!(
        faixa_da_busca(&MINI, &no_corte).expect("faixa"),
        faixa(2, 19)
    );
    // Índice começando no 5: o 5 recebe o acaso e fica de fora.
    let do_5: Vec<Observacao> = longa.iter().filter(|o| o.concurso >= 5).copied().collect();
    assert_eq!(faixa_da_busca(&MINI, &do_5).expect("faixa"), faixa(6, 20));
    // Outro corte no config, outra faixa.
    assert_eq!(
        faixa_da_busca(&mini_com_corte(12), &longa).expect("faixa"),
        faixa(2, 12)
    );
}

#[test]
fn walk_forward_padrao_termina_no_corte_do_config() {
    let serie = serie_aleatoria(&MINI, 31, 5, true);
    let wf = walk_forward(&MINI, &serie, 2).expect("walk-forward");
    assert_eq!(wf.corte, MINI.ultimo_concurso_busca);
    // 2..=20: T = 19 → 3, 3, 3, 3 e a última com 7.
    assert_eq!(
        wf.dobras.iter().map(|d| d.faixa).collect::<Vec<_>>(),
        [
            faixa(2, 4),
            faixa(5, 7),
            faixa(8, 10),
            faixa(11, 13),
            faixa(14, 20)
        ]
    );
    for d in &wf.dobras {
        assert_eq!(d.metricas.primeiro_concurso, d.faixa.primeiro);
        assert_eq!(d.metricas.ultimo_concurso, d.faixa.ultimo);
        assert_eq!(d.metricas.concursos_avaliados as u64, d.faixa.tamanho());
        assert_eq!(d.topk.primeiro_concurso, d.faixa.primeiro);
        assert_eq!(d.topk.ultimo_concurso, d.faixa.ultimo);
        assert_eq!(d.topk.concursos_avaliados as u64, d.faixa.tamanho());
        assert_eq!(d.topk.k, 2);
    }
    // Mudar só o corte do config muda onde o walk-forward termina.
    let wf12 = walk_forward(&mini_com_corte(12), &serie, 2).expect("walk-forward");
    assert_eq!(wf12.corte, 12);
    assert_eq!(wf12.dobras[4].faixa.ultimo, 12);
}

#[test]
fn nada_acima_do_corte_e_lido_no_walk_forward() {
    let ate_o_corte = serie_aleatoria(&MINI, 20, 21, false);
    let base = walk_forward(&MINI, &ate_o_corte, 2).expect("walk-forward");

    // Mais concursos válidos (o trancado) e o snapshot: nada muda.
    let mut longa = ate_o_corte.clone();
    longa.extend(
        serie_aleatoria(&MINI, 31, 99, true)
            .into_iter()
            .filter(|o| o.concurso > 20),
    );
    assert_eq!(
        bits(&walk_forward(&MINI, &longa, 2).expect("walk-forward")),
        bits(&base)
    );
    assert_eq!(
        dobras_da_busca(&MINI, &longa).expect("dobras"),
        dobras_da_busca(&MINI, &ate_o_corte).expect("dobras")
    );

    // Lixo acima do corte (concurso incompleto, dezena fora da faixa,
    // buraco, `saiu` NULL no meio): seria recusado se fosse lido.
    let mut lixo = ate_o_corte.clone();
    lixo.push(obs(21, 99, 7, Some(true)));
    lixo.extend(concurso_mini(23, [1, 1, 1, 1], None));
    lixo.extend(concurso_mini(24, [1, 2, 3, 4], Some([1, 2])));
    assert!(walk_forward(&MINI, &lixo, 2).is_ok());
    assert_eq!(
        bits(&walk_forward(&MINI, &lixo, 2).expect("walk-forward")),
        bits(&base)
    );
    // O mesmo lixo abaixo do corte é recusado: a validação está ligada.
    let mut lixo_abaixo: Vec<Observacao> = ate_o_corte
        .iter()
        .filter(|o| o.concurso != 20)
        .copied()
        .collect();
    lixo_abaixo.push(obs(20, 99, 7, Some(true)));
    assert!(matches!(
        walk_forward(&MINI, &lixo_abaixo, 2),
        Err(SpiderError::AvaliacaoEntradaInvalida(_))
    ));
}

#[test]
fn corte_padrao_vem_do_config_nas_tres_loterias() {
    for (slug, corte) in [("lotofacil", 3416), ("megasena", 2759), ("quina", 6421)] {
        let c = cfg(slug);
        assert_eq!(c.ultimo_concurso_busca, corte, "{slug}");
        // Histórico sintético que passa do corte (3 trancados + snapshot).
        let serie = serie_aleatoria(c, corte + 4, 3, true);
        let dobras = dobras_da_busca(c, &serie).expect("dobras");
        assert_eq!(dobras[0].primeiro, 2, "{slug}");
        assert_eq!(dobras[4].ultimo, corte, "{slug}");
        assert_eq!(
            dobras,
            dividir_em_dobras(faixa(2, corte), QTD_DOBRAS).expect("dobras"),
            "{slug}"
        );
        // Cortar a entrada no corte não muda as dobras.
        let ate_o_corte: Vec<Observacao> = serie
            .iter()
            .filter(|o| o.concurso <= corte)
            .copied()
            .collect();
        assert_eq!(
            dobras_da_busca(c, &ate_o_corte).expect("dobras"),
            dobras,
            "{slug}"
        );
    }
}

#[test]
fn walk_forward_real_termina_no_corte_e_ignora_o_trancado() {
    // Mega-Sena, a loteria real mais barata: walk-forward completo com o
    // corte do config (2759), com e sem os concursos acima dele.
    let c = cfg("megasena");
    let corte = c.ultimo_concurso_busca;
    let serie = serie_aleatoria(c, corte + 4, 8, true);
    let k = c.qtd_dezenas as u32;
    let wf = walk_forward(c, &serie, k).expect("walk-forward");
    assert_eq!(wf.corte, corte);
    assert_eq!(wf.dobras[4].faixa.ultimo, corte);
    assert_eq!(wf.dobras[4].metricas.ultimo_concurso, corte);
    let ate_o_corte: Vec<Observacao> = serie
        .iter()
        .filter(|o| o.concurso <= corte)
        .copied()
        .collect();
    assert_eq!(
        bits(&walk_forward(c, &ate_o_corte, k).expect("walk-forward")),
        bits(&wf)
    );
}

// ---------------------------------------- calibração separada da avaliação

#[test]
fn sem_limite_inferior_e_a_avaliacao_da_1c1_e_da_1c2() {
    let serie = serie_mini();
    let completo = avaliar(&MINI, &serie, u32::MAX).expect("avaliar");
    let topk = avaliar_topk(&MINI, &serie, 2, u32::MAX).expect("top-k");
    // O primeiro da entrada é o 1: limite 0, 1 ou 2 dá o mesmo trecho.
    for inicio in [0, 1, 2] {
        assert_eq!(
            avaliar_entre(&MINI, &serie, inicio, u32::MAX).expect("avaliar"),
            completo,
            "início {inicio}"
        );
        assert_eq!(
            avaliar_topk_entre(&MINI, &serie, 2, inicio, u32::MAX).expect("top-k"),
            topk,
            "início {inicio}"
        );
    }
    // inicio > fim e trecho sem sorteado: nada para avaliar.
    for (inicio, fim) in [(3, 2), (4, u32::MAX), (1, 1)] {
        assert!(matches!(
            avaliar_entre(&MINI, &serie, inicio, fim),
            Err(SpiderError::AvaliacaoSemConcursos(_))
        ));
        assert!(matches!(
            avaliar_topk_entre(&MINI, &serie, 2, inicio, fim),
            Err(SpiderError::AvaliacaoSemConcursos(_))
        ));
    }
}

#[test]
fn limite_inferior_nao_recomeca_a_calibracao_conta_a_mao() {
    let [(ll2, br2), (ll3, br3)] = metricas_mini_a_mao();
    // Só o concurso 3, calibrado com 1 e 2: p = (208, 204, 204, 200)/408.
    let so_o_3 = avaliar_entre(&MINI, &serie_mini(), 3, 3).expect("avaliar");
    assert_eq!(so_o_3.concursos_avaliados, 1);
    assert_eq!((so_o_3.primeiro_concurso, so_o_3.ultimo_concurso), (3, 3));
    assert!(perto(so_o_3.log_loss.perfil, ll3), "{so_o_3:?}");
    assert!(perto(so_o_3.brier.perfil, br3), "{so_o_3:?}");

    // Recomeçar a calibração no 2 (entrada a partir do 2) daria outro valor:
    // antes do 3 só o 2, saídas por posição (1, 0, 1, 0) →
    // p = (204, 200, 204, 200)/404 nas dezenas 1..4; saíram 1 e 4, e as
    // médias coincidem com as do concurso 2 da série completa.
    let do_2: Vec<Observacao> = serie_mini()
        .into_iter()
        .filter(|o| o.concurso >= 2)
        .collect();
    let recomecando = avaliar(&MINI, &do_2, 3).expect("avaliar");
    assert!(perto(recomecando.log_loss.perfil, ll2));
    assert!(perto(recomecando.brier.perfil, br2));
    assert!(!perto(ll2, ll3) && !perto(br2, br3));
}

/// As métricas de cada dobra são o trecho da série por concurso calculada
/// de uma vez até o corte: a calibração da dobra começa no primeiro
/// concurso, não no começo da dobra.
fn conferir_dobras_contra_a_serie_inteira(c: &LoteriaConfig, serie: &[Observacao], k: u32) {
    let corte = c.ultimo_concurso_busca;
    let metricas = metricas_por_concurso(c, serie, corte).expect("métricas");
    let acertos = acertos_por_concurso(c, serie, k, corte).expect("acertos");
    let wf = walk_forward(c, serie, k).expect("walk-forward");
    for d in &wf.dobras {
        let m: Vec<_> = metricas
            .iter()
            .filter(|m| d.faixa.contem(m.concurso))
            .cloned()
            .collect();
        let a: Vec<_> = acertos
            .iter()
            .filter(|a| d.faixa.contem(a.concurso))
            .copied()
            .collect();
        assert_eq!(
            m.len() as u64,
            d.faixa.tamanho(),
            "{} {:?}",
            c.slug,
            d.faixa
        );
        assert_eq!(
            bits(&resumir(&m).expect("resumo")),
            bits(&d.metricas),
            "{} {:?}",
            c.slug,
            d.faixa
        );
        assert_eq!(
            bits(&resumir_topk(c, k, &a).expect("resumo")),
            bits(&d.topk),
            "{} {:?}",
            c.slug,
            d.faixa
        );
    }
}

#[test]
fn metricas_de_cada_dobra_sao_trecho_da_serie_inteira() {
    conferir_dobras_contra_a_serie_inteira(&MINI, &serie_aleatoria(&MINI, 31, 13, true), 1);
    conferir_dobras_contra_a_serie_inteira(&MINI, &serie_aleatoria(&MINI, 31, 14, true), 2);
    for slug in ["lotofacil", "megasena", "quina"] {
        // Corte menor que o real, para o teste ser rápido em debug.
        let c = LoteriaConfig {
            ultimo_concurso_busca: 60,
            ..*cfg(slug)
        };
        let serie = serie_aleatoria(&c, 70, 15, true);
        conferir_dobras_contra_a_serie_inteira(&c, &serie, c.qtd_dezenas as u32);
    }
}

#[test]
fn ranking_previsivel_e_aprendido_desde_o_primeiro_concurso() {
    // As posições 1 e 2 saem sempre: com a calibração acumulada desde o 1,
    // a última dobra já tem probabilidade bem acima do acaso nessas posições;
    // recomeçando no início da dobra, o primeiro concurso dela voltaria ao
    // acaso (log loss do acaso, ln 2).
    let mut g = Gerador(77);
    let mut serie = Vec::new();
    for c in 1..=20 {
        let mut posicoes = [1_u32, 2, 3, 4];
        g.embaralhar(&mut posicoes);
        let saidas: Vec<u32> = (1..=4).filter(|&d| posicoes[d as usize - 1] <= 2).collect();
        serie.extend(concurso_mini(c, posicoes, Some([saidas[0], saidas[1]])));
    }
    let wf = walk_forward(&MINI, &serie, 2).expect("walk-forward");
    let ultima = &wf.dobras[4];
    assert_eq!(ultima.faixa, faixa(14, 20));
    // Antes do 14: 13 concursos, posição 1 saiu 13 vezes →
    // p = (13·4 + 200)/((13 + 100)·4) = 252/452 nas sorteadas, 200/452 nas
    // outras; no 14 o par sorteado tem −ln(252/452), o outro −ln(1−200/452).
    let p14 = metricas_por_concurso_entre(&MINI, &serie, 14, 14).expect("métricas");
    let esperado = -((252.0_f64 / 452.0).ln() + (252.0_f64 / 452.0).ln()) / 2.0;
    assert!(perto(p14[0].log_loss_perfil, esperado), "{p14:?}");
    assert!(ultima.metricas.log_loss.diferenca < 0.0);
    assert_eq!(ultima.topk.total_acertos, 2 * 7);
    assert!(ultima.topk.p_valor.probabilidade < 1e-3);
}

// ------------------------------------------------------------ ponto no tempo

#[test]
fn mudar_concursos_depois_da_dobra_nao_muda_a_dobra() {
    let base_serie = serie_aleatoria(&MINI, 20, 31, false);
    let outra = serie_aleatoria(&MINI, 20, 32, false);
    let base = walk_forward(&MINI, &base_serie, 2).expect("walk-forward");
    for (i, dobra) in base.dobras.iter().enumerate().take(4) {
        let limite = dobra.faixa.ultimo;
        // Até o fim da dobra i, a série base; depois, outros sorteios e
        // outras posições.
        let mista: Vec<Observacao> = base_serie
            .iter()
            .filter(|o| o.concurso <= limite)
            .chain(outra.iter().filter(|o| o.concurso > limite))
            .copied()
            .collect();
        let alterado = walk_forward(&MINI, &mista, 2).expect("walk-forward");
        assert_eq!(
            bits(&alterado.dobras[..=i]),
            bits(&base.dobras[..=i]),
            "dobra {i}"
        );
        assert_ne!(
            bits(&alterado.dobras[i + 1..]),
            bits(&base.dobras[i + 1..]),
            "a mudança depois da dobra {i} tem efeito"
        );
    }
}

#[test]
fn mudar_so_o_saiu_do_concurso_seguinte_nao_muda_a_dobra() {
    let serie = serie_aleatoria(&MINI, 20, 41, false);
    let base = walk_forward(&MINI, &serie, 2).expect("walk-forward");
    let limite = base.dobras[1].faixa.ultimo;
    // Mesmo concurso limite + 1, mesmas posições, resultado trocado pelo
    // complemento (na MINI, 2 de 4, o complemento também é um sorteio
    // válido e sempre diferente).
    let alterada: Vec<Observacao> = serie
        .iter()
        .map(|o| {
            if o.concurso == limite + 1 {
                Observacao {
                    saiu: o.saiu.map(|s| !s),
                    ..*o
                }
            } else {
                *o
            }
        })
        .collect();
    let alterado = walk_forward(&MINI, &alterada, 2).expect("walk-forward");
    assert_eq!(bits(&alterado.dobras[..2]), bits(&base.dobras[..2]));
    assert_ne!(bits(&alterado.dobras[2..]), bits(&base.dobras[2..]));
}

// ------------------------------------------------------------------ trancado

#[test]
fn trancado_vai_do_corte_mais_um_ao_ultimo_sorteado_com_o_passado() {
    let serie = serie_aleatoria(&MINI, 31, 51, true);
    let trancado = avaliar_trancado(&MINI, &serie, 2).expect("trancado");
    assert_eq!(trancado.faixa, faixa(21, 30));
    assert_eq!(trancado.metricas.concursos_avaliados, 10);
    assert_eq!(trancado.topk.concursos_avaliados, 10);

    // Igual ao trecho 21..=30 da série calibrada desde o concurso 1.
    let metricas: Vec<_> = metricas_por_concurso(&MINI, &serie, u32::MAX)
        .expect("métricas")
        .into_iter()
        .filter(|m| m.concurso > 20)
        .collect();
    let acertos: Vec<_> = acertos_por_concurso(&MINI, &serie, 2, u32::MAX)
        .expect("acertos")
        .into_iter()
        .filter(|a| a.concurso > 20)
        .collect();
    assert_eq!(
        bits(&resumir(&metricas).expect("resumo")),
        bits(&trancado.metricas)
    );
    assert_eq!(
        bits(&resumir_topk(&MINI, 2, &acertos).expect("resumo")),
        bits(&trancado.topk)
    );

    // Calibrar só com o trancado (entrada a partir do 21) daria outra coisa.
    let so_trancado: Vec<Observacao> = serie.iter().filter(|o| o.concurso > 20).copied().collect();
    let sem_passado = avaliar(&MINI, &so_trancado, u32::MAX).expect("avaliar");
    assert_eq!(sem_passado.primeiro_concurso, 22);
    assert_ne!(
        sem_passado.log_loss.perfil,
        trancado.metricas.log_loss.perfil
    );

    // O walk-forward da mesma série não vê o trancado, e o trancado não
    // muda o walk-forward.
    let wf = walk_forward(&MINI, &serie, 2).expect("walk-forward");
    assert!(wf.dobras.iter().all(|d| d.faixa.ultimo <= 20));
}

#[test]
fn trancado_vazio_e_erro() {
    // Sem nada acima do corte; só o snapshot acima do corte.
    for serie in [
        serie_aleatoria(&MINI, 20, 61, false),
        serie_aleatoria(&MINI, 21, 61, true),
        serie_aleatoria(&MINI, 12, 61, false),
    ] {
        let erro = avaliar_trancado(&MINI, &serie, 2).expect_err("trancado vazio");
        assert!(
            matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
            "{erro}"
        );
    }
    // Corte no maior concurso possível: não há trancado.
    let sem_trancado = mini_com_corte(u32::MAX);
    assert!(matches!(
        avaliar_trancado(&sem_trancado, &serie_aleatoria(&MINI, 30, 61, false), 2),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

#[test]
fn trancado_com_um_concurso_fica_sem_erro_padrao() {
    let serie = serie_aleatoria(&MINI, 21, 71, false);
    let trancado = avaliar_trancado(&MINI, &serie, 2).expect("trancado");
    assert_eq!(trancado.faixa, faixa(21, 21));
    assert_eq!(trancado.metricas.log_loss.erro_padrao, None);
    assert_eq!(trancado.topk.acertos.ic95, None);
}

#[test]
fn trancado_e_walk_forward_recusam_k_invalido() {
    let serie = serie_aleatoria(&MINI, 31, 81, true);
    for k in [0, 4] {
        assert!(matches!(
            avaliar_trancado(&MINI, &serie, k),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
        assert!(matches!(
            walk_forward(&MINI, &serie, k),
            Err(SpiderError::AvaliacaoEntradaInvalida(_))
        ));
    }
}

// ------------------------------------------------------------------- erros

#[test]
fn busca_com_menos_de_cinco_concursos_avaliados_e_erro() {
    // 1..=5: avaliados 2..=5, só 4.
    let erro =
        walk_forward(&MINI, &serie_aleatoria(&MINI, 5, 91, false), 2).expect_err("4 concursos");
    assert!(
        matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
        "{erro}"
    );
    // 1..=6 com o 6 sem resultado: ainda 4.
    let erro =
        walk_forward(&MINI, &serie_aleatoria(&MINI, 6, 91, true), 2).expect_err("4 concursos");
    assert!(
        matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
        "{erro}"
    );
    // Entrada vazia ou só o primeiro concurso.
    for serie in [Vec::new(), serie_aleatoria(&MINI, 1, 91, false)] {
        assert!(matches!(
            walk_forward(&MINI, &serie, 2),
            Err(SpiderError::AvaliacaoSemConcursos(_))
        ));
    }
    // 1..=6 sorteados: 5 concursos, um por dobra.
    let wf = walk_forward(&MINI, &serie_aleatoria(&MINI, 6, 91, false), 2).expect("5 concursos");
    assert_eq!(
        wf.dobras.iter().map(|d| d.faixa).collect::<Vec<_>>(),
        (2..=6).map(|c| faixa(c, c)).collect::<Vec<_>>()
    );
    assert!(wf
        .dobras
        .iter()
        .all(|d| d.metricas.log_loss.erro_padrao.is_none()));
}

// -------------------------------------------------------------- determinismo

#[test]
fn walk_forward_e_trancado_sao_deterministicos_e_nao_dependem_da_ordem() {
    let serie = serie_aleatoria(&MINI, 31, 101, true);
    let wf = walk_forward(&MINI, &serie, 2).expect("walk-forward");
    let tr = avaliar_trancado(&MINI, &serie, 2).expect("trancado");
    let mut embaralhada = serie.clone();
    Gerador(5).embaralhar(&mut embaralhada);
    for entrada in [&serie, &embaralhada] {
        assert_eq!(
            bits(&walk_forward(&MINI, entrada, 2).expect("walk-forward")),
            bits(&wf)
        );
        assert_eq!(
            bits(&avaliar_trancado(&MINI, entrada, 2).expect("trancado")),
            bits(&tr)
        );
    }
}

// ------------------------------------------- bordas do limite inferior (entre)

fn eh_erro_entrada_invalida<T: std::fmt::Debug>(r: Result<T>) -> bool {
    matches!(r, Err(SpiderError::AvaliacaoEntradaInvalida(_)))
}

fn eh_erro_sem_concursos<T: std::fmt::Debug>(r: Result<T>) -> bool {
    matches!(r, Err(SpiderError::AvaliacaoSemConcursos(_)))
}

/// A mesma série trocando `saiu` pelo complemento no concurso `alvo` (na
/// MINI, 2 de 4, o complemento também é um sorteio válido).
fn complementando_o_saiu(serie: &[Observacao], alvo: u32) -> Vec<Observacao> {
    serie
        .iter()
        .map(|o| {
            if o.concurso == alvo {
                Observacao {
                    saiu: o.saiu.map(|s| !s),
                    ..*o
                }
            } else {
                *o
            }
        })
        .collect()
}

#[test]
fn metricas_e_acertos_entre_sao_o_filtro_da_serie_sem_limite_inferior() {
    // 1..=11 sorteados e o 12 como snapshot; a segunda entrada começa no 5.
    let desde_1 = serie_aleatoria(&MINI, 12, 201, true);
    let desde_5: Vec<Observacao> = desde_1
        .iter()
        .filter(|o| o.concurso >= 5)
        .copied()
        .collect();
    let limites = [0, 1, 2, 4, 5, 6, 7, 9, 11, 12, 13, u32::MAX];
    for (primeiro_da_entrada, serie) in [(1_u32, &desde_1), (5, &desde_5)] {
        for fim in limites {
            let metricas_base = metricas_por_concurso(&MINI, serie, fim).expect("métricas");
            let acertos_base = acertos_por_concurso(&MINI, serie, 2, fim).expect("acertos");
            for inicio in limites {
                let contexto = format!("entrada desde o {primeiro_da_entrada}, {inicio}..={fim}");
                let metricas =
                    metricas_por_concurso_entre(&MINI, serie, inicio, fim).expect("métricas");
                let acertos =
                    acertos_por_concurso_entre(&MINI, serie, 2, inicio, fim).expect("acertos");

                let esperado_metricas: Vec<_> = metricas_base
                    .iter()
                    .filter(|m| m.concurso >= inicio)
                    .cloned()
                    .collect();
                let esperado_acertos: Vec<_> = acertos_base
                    .iter()
                    .filter(|a| a.concurso >= inicio)
                    .copied()
                    .collect();
                assert_eq!(bits(&metricas), bits(&esperado_metricas), "{contexto}");
                assert_eq!(acertos, esperado_acertos, "{contexto}");

                // Conferência independente do trecho: o primeiro da entrada
                // nunca é avaliado, o snapshot (12) também não, e `inicio`
                // acima do último sorteado (11) ou `inicio > fim` esvazia.
                let concursos: Vec<u32> =
                    (inicio.max(primeiro_da_entrada + 1)..=fim.min(11)).collect();
                assert_eq!(
                    metricas.iter().map(|m| m.concurso).collect::<Vec<_>>(),
                    concursos,
                    "{contexto}"
                );
                assert_eq!(
                    acertos.iter().map(|a| a.concurso).collect::<Vec<_>>(),
                    concursos,
                    "{contexto}"
                );
            }
        }
    }
}

#[test]
fn avaliar_entre_com_fim_abaixo_do_inicio_e_sem_concursos() {
    let serie = serie_aleatoria(&MINI, 12, 211, false);
    for (inicio, fim) in [(8, 5), (8, 7), (2, 1), (u32::MAX, 0)] {
        assert!(
            eh_erro_sem_concursos(avaliar_entre(&MINI, &serie, inicio, fim)),
            "{inicio}..={fim}"
        );
        assert!(
            eh_erro_sem_concursos(avaliar_topk_entre(&MINI, &serie, 2, inicio, fim)),
            "{inicio}..={fim}"
        );
    }
    // inicio = fim num concurso avaliável é uma avaliação de um só concurso.
    let um = avaliar_entre(&MINI, &serie, 8, 8).expect("um concurso");
    assert_eq!(um.concursos_avaliados, 1);
    assert_eq!((um.primeiro_concurso, um.ultimo_concurso), (8, 8));
    // inicio = fim = primeiro da entrada: ele nunca é avaliado.
    assert!(eh_erro_sem_concursos(avaliar_entre(&MINI, &serie, 1, 1)));
}

#[test]
fn avaliar_topk_entre_recusa_k_invalido_antes_de_olhar_o_trecho() {
    let serie = serie_aleatoria(&MINI, 12, 211, false);
    for k in [0, 4] {
        // Mesmo com inicio > fim (trecho vazio), k fora de 1..=N−1 é entrada inválida.
        assert!(eh_erro_entrada_invalida(avaliar_topk_entre(
            &MINI, &serie, k, 8, 5
        )));
        assert!(eh_erro_entrada_invalida(acertos_por_concurso_entre(
            &MINI, &serie, k, 2, 9
        )));
    }
}

#[test]
fn entrada_invalida_abaixo_do_fim_e_recusada_e_lixo_acima_do_fim_e_ignorado() {
    let limpa = serie_aleatoria(&MINI, 12, 221, false);
    let base = avaliar_entre(&MINI, &limpa, 8, 10).expect("avaliar");
    let base_topk = avaliar_topk_entre(&MINI, &limpa, 2, 8, 10).expect("top-k");

    // Lixo acima do fim (11 inválido, 13 depois de um buraco): ignorado.
    let mut lixo_acima: Vec<Observacao> =
        limpa.iter().filter(|o| o.concurso <= 10).copied().collect();
    lixo_acima.push(obs(11, 99, 7, Some(true)));
    lixo_acima.extend(concurso_mini(13, [1, 1, 1, 1], None));
    assert_eq!(
        bits(&avaliar_entre(&MINI, &lixo_acima, 8, 10).expect("avaliar")),
        bits(&base)
    );
    assert_eq!(
        bits(&avaliar_topk_entre(&MINI, &lixo_acima, 2, 8, 10).expect("top-k")),
        bits(&base_topk)
    );
    // O mesmo lixo com o fim acima dele é recusado.
    assert!(eh_erro_entrada_invalida(avaliar_entre(
        &MINI,
        &lixo_acima,
        8,
        11
    )));

    // Lixo abaixo do fim, inclusive abaixo do inicio (a validação vai desde
    // o primeiro concurso): dezena fora da faixa no 3, buraco no 6.
    let mut dezena_fora: Vec<Observacao> = limpa
        .iter()
        .filter(|o| !(o.concurso == 3 && o.dezena == 4))
        .copied()
        .collect();
    dezena_fora.push(obs(3, 99, 4, Some(false)));
    let sem_o_6: Vec<Observacao> = limpa.iter().filter(|o| o.concurso != 6).copied().collect();
    for (nome, serie) in [("dezena fora da faixa", &dezena_fora), ("buraco", &sem_o_6)] {
        assert!(
            eh_erro_entrada_invalida(avaliar_entre(&MINI, serie, 8, 10)),
            "{nome}"
        );
        assert!(
            eh_erro_entrada_invalida(avaliar_topk_entre(&MINI, serie, 2, 8, 10)),
            "{nome}"
        );
        assert!(
            eh_erro_entrada_invalida(metricas_por_concurso_entre(&MINI, serie, 8, 10)),
            "{nome}"
        );
    }
    // O buraco no 6 não importa se o fim é anterior a ele.
    assert!(avaliar_entre(&MINI, &sem_o_6, 3, 5).is_ok());
}

// ---------------------------------------------- trancado: bordas e passado

#[test]
fn trancado_com_entrada_acima_do_corte_deixa_o_primeiro_da_entrada_de_fora() {
    // Entrada 25..=40 (o 40 é o snapshot), toda acima do corte (20): o 25
    // recebe o acaso e fica de fora, a faixa começa no 26.
    let completa = serie_aleatoria(&MINI, 40, 231, true);
    let do_25: Vec<Observacao> = completa
        .iter()
        .filter(|o| o.concurso >= 25)
        .copied()
        .collect();
    let trancado = avaliar_trancado(&MINI, &do_25, 2).expect("trancado");
    assert_eq!(trancado.faixa, faixa(26, 39));
    assert_eq!(trancado.metricas.concursos_avaliados, 14);
    assert_eq!(trancado.topk.concursos_avaliados, 14);
    assert_eq!(
        (
            trancado.metricas.primeiro_concurso,
            trancado.metricas.ultimo_concurso
        ),
        (26, 39)
    );

    // O 25 recebe o acaso (1/2 = K/N) e a calibração do 26 em diante só
    // tem a entrada: igual à série calibrada sem limite inferior.
    let probabilidades = calibrar(&MINI, &do_25).expect("calibrar");
    assert!(probabilidades
        .iter()
        .filter(|p| p.concurso == 25)
        .all(|p| p.probabilidade == 0.5));
    let metricas = metricas_por_concurso(&MINI, &do_25, u32::MAX).expect("métricas");
    assert_eq!(metricas[0].concurso, 26);
    assert_eq!(
        bits(&resumir(&metricas).expect("resumo")),
        bits(&trancado.metricas)
    );
    let acertos = acertos_por_concurso(&MINI, &do_25, 2, u32::MAX).expect("acertos");
    assert_eq!(
        bits(&resumir_topk(&MINI, 2, &acertos).expect("resumo")),
        bits(&trancado.topk)
    );

    // Sem nada abaixo do corte, o walk-forward não tem o que avaliar.
    assert!(eh_erro_sem_concursos(walk_forward(&MINI, &do_25, 2)));
    assert!(eh_erro_sem_concursos(faixa_da_busca(&MINI, &do_25)));
}

#[test]
fn trancado_com_entrada_que_comeca_logo_apos_o_corte() {
    // Entrada 21..=30: o 21 (primeiro do trancado) é o primeiro da entrada e
    // fica de fora; avalia 22..=30.
    let completa = serie_aleatoria(&MINI, 30, 241, false);
    let do_21: Vec<Observacao> = completa
        .iter()
        .filter(|o| o.concurso >= 21)
        .copied()
        .collect();
    let trancado = avaliar_trancado(&MINI, &do_21, 2).expect("trancado");
    assert_eq!(trancado.faixa, faixa(22, 30));
    // Entrada só com o 21: nada para avaliar.
    let so_o_21: Vec<Observacao> = completa
        .iter()
        .filter(|o| o.concurso == 21)
        .copied()
        .collect();
    assert!(eh_erro_sem_concursos(avaliar_trancado(&MINI, &so_o_21, 2)));
}

#[test]
fn alterar_o_passado_da_busca_muda_o_trancado() {
    let serie = serie_aleatoria(&MINI, 31, 301, true);
    let base = avaliar_trancado(&MINI, &serie, 2).expect("trancado");
    assert_eq!(base.faixa, faixa(21, 30));

    // Outro sorteio no concurso 5 e no 20 (o corte): a calibração usa o
    // passado, então as métricas do trancado mudam; a faixa não.
    for alvo in [5, 20] {
        let alterada = complementando_o_saiu(&serie, alvo);
        let alterado = avaliar_trancado(&MINI, &alterada, 2).expect("trancado");
        assert_eq!(alterado.faixa, base.faixa, "alvo {alvo}");
        assert_ne!(
            bits(&alterado.metricas),
            bits(&base.metricas),
            "alterar o concurso {alvo} deve mudar o trancado"
        );
    }

    // Lixo no passado (dezena fora da faixa no concurso 10) é recusado, mesmo
    // com o trecho avaliado intacto; lixo dentro do trancado também.
    for alvo in [10, 25] {
        let mut lixo: Vec<Observacao> = serie
            .iter()
            .filter(|o| !(o.concurso == alvo && o.dezena == 4))
            .copied()
            .collect();
        lixo.push(obs(alvo, 99, 4, Some(false)));
        assert!(
            eh_erro_entrada_invalida(avaliar_trancado(&MINI, &lixo, 2)),
            "lixo no concurso {alvo}"
        );
    }
}

#[test]
fn alterar_o_saiu_de_um_concurso_do_trancado_nao_muda_os_anteriores_a_ele() {
    let serie = serie_aleatoria(&MINI, 31, 311, true);
    let alterada = complementando_o_saiu(&serie, 25);

    let base = metricas_por_concurso_entre(&MINI, &serie, 21, u32::MAX).expect("métricas");
    let alterado = metricas_por_concurso_entre(&MINI, &alterada, 21, u32::MAX).expect("métricas");
    assert_eq!(base.len(), 10);
    assert_eq!(alterado.len(), 10);
    // 21..=24 idênticos, bit a bit.
    assert_eq!(bits(&alterado[..4]), bits(&base[..4]));
    assert_eq!(base[3].concurso, 24);
    // O 25 tem a mesma probabilidade (só o passado), mas outro resultado:
    // o log loss do perfil muda; do 26 em diante, a calibração também.
    assert_eq!(base[4].concurso, 25);
    assert_ne!(base[4].log_loss_perfil, alterado[4].log_loss_perfil);
    assert_ne!(bits(&alterado[5..]), bits(&base[5..]));

    // Acertos dos anteriores também não mudam, e a avaliação 21..=24 é a mesma.
    let acertos_base = acertos_por_concurso_entre(&MINI, &serie, 2, 21, u32::MAX).expect("acertos");
    let acertos_alt =
        acertos_por_concurso_entre(&MINI, &alterada, 2, 21, u32::MAX).expect("acertos");
    assert_eq!(acertos_alt[..4], acertos_base[..4]);
    assert_eq!(
        bits(&avaliar_entre(&MINI, &alterada, 21, 24).expect("avaliar")),
        bits(&avaliar_entre(&MINI, &serie, 21, 24).expect("avaliar"))
    );
    assert_ne!(
        bits(&avaliar_trancado(&MINI, &alterada, 2).expect("trancado")),
        bits(&avaliar_trancado(&MINI, &serie, 2).expect("trancado"))
    );
}

// ------------------------------------------- walk-forward real, série curta

#[test]
fn megasena_real_com_serie_que_termina_antes_do_corte() {
    // 1..=1000 sorteados + snapshot (1001), com o corte do config em 2759:
    // a busca termina no último sorteado (1000), não no corte.
    let c = cfg("megasena");
    assert_eq!(c.ultimo_concurso_busca, 2759);
    let serie = serie_aleatoria(c, 1001, 9, true);
    let k = c.qtd_dezenas as u32;

    // T = 999 = 5·199 + 4: quatro dobras de 199 e a última com 203.
    assert_eq!(faixa_da_busca(c, &serie).expect("faixa"), faixa(2, 1000));
    let esperado = dividir_em_dobras(faixa(2, 1000), QTD_DOBRAS).expect("dobras");
    assert_eq!(
        pares(&esperado),
        [(2, 200), (201, 399), (400, 598), (599, 797), (798, 1000)]
    );
    assert_eq!(dobras_da_busca(c, &serie).expect("dobras"), esperado);

    let wf = walk_forward(c, &serie, k).expect("walk-forward");
    assert_eq!(wf.corte, 2759);
    assert_eq!(
        wf.dobras.iter().map(|d| d.faixa).collect::<Vec<_>>(),
        esperado
    );
    let ultima = &wf.dobras[4];
    assert_eq!(ultima.faixa.ultimo, 1000);
    assert_eq!(ultima.metricas.ultimo_concurso, 1000);
    assert_eq!(ultima.topk.ultimo_concurso, 1000);
    assert_eq!(ultima.metricas.concursos_avaliados, 203);
    assert_eq!(ultima.topk.concursos_avaliados, 203);

    // O snapshot não entra: sem ele (1..=1000) o resultado é idêntico.
    let sem_snapshot = serie_aleatoria(c, 1000, 9, false);
    assert_eq!(
        bits(&walk_forward(c, &sem_snapshot, k).expect("walk-forward")),
        bits(&wf)
    );
    // Cada dobra confere com a série calibrada de uma vez.
    conferir_dobras_contra_a_serie_inteira(c, &serie, k);
    // Não há trancado nessa série.
    assert!(eh_erro_sem_concursos(avaliar_trancado(c, &serie, k)));
}

// ------------------------------------------------ dobra pequena, conta à mão

/// MINI com posições fixas (dezena `d` na posição `d`) e as saídas dadas,
/// concursos 1, 2, … Contagens `s` de saídas por posição e concursos `c`
/// antes de cada concurso, com N = 4, K = 2 e m = 100:
/// `p = (4·s + 200) / (4·(c + 100))`.
///
/// | concurso | saiu   | s antes (pos. 1..4) | c |
/// |---|---|---|---|
/// | 1 | 1,2 | 0,0,0,0 | 0 |
/// | 2 | 1,3 | 1,1,0,0 | 1 |
/// | 3 | 1,2 | 2,1,1,0 | 2 |
/// | 4 | 1,4 | 3,2,1,0 | 3 |
/// | 5 | 1,2 | 4,2,1,1 | 4 |
/// | 6 | 1,3 | 5,3,1,1 | 5 |
/// | 7 | 1,2 | 6,3,2,1 | 6 |
fn serie_de_posicoes_fixas() -> Vec<Observacao> {
    [[1, 2], [1, 3], [1, 2], [1, 4], [1, 2], [1, 3], [1, 2]]
        .iter()
        .enumerate()
        .flat_map(|(i, s)| concurso_mini(i as u32 + 1, [1, 2, 3, 4], Some(*s)))
        .collect()
}

#[test]
fn dobra_de_um_concurso_confere_com_fracao_inteira() {
    // Corte 7: avaliados 2..=7, T = 6 → dobras 1, 1, 1, 1 e 2. A dobra 2 é o
    // concurso 4.
    let cfg7 = mini_com_corte(7);
    let wf = walk_forward(&cfg7, &serie_de_posicoes_fixas(), 2).expect("walk-forward");
    assert_eq!(
        wf.dobras.iter().map(|d| d.faixa).collect::<Vec<_>>(),
        [
            faixa(2, 2),
            faixa(3, 3),
            faixa(4, 4),
            faixa(5, 5),
            faixa(6, 7)
        ]
    );
    let d = &wf.dobras[2];

    // Concurso 4: s = (3,2,1,0), c = 3 → denominador 4·103 = 412 e
    // p = (212, 208, 204, 200)/412 nas dezenas 1..4. Saíram 1 e 4:
    // dezena 1 → 212/412; 2 (não saiu) → 1 − 208/412 = 204/412;
    // 3 (não saiu) → 1 − 204/412 = 208/412; 4 → 200/412.
    let ll = -((212.0_f64 / 412.0).ln()
        + (204.0_f64 / 412.0).ln()
        + (208.0_f64 / 412.0).ln()
        + (200.0_f64 / 412.0).ln())
        / 4.0;
    // Brier: erros 200, 208, 204 e 212 (sobre 412):
    // (200² + 208² + 204² + 212²)/(4·412²) = 169824/678976.
    let brier = 169824.0_f64 / 678976.0;

    assert_eq!(d.metricas.concursos_avaliados, 1);
    assert_eq!(
        (d.metricas.primeiro_concurso, d.metricas.ultimo_concurso),
        (4, 4)
    );
    assert!(perto(d.metricas.log_loss.perfil, ll), "{:?}", d.metricas);
    assert!(perto(d.metricas.brier.perfil, brier), "{:?}", d.metricas);
    assert!(perto(d.metricas.log_loss.acaso, std::f64::consts::LN_2));
    assert_eq!(d.metricas.brier.acaso, 0.25);
    assert!(perto(
        d.metricas.log_loss.diferenca,
        ll - std::f64::consts::LN_2
    ));
    assert!(perto(d.metricas.brier.diferenca, brier - 0.25));
    assert_eq!(d.metricas.log_loss.erro_padrao, None);

    // Top-2 = posições 1 e 2 = dezenas 1 e 2; saíram 1 e 4 → 1 acerto.
    assert_eq!((d.topk.k, d.topk.concursos_avaliados), (2, 1));
    assert_eq!(d.topk.total_acertos, 1);
}

#[test]
fn dobra_de_dois_concursos_confere_com_fracao_inteira_e_erro_padrao() {
    let cfg7 = mini_com_corte(7);
    let wf = walk_forward(&cfg7, &serie_de_posicoes_fixas(), 2).expect("walk-forward");
    let d = &wf.dobras[4];
    assert_eq!(d.faixa, faixa(6, 7));

    // Concurso 6: s = (5,3,1,1), c = 5 → denominador 4·105 = 420 e
    // p = (220, 212, 204, 204)/420. Saíram 1 e 3:
    // 1 → 220; 2 (não) → 1 − 212/420 = 208/420; 3 → 204; 4 (não) → 216/420.
    let ll6 = -((220.0_f64 / 420.0).ln()
        + (208.0_f64 / 420.0).ln()
        + (204.0_f64 / 420.0).ln()
        + (216.0_f64 / 420.0).ln())
        / 4.0;
    // Erros 200, 212, 216 e 204: 40000 + 44944 + 46656 + 41616 = 173216.
    let br6 = 173216.0_f64 / (4.0 * 420.0 * 420.0);

    // Concurso 7: s = (6,3,2,1), c = 6 → denominador 4·106 = 424 e
    // p = (224, 212, 208, 204)/424. Saíram 1 e 2:
    // 1 → 224; 2 → 212; 3 (não) → 1 − 208/424 = 216/424; 4 (não) → 220/424.
    let ll7 = -((224.0_f64 / 424.0).ln()
        + (212.0_f64 / 424.0).ln()
        + (216.0_f64 / 424.0).ln()
        + (220.0_f64 / 424.0).ln())
        / 4.0;
    // Erros 200, 212, 208 e 204: 40000 + 44944 + 43264 + 41616 = 169824.
    let br7 = 169824.0_f64 / (4.0 * 424.0 * 424.0);

    let ln2 = std::f64::consts::LN_2;
    assert_eq!(d.metricas.concursos_avaliados, 2);
    assert!(perto(d.metricas.log_loss.perfil, (ll6 + ll7) / 2.0));
    assert!(perto(d.metricas.brier.perfil, (br6 + br7) / 2.0));
    assert!(perto(
        d.metricas.log_loss.diferenca,
        (ll6 + ll7) / 2.0 - ln2
    ));
    assert!(perto(d.metricas.brier.diferenca, (br6 + br7) / 2.0 - 0.25));
    // T = 2: erro padrão = |d6 − d7| / 2.
    let ep_ll = d.metricas.log_loss.erro_padrao.expect("T = 2");
    let ep_br = d.metricas.brier.erro_padrao.expect("T = 2");
    assert!(perto(ep_ll, (ll6 - ll7).abs() / 2.0), "{ep_ll}");
    assert!(perto(ep_br, (br6 - br7).abs() / 2.0), "{ep_br}");

    // Top-2 = dezenas 1 e 2: concurso 6 (saíram 1 e 3) → 1; concurso 7
    // (saíram 1 e 2) → 2.
    assert_eq!(d.topk.concursos_avaliados, 2);
    assert_eq!(d.topk.total_acertos, 3);
}

// ------------------------------------------------------------------ medição

/// Custo do walk-forward (5 dobras × 3 loterias, com p-valor por dobra) e do
/// trancado, com o tamanho real do histórico de 2026-10-05. Rode em release:
/// `cargo test --release custo_do_walk_forward -- --ignored --nocapture`.
#[test]
#[ignore = "medição de tempo; rodar em release"]
fn custo_do_walk_forward_nas_tres_loterias() {
    let mut total_wf = std::time::Duration::ZERO;
    let mut total_p = std::time::Duration::ZERO;
    for (slug, trancados) in [("lotofacil", 380), ("megasena", 307), ("quina", 713)] {
        let c = cfg(slug);
        let k = c.qtd_dezenas as u32;
        let serie = serie_aleatoria(c, c.ultimo_concurso_busca + trancados + 1, 1, true);

        let inicio = std::time::Instant::now();
        let wf = walk_forward(c, &serie, k).expect("walk-forward");
        let t_wf = inicio.elapsed();
        total_wf += t_wf;
        let inicio = std::time::Instant::now();
        let tr = avaliar_trancado(c, &serie, k).expect("trancado");
        let t_tr = inicio.elapsed();
        println!(
            "{slug}: walk-forward {:?} (dobras {:?}), trancado {:?} em {t_tr:?}",
            t_wf,
            wf.dobras.iter().map(|d| d.faixa).collect::<Vec<_>>(),
            tr.faixa
        );

        // Pior caso do p-valor por dobra: total logo acima da média (ramo
        // inclinado, sem poda por baixo que ajude).
        let h = Hipergeometrica::da_loteria(c, k).expect("k válido");
        let inicio = std::time::Instant::now();
        for d in &wf.dobras {
            let t = d.faixa.tamanho() as usize;
            let total = (t as f64 * h.media()).ceil() as u64 + 1;
            p_valor_soma(&h, t, total).expect("possível");
        }
        let t_p = inicio.elapsed();
        total_p += t_p;
        println!("{slug}: p-valor no pior caso, 5 dobras: {t_p:?}");
    }
    println!("total: walk-forward {total_wf:?}; p-valor no pior caso {total_p:?}");
}

// ------------------------------------------------------------ veredito (1c.5)

/// Série perfil − acaso por concurso do log loss e do Brier em
/// `inicio..=fim`, calibrada desde o primeiro concurso da entrada.
fn diferencas(c: &LoteriaConfig, serie: &[Observacao], inicio: u32, fim: u32) -> [Vec<f64>; 2] {
    let m = metricas_por_concurso_entre(c, serie, inicio, fim).expect("métricas");
    [
        m.iter()
            .map(|x| x.log_loss_perfil - x.log_loss_acaso)
            .collect(),
        m.iter().map(|x| x.brier_perfil - x.brier_acaso).collect(),
    ]
}

fn bootstrap_padrao(d: &[f64]) -> Bootstrap {
    bootstrap_circular(
        d,
        BLOCO_BOOTSTRAP,
        REAMOSTRAGENS_BOOTSTRAP,
        SEMENTE_BOOTSTRAP,
    )
    .expect("bootstrap")
}

#[test]
fn veredito_da_busca_e_o_bootstrap_da_uniao_das_dobras() {
    let c = mini_com_corte(300);
    let serie = serie_aleatoria(&MINI, 320, 404, true);
    let wf = walk_forward(&c, &serie, 2).expect("walk-forward");
    let v = veredito_da_busca(&c, &serie, 2).expect("veredito");

    // O trecho é a união das 5 dobras: do primeiro da 1ª ao último da 5ª.
    assert_eq!(v.avaliacao.faixa, faixa(2, 300));
    assert_eq!(v.avaliacao.faixa.primeiro, wf.dobras[0].faixa.primeiro);
    assert_eq!(v.avaliacao.faixa.ultimo, wf.dobras[4].faixa.ultimo);
    assert_eq!(v.concursos_minimos, 2 * BLOCO_BOOTSTRAP);

    // Métricas e top-k do trecho = as da série inteira até o corte.
    let metricas = metricas_por_concurso(&c, &serie, 300).expect("métricas");
    assert_eq!(
        bits(&resumir(&metricas).expect("resumo")),
        bits(&v.avaliacao.metricas)
    );
    let acertos = acertos_por_concurso(&c, &serie, 2, 300).expect("acertos");
    assert_eq!(
        bits(&resumir_topk(&c, 2, &acertos).expect("resumo")),
        bits(&v.avaliacao.topk)
    );

    // Bootstrap: o da série d_t por concurso, bit a bit, log loss e Brier.
    let [ll, br] = diferencas(&c, &serie, 2, 300);
    assert_eq!(ll.len(), 299);
    let b = v.bootstrap.as_ref().expect("299 ≥ 100 concursos");
    assert_eq!(bits(&b.log_loss), bits(&bootstrap_padrao(&ll)));
    assert_eq!(bits(&b.brier), bits(&bootstrap_padrao(&br)));
    assert_eq!(b.log_loss.concursos, 299);
    assert_eq!(
        (
            b.log_loss.bloco,
            b.log_loss.reamostragens,
            b.log_loss.semente
        ),
        (BLOCO_BOOTSTRAP, REAMOSTRAGENS_BOOTSTRAP, SEMENTE_BOOTSTRAP)
    );
    // Série aleatória: sem ganho sobre o acaso.
    assert_eq!(v.supera_o_acaso(), Some(false), "{:?}", b.log_loss);

    // Determinístico.
    assert_eq!(v, veredito_da_busca(&c, &serie, 2).expect("veredito"));
}

#[test]
fn veredito_da_busca_nao_le_nada_acima_do_corte() {
    let c = mini_com_corte(150);
    let serie = serie_aleatoria(&MINI, 220, 405, true);
    let antes = veredito_da_busca(&c, &serie, 2).expect("veredito");
    // Lixo acima do corte (posições e `saiu` trocados, concurso sem saída).
    let mut alterada = complementando_o_saiu(&serie, 151);
    alterada.retain(|o| o.concurso <= 160);
    alterada.extend(concurso_mini(161, [4, 3, 2, 1], None));
    let depois = veredito_da_busca(&c, &alterada, 2).expect("veredito");
    assert_eq!(bits(&antes), bits(&depois));
}

#[test]
fn veredito_sem_bootstrap_quando_o_trecho_e_curto() {
    // Busca da MINI: 2..=20, 19 concursos < 100.
    let serie = serie_aleatoria(&MINI, 31, 406, true);
    let v = veredito_da_busca(&MINI, &serie, 2).expect("sem erro");
    assert_eq!(v.avaliacao.faixa, faixa(2, 20));
    assert_eq!(v.bootstrap, None);
    assert_eq!(v.supera_o_acaso(), None);
    // Limite: 99 concursos sem, 100 com.
    let serie = serie_aleatoria(&MINI, 120, 407, true);
    assert_eq!(
        veredito_da_busca(&mini_com_corte(100), &serie, 2)
            .expect("99")
            .bootstrap,
        None
    );
    assert!(veredito_da_busca(&mini_com_corte(101), &serie, 2)
        .expect("100")
        .bootstrap
        .is_some());
}

#[test]
fn veredito_do_trancado_usa_o_trancado_inteiro_com_o_passado() {
    let c = mini_com_corte(100);
    let serie = serie_aleatoria(&MINI, 301, 408, true);
    let v = veredito_do_trancado(&c, &serie, 2).expect("veredito");
    let trancado = avaliar_trancado(&c, &serie, 2).expect("trancado");
    assert_eq!(v.avaliacao, trancado);
    assert_eq!(v.avaliacao.faixa, faixa(101, 300));
    let [ll, br] = diferencas(&c, &serie, 101, 300);
    let b = v.bootstrap.as_ref().expect("200 concursos");
    assert_eq!(bits(&b.log_loss), bits(&bootstrap_padrao(&ll)));
    assert_eq!(bits(&b.brier), bits(&bootstrap_padrao(&br)));

    // Trancado vazio é erro, como em `avaliar_trancado`.
    let curta = serie_aleatoria(&MINI, 101, 408, true);
    assert!(eh_erro_sem_concursos(veredito_do_trancado(&c, &curta, 2)));
}

#[test]
fn veredito_recusa_k_invalido() {
    let serie = serie_aleatoria(&MINI, 31, 409, true);
    for k in [0, 4] {
        assert!(eh_erro_entrada_invalida(veredito_da_busca(
            &MINI, &serie, k
        )));
        assert!(eh_erro_entrada_invalida(veredito_do_trancado(
            &MINI, &serie, k
        )));
    }
}

#[test]
fn ranking_previsivel_supera_o_acaso_no_veredito() {
    // As posições 1 e 2 saem sempre: a calibração aprende e o log loss do
    // perfil fica abaixo do acaso em todo bloco.
    let mut g = Gerador(78);
    let mut serie = Vec::new();
    for c in 1..=300 {
        let mut posicoes = [1_u32, 2, 3, 4];
        g.embaralhar(&mut posicoes);
        let saidas: Vec<u32> = (1..=4).filter(|&d| posicoes[d as usize - 1] <= 2).collect();
        serie.extend(concurso_mini(c, posicoes, Some([saidas[0], saidas[1]])));
    }
    let v = veredito_da_busca(&mini_com_corte(300), &serie, 2).expect("veredito");
    let b = v.bootstrap.as_ref().expect("bootstrap");
    assert!(b.log_loss.media < 0.0);
    assert!(b.log_loss.ic95.superior < 0.0, "{:?}", b.log_loss);
    // Toda reamostra tem média negativa: p no mínimo, 1/(B+1).
    assert_eq!(
        b.log_loss.p_valor,
        1.0 / (REAMOSTRAGENS_BOOTSTRAP + 1) as f64
    );
    assert_eq!(v.supera_o_acaso(), Some(true));
}
