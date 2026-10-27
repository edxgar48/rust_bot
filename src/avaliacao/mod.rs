//! Avaliação do índice de força contra o acaso.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "1c — Avaliação" (decisões de
//! 2026-10-05). Premissa: os sorteios são independentes, então o esperado de
//! qualquer perfil é **empatar** com o acaso; ganho aparente é suspeita de
//! vazamento até prova em contrário.
//!
//! Funções puras: sem I/O, sem SQL e sem nada específico de uma loteria além
//! do que vem em `LoteriaConfig`. Quem lê o banco (índice + `saiu` das
//! features) monta as [`Observacao`]es e chama [`walk_forward`] (ou, só
//! quando pedido, `avaliar_trancado`).
//!
//! Ponto no tempo: a probabilidade do concurso `t` ([`calibrar`]) usa só os
//! concursos anteriores a `t`; as avaliações descartam tudo acima do corte
//! antes de qualquer cálculo, então o conjunto trancado não é lido nem na
//! validação.
//!
//! Corte da busca e trancado (1c.3): o caminho padrão é [`walk_forward`]
//! (com o veredito em [`veredito_da_busca`]), que tira o corte de
//! `LoteriaConfig::ultimo_concurso_busca`; o trancado só entra por
//! `avaliar_trancado` e `veredito_do_trancado`, pedido explicitamente.
//! As funções com corte ou limites recebidos como argumento (`avaliar`,
//! `avaliar_entre`, `avaliar_topk`…) são blocos de construção, só visíveis
//! dentro do crate (1c.5): em produção, o corte vem sempre do config, nunca
//! `u32::MAX`.
//!
//! Veredito "supera o acaso" (1c.5): bootstrap circular por blocos
//! ([`bootstrap_circular`]) sobre a série por concurso perfil − acaso do log
//! loss, com gerador próprio ([`SplitMix64`]) e semente fixa.
//!
//! Testes de aleatoriedade do histórico (1c.4, [`testar_aleatoriedade`]):
//! qui-quadrado de frequência e runs test por dezena com Holm, sobre
//! `resultados` inteiro (não usam índice nem perfil).
//!
//! Evidência bayesiana por dezena (1d.2, [`evidencia_por_dezena`]): fator de
//! Bayes em cinco graus, de fórmula fechada, sobre `resultados` até `t−1`.
//!
//! Evidência bayesiana do índice de força (1d.3, [`evidencia_do_indice`]): o
//! mesmo número da diferença de log loss do walk-forward (dobras de teste 2 a
//! 5), em outra escala e em graus, ao lado do esperado de um índice inerte
//! ([`esperado_indice_inerte`]).
//!
//! Calibração linear na posição (1e.1, [`calibrar_linear`]): um parâmetro em
//! vez de *N*, para uma régua que distingue "sem sinal" de "sinal fraco".
//! A calibração por posição da 1c ([`calibrar`]) não muda.
//!
//! Taxas históricas por posição e por (componente, faixa) (1f.1,
//! [`calcular_taxas`]): a taxa da 1c por posição mais a taxa de cada tercil
//! dos 9 componentes, só com o passado, em contagens incrementais.
//!
//! Régua sensível (1e.2, [`Regua`]): B e o índice inerte passam a usar a
//! calibração linear; o `sync avaliar` ganha uma linha só informativa de log
//! loss com ela ([`log_loss_linear_da_busca`]).

mod aleatoriedade;
mod bootstrap;
mod calibracao;
mod evidencia;
mod evidencia_indice;
mod metricas;
mod posicoes;
mod topk;
mod walk_forward;

// Blocos de construção com corte ou limites como argumento: visíveis só no
// crate (1c.5) e, como nenhum fluxo de produção os chama (o corte vem do
// config via `walk_forward`/`avaliar_trancado`), reexportados só para os
// testes.
#[cfg(test)]
pub(crate) use metricas::{avaliar, avaliar_entre};
#[cfg(test)]
pub(crate) use topk::{acertos_por_concurso_entre, avaliar_topk, avaliar_topk_entre};

pub use aleatoriedade::{
    bonferroni, holm, p_valor_qui_quadrado, qui_quadrado_frequencia, testar_aleatoriedade,
    teste_de_runs, QuiQuadradoFrequencia, ResumoAleatoriedade, RunsDezena, TesteRuns, ALFA,
};
pub use bootstrap::{
    bootstrap_circular, concursos_minimos, Bootstrap, SplitMix64, BLOCO_BOOTSTRAP,
    REAMOSTRAGENS_BOOTSTRAP, SEMENTE_BOOTSTRAP,
};
pub use calibracao::{
    calibrar, calibrar_linear, Observacao, ProbabilidadeDezena, Regua, CONCURSOS_FICTICIOS,
};
pub use evidencia::{
    classificar, esperado_sob_o_acaso, evidencia_por_dezena, ln_gamma, log10_fator_bayes,
    teto_a_favor_do_acaso, Classificacao, Direcao, EsperadoAcaso, EsperadoPriori, EvidenciaDezena,
    EvidenciaJanela, Grade, Grau, Janela, Priori, DESVIOS_PRIORI, JANELA_SECUNDARIA, QTD_PRIORIS,
    REPLICAS_ESPERADO, SEMENTE_ESPERADO,
};
pub use evidencia_indice::{
    classificar_indice, esperado_indice_inerte, evidencia_do_indice, evidencia_do_indice_com,
    DirecaoIndice, DobraEvidenciaIndice, EvidenciaIndice, IndiceInerte, PRIMEIRA_DOBRA_DE_TESTE,
    REPLICAS_INERTE, SEMENTE_INERTE,
};
pub use metricas::{
    brier, log_loss, metricas_por_concurso, resumir, Comparacao, Intervalo, MetricasConcurso,
    ResumoAvaliacao, LIMITE_PROBABILIDADE, Z_95,
};
pub use posicoes::{
    calcular_taxas, faixas_do_componente, Contagem, Faixa, LinhaTaxas, ObservacaoPosicoes,
    SerieTaxas, TaxasAcumuladas, NOMES_COMPONENTES, QTD_COMPONENTES, QTD_FAIXAS,
};
pub use topk::{
    acertos_por_concurso, binomial, p_valor_soma, resumir_topk, AcertosConcurso, Cauda,
    FaixaAcertos, Hipergeometrica, ResumoTopK,
};
pub use walk_forward::{
    dividir_em_dobras, dobras_da_busca, faixa_da_busca, log_loss_linear_da_busca,
    veredito_da_busca, walk_forward, AvaliacaoFaixa, BootstrapTrecho, FaixaConcursos,
    LogLossLinear, VereditoTrecho, WalkForward, QTD_DOBRAS,
};
// O trancado só é avaliado pela orquestração, que passa pelo registro de uso
// único (`registros`); fora do crate não há como avaliá-lo sem o registro.
// `avaliar_trancado` (avaliação por faixa do trancado) só existe nos testes.
pub(crate) use walk_forward::veredito_do_trancado;

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
