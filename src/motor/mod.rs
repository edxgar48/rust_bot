//! Motor estatístico: features de cada dezena como série temporal.
//!
//! O snapshot do concurso `t` descreve cada dezena imediatamente antes de
//! `t`, usando só os concursos `1 … t−1`, em cada janela de [`JANELAS`].
//! Definições em `docs/PLANEJAMENTO.md`, "Especificação das features".
//!
//! Funções puras: sem I/O, sem SQL e sem nada específico de uma loteria além
//! do que vem em `LoteriaConfig`. O cálculo completo ([`calcular`]) é o mesmo
//! caminho do incremental ([`EstadoMotor::adicionar`]): parte de um estado
//! vazio e adiciona concurso a concurso.

mod estado;
mod features;

pub use estado::EstadoMotor;
pub use features::FeaturesDezena;

use crate::config::LoteriaConfig;
use crate::error::Result;
use crate::model::Resultado;

/// Versão das fórmulas das features (coluna `feature_version`). Mudou uma
/// definição → incremente; estado salvo de outra versão não é retomado.
pub const FEATURE_VERSION: u32 = 1;

/// Valor de `janela` que representa o histórico inteiro (`1 … t−1`).
pub const JANELA_TOTAL: u32 = 0;

/// Janelas calculadas: total (0), 10, 50 e 100 concursos. Em ordem crescente
/// de valor para que a saída já saia ordenada por (concurso, dezena, janela),
/// a mesma ordem da chave primária de `features_dezena`.
pub const JANELAS: [u32; 4] = [JANELA_TOTAL, 10, 50, 100];

/// Maior janela finita: quantos concursos recentes o estado guarda por dezena.
pub(crate) const JANELA_MAX: u32 = maior_janela(&JANELAS);

const fn maior_janela(janelas: &[u32]) -> u32 {
    let mut maior = 0;
    let mut i = 0;
    while i < janelas.len() {
        if janelas[i] > maior {
            maior = janelas[i];
        }
        i += 1;
    }
    maior
}

/// Calcula do zero os snapshots de todos os concursos de `resultados`
/// (que devem ser `1, 2, 3, …` em ordem, sem buraco) e devolve o estado final
/// junto, pronto para retomar. Os snapshots têm `saiu` preenchido e saem
/// ordenados por (concurso, dezena, janela); o snapshot do próximo concurso,
/// ainda não sorteado, é `estado.snapshot()`.
pub fn calcular(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
) -> Result<(EstadoMotor, Vec<FeaturesDezena>)> {
    let mut estado = EstadoMotor::novo(cfg);
    let features = estado.avancar(cfg, resultados)?;
    Ok((estado, features))
}

#[cfg(test)]
mod tests;
