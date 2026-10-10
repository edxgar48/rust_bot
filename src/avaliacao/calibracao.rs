//! Índice → probabilidade: duas calibrações pela posição, só com o passado.
//!
//! - [`calibrar`] (1c): uma taxa por posição, *N* parâmetros.
//! - [`calibrar_linear`] (1e): um único parâmetro, `p_t(r) = K/N + b_t·z(r)`,
//!   com `z` linear na posição; ver a documentação da função.
//!
//! Calibração por posição: `p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)`
//! com `m` = [`CONCURSOS_FICTICIOS`] e o histórico inteiro, sem janela
//! (`docs/PLANEJAMENTO.md`, 1c, decisões de 2026-10-05). Os `m` concursos
//! fictícios puxam a taxa observada para o acaso, então o primeiro concurso
//! recebe exatamente *K/N* e as *N* probabilidades de cada concurso somam *K*
//! (cada concurso sorteado soma *K* saídas às contagens).
//!
//! Ordem do laço (a mesma nas duas): probabilidades de `t` → só então `t` entra
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

/// Calibração linear na posição (1e): um parâmetro em vez de *N*.
///
/// Modelo (`docs/PLANEJAMENTO.md`, Fase 1e): `p_t(r) = K/N + b_t·z(r)`, com
/// `z(r) = ((N+1) − 2r)/(N−1)` (posição 1 = +1, posição *N* = −1; `Σ z = 0`,
/// então as *N* probabilidades de cada concurso somam exatamente *K*).
/// `b_t` é de mínimos quadrados com encolhimento para zero, só com o passado:
///
/// `b_t = Σ_(s<t) Σ_r z(r)·saiu(s,r) / ((c + m)·Σ_r z(r)²)`
///
/// com `c` = concursos sorteados antes de `t` na entrada e `m` =
/// [`CONCURSOS_FICTICIOS`] concursos fictícios com `b = 0`. O primeiro
/// concurso da entrada recebe exatamente *K/N*.
///
/// Contas em inteiros e uma única divisão no fim. Com `w(r) = (N+1) − 2r`,
/// `W2 = Σ w²`, `S = Σ_(s<t) Σ_r w(r)·saiu(s,r)` e `D = (c + m)·W2`:
///
/// - `b_t = S·(N−1)/D` e `b_t·z(r) = S·w(r)/D` (o `N−1` do `z` cancela);
/// - `p = (K·D + N·S·w(r)) / (N·D)`.
///
/// Limite de `b_t`: `|b_t| ≤ B = min(K, N−K)/N`, o que mantém todo *p* em
/// [0, 1]. A comparação é exata, sem ponto flutuante:
/// `|S|·(N−1)·N > D·min(K, N−K)`. Se for verdadeira, `b_t = ±B` e
/// `p = (K·(N−1) ± min(K, N−K)·w(r)) / (N·(N−1))`, também uma única divisão
/// de inteiros. Com isso o numerador nunca é negativo nem passa do
/// denominador. O limite do log loss (1e-15) continua sendo de
/// `log_loss`; aqui *p* pode valer exatamente 0 ou 1 na saturação.
///
/// Mesma entrada, validação, saída e ordem do laço de [`calibrar`]: o
/// snapshot sem `saiu` recebe probabilidade e não entra nas somas.
pub fn calibrar_linear(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<Vec<ProbabilidadeDezena>> {
    let linhas = ordenar_e_validar(cfg, observacoes)?;

    let n = i128::from(cfg.qtd_dezenas_possiveis());
    if n < 2 {
        // z(r) divide por N − 1; uma loteria de uma dezena não tem posição.
        return Err(invalida(format!(
            "calibração linear exige ao menos 2 dezenas possíveis, a {} tem {n}",
            cfg.nome_exibicao
        )));
    }
    let k = cfg.qtd_dezenas as i128;
    let m = i128::from(CONCURSOS_FICTICIOS);
    let menor_lado = k.min(n - k);
    let peso = |posicao: u32| (n + 1) - 2 * i128::from(posicao);
    let w2: i128 = (1..=cfg.qtd_dezenas_possiveis())
        .map(|r| peso(r) * peso(r))
        .sum();

    // S: soma dos pesos das posições que saíram, nos concursos já sorteados.
    let mut soma_pesos = 0_i128;
    let mut concursos_sorteados = 0_i128;

    let mut saida = Vec::with_capacity(linhas.len());
    for grupo in linhas.chunk_by(|a, b| a.concurso == b.concurso) {
        let d = (concursos_sorteados + m) * w2;
        let saturado = soma_pesos.abs() * (n - 1) * n > d * menor_lado;
        for o in grupo {
            // `ordenar_e_validar` garante posicao em 1..=N.
            let w = peso(o.posicao);
            let (numerador, denominador) = if saturado {
                (
                    k * (n - 1) + soma_pesos.signum() * menor_lado * w,
                    n * (n - 1),
                )
            } else {
                (k * d + n * soma_pesos * w, n * d)
            };
            saida.push(ProbabilidadeDezena {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                probabilidade: numerador as f64 / denominador as f64,
                saiu: o.saiu,
            });
        }
        // Só agora o concurso entra nas somas.
        if grupo[0].saiu.is_some() {
            soma_pesos += grupo
                .iter()
                .filter(|o| o.saiu == Some(true))
                .map(|o| peso(o.posicao))
                .sum::<i128>();
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
