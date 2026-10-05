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
//! concursos anteriores a `t`; [`avaliar`] e [`avaliar_topk`] descartam tudo
//! acima do corte antes de qualquer cálculo, então o conjunto trancado não é
//! lido nem na validação.

mod calibracao;
mod metricas;
mod topk;

pub use calibracao::{calibrar, Observacao, ProbabilidadeDezena, CONCURSOS_FICTICIOS};
pub use metricas::{
    avaliar, brier, log_loss, metricas_por_concurso, resumir, Comparacao, Intervalo,
    MetricasConcurso, ResumoAvaliacao, LIMITE_PROBABILIDADE, Z_95,
};
pub use topk::{
    acertos_por_concurso, avaliar_topk, binomial, p_valor_soma, resumir_topk, AcertosConcurso,
    Cauda, FaixaAcertos, Hipergeometrica, ResumoTopK,
};

use crate::config::LoteriaConfig;

/// Probabilidade de cada dezena sair num sorteio ao acaso: *K/N*
/// (Lotofácil 15/25, Mega-Sena 6/60, Quina 5/80), derivada do
/// `LoteriaConfig`.
pub fn taxa_do_acaso(cfg: &LoteriaConfig) -> f64 {
    cfg.qtd_dezenas as f64 / f64::from(cfg.qtd_dezenas_possiveis())
}

/// Observações com concurso ≤ `corte`, na ordem da entrada. É o primeiro
/// passo de toda avaliação: o que está acima do corte (o conjunto trancado,
/// quando o corte é o da busca) não chega nem à validação.
fn ate_o_corte(observacoes: &[Observacao], corte: u32) -> Vec<Observacao> {
    observacoes
        .iter()
        .filter(|o| o.concurso <= corte)
        .copied()
        .collect()
}

/// Os concursos avaliados de uma série já ordenada por concurso e validada
/// (`ordenar_e_validar`), um grupo de linhas por concurso: do **segundo**
/// concurso da série em diante, só os já sorteados. O primeiro fica de fora
/// porque recebe o acaso por construção na calibração; a regra vale igual
/// para log loss/Brier (1c.1) e para o top-k (1c.2), para as duas avaliarem
/// o mesmo trecho.
///
/// A validação garante que `saiu` é todo conhecido ou todo `None` em cada
/// concurso, então basta olhar a primeira linha do grupo.
fn concursos_avaliados<T>(
    linhas: &[T],
    concurso: fn(&T) -> u32,
    sorteado: fn(&T) -> bool,
) -> impl Iterator<Item = &[T]> {
    linhas
        .chunk_by(move |a, b| concurso(a) == concurso(b))
        .skip(1)
        .filter(move |grupo| sorteado(&grupo[0]))
}

#[cfg(test)]
mod tests;
