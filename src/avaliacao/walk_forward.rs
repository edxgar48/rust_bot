//! Walk-forward no trecho de busca e avaliação do conjunto trancado (1c.3).
//!
//! Regras em `docs/PLANEJAMENTO.md`, 1c, "Validação walk-forward" e
//! "Conjunto final trancado" (decisões de 2026-10-05):
//! - o corte vem de `LoteriaConfig::ultimo_concurso_busca`, nunca de um
//!   argumento: o caminho padrão (busca) não tem como ler o trancado;
//! - walk-forward = o trecho avaliado da busca (do segundo concurso do índice
//!   até o corte, só os sorteados) em [`QTD_DOBRAS`] dobras **consecutivas**
//!   de tamanho igual, a sobra da divisão na última ([`dividir_em_dobras`]);
//!   em cada dobra, log loss/Brier (1c.1) e top-k com p-valor (1c.2) contra o
//!   acaso. O tempo nunca é embaralhado;
//! - a calibração começa sempre no primeiro concurso da entrada e usa todo o
//!   passado; a dobra só define quais concursos entram nas métricas
//!   ([`super::metricas_por_concurso_entre`]);
//! - o trancado ([`avaliar_trancado`]) só é avaliado quando pedido: do
//!   corte + 1 até o último sorteado, com a calibração usando todo o passado
//!   (inclusive o trecho de busca). Registrar o "uma única vez" fica na 1c.5.
//!
//! Ponto no tempo: cada dobra é avaliada sem ler nada acima do seu último
//! concurso (nem para validar); a divisão em dobras depende só dos números
//! dos concursos sorteados até o corte, nunca de `saiu` nem de posições.
//!
//! Sobre o p-valor por dobra: vale a mesma condição da 1c.2 (perfil e k
//! escolhidos sem ver os sorteios da dobra). Numa busca de pesos que ajusta
//! até uma dobra e testa nela (1.5.3), escolher o melhor entre muitos perfis
//! deixa o p-valor otimista; o trancado existe para isso.

use super::calibracao::{ordenar_e_validar, Observacao};
use super::metricas::{metricas_por_concurso_entre, resumir, ResumoAvaliacao};
use super::topk::{acertos_por_concurso_entre, resumir_topk, validar_k, ResumoTopK};
use super::{ate_o_corte, concursos_avaliados};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Quantas dobras consecutivas o walk-forward usa no trecho de busca. A
/// busca de pesos da 1.5.3 usa as mesmas.
pub const QTD_DOBRAS: u32 = 5;

/// Faixa inclusiva de concursos `primeiro..=ultimo`: uma dobra do
/// walk-forward ou o trecho avaliado do trancado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FaixaConcursos {
    pub primeiro: u32,
    pub ultimo: u32,
}

impl FaixaConcursos {
    /// Quantos concursos a faixa cobre (0 se `primeiro > ultimo`).
    pub const fn tamanho(&self) -> u64 {
        if self.primeiro > self.ultimo {
            0
        } else {
            self.ultimo as u64 - self.primeiro as u64 + 1
        }
    }

    pub const fn contem(&self, concurso: u32) -> bool {
        concurso >= self.primeiro && concurso <= self.ultimo
    }
}

