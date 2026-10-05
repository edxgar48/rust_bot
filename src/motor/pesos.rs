//! Pesos e normalização do índice de força, como o motor os consome.
//!
//! Ficam no motor (e não em `perfil`, que lê os `.toml` do disco) para o
//! cálculo puro não depender de I/O; `perfil` reexporta os dois tipos.

use serde::{Deserialize, Serialize};

/// Como os componentes são normalizados entre as dezenas de um concurso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Normalizacao {
    Zscore,
    Minmax,
}

impl Normalizacao {
    /// Texto usado no `.toml` e na coluna `perfis.normalizacao`.
    pub const fn como_str(self) -> &'static str {
        match self {
            Normalizacao::Zscore => "zscore",
            Normalizacao::Minmax => "minmax",
        }
    }
}

/// Peso de cada componente, com os mesmos nomes das colunas de
/// `features_dezena` (e dos campos de `motor::FeaturesDezena`). Todos
/// obrigatórios: peso 0 é escrito por extenso.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pesos {
    pub frequencia: f64,
    pub maior_espaco: f64,
    pub maior_espaco_ocorrencias: f64,
    pub menor_espaco: f64,
    pub menor_espaco_ocorrencias: f64,
    pub soma_variacoes: f64,
    pub espaco_final: f64,
    pub repeticoes_total: f64,
    pub repeticao_atual: f64,
}

impl Pesos {
    /// Os 9 pesos com o nome do componente, na ordem das colunas de
    /// `features_dezena`.
    pub const fn componentes(&self) -> [(&'static str, f64); 9] {
        [
            ("frequencia", self.frequencia),
            ("maior_espaco", self.maior_espaco),
            ("maior_espaco_ocorrencias", self.maior_espaco_ocorrencias),
            ("menor_espaco", self.menor_espaco),
            ("menor_espaco_ocorrencias", self.menor_espaco_ocorrencias),
            ("soma_variacoes", self.soma_variacoes),
            ("espaco_final", self.espaco_final),
            ("repeticoes_total", self.repeticoes_total),
            ("repeticao_atual", self.repeticao_atual),
        ]
    }
}
