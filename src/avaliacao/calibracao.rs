//! Índice → probabilidade: calibração pela posição, só com o passado.
//!
//! `p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)`
//! com `m` = [`CONCURSOS_FICTICIOS`] e o histórico inteiro, sem janela
//! (`docs/PLANEJAMENTO.md`, 1c, decisões de 2026-10-05). Os `m` concursos
//! fictícios puxam a taxa observada para o acaso, então o primeiro concurso
//! recebe exatamente *K/N* e as *N* probabilidades de cada concurso somam *K*
//! (cada concurso sorteado soma *K* saídas às contagens).
//!
//! Ordem do laço, como no motor: probabilidades de `t` → só então `t` entra
//! nas contagens. Nada de `t` (nem `saiu`, nem posições) nem de concursos
//! posteriores influencia `p_t(r)`.

use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Força do prior: quantos concursos fictícios, todos com a taxa do acaso,
/// entram na calibração antes do histórico real (`m` do plano).
pub const CONCURSOS_FICTICIOS: u32 = 100;

/// Uma dezena num concurso: a posição que o índice lhe deu e se saiu.
/// Montada por quem lê o banco a partir de `indice` (posição) e
/// `features_dezena` (`saiu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observacao {
    pub concurso: u32,
    pub dezena: u32,
    /// 1 … N no concurso (1 = mais forte), como em `IndiceDezena::posicao`.
    pub posicao: u32,
    /// `None` só no snapshot do próximo concurso, ainda não sorteado.
    pub saiu: Option<bool>,
}

/// Probabilidade calibrada de uma dezena sair num concurso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProbabilidadeDezena {
    pub concurso: u32,
    pub dezena: u32,
    pub posicao: u32,
    /// `p_t(posicao)`, calculada só com os concursos anteriores.
    pub probabilidade: f64,
    /// Copiado da entrada; não entra no cálculo de `probabilidade`.
    pub saiu: Option<bool>,
}