/// Divide `faixa` em `qtd` dobras consecutivas de `⌊T/qtd⌋` concursos, com
/// a sobra `T mod qtd` na última (T = `faixa.tamanho()`). Em ordem, sem
/// buraco nem sobreposição, cobrindo a faixa inteira.
///
/// Depende só dos limites, então quem tem uma série de concursos seguidos
/// (a avaliação, o dataset) chega às mesmas dobras.
///
/// - `qtd` = 0 é [`SpiderError::AvaliacaoEntradaInvalida`];
/// - menos de `qtd` concursos (alguma dobra ficaria vazia), inclusive faixa
///   vazia, é [`SpiderError::AvaliacaoSemConcursos`].
pub fn dividir_em_dobras(faixa: FaixaConcursos, qtd: u32) -> Result<Vec<FaixaConcursos>> {
    if qtd == 0 {
        return Err(SpiderError::AvaliacaoEntradaInvalida(
            "walk-forward com 0 dobras".to_string(),
        ));
    }
    let total = faixa.tamanho();
    if total < u64::from(qtd) {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "a faixa {}..={} tem {total} concurso(s), menos que as {qtd} dobras do walk-forward (cada dobra precisa de ao menos um concurso)",
            faixa.primeiro, faixa.ultimo
        )));
    }
    // total ≥ qtd ≥ 1 e total ≤ u32::MAX + 1; tamanho ≥ 1. Com a faixa
    // cobrindo todo u32 (total = 2^32) as contas abaixo seguem em u64.
    let tamanho = total / u64::from(qtd);
    let primeiro = u64::from(faixa.primeiro);
    Ok((0..u64::from(qtd))
        .map(|i| {
            let inicio = primeiro + i * tamanho;
            let fim = if i + 1 == u64::from(qtd) {
                u64::from(faixa.ultimo)
            } else {
                inicio + tamanho - 1
            };
            // Ambos ≤ faixa.ultimo, então cabem em u32.
            FaixaConcursos {
                primeiro: inicio as u32,
                ultimo: fim as u32,
            }
        })
        .collect())
}

/// Trecho avaliado da busca: do segundo concurso da entrada (o segundo do
/// índice) até `cfg.ultimo_concurso_busca`, só os sorteados. Nada acima do
/// corte é lido, nem para validar. Sem nenhum concurso nesse trecho é
/// [`SpiderError::AvaliacaoSemConcursos`].
pub fn faixa_da_busca(cfg: &LoteriaConfig, observacoes: &[Observacao]) -> Result<FaixaConcursos> {
    let corte = cfg.ultimo_concurso_busca;
    faixa_avaliada(cfg, observacoes, 0, corte)?.ok_or_else(|| {
        SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado do segundo concurso da entrada até o corte da busca da {} ({corte}); o primeiro concurso da entrada recebe o acaso por construção e não é avaliado",
            cfg.nome_exibicao
        ))
    })
}

/// As [`QTD_DOBRAS`] dobras do walk-forward: [`faixa_da_busca`] dividida
/// por [`dividir_em_dobras`]. A última termina no corte quando ele já foi
/// sorteado.
pub fn dobras_da_busca(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<Vec<FaixaConcursos>> {
    dividir_em_dobras(faixa_da_busca(cfg, observacoes)?, QTD_DOBRAS)
}

/// Avaliação de uma faixa de concursos contra o acaso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvaliacaoFaixa {
    pub faixa: FaixaConcursos,
    /// Log loss e Brier (1c.1), com a calibração desde o primeiro concurso.
    pub metricas: ResumoAvaliacao,
    /// Acertos do top-k contra a hipergeométrica, com p-valor (1c.2).
    pub topk: ResumoTopK,
}

/// Resultado do walk-forward no trecho de busca.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalkForward {
    /// `cfg.ultimo_concurso_busca` usado como limite superior.
    pub corte: u32,
    /// [`QTD_DOBRAS`] dobras, em ordem de concurso.
    pub dobras: Vec<AvaliacaoFaixa>,
}

/// Walk-forward no trecho de busca (o caminho padrão da avaliação): cada
/// dobra de [`dobras_da_busca`] avaliada com log loss/Brier e top-`k`. A
/// dobra `i` é avaliada lendo só os concursos até o seu último (a calibração
/// desde o primeiro concurso da entrada); nada acima do corte da busca é lido.
///
/// Erros: série inválida até o corte ou `k` fora de `1..=N−1` são
/// [`SpiderError::AvaliacaoEntradaInvalida`]; menos de [`QTD_DOBRAS`]
/// concursos avaliáveis na busca é [`SpiderError::AvaliacaoSemConcursos`].
pub fn walk_forward(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
) -> Result<WalkForward> {
    validar_k(cfg, k)?;
    let dobras = dobras_da_busca(cfg, observacoes)?
        .into_iter()
        .map(|dobra| avaliar_faixa(cfg, observacoes, k, dobra))
        .collect::<Result<Vec<_>>>()?;
    Ok(WalkForward {
        corte: cfg.ultimo_concurso_busca,
        dobras,
    })
}

