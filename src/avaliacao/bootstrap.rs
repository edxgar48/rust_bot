//! Veredito "supera o acaso": bootstrap circular por blocos (1c.5).
//!
//! Regras em `docs/PLANEJAMENTO.md`, 1c, "Comando `sync avaliar`"
//! (decisões de 2026-10-05):
//! - bootstrap **circular por blocos** (Politis & Romano, 1992) sobre a série
//!   por concurso `d_t` = perfil − acaso (log loss; o Brier só informa);
//! - blocos **fixos** de [`BLOCO_BOOTSTRAP`] concursos: a calibração
//!   acumulada (1c.1) dá memória longa à série, e blocos curtos subestimariam
//!   a variância da média;
//! - [`REAMOSTRAGENS_BOOTSTRAP`] reamostragens com gerador próprio
//!   ([`SplitMix64`], sem crate nova) e semente fixa
//!   ([`SEMENTE_BOOTSTRAP`]): mesma entrada, mesma saída, bit a bit;
//! - p-valor **unilateral** (perfil melhor = média de `d_t` menor que zero) e
//!   IC 95% **percentil**; a decisão usa α = [`super::ALFA`].
//!
//! # Reamostragem
//!
//! Com T concursos e blocos de tamanho b, cada reamostra junta
//! ⌈T/b⌉ blocos de b valores seguidos, cada um começando num índice sorteado
//! uniformemente em `0..T` e **dando a volta** no fim da série (índice
//! `(início + j) mod T`); a última é cortada para a reamostra ter T valores.
//! Dar a volta deixa todo concurso com a mesma chance de entrar (no bootstrap
//! por blocos móveis, sem volta, as pontas da série entram menos).
//!
//! # p-valor (centragem)
//!
//! A média reamostrada `m*` varia em torno da média observada `m` como `m`
//! varia em torno da média verdadeira μ. Sob a hipótese nula μ = 0 (o perfil
//! empata com o acaso), a distribuição de `m − 0` é aproximada pela de
//! `m* − m`; o p-valor unilateral é a chance de, sob o nulo, sair uma média
//! tão negativa quanto a observada:
//!
//! `p = (1 + #{r : m*_r − m ≤ m}) / (B + 1)`
//!
//! O "+1" no numerador e no denominador (Davison & Hinkley, 1997, §4.2) conta
//! a própria amostra observada como uma das reamostras: o p-valor nunca é 0
//! (com B reamostras não há como afirmar menos que 1/(B+1)) e o teste não
//! rejeita mais que α sob o nulo.
//!
//! # IC 95% percentil
//!
//! Quantis empíricos (inversa da distribuição empírica, sem interpolação) das
//! `m*` ordenadas: a ⌈0,025·B⌉-ésima e a ⌈0,975·B⌉-ésima (com B = 10 000, a
//! 250ª e a 9 750ª). Contas em inteiros, sem arredondamento de `f64`.
//!
//! # Concursos mínimos
//!
//! Com T ≤ b, toda reamostra é uma rotação da série e tem a mesma média: o
//! bootstrap não vê variação nenhuma e o p-valor sairia mínimo para qualquer
//! média negativa (alarme falso garantido). Por isso o bootstrap exige
//! **ao menos dois blocos** (T ≥ 2·b, [`concursos_minimos`]); abaixo disso
//! quem chama não tem veredito.

use super::metricas::Intervalo;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Tamanho fixo dos blocos de concursos seguidos.
pub const BLOCO_BOOTSTRAP: usize = 50;

/// Número de reamostragens (B).
pub const REAMOSTRAGENS_BOOTSTRAP: usize = 10_000;

/// Semente fixa do gerador (a data da decisão, 2026-10-05): trocar muda
/// todos os p-valores e intervalos do bootstrap.
pub const SEMENTE_BOOTSTRAP: u64 = 20_261_005;

/// SplitMix64 (Steele, Lea & Flood, 2014; constantes da implementação de
/// referência de Vigna): estado de 64 bits, período 2^64, passa no BigCrush.
/// Escolhido por ser curto, conhecido e ter valores de referência públicos;
/// sem crate nova.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    estado: u64,
}

impl SplitMix64 {
    pub const fn nova(semente: u64) -> Self {
        Self { estado: semente }
    }

