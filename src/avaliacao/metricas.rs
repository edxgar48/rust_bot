//! Log loss, Brier e o resumo perfil × acaso com incerteza por concurso.
//!
//! Regras em `docs/PLANEJAMENTO.md`, 1c (decisões de 2026-10-05):
//! - log loss com logaritmo natural, `p` limitado a
//!   [[`LIMITE_PROBABILIDADE`], 1 − [`LIMITE_PROBABILIDADE`]];
//! - Brier = (p − saiu)², sem limitar `p`;
//! - as duas são médias por par (concurso, dezena);
//! - a incerteza vem da série **por concurso** `d_t` = média do concurso no
//!   perfil − média no acaso, porque as dezenas de um mesmo concurso não são
//!   independentes (somam exatamente *K* saídas).

use super::calibracao::{calibrar, Observacao};
use super::{ate_o_corte, concursos_avaliados, taxa_do_acaso};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Limite de `p` no log loss, para `ln(0)` não virar infinito.
pub const LIMITE_PROBABILIDADE: f64 = 1e-15;

/// Quantil da normal padrão para o intervalo de 95%.
pub const Z_95: f64 = 1.96;

/// Log loss de um par: `−ln p` se saiu, `−ln(1 − p)` se não saiu, com `p`
/// limitado a [`LIMITE_PROBABILIDADE`] e `1 − LIMITE_PROBABILIDADE`.
pub fn log_loss(p: f64, saiu: bool) -> f64 {
    let p = p.clamp(LIMITE_PROBABILIDADE, 1.0 - LIMITE_PROBABILIDADE);
    if saiu {
        -p.ln()
    } else {
        -(1.0 - p).ln()
    }
}

/// Brier de um par: `(p − saiu)²`, com `saiu` = 1 ou 0.
pub fn brier(p: f64, saiu: bool) -> f64 {
    let erro = p - if saiu { 1.0 } else { 0.0 };
    erro * erro
}

/// Médias de um concurso avaliado, sobre as suas *N* dezenas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricasConcurso {
    pub concurso: u32,
    pub log_loss_perfil: f64,
    pub log_loss_acaso: f64,
    pub brier_perfil: f64,
    pub brier_acaso: f64,
}

/// Intervalo `[inferior, superior]`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Intervalo {
    pub inferior: f64,
    pub superior: f64,
}

/// Uma métrica do perfil contra a do acaso. Em log loss e Brier menor é
/// melhor (`diferenca` negativa = perfil melhor que o acaso); nos acertos do
/// top-k ([`super::ResumoTopK`]) maior é melhor (`diferenca` positiva =
/// perfil acima do acaso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparacao {
    /// Média no perfil: por par (concurso, dezena) em log loss e Brier, por
    /// concurso nos acertos do top-k.
    pub perfil: f64,
    /// Mesma média no acaso (*K/N* em todas as dezenas; *k·K/N* acertos no
    /// top-k).
    pub acaso: f64,
    /// Média de `d_t` = perfil − acaso por concurso (igual a
    /// `perfil − acaso`, a menos de arredondamento).
    pub diferenca: f64,
    /// Desvio amostral (÷(T−1)) de `d_t` dividido por √T; `None` com menos
    /// de 2 concursos.
    pub erro_padrao: Option<f64>,
    /// `diferenca ± Z_95 · erro_padrao` (aproximação normal); `None` junto
    /// com o erro padrão.
    pub ic95: Option<Intervalo>,
}

/// Resumo da avaliação de um perfil num trecho da série.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumoAvaliacao {
    /// T: quantos concursos entraram (cada um com *N* pares).
    pub concursos_avaliados: usize,
    pub primeiro_concurso: u32,
    pub ultimo_concurso: u32,
    pub log_loss: Comparacao,
    pub brier: Comparacao,
}