/// Avalia o conjunto **trancado**: do concurso seguinte ao corte da busca
/// até o último sorteado, com a calibração usando todo o passado, desde o
/// primeiro concurso da entrada (inclusive o trecho de busca). Só deve ser
/// chamado quando o trancado for pedido explicitamente; o registro de que
/// ele já foi usado fica com quem chama (1c.5).
///
/// Quem chama (a 1c.5) deve passar a série **desde o primeiro concurso do
/// índice**, não só o trecho trancado: a calibração usa apenas o que recebe.
/// Se a entrada começar acima do corte, a calibração parte do primeiro
/// concurso da entrada, sem o passado da busca, e esse primeiro concurso
/// (que recebe o acaso por construção) fica fora da avaliação.
///
/// Erros: série inválida ou `k` fora de `1..=N−1` são
/// [`SpiderError::AvaliacaoEntradaInvalida`]; nenhum concurso sorteado acima
/// do corte é [`SpiderError::AvaliacaoSemConcursos`].
pub fn avaliar_trancado(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
) -> Result<AvaliacaoFaixa> {
    validar_k(cfg, k)?;
    let corte = cfg.ultimo_concurso_busca;
    let faixa = match corte.checked_add(1) {
        Some(inicio) => faixa_avaliada(cfg, observacoes, inicio, u32::MAX)?,
        None => None,
    }
    .ok_or_else(|| {
        SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado no conjunto trancado da {} (do concurso seguinte ao corte {corte} até o último sorteado; o primeiro concurso da entrada não é avaliado)",
            cfg.nome_exibicao
        ))
    })?;
    avaliar_faixa(cfg, observacoes, k, faixa)
}

/// Primeiro e último concurso avaliados em `inicio..=fim` (mesma regra de
/// [`concursos_avaliados`]), lendo só até `fim`; `None` se não há nenhum.
/// Como a série validada não tem buraco e só o último concurso pode estar sem
/// resultado, os avaliados formam exatamente a faixa devolvida.
fn faixa_avaliada(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    inicio: u32,
    fim: u32,
) -> Result<Option<FaixaConcursos>> {
    let linhas = ordenar_e_validar(cfg, &ate_o_corte(observacoes, fim))?;
    let mut avaliados = concursos_avaliados(&linhas, inicio, |o| o.concurso, |o| o.saiu.is_some());
    let Some(primeiro) = avaliados.next() else {
        return Ok(None);
    };
    let primeiro = primeiro[0].concurso;
    let ultimo = avaliados.last().map_or(primeiro, |grupo| grupo[0].concurso);
    Ok(Some(FaixaConcursos { primeiro, ultimo }))
}

/// Log loss/Brier e top-`k` dos concursos de `faixa`, lendo só até
/// `faixa.ultimo` e calibrando desde o primeiro concurso da entrada.
fn avaliar_faixa(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
    faixa: FaixaConcursos,
) -> Result<AvaliacaoFaixa> {
    let metricas = metricas_por_concurso_entre(cfg, observacoes, faixa.primeiro, faixa.ultimo)?;
    let acertos = acertos_por_concurso_entre(cfg, observacoes, k, faixa.primeiro, faixa.ultimo)?;
    // Defesa: a faixa vem de `faixa_avaliada` com a mesma regra, então as
    // duas séries cobrem a faixa inteira.
    if metricas.len() as u64 != faixa.tamanho() || acertos.len() as u64 != faixa.tamanho() {
        return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
            "faixa {}..={}: {} concurso(s) nas métricas e {} no top-k, esperados {}",
            faixa.primeiro,
            faixa.ultimo,
            metricas.len(),
            acertos.len(),
            faixa.tamanho()
        )));
    }
    Ok(AvaliacaoFaixa {
        faixa,
        metricas: resumir(&metricas)?,
        topk: resumir_topk(cfg, k, &acertos)?,
    })
}

#[cfg(test)]
mod tests;
