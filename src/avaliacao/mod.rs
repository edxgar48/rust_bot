//! Avaliação do índice de força contra o acaso.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "1c — Avaliação" (decisões de
//! 2026-10-05). Premissa: os sorteios são independentes, então o esperado de
//! qualquer perfil é **empatar** com o acaso; ganho aparente é suspeita de
//! vazamento até prova em contrário.
//!
//! Funções puras: sem I/O, sem SQL e sem nada específico de uma loteria além
//! do que vem em `LoteriaConfig`. Quem lê o banco (índice + `saiu` das
//! features) monta as [`Observacao`]es e chama [`avaliar`].
//!
//! Ponto no tempo: a probabilidade do concurso `t` ([`calibrar`]) usa só os
//! concursos anteriores a `t`; [`avaliar`] descarta tudo acima do corte antes
//! de qualquer cálculo, então o conjunto trancado não é lido nem na validação.

mod calibracao;
mod metricas;

pub use calibracao::{calibrar, Observacao, ProbabilidadeDezena, CONCURSOS_FICTICIOS};
pub use metricas::{
    avaliar, brier, log_loss, metricas_por_concurso, resumir, Comparacao, Intervalo,
    MetricasConcurso, ResumoAvaliacao, LIMITE_PROBABILIDADE, Z_95,
};

use crate::config::LoteriaConfig;

/// Probabilidade de cada dezena sair num sorteio ao acaso: *K/N*
/// (Lotofácil 15/25, Mega-Sena 6/60, Quina 5/80), derivada do
/// `LoteriaConfig`.
pub fn taxa_do_acaso(cfg: &LoteriaConfig) -> f64 {
    cfg.qtd_dezenas as f64 / f64::from(cfg.qtd_dezenas_possiveis())
}

#[cfg(test)]
mod tests;