/// Métricas por concurso avaliado, em ordem de concurso.
///
/// Antes de tudo descarta as observações acima de `corte` (inclusive o
/// conjunto trancado, quando o corte é o da busca): nada acima do corte é
/// lido, nem para validar. Depois calibra ([`calibrar`]) e avalia os
/// concursos do segundo da entrada até `corte`, só os já sorteados. O
/// primeiro fica de fora porque recebe o acaso por construção; mas entra na
/// calibração dos seguintes, assim como qualquer concurso anterior.
pub fn metricas_por_concurso(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    corte: u32,
) -> Result<Vec<MetricasConcurso>> {
    let probabilidades = calibrar(cfg, &ate_o_corte(observacoes, corte))?;
    let acaso = taxa_do_acaso(cfg);

    // Do segundo concurso da entrada em diante, só os sorteados: o snapshot
    // ainda não sorteado (`saiu` NULL no concurso inteiro, validado em
    // `calibrar`) fica de fora.
    let grupos = concursos_avaliados(&probabilidades, |p| p.concurso, |p| p.saiu.is_some());

    let mut saida = Vec::new();
    for grupo in grupos {
        let mut soma = [0.0_f64; 4];
        let mut pares = 0_u32;
        for p in grupo {
            // Defesa: inalcançável após `concursos_avaliados` (só grupos
            // sorteados, com `saiu` todo conhecido pela validação).
            let Some(saiu) = p.saiu else { continue };
            soma[0] += log_loss(p.probabilidade, saiu);
            soma[1] += log_loss(acaso, saiu);
            soma[2] += brier(p.probabilidade, saiu);
            soma[3] += brier(acaso, saiu);
            pares += 1;
        }
        // Defesa: inalcançável após `concursos_avaliados` (grupo não vazio,
        // todo sorteado).
        if pares == 0 {
            continue;
        }
        let n = f64::from(pares);
        saida.push(MetricasConcurso {
            concurso: grupo[0].concurso,
            log_loss_perfil: soma[0] / n,
            log_loss_acaso: soma[1] / n,
            brier_perfil: soma[2] / n,
            brier_acaso: soma[3] / n,
        });
    }
    Ok(saida)
}

/// Resume uma série de métricas por concurso. Como todo concurso tem as
/// mesmas *N* dezenas, a média das médias por concurso é a média por par.
/// Série vazia é [`SpiderError::AvaliacaoSemConcursos`].
pub fn resumir(serie: &[MetricasConcurso]) -> Result<ResumoAvaliacao> {
    let (Some(primeiro), Some(ultimo)) = (
        serie.iter().map(|m| m.concurso).min(),
        serie.iter().map(|m| m.concurso).max(),
    ) else {
        return Err(SpiderError::AvaliacaoSemConcursos(
            "a série de métricas por concurso está vazia".to_string(),
        ));
    };
    let log_loss: Vec<(f64, f64)> = serie
        .iter()
        .map(|m| (m.log_loss_perfil, m.log_loss_acaso))
        .collect();
    let brier: Vec<(f64, f64)> = serie
        .iter()
        .map(|m| (m.brier_perfil, m.brier_acaso))
        .collect();
    Ok(ResumoAvaliacao {
        concursos_avaliados: serie.len(),
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        log_loss: comparar(&log_loss),
        brier: comparar(&brier),
    })
}

/// [`metricas_por_concurso`] seguido de [`resumir`]. Sem nenhum concurso
/// sorteado entre o segundo da entrada e `corte` é
/// [`SpiderError::AvaliacaoSemConcursos`].
pub fn avaliar(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    corte: u32,
) -> Result<ResumoAvaliacao> {
    let serie = metricas_por_concurso(cfg, observacoes, corte)?;
    if serie.is_empty() {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado entre o segundo concurso do índice e o corte {corte}"
        )));
    }
    resumir(&serie)
}

/// `pares` = (média do concurso no perfil, média no acaso), não vazio.
/// Somas em sequência, na ordem da série: mesma entrada, mesmos bits.
pub(super) fn comparar(pares: &[(f64, f64)]) -> Comparacao {
    let t = pares.len() as f64;
    let perfil = pares.iter().map(|&(p, _)| p).sum::<f64>() / t;
    let acaso = pares.iter().map(|&(_, a)| a).sum::<f64>() / t;
    let diferencas: Vec<f64> = pares.iter().map(|&(p, a)| p - a).collect();
    let diferenca = diferencas.iter().sum::<f64>() / t;

    let erro_padrao = (pares.len() >= 2).then(|| {
        let soma_quadrados: f64 = diferencas
            .iter()
            .map(|d| (d - diferenca) * (d - diferenca))
            .sum();
        (soma_quadrados / (t - 1.0)).sqrt() / t.sqrt()
    });
    let ic95 = erro_padrao.map(|ep| Intervalo {
        inferior: diferenca - Z_95 * ep,
        superior: diferenca + Z_95 * ep,
    });
    Comparacao {
        perfil,
        acaso,
        diferenca,
        erro_padrao,
        ic95,
    }
}
