//! Índice de força das posições (Fase 1g): a posição que cada dezena ocupava
//! na tabela de força passa a ser tratada como se fosse uma dezena.
//!
//! Em cada concurso *t*, as posições ocupadas em `indice(t)` (medidas antes do
//! sorteio) pelas dezenas sorteadas viram as "dezenas" de um resultado
//! sintético com K posições. O motor de features e o índice rodam sobre esses
//! resultados sem nenhuma alteração: nas linhas devolvidas, o campo `dezena`
//! é a **posição** (1…N). No índice das posições, o campo `posicao` é o `g_1`
//! (1 = a posição de maior total).
//!
//! Ponto no tempo: a marca de *t* usa `indice(t)`, que só enxerga os concursos
//! até *t−1*, e o resultado de *t*; as features da posição em *t* usam só as
//! marcas até *t−1*. Definições e golden em `docs/PLANEJAMENTO.md`, Fase 1g.
//!
//! Funções puras: sem I/O e sem SQL.

use std::collections::BTreeMap;

use super::{
    calcular, calcular_indice, EstadoMotor, FeaturesDezena, IndiceDezena, Normalizacao, Pesos,
};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;

/// Dezena ordenada pelo índice das posições: a dezena herda o `g_1` da
/// posição que ocupa no concurso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DezenaViaPosicao {
    pub dezena: u32,
    /// Posição que a dezena ocupa em `indice(concurso)`.
    pub posicao: u32,
    /// `g_1` dessa posição no índice das posições (1 = a de maior total).
    pub g_1: u32,
}

/// Configuração do motor das posições: as posições `1..=N` são sorteadas K a
/// K, com N e K da loteria. O resto (`slug` inclusive) é o da loteria.
pub fn config_posicoes(cfg: &LoteriaConfig) -> LoteriaConfig {
    LoteriaConfig {
        dezena_min: 1,
        dezena_max: cfg.qtd_dezenas_possiveis(),
        ..*cfg
    }
}

/// Marca de um concurso: o resultado sintético com as posições ocupadas pelas
/// dezenas sorteadas, em ordem crescente. `posicao_da_dezena[i]` é a posição
/// da dezena `dezena_min + i` em `indice(r.concurso)`.
pub fn marcar(cfg: &LoteriaConfig, r: &Resultado, posicao_da_dezena: &[u32]) -> Result<Resultado> {
    if posicao_da_dezena.len() != cfg.qtd_dezenas_possiveis() as usize {
        return Err(invalida(format!(
            "concurso {}: {} posições para {} dezenas",
            r.concurso,
            posicao_da_dezena.len(),
            cfg.qtd_dezenas_possiveis()
        )));
    }
    let mut posicoes = Vec::with_capacity(r.dezenas.len());
    for &d in &r.dezenas {
        if !cfg.contem(d) {
            return Err(SpiderError::MotorResultadoInvalido {
                concurso: r.concurso,
                detalhe: format!(
                    "dezena {d} fora da faixa {}..={}",
                    cfg.dezena_min, cfg.dezena_max
                ),
            });
        }
        posicoes.push(posicao_da_dezena[(d - cfg.dezena_min) as usize]);
    }
    posicoes.sort_unstable();
    Ok(Resultado {
        loteria: r.loteria.clone(),
        concurso: r.concurso,
        data_sorteio: r.data_sorteio.clone(),
        dezenas: posicoes,
    })
}

/// As marcas de todos os `resultados`, usando as posições de `indice` (que
/// pode trazer também o snapshot do próximo concurso, ignorado aqui). Erro se
/// faltar o `indice` de algum concurso ou se ele não for uma permutação de
/// `1..=N` por concurso.
pub fn marcar_historico(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
    indice: &[IndiceDezena],
) -> Result<Vec<Resultado>> {
    let por_concurso = posicoes_por_concurso(cfg, indice)?;
    resultados
        .iter()
        .map(|r| {
            let posicoes = por_concurso.get(&r.concurso).ok_or_else(|| {
                invalida(format!(
                    "sem índice de força para o concurso {}",
                    r.concurso
                ))
            })?;
            marcar(cfg, r, posicoes)
        })
        .collect()
}

