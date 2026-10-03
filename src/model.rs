use serde::{Deserialize, Serialize};

/// Um resultado (concurso) de loteria já normalizado, pronto para persistir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resultado {
    pub loteria: String,
    pub concurso: u32,
    /// Data no formato ISO `AAAA-MM-DD`.
    pub data_sorteio: String,
    pub dezenas: Vec<u32>,
}