/// Calcula `p_t(r)` para cada (concurso, dezena) da entrada, inclusive o
/// snapshot ainda não sorteado. Devolve as linhas ordenadas por
/// (concurso, dezena).
///
/// A entrada pode vir em qualquer ordem, mas tem de formar uma série
/// completa; senão é [`SpiderError::AvaliacaoEntradaInvalida`] (regras em
/// `ordenar_e_validar`). Entrada vazia devolve vazio.
///
/// Só concursos sorteados entram nas contagens; o primeiro concurso da
/// entrada (o primeiro do índice) recebe a taxa do acaso.
pub fn calibrar(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<Vec<ProbabilidadeDezena>> {
    let linhas = ordenar_e_validar(cfg, observacoes)?;

    let n = u64::from(cfg.qtd_dezenas_possiveis());
    let k = cfg.qtd_dezenas as u64;
    let m = u64::from(CONCURSOS_FICTICIOS);
    // Contas em inteiros e uma única divisão no fim:
    // p = (s·N + m·K) / ((c + m)·N) é o mesmo valor do plano, e o f64 sai
    // correto até o último bit (inteiros < 2^53 são exatos). Em particular, o
    // primeiro concurso dá m·K / (m·N), que arredonda para o mesmo f64 que
    // `taxa_do_acaso` (K / N).
    let mut saidas_por_posicao = vec![0_u64; n as usize];
    let mut concursos_sorteados = 0_u64;

    let mut saida = Vec::with_capacity(linhas.len());
    for grupo in linhas.chunk_by(|a, b| a.concurso == b.concurso) {
        let denominador = ((concursos_sorteados + m) * n) as f64;
        for o in grupo {
            // `ordenar_e_validar` garante posicao em 1..=N.
            let s = saidas_por_posicao[(o.posicao - 1) as usize];
            saida.push(ProbabilidadeDezena {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                probabilidade: (s * n + m * k) as f64 / denominador,
                saiu: o.saiu,
            });
        }
        // Só agora o concurso entra nas contagens: as probabilidades acima
        // não viram o próprio resultado.
        if grupo[0].saiu.is_some() {
            for o in grupo.iter().filter(|o| o.saiu == Some(true)) {
                saidas_por_posicao[(o.posicao - 1) as usize] += 1;
            }
            concursos_sorteados += 1;
        }
    }
    Ok(saida)
}

/// Ordena por (concurso, dezena) e confere que a entrada é uma série
/// completa, que a calibração pode percorrer sem buraco:
/// - concursos seguidos, sem buraco;
/// - em cada concurso, exatamente as dezenas da loteria, uma vez cada;
/// - posições formando uma permutação de `1..=N`;
/// - `saiu` todo conhecido (com exatamente *K* saídas) ou todo `None`, e
///   `None` só no último concurso (o snapshot ainda não sorteado).
pub(super) fn ordenar_e_validar(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<Vec<Observacao>> {
    let mut linhas = observacoes.to_vec();
    // Instável e determinística: chaves repetidas são recusadas abaixo antes
    // de qualquer saída; sem repetidas, a ordem final não depende da entrada.
    linhas.sort_unstable_by_key(|o| (o.concurso, o.dezena));

    let n = cfg.qtd_dezenas_possiveis();
    let dezenas_esperadas: Vec<u32> = cfg.dezenas().collect();
    let mut grupos = linhas.chunk_by(|a, b| a.concurso == b.concurso).peekable();
    let mut anterior: Option<u32> = None;

    while let Some(grupo) = grupos.next() {
        let concurso = grupo[0].concurso;
        if let Some(ultimo) = anterior {
            if concurso != ultimo + 1 {
                let faltando = if concurso - 1 == ultimo + 1 {
                    format!("o concurso {}", ultimo + 1)
                } else {
                    format!("os concursos {}..={}", ultimo + 1, concurso - 1)
                };
                return Err(invalida(format!(
                    "faltam {faltando} (entre o {ultimo} e o {concurso})"
                )));
            }
        }
        anterior = Some(concurso);

        let dezenas: Vec<u32> = grupo.iter().map(|o| o.dezena).collect();
        conferir_dezenas(cfg, concurso, &dezenas, &dezenas_esperadas)?;

        let mut posicoes: Vec<u32> = grupo.iter().map(|o| o.posicao).collect();
        posicoes.sort_unstable();
        if let Some(par) = posicoes.windows(2).find(|par| par[0] == par[1]) {
            return Err(invalida(format!(
                "concurso {concurso}: posição {} repetida (as posições devem ser 1..={n}, uma por dezena)",
                par[0]
            )));
        }
        if let Some(fora) = posicoes.iter().find(|&&p| p == 0 || p > n) {
            return Err(invalida(format!(
                "concurso {concurso}: posição {fora} fora de 1..={n}"
            )));
        }

        let desconhecidos = grupo.iter().filter(|o| o.saiu.is_none()).count();
        if desconhecidos == grupo.len() {
            if grupos.peek().is_some() {
                return Err(invalida(format!(
                    "concurso {concurso} sem resultado (`saiu` NULL) no meio da série — só o último concurso, o snapshot do próximo sorteio, pode estar sem resultado"
                )));
            }
        } else if desconhecidos > 0 {
            return Err(invalida(format!(
                "concurso {concurso}: `saiu` conhecido em {} dezena(s) e NULL em {desconhecidos}",
                grupo.len() - desconhecidos
            )));
        } else {
            let saidas = grupo.iter().filter(|o| o.saiu == Some(true)).count();
            if saidas != cfg.qtd_dezenas {
                return Err(invalida(format!(
                    "concurso {concurso}: {saidas} dezena(s) com `saiu`, mas a {} sorteia {}",
                    cfg.nome_exibicao, cfg.qtd_dezenas
                )));
            }
        }
    }
    Ok(linhas)
}

/// `dezenas` vem ordenada; tem de ser exatamente a faixa da loteria.
fn conferir_dezenas(
    cfg: &LoteriaConfig,
    concurso: u32,
    dezenas: &[u32],
    esperadas: &[u32],
) -> Result<()> {
    if let Some(par) = dezenas.windows(2).find(|par| par[0] == par[1]) {
        return Err(invalida(format!(
            "concurso {concurso}: dezena {} repetida",
            par[0]
        )));
    }
    if let Some(fora) = dezenas.iter().find(|&&d| !cfg.contem(d)) {
        return Err(invalida(format!(
            "concurso {concurso}: dezena {fora} fora da faixa {}..={} da {}",
            cfg.dezena_min, cfg.dezena_max, cfg.nome_exibicao
        )));
    }
    if dezenas != esperadas {
        // Sem repetidas e dentro da faixa: só pode faltar dezena.
        let faltando: Vec<String> = esperadas
            .iter()
            .filter(|d| dezenas.binary_search(d).is_err())
            .map(u32::to_string)
            .collect();
        return Err(invalida(format!(
            "concurso {concurso}: faltam as dezenas {} ({} de {} presentes)",
            faltando.join(", "),
            dezenas.len(),
            esperadas.len()
        )));
    }
    Ok(())
}

fn invalida(detalhe: String) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe)
}
