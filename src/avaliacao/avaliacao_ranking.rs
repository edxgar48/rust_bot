//! Avaliação do ranking por taxa histórica contra o acaso — Fase 1f, tarefa
//! 1f.4.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1f", tarefa 1f.4 (decisões de
//! 2026-10-27). Função pura (sem I/O, sem SQL, determinística) sobre as linhas
//! de [`super::ranquear`]. É descrição do passado: sorteios são independentes e
//! o esperado é **não superar o acaso**; ganho aparente é suspeita de vazamento.
//!
//! # O que se avalia
//!
//! As mesmas dobras do `sync avaliar` ([`dobras_da_busca`]), mas **só as de
//! teste, 2 a 5** (a dobra 1 fica fora, como na parte B do `sync evidencia`):
//! métricas por dobra e um total sobre as dobras 2 a 5. A calibração parte
//! sempre do primeiro concurso da entrada e usa todo o passado, inclusive o da
//! dobra 1, que só deixa de ser *avaliado*.
//!
//! - **Ordem A** (`posicao_ranking_a`): o log loss e o Brier de A são, por
//!   construção, os do perfil no `sync avaliar` (a nota A é a taxa da posição,
//!   igual à [`super::calibrar`] da 1c, bit a bit) e **não se repetem**.
//!   Avalia-se só o que é novo: o top-k pela ordem de A, contra a
//!   hipergeométrica, e o log loss com a calibração linear da 1e
//!   ([`Regua::Linear`]) sobre a ordem de A, linha só informativa.
//! - **Ordem B** (`posicao_ranking_b`, EXPERIMENTAL): a probabilidade é a
//!   [`super::calibrar`] da 1c (m = 100) sobre a posição pela nota B; log loss e
//!   Brier por dobra e no total, top-k e o bootstrap circular da 1c (blocos de
//!   [`BLOCO_BOOTSTRAP`], B = [`REAMOSTRAGENS_BOOTSTRAP`], semente fixa) sobre
//!   `d_t` = log loss de B − acaso por concurso, mais a calibração linear sobre
//!   a ordem de B (sem bootstrap, só informativa).
//!
//! # Conjunto trancado
//!
//! Fechado: nada acima de `LoteriaConfig::ultimo_concurso_busca` entra, nem para
//! validar (as linhas são cortadas antes de qualquer cálculo). Não há
//! `--trancado` nem registro de uso. O corte vem sempre do config.
//!
//! # Ponto no tempo
//!
//! A ordem de `t` já vem de taxas com concursos até `t−1`
//! ([`super::calcular_taxas`]) e a calibração de `t` (pela posição e linear)
//! usa só concursos `< t`. A evidência e o esperado da 1f.2 (histórico inteiro)
//! não são entrada desta avaliação.
//!
//! # p-valores
//!
//! Valem as ressalvas da 1c: o p-valor do top-k só vale se o perfil e o `k`
//! foram fixados sem ver os sorteios do trecho (aqui, o perfil ativo e k = K,
//! sem busca), e o IC normal ignora a correlação serial da calibração
//! acumulada e tende a sair estreito; o veredito usa o bootstrap por blocos.

use super::bootstrap::{
    bootstrap_circular, concursos_minimos, BLOCO_BOOTSTRAP, REAMOSTRAGENS_BOOTSTRAP,
    SEMENTE_BOOTSTRAP,
};
use super::calibracao::{Observacao, Regua};
use super::evidencia_indice::PRIMEIRA_DOBRA_DE_TESTE;
use super::metricas::{
    metricas_por_concurso_entre, metricas_por_concurso_entre_com, resumir, MetricasConcurso,
    ResumoAvaliacao,
};
use super::topk::{
    acertos_por_concurso_entre, resumir_topk, validar_k, AcertosConcurso, ResumoTopK,
};
use super::walk_forward::{dobras_da_busca, BootstrapTrecho, FaixaConcursos};
use super::{LinhaRanking, ALFA};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Uma medida por dobra de teste (2 a 5, em ordem) e a do trecho inteiro
/// (a união das dobras de teste).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PorTrecho<T> {
    pub dobras: Vec<T>,
    pub total: T,
}

/// Uma dobra de teste do walk-forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DobraDeTeste {
    /// Número da dobra no walk-forward (2 a 5).
    pub dobra: u32,
    pub faixa: FaixaConcursos,
}

/// O que é novo para a ordem de A (ver o topo do módulo): o log loss e o Brier
/// de A são os do `sync avaliar` e não se repetem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvaliacaoOrdemA {
    /// Acertos do top-`k` pela ordem de A, com p-valor exato contra a
    /// hipergeométrica (como na 1c.2).
    pub topk: PorTrecho<ResumoTopK>,
    /// Log loss com a calibração linear (1e) sobre a ordem de A; só
    /// informativo (a diferença < 0 = melhor que o acaso).
    pub linear: PorTrecho<ResumoAvaliacao>,
}