/// Calcula do zero as features e o índice das posições de todos os concursos
/// de `resultados` (numeração contínua a partir do 1, como no motor). Devolve
/// o estado final, pronto para retomar, as features (com o snapshot do próximo
/// concurso, de `saiu = None`, no fim) e o índice das posições, uma linha por
/// (concurso, posição), com o snapshot do próximo concurso incluído.
///
/// `janela`, `normalizacao` e `pesos` são os do perfil das posições.
pub fn calcular_posicoes(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
    indice_dezenas: &[IndiceDezena],
    janela: u32,
    normalizacao: Normalizacao,
    pesos: &Pesos,
) -> Result<(EstadoMotor, Vec<FeaturesDezena>, Vec<IndiceDezena>)> {
    let marcas = marcar_historico(cfg, resultados, indice_dezenas)?;
    let cfg_pos = config_posicoes(cfg);
    let (estado, mut features) = calcular(&cfg_pos, &marcas)?;
    features.extend(estado.snapshot());
    let indice = calcular_indice(&features, janela, normalizacao, pesos)?;
    Ok((estado, features, indice))
}

/// Ordem das dezenas do `concurso` pelo índice das posições: cada dezena herda
/// o `g_1` da posição que ocupa em `indice_dezenas`. Devolve as N dezenas
/// ordenadas por `g_1` (1 = a primeira). Ambos os índices precisam ter o
/// `concurso`, e `indice_posicoes` vem de [`calcular_posicoes`].
pub fn dezenas_via_posicao(
    cfg: &LoteriaConfig,
    concurso: u32,
    indice_dezenas: &[IndiceDezena],
    indice_posicoes: &[IndiceDezena],
) -> Result<Vec<DezenaViaPosicao>> {
    let do_concurso = |linhas: &[IndiceDezena]| -> Vec<IndiceDezena> {
        linhas
            .iter()
            .filter(|l| l.concurso == concurso)
            .cloned()
            .collect()
    };
    let posicoes = posicoes_por_concurso(cfg, &do_concurso(indice_dezenas))?;
    let g_1 = posicoes_por_concurso(&config_posicoes(cfg), &do_concurso(indice_posicoes))?;
    let (Some(posicao_da_dezena), Some(g_1_da_posicao)) =
        (posicoes.get(&concurso), g_1.get(&concurso))
    else {
        return Err(invalida(format!(
            "o concurso {concurso} precisa estar no índice das dezenas e no das posições"
        )));
    };
    let mut saida: Vec<DezenaViaPosicao> = (cfg.dezena_min..)
        .zip(posicao_da_dezena)
        .map(|(dezena, &posicao)| DezenaViaPosicao {
            dezena,
            posicao,
            g_1: g_1_da_posicao[(posicao - 1) as usize],
        })
        .collect();
    saida.sort_unstable_by_key(|d| d.g_1);
    Ok(saida)
}

/// Agrupa as linhas por concurso: para cada um, o vetor `posição` indexado por
/// `dezena - dezena_min`. Exige, por concurso, todas as dezenas da faixa uma
/// vez e as posições formando uma permutação de `1..=N`.
fn posicoes_por_concurso(
    cfg: &LoteriaConfig,
    indice: &[IndiceDezena],
) -> Result<BTreeMap<u32, Vec<u32>>> {
    let n = cfg.qtd_dezenas_possiveis() as usize;
    let mut mapa: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for linha in indice {
        if !cfg.contem(linha.dezena) {
            return Err(invalida(format!(
                "concurso {}: dezena {} fora da faixa {}..={}",
                linha.concurso, linha.dezena, cfg.dezena_min, cfg.dezena_max
            )));
        }
        if linha.posicao < 1 || linha.posicao as usize > n {
            return Err(invalida(format!(
                "concurso {}: posição {} da dezena {} fora de 1..={n}",
                linha.concurso, linha.posicao, linha.dezena
            )));
        }
        let posicoes = mapa.entry(linha.concurso).or_insert_with(|| vec![0; n]);
        let casa = &mut posicoes[(linha.dezena - cfg.dezena_min) as usize];
        if *casa != 0 {
            return Err(invalida(format!(
                "concurso {}: dezena {} repetida no índice",
                linha.concurso, linha.dezena
            )));
        }
        *casa = linha.posicao;
    }
    for (concurso, posicoes) in &mapa {
        let mut ocupada = vec![false; n];
        for (dezena, &posicao) in (cfg.dezena_min..).zip(posicoes) {
            if posicao == 0 {
                return Err(invalida(format!(
                    "concurso {concurso}: falta a dezena {dezena} no índice"
                )));
            }
            if std::mem::replace(&mut ocupada[(posicao - 1) as usize], true) {
                return Err(invalida(format!(
                    "concurso {concurso}: posição {posicao} repetida no índice"
                )));
            }
        }
    }
    Ok(mapa)
}

fn invalida(detalhe: String) -> SpiderError {
    SpiderError::MotorIndiceEntradaInvalida(detalhe)
}

#[cfg(test)]
mod tests;
