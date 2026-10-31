//! Avaliação do índice de força das posições (G-1) contra o acaso — Fase 1g,
//! tarefa 1g.4.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1g". Função pura (sem I/O, sem SQL,
//! determinística). É descrição do passado: sorteios são independentes e o
//! esperado é **não superar o acaso**; ganho aparente é suspeita de vazamento.
//!
//! # A ordem G-1
//!
//! No concurso `t`, cada dezena herda o `g_1` da posição que ocupava na tabela
//! de força antes do sorteio ([`crate::motor::dezenas_via_posicao`]). É uma
//! ordem 1…N das dezenas, como a da nota A da 1f, e passa pela mesma régua.
//!
//! # A régua
//!
//! A de [`avaliar_ranking`] (1f.4), sem código novo de métrica: as mesmas
//! dobras do walk-forward da busca, só as de teste (2 a 5), log loss e Brier
//! com a calibração pela posição da 1c (m = 100), top-k contra a
//! hipergeométrica, bootstrap circular por blocos e a calibração linear da 1e.
//! A ordem G-1 e a da nota A (a referência, a taxa histórica da posição na
//! tabela de força) passam pelas **mesmas** linhas e dobras, para a comparação
//! ser justa.
//!
//! # Aquecimento
//!
//! Nos primeiros concursos a tabela de força ainda não tem histórico (as
//! dezenas estão empatadas) e as posições marcadas são ruído. As linhas dos
//! concursos `1 ..= `[`AQUECIMENTO`] saem **da avaliação inteira**, calibração
//! inclusive: a série avaliada começa no concurso `AQUECIMENTO + 1` (o primeiro da
//! entrada, que recebe o acaso por construção e não é avaliado), e as dobras se
//! dividem a partir do seguinte. Por isso os números **não** são comparáveis com os
//! do `sync ranking --avaliar`, que parte do concurso 1; a ordem de A é
//! reavaliada aqui, no mesmo recorte, justamente para a comparação valer.
//!
//! # Conjunto trancado
//!
//! Fechado, como na 1f.4: nada acima de `LoteriaConfig::ultimo_concurso_busca`
//! entra. Não há `--trancado` nem registro de uso.
//!
//! # p-valores
//!
//! Valem as ressalvas da 1c e da 1f.4: o p-valor do top-k só vale se o perfil e
//! o `k` foram fixados sem ver os sorteios do trecho, e esta saída traz vários
//! p-valores sem correção para múltiplas comparações.

use super::avaliacao_ranking::{avaliar_ranking, AvaliacaoOrdemB, AvaliacaoRanking, DobraDeTeste};
use super::walk_forward::FaixaConcursos;
use super::LinhaRanking;
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Concursos `1 ..= AQUECIMENTO` ficam fora da avaliação (decisão de
/// 2026-10-31): a maior janela finita do motor (`JANELA_MAX`), o tempo que a
/// janela de 100 concursos leva para encher.
pub const AQUECIMENTO: u32 = 100;

/// Uma dezena num concurso, com a posição que cada ordem lhe dá.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinhaG1 {
    pub concurso: u32,
    pub dezena: u32,
    /// Posição 1…N pela nota A da 1f (a referência).
    pub posicao_ranking_a: u32,
    /// Posição 1…N pelo `g_1` da posição que a dezena ocupava (G-1).
    pub g_1: u32,
    pub saiu: bool,
}

/// Resultado de [`avaliar_g1`]: G-1 e a referência A na mesma régua.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvaliacaoG1 {
    /// `cfg.ultimo_concurso_busca`, o limite superior de tudo o que se leu.
    pub corte: u32,
    /// O `k` do top-k.
    pub k: u32,
    /// Concursos `1 ..= aquecimento` ficaram fora (ver [`AQUECIMENTO`]).
    pub aquecimento: u32,
    /// As dobras de teste, 2 a 5, em ordem.
    pub dobras: Vec<DobraDeTeste>,
    /// Do primeiro concurso da dobra 2 ao último da dobra 5.
    pub faixa: FaixaConcursos,
    /// Concursos avaliados no trecho inteiro.
    pub concursos: usize,
    /// A ordem G-1: a régua da 1c inteira e a linha linear da 1e.
    pub g: AvaliacaoOrdemB,
    /// A ordem da nota A (a referência), na mesma régua e nas mesmas dobras.
    pub a: AvaliacaoOrdemB,
}

/// Linhas no formato de [`avaliar_ranking`] com `outra` na posição de B (a
/// ordem que sai com a régua inteira). As notas não são usadas pela avaliação.
fn como_ranking(
    linhas: &[LinhaG1],
    outra: fn(&LinhaG1) -> u32,
    referencia: fn(&LinhaG1) -> u32,
) -> Vec<LinhaRanking> {
    linhas
        .iter()
        .map(|l| LinhaRanking {
            concurso: l.concurso,
            dezena: l.dezena,
            posicao_indice: referencia(l),
            saiu: Some(l.saiu),
            nota_a: 0.0,
            nota_b: 0.0,
            posicao_ranking_a: referencia(l),
            posicao_ranking_b: outra(l),
        })
        .collect()
}

/// Avalia a ordem G-1 e a da nota A nas dobras de teste (2 a 5) do
/// walk-forward da busca, depois de tirar o aquecimento. `linhas` são as de uma
/// série completa (cada concurso com as *N* dezenas e as duas posições formando
/// permutações de 1 a *N*); o que está acima de `cfg.ultimo_concurso_busca` e o
/// que está em `1 ..= `[`AQUECIMENTO`] são descartados antes de tudo. `k` é o do
/// top-k (`1..=N−1`).
///
/// Erros: os de [`avaliar_ranking`] (`k` fora de `1..=N−1`, série inválida,
/// menos concursos avaliáveis que dobras do walk-forward). Trecho curto demais
/// para o bootstrap não é erro: sai sem veredito.
pub fn avaliar_g1(cfg: &LoteriaConfig, linhas: &[LinhaG1], k: u32) -> Result<AvaliacaoG1> {
    let avaliaveis: Vec<LinhaG1> = linhas
        .iter()
        .filter(|l| l.concurso > AQUECIMENTO && l.concurso <= cfg.ultimo_concurso_busca)
        .copied()
        .collect();

    let com_g = avaliar_ranking(
        cfg,
        &como_ranking(&avaliaveis, |l| l.g_1, |l| l.posicao_ranking_a),
        k,
    )?;
    let com_a = avaliar_ranking(
        cfg,
        &como_ranking(&avaliaveis, |l| l.posicao_ranking_a, |l| l.g_1),
        k,
    )?;
    // As duas chamadas veem os mesmos concursos: mesmas dobras.
    if com_g.dobras != com_a.dobras || com_g.faixa != com_a.faixa {
        return Err(SpiderError::AvaliacaoEntradaInvalida(
            "as ordens G-1 e A saíram com dobras diferentes".to_string(),
        ));
    }
    let AvaliacaoRanking {
        corte,
        k,
        dobras,
        faixa,
        concursos,
        b: g,
        ..
    } = com_g;
    Ok(AvaliacaoG1 {
        corte,
        k,
        aquecimento: AQUECIMENTO,
        dobras,
        faixa,
        concursos,
        g,
        a: com_a.b,
    })
}

#[cfg(test)]
mod tests;
