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
//! - o trancado (`avaliar_trancado`) só é avaliado quando pedido: do
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
//!
//! Veredito "supera o acaso" (1c.5, [`veredito_da_busca`] e
//! `veredito_do_trancado`): sobre o trecho **inteiro** (na busca, a união
//! das dobras; no trancado, o próprio trancado), com o bootstrap circular por
//! blocos ([`super::bootstrap`]) da série por concurso perfil − acaso do log
//! loss; o Brier passa pelo mesmo bootstrap só para informar. As dobras do
//! [`walk_forward`] mostram a estabilidade, não decidem. Os limites do trecho
//! vêm do config, como no walk-forward e no trancado.

use super::aleatoriedade::ALFA;
use super::bootstrap::{
    bootstrap_circular, concursos_minimos, Bootstrap, BLOCO_BOOTSTRAP, REAMOSTRAGENS_BOOTSTRAP,
    SEMENTE_BOOTSTRAP,
};
use super::calibracao::{ordenar_e_validar, Observacao, Regua};
use super::metricas::{
    metricas_por_concurso_entre, metricas_por_concurso_entre_com, resumir, MetricasConcurso,
    ResumoAvaliacao,
};
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
        .map(|dobra| Ok(avaliar_faixa(cfg, observacoes, k, dobra)?.0))
        .collect::<Result<Vec<_>>>()?;
    Ok(WalkForward {
        corte: cfg.ultimo_concurso_busca,
        dobras,
    })
}

/// Log loss (perfil − acaso) com a calibração **linear** da 1e, nas mesmas
/// dobras e no mesmo trecho do [`walk_forward`]. Só informativo: o veredito, o
/// bootstrap e o top-k seguem com a calibração pela posição.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogLossLinear {
    /// Uma por dobra do [`walk_forward`], na mesma ordem. Traz também o Brier
    /// (de graça, mesma série); quem imprime mostra só o log loss.
    pub dobras: Vec<ResumoAvaliacao>,
    /// O trecho de busca inteiro (a união das dobras).
    pub total: ResumoAvaliacao,
}

/// [`LogLossLinear`] do trecho de busca: cada dobra de [`dobras_da_busca`] e o
/// trecho inteiro, com [`Regua::Linear`] calibrada desde o primeiro concurso
/// da entrada (só com o passado de cada concurso). Como o corte vem do config,
/// nada acima dele é lido, nem para validar.
///
/// Erros: os de [`dobras_da_busca`] (série inválida, menos concursos que
/// dobras) e os da calibração linear.
pub fn log_loss_linear_da_busca(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<LogLossLinear> {
    let faixa = faixa_da_busca(cfg, observacoes)?;
    let resumo = |faixa: FaixaConcursos| {
        let serie = metricas_por_concurso_entre_com(
            Regua::Linear,
            cfg,
            observacoes,
            faixa.primeiro,
            faixa.ultimo,
        )?;
        // Defesa: a faixa vem de `faixa_avaliada`, então a série a cobre.
        if serie.len() as u64 != faixa.tamanho() {
            return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
                "faixa {}..={}: {} concurso(s) na calibração linear, esperados {}",
                faixa.primeiro,
                faixa.ultimo,
                serie.len(),
                faixa.tamanho()
            )));
        }
        resumir(&serie)
    };
    let dobras = dividir_em_dobras(faixa, QTD_DOBRAS)?
        .into_iter()
        .map(resumo)
        .collect::<Result<Vec<_>>>()?;
    Ok(LogLossLinear {
        dobras,
        total: resumo(faixa)?,
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
#[cfg(test)]
pub(crate) fn avaliar_trancado(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
) -> Result<AvaliacaoFaixa> {
    validar_k(cfg, k)?;
    let faixa = faixa_do_trancado(cfg, observacoes)?;
    Ok(avaliar_faixa(cfg, observacoes, k, faixa)?.0)
}

/// Trecho avaliado do trancado: do concurso seguinte ao corte da busca até o
/// último sorteado (o snapshot ainda não sorteado fica de fora). Nenhum
/// concurso sorteado acima do corte é [`SpiderError::AvaliacaoSemConcursos`].
fn faixa_do_trancado(cfg: &LoteriaConfig, observacoes: &[Observacao]) -> Result<FaixaConcursos> {
    let corte = cfg.ultimo_concurso_busca;
    // O trancado vai até o último concurso da entrada (é o único caminho que
    // lê acima do corte); o limite é o maior concurso recebido, não um
    // sentinela como `u32::MAX`.
    let ultimo_da_entrada = observacoes.iter().map(|o| o.concurso).max();
    match (corte.checked_add(1), ultimo_da_entrada) {
        (Some(inicio), Some(fim)) if inicio <= fim => {
            faixa_avaliada(cfg, observacoes, inicio, fim)?
        }
        _ => None,
    }
    .ok_or_else(|| {
        SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado no conjunto trancado da {} (do concurso seguinte ao corte {corte} até o último sorteado; o primeiro concurso da entrada não é avaliado)",
            cfg.nome_exibicao
        ))
    })
}