/// A ordem de B (EXPERIMENTAL): a régua inteira da 1c e a linha linear da 1e.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvaliacaoOrdemB {
    /// Log loss e Brier com a calibração pela posição da 1c (m = 100) sobre a
    /// ordem de B.
    pub posicao: PorTrecho<ResumoAvaliacao>,
    /// Acertos do top-`k` pela ordem de B.
    pub topk: PorTrecho<ResumoTopK>,
    /// Log loss com a calibração linear (1e) sobre a ordem de B; só
    /// informativo.
    pub linear: PorTrecho<ResumoAvaliacao>,
    /// Bootstrap circular da 1c sobre o trecho inteiro (dobras 2 a 5); `None`
    /// quando o trecho tem menos de `concursos_minimos` concursos.
    pub bootstrap: Option<BootstrapTrecho>,
    /// Dois blocos de [`BLOCO_BOOTSTRAP`] concursos.
    pub concursos_minimos: usize,
}

impl AvaliacaoOrdemB {
    /// "Supera o acaso" = p-valor do bootstrap do log loss ≤ [`ALFA`] (`None`
    /// sem bootstrap), como em `VereditoTrecho::supera_o_acaso`.
    pub fn supera_o_acaso(&self) -> Option<bool> {
        self.bootstrap.as_ref().map(|b| b.log_loss.p_valor <= ALFA)
    }
}

/// Resultado de [`avaliar_ranking`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvaliacaoRanking {
    /// `cfg.ultimo_concurso_busca`, o limite superior de tudo o que se leu.
    pub corte: u32,
    /// O `k` do top-k.
    pub k: u32,
    /// As dobras de teste, 2 a 5, em ordem.
    pub dobras: Vec<DobraDeTeste>,
    /// Do primeiro concurso da dobra 2 ao último da dobra 5 (consecutivos).
    pub faixa: FaixaConcursos,
    /// Concursos avaliados no trecho inteiro.
    pub concursos: usize,
    pub a: AvaliacaoOrdemA,
    pub b: AvaliacaoOrdemB,
}

fn invalida(detalhe: String) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe)
}

/// As linhas até o corte, na ordem de uma das posições do ranking, no formato
/// da calibração da 1c. O que está acima do corte (o trancado) é descartado
/// aqui, antes de qualquer validação ou conta.
fn observacoes_da_ordem(
    linhas: &[LinhaRanking],
    corte: u32,
    posicao: fn(&LinhaRanking) -> u32,
) -> Vec<Observacao> {
    linhas
        .iter()
        .filter(|l| l.concurso <= corte)
        .map(|l| Observacao {
            concurso: l.concurso,
            dezena: l.dezena,
            posicao: posicao(l),
            saiu: l.saiu,
        })
        .collect()
}

/// O pedaço de uma série ordenada por concurso que cai em `faixa`; a série tem
/// de cobri-la inteira, sem buraco.
fn fatia<S>(serie: &[S], concurso: fn(&S) -> u32, faixa: FaixaConcursos) -> Result<&[S]> {
    let inicio = serie.partition_point(|s| concurso(s) < faixa.primeiro);
    let fim = serie.partition_point(|s| concurso(s) <= faixa.ultimo);
    let pedaco = &serie[inicio..fim];
    // Defesa: a faixa vem de `dobras_da_busca` sobre a mesma série.
    if pedaco.len() as u64 != faixa.tamanho() {
        return Err(invalida(format!(
            "faixa {}..={}: {} concurso(s) na série avaliada, esperados {}",
            faixa.primeiro,
            faixa.ultimo,
            pedaco.len(),
            faixa.tamanho()
        )));
    }
    Ok(pedaco)
}

/// Resume uma série por concurso em cada dobra de teste e no trecho inteiro.
fn por_trecho<S, T>(
    serie: &[S],
    concurso: fn(&S) -> u32,
    dobras: &[DobraDeTeste],
    faixa: FaixaConcursos,
    resumo: impl Fn(&[S]) -> Result<T>,
) -> Result<PorTrecho<T>> {
    let por_dobra = dobras
        .iter()
        .map(|d| resumo(fatia(serie, concurso, d.faixa)?))
        .collect::<Result<Vec<_>>>()?;
    Ok(PorTrecho {
        dobras: por_dobra,
        total: resumo(fatia(serie, concurso, faixa)?)?,
    })
}

/// Bootstrap da 1c (log loss e Brier, mesmas reamostras) sobre as séries de B;
/// `None` abaixo de dois blocos.
fn bootstrap_do_trecho(serie: &[MetricasConcurso]) -> Result<(Option<BootstrapTrecho>, usize)> {
    let minimo = concursos_minimos(BLOCO_BOOTSTRAP);
    if serie.len() < minimo {
        return Ok((None, minimo));
    }
    let reamostrar = |d: &[f64]| {
        bootstrap_circular(
            d,
            BLOCO_BOOTSTRAP,
            REAMOSTRAGENS_BOOTSTRAP,
            SEMENTE_BOOTSTRAP,
        )
    };
    let log_loss: Vec<f64> = serie
        .iter()
        .map(|m| m.log_loss_perfil - m.log_loss_acaso)
        .collect();
    let brier: Vec<f64> = serie
        .iter()
        .map(|m| m.brier_perfil - m.brier_acaso)
        .collect();
    Ok((
        Some(BootstrapTrecho {
            log_loss: reamostrar(&log_loss)?,
            brier: reamostrar(&brier)?,
        }),
        minimo,
    ))
}