    /// Próximo valor de 64 bits.
    pub fn proximo(&mut self) -> u64 {
        self.estado = self.estado.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.estado;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Inteiro uniforme em `0..n`, **sem viés**: descarta os valores abaixo
    /// de `2^64 mod n` (o que sobra é múltiplo de `n`) e tira o resto. `n` = 0
    /// é [`SpiderError::AvaliacaoEntradaInvalida`].
    pub fn abaixo_de(&mut self, n: u64) -> Result<u64> {
        if n == 0 {
            return Err(SpiderError::AvaliacaoEntradaInvalida(
                "sorteio uniforme em 0..0".to_string(),
            ));
        }
        // 2^64 mod n, sem u128: (2^64 − n) mod n.
        let limiar = n.wrapping_neg() % n;
        loop {
            let x = self.proximo();
            if x >= limiar {
                return Ok(x % n);
            }
        }
    }
}

/// Resultado do bootstrap de uma série por concurso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bootstrap {
    /// T: tamanho da série.
    pub concursos: usize,
    /// b: tamanho dos blocos.
    pub bloco: usize,
    /// B: reamostragens.
    pub reamostragens: usize,
    pub semente: u64,
    /// Média observada da série (para `d_t`, a diferença perfil − acaso).
    pub media: f64,
    /// IC 95% percentil da média.
    pub ic95: Intervalo,
    /// p-valor unilateral de a média verdadeira ser < 0, por centragem e com
    /// o "+1" (fórmula na documentação do módulo). Em `[1/(B+1), 1]`.
    pub p_valor: f64,
}

/// Menor série que o bootstrap aceita com blocos de `bloco`: dois blocos.
pub const fn concursos_minimos(bloco: usize) -> usize {
    bloco.saturating_mul(2)
}

/// Bootstrap circular por blocos da média de `serie` (regras e fórmulas na
/// documentação do módulo). Determinístico: depende só de `serie`, `bloco`,
/// `reamostragens` e `semente`. Os inícios dos blocos dependem só de T,
/// `bloco`, `reamostragens` e `semente`, então duas séries do mesmo tamanho
/// com a mesma semente (log loss e Brier) usam exatamente as mesmas
/// reamostras: a comparação é pareada.
///
/// Erros: `bloco` ou `reamostragens` = 0, ou valor não finito na série, é
/// [`SpiderError::AvaliacaoEntradaInvalida`]; série com menos de
/// [`concursos_minimos`] valores é [`SpiderError::AvaliacaoSemConcursos`].
pub fn bootstrap_circular(
    serie: &[f64],
    bloco: usize,
    reamostragens: usize,
    semente: u64,
) -> Result<Bootstrap> {
    if bloco == 0 || reamostragens == 0 {
        return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
            "bootstrap com blocos de {bloco} e {reamostragens} reamostragem(ns)"
        )));
    }
    let t = serie.len();
    let minimo = concursos_minimos(bloco);
    if t < minimo {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "o bootstrap por blocos de {bloco} concursos precisa de ao menos {minimo} concursos (dois blocos), e o trecho tem {t}: com um bloco só, toda reamostra é uma rotação da série e a incerteza sai zero"
        )));
    }
    if let Some((i, v)) = serie.iter().enumerate().find(|(_, v)| !v.is_finite()) {
        return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
            "bootstrap: valor {v} não finito na posição {i} da série"
        )));
    }

    let media = serie.iter().sum::<f64>() / t as f64;
    let mut gerador = SplitMix64::nova(semente);
    let mut medias = Vec::with_capacity(reamostragens);
    for _ in 0..reamostragens {
        let mut soma = 0.0;
        let mut preenchidos = 0;
        while preenchidos < t {
            // t ≥ 2·bloco ≥ 2, cabe em u64 e o resultado cabe em usize.
            let inicio = gerador.abaixo_de(t as u64)? as usize;
            let quantos = bloco.min(t - preenchidos);
            for i in indices_do_bloco(inicio, quantos, t) {
                soma += serie[i];
            }
            preenchidos += quantos;
        }
        medias.push(soma / t as f64);
    }

    let extremas = medias.iter().filter(|&&m| m - media <= media).count();
    let p_valor = (1 + extremas) as f64 / (reamostragens + 1) as f64;

    medias.sort_by(f64::total_cmp);
    let quantil = |milesimos: usize| medias[(reamostragens * milesimos).div_ceil(1000).max(1) - 1];
    Ok(Bootstrap {
        concursos: t,
        bloco,
        reamostragens,
        semente,
        media,
        ic95: Intervalo {
            inferior: quantil(25),
            superior: quantil(975),
        },
        p_valor,
    })
}

/// Índices de um bloco de `quantos` valores que começa em `inicio`, dando a
/// volta no fim de uma série de tamanho `t` (`inicio < t`).
fn indices_do_bloco(inicio: usize, quantos: usize, t: usize) -> impl Iterator<Item = usize> {
    (0..quantos).map(move |j| (inicio + j) % t)
}

#[cfg(test)]
mod tests;