/// Bootstrap do veredito: log loss (decide) e Brier (só informa), com as
/// mesmas reamostras (mesmo T e mesma semente).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BootstrapTrecho {
    /// Sobre `d_t` = log loss do perfil − log loss do acaso, por concurso.
    pub log_loss: Bootstrap,
    /// Sobre `d_t` = Brier do perfil − Brier do acaso, por concurso.
    pub brier: Bootstrap,
}

/// Veredito de um trecho inteiro (a união das dobras na busca, ou o
/// trancado).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VereditoTrecho {
    /// Log loss/Brier e top-k do trecho inteiro, como numa dobra: o p-valor
    /// exato do top-k (1c.2) é o do trecho todo.
    pub avaliacao: AvaliacaoFaixa,
    /// `None` quando o trecho tem menos de `concursos_minimos` concursos: o
    /// bootstrap por blocos não estima a incerteza e não há veredito.
    pub bootstrap: Option<BootstrapTrecho>,
    /// Dois blocos de [`BLOCO_BOOTSTRAP`] concursos.
    pub concursos_minimos: usize,
}

impl VereditoTrecho {
    /// "Supera o acaso" = p-valor do bootstrap do log loss ≤ [`ALFA`]
    /// (`None` sem bootstrap). Não corrige por múltiplas comparações: quem
    /// testou vários perfis ou k aplica a correção ao p-valor
    /// ([`super::bonferroni`]).
    pub fn supera_o_acaso(&self) -> Option<bool> {
        self.bootstrap.as_ref().map(|b| b.log_loss.p_valor <= ALFA)
    }
}

/// Veredito do trecho de **busca** inteiro (o mesmo das dobras do
/// [`walk_forward`]: do segundo concurso da entrada até o corte do config, só
/// os sorteados). Nada acima do corte é lido.
///
/// Erros: os de [`walk_forward`] (série inválida, `k` fora de `1..=N−1`,
/// nenhum concurso na busca). Trecho curto demais para o bootstrap não é
/// erro: sai sem veredito (`bootstrap` = `None`).
pub fn veredito_da_busca(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
) -> Result<VereditoTrecho> {
    validar_k(cfg, k)?;
    let faixa = faixa_da_busca(cfg, observacoes)?;
    veredito_da_faixa(cfg, observacoes, k, faixa)
}

/// Veredito do conjunto **trancado** inteiro (o mesmo trecho de
/// `avaliar_trancado`, com a calibração usando todo o passado). Só deve
/// ser chamado quando o trancado for pedido explicitamente; o registro do
/// uso fica com quem chama.
///
/// Erros: os de `avaliar_trancado`. Trecho curto demais para o bootstrap
/// sai sem veredito (`bootstrap` = `None`).
pub(crate) fn veredito_do_trancado(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
) -> Result<VereditoTrecho> {
    validar_k(cfg, k)?;
    let faixa = faixa_do_trancado(cfg, observacoes)?;
    veredito_da_faixa(cfg, observacoes, k, faixa)
}

fn veredito_da_faixa(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
    faixa: FaixaConcursos,
) -> Result<VereditoTrecho> {
    let (avaliacao, serie) = avaliar_faixa(cfg, observacoes, k, faixa)?;
    let minimo = concursos_minimos(BLOCO_BOOTSTRAP);
    let bootstrap = if serie.len() >= minimo {
        let log_loss: Vec<f64> = serie
            .iter()
            .map(|m| m.log_loss_perfil - m.log_loss_acaso)
            .collect();
        let brier: Vec<f64> = serie
            .iter()
            .map(|m| m.brier_perfil - m.brier_acaso)
            .collect();
        let reamostrar = |d: &[f64]| {
            bootstrap_circular(
                d,
                BLOCO_BOOTSTRAP,
                REAMOSTRAGENS_BOOTSTRAP,
                SEMENTE_BOOTSTRAP,
            )
        };
        Some(BootstrapTrecho {
            log_loss: reamostrar(&log_loss)?,
            brier: reamostrar(&brier)?,
        })
    } else {
        None
    };
    Ok(VereditoTrecho {
        avaliacao,
        bootstrap,
        concursos_minimos: minimo,
    })
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
/// `faixa.ultimo` e calibrando desde o primeiro concurso da entrada. Devolve
/// também a série de métricas por concurso (a do bootstrap do veredito).
fn avaliar_faixa(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
    faixa: FaixaConcursos,
) -> Result<(AvaliacaoFaixa, Vec<MetricasConcurso>)> {
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
    let avaliacao = AvaliacaoFaixa {
        faixa,
        metricas: resumir(&metricas)?,
        topk: resumir_topk(cfg, k, &acertos)?,
    };
    Ok((avaliacao, metricas))
}

#[cfg(test)]
mod tests;