/// Avalia o ranking por taxa histórica nas dobras de teste (2 a 5) do
/// walk-forward da busca. `linhas` são as de [`super::ranquear`] (qualquer
/// ordem; cada concurso com as *N* dezenas e posições de 1 a *N* nas duas
/// ordens); o que está acima de `cfg.ultimo_concurso_busca` é descartado antes
/// de tudo. `k` é o do top-k (`1..=N−1`).
///
/// Erros: `k` fora de `1..=N−1` ou série inválida até o corte são
/// [`SpiderError::AvaliacaoEntradaInvalida`]; menos concursos avaliáveis na
/// busca que dobras do walk-forward é [`SpiderError::AvaliacaoSemConcursos`].
/// Trecho curto demais para o bootstrap não é erro: sai sem veredito.
pub fn avaliar_ranking(
    cfg: &LoteriaConfig,
    linhas: &[LinhaRanking],
    k: u32,
) -> Result<AvaliacaoRanking> {
    validar_k(cfg, k)?;
    let corte = cfg.ultimo_concurso_busca;
    let ordem_a = observacoes_da_ordem(linhas, corte, |l| l.posicao_ranking_a);
    let ordem_b = observacoes_da_ordem(linhas, corte, |l| l.posicao_ranking_b);

    // As dobras dependem só dos números dos concursos sorteados até o corte,
    // iguais nas duas ordens.
    let todas = dobras_da_busca(cfg, &ordem_a)?;
    let primeira = PRIMEIRA_DOBRA_DE_TESTE as usize - 1;
    let (Some(inicial), Some(ultima)) = (todas.get(primeira), todas.last()) else {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "o walk-forward tem {} dobra(s), menos que a primeira dobra de teste ({PRIMEIRA_DOBRA_DE_TESTE})",
            todas.len()
        )));
    };
    let faixa = FaixaConcursos {
        primeiro: inicial.primeiro,
        ultimo: ultima.ultimo,
    };
    let dobras: Vec<DobraDeTeste> = (PRIMEIRA_DOBRA_DE_TESTE..)
        .zip(&todas[primeira..])
        .map(|(dobra, faixa)| DobraDeTeste {
            dobra,
            faixa: *faixa,
        })
        .collect();

    let resumo_metricas = |f: &[MetricasConcurso]| resumir(f);
    let concurso_metricas: fn(&MetricasConcurso) -> u32 = |m| m.concurso;
    let concurso_acertos: fn(&AcertosConcurso) -> u32 = |a| a.concurso;

    // Ordem A: só o novo (top-k e a linha linear).
    let acertos_a = acertos_por_concurso_entre(cfg, &ordem_a, k, faixa.primeiro, faixa.ultimo)?;
    let linear_a = metricas_por_concurso_entre_com(
        Regua::Linear,
        cfg,
        &ordem_a,
        faixa.primeiro,
        faixa.ultimo,
    )?;
    let a = AvaliacaoOrdemA {
        topk: por_trecho(&acertos_a, concurso_acertos, &dobras, faixa, |f| {
            resumir_topk(cfg, k, f)
        })?,
        linear: por_trecho(
            &linear_a,
            concurso_metricas,
            &dobras,
            faixa,
            resumo_metricas,
        )?,
    };

    // Ordem B: a régua da 1c inteira, mais a linha linear.
    let metricas_b = metricas_por_concurso_entre(cfg, &ordem_b, faixa.primeiro, faixa.ultimo)?;
    let acertos_b = acertos_por_concurso_entre(cfg, &ordem_b, k, faixa.primeiro, faixa.ultimo)?;
    let linear_b = metricas_por_concurso_entre_com(
        Regua::Linear,
        cfg,
        &ordem_b,
        faixa.primeiro,
        faixa.ultimo,
    )?;
    let (bootstrap, concursos_minimos) =
        bootstrap_do_trecho(fatia(&metricas_b, concurso_metricas, faixa)?)?;
    let b = AvaliacaoOrdemB {
        posicao: por_trecho(
            &metricas_b,
            concurso_metricas,
            &dobras,
            faixa,
            resumo_metricas,
        )?,
        topk: por_trecho(&acertos_b, concurso_acertos, &dobras, faixa, |f| {
            resumir_topk(cfg, k, f)
        })?,
        linear: por_trecho(
            &linear_b,
            concurso_metricas,
            &dobras,
            faixa,
            resumo_metricas,
        )?,
        bootstrap,
        concursos_minimos,
    };

    Ok(AvaliacaoRanking {
        corte,
        k,
        concursos: faixa.tamanho() as usize,
        dobras,
        faixa,
        a,
        b,
    })
}

#[cfg(test)]
mod tests;
