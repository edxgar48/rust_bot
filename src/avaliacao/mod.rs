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
//!
//! Corte da busca e trancado (1c.3): o caminho padrão é [`walk_forward`],
//! que tira o corte de `LoteriaConfig::ultimo_concurso_busca`; o trancado só
//! entra por [`avaliar_trancado`], pedido explicitamente. As funções com
//! corte ou limites recebidos como argumento (`avaliar`, `avaliar_entre`…)
//! são blocos de construção: em produção, o corte vem sempre do config,
//! nunca `u32::MAX`.
//!
//! Testes de aleatoriedade do histórico (1c.4, [`testar_aleatoriedade`]):
//! qui-quadrado de frequência e runs test por dezena com Holm, sobre
//! `resultados` inteiro (não usam índice nem perfil).

mod aleatoriedade;
mod calibracao;
mod metricas;
mod topk;
mod walk_forward;

pub use aleatoriedade::{
    holm, p_valor_qui_quadrado, p_valor_runs_exato, qui_quadrado_frequencia, testar_aleatoriedade,
    teste_de_runs, QuiQuadradoFrequencia, ResumoAleatoriedade, RunsDezena, TesteRuns, ALFA,
};
pub use calibracao::{calibrar, Observacao, ProbabilidadeDezena, CONCURSOS_FICTICIOS};
pub use metricas::{
    avaliar, avaliar_entre, brier, log_loss, metricas_por_concurso, metricas_por_concurso_entre,
    resumir, Comparacao, Intervalo, MetricasConcurso, ResumoAvaliacao, LIMITE_PROBABILIDADE, Z_95,
};
pub use topk::{
    acertos_por_concurso, acertos_por_concurso_entre, avaliar_topk, avaliar_topk_entre, binomial,
    p_valor_soma, resumir_topk, AcertosConcurso, Cauda, FaixaAcertos, Hipergeometrica, ResumoTopK,
};
pub use walk_forward::{
    avaliar_trancado, dividir_em_dobras, dobras_da_busca, faixa_da_busca, walk_forward,
    AvaliacaoFaixa, FaixaConcursos, WalkForward, QTD_DOBRAS,
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
/// concurso da série em diante, só os já sorteados e só a partir de `inicio`
/// (o limite inferior da avaliação, 1c.3). O primeiro da série fica de fora
/// porque recebe o acaso por construção na calibração; a regra vale igual
/// para log loss/Brier (1c.1) e para o top-k (1c.2), para as duas avaliarem
/// o mesmo trecho.
///
/// `inicio` só escolhe quais concursos entram nas métricas: quem chama
/// calibra a série inteira, desde o primeiro concurso, e o "segundo da série"
/// continua contado a partir do primeiro da entrada, não de `inicio`.
///
/// A validação garante que `saiu` é todo conhecido ou todo `None` em cada
/// concurso, então basta olhar a primeira linha do grupo.
fn concursos_avaliados<T>(
    linhas: &[T],
    inicio: u32,
    concurso: fn(&T) -> u32,
    sorteado: fn(&T) -> bool,
) -> impl Iterator<Item = &[T]> {
    linhas
        .chunk_by(move |a, b| concurso(a) == concurso(b))
        .skip(1)
        .filter(move |grupo| concurso(&grupo[0]) >= inicio && sorteado(&grupo[0]))
}

#[cfg(test)]
mod tests;
