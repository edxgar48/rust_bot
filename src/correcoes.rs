//! Aplica as correções conhecidas (`config::CORRECOES_DATA`) a resultados
//! recém-lidos, antes de gravar. Função pura: não faz I/O nem SQL.

use crate::config::{CorrecaoData, CORRECOES_DATA};
use crate::model::Resultado;
use std::fmt;

/// O que aconteceu com uma correção conhecida num lote de resultados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ocorrencia {
    /// A data veio com o erro conhecido e foi corrigida.
    Aplicada(CorrecaoData),
    /// A data não é nem a original nem a corrigida: a fonte mudou de um jeito
    /// inesperado. Nada é alterado; a correção precisa ser revista.
    Divergente {
        correcao: CorrecaoData,
        data_recebida: String,
    },
}

impl fmt::Display for Ocorrencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ocorrencia::Aplicada(c) => write!(
                f,
                "concurso {}: data corrigida de {} para {} ({})",
                c.concurso, c.data_original, c.data_corrigida, c.motivo
            ),
            Ocorrencia::Divergente {
                correcao: c,
                data_recebida,
            } => write!(
                f,
                "aviso: concurso {}: correção conhecida espera {} (→ {}), mas a fonte \
                 trouxe {data_recebida}; revise `CORRECOES_DATA` em config.rs",
                c.concurso, c.data_original, c.data_corrigida
            ),
        }
    }
}

/// Aplica as correções de `CORRECOES_DATA` da loteria `slug`.
pub fn aplicar(slug: &str, resultados: &mut [Resultado]) -> Vec<Ocorrencia> {
    aplicar_lista(CORRECOES_DATA, slug, resultados)
}

fn aplicar_lista(
    correcoes: &[CorrecaoData],
    slug: &str,
    resultados: &mut [Resultado],
) -> Vec<Ocorrencia> {
    let mut ocorrencias = Vec::new();
    for c in correcoes.iter().filter(|c| c.loteria == slug) {
        for r in resultados.iter_mut().filter(|r| r.concurso == c.concurso) {
            if r.data_sorteio == c.data_original {
                r.data_sorteio = c.data_corrigida.to_string();
                ocorrencias.push(Ocorrencia::Aplicada(*c));
            } else if r.data_sorteio != c.data_corrigida {
                ocorrencias.push(Ocorrencia::Divergente {
                    correcao: *c,
                    data_recebida: r.data_sorteio.clone(),
                });
            }
        }
    }
    ocorrencias
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validacao::data_iso_valida;

    const CORRECAO: CorrecaoData = CorrecaoData {
        loteria: "quina",
        concurso: 380,
        data_original: "1997-03-12",
        data_corrigida: "1998-03-12",
        motivo: "teste",
    };

    fn res(loteria: &str, concurso: u32, data: &str) -> Resultado {
        Resultado {
            loteria: loteria.to_string(),
            concurso,
            data_sorteio: data.to_string(),
            dezenas: vec![1, 2, 3, 4, 5],
        }
    }

    #[test]
    fn corrige_data_com_erro_conhecido() {
        let mut lote = vec![
            res("quina", 379, "1998-03-08"),
            res("quina", 380, "1997-03-12"),
        ];
        let ocorrencias = aplicar_lista(&[CORRECAO], "quina", &mut lote);
        assert_eq!(lote[1].data_sorteio, "1998-03-12");
        assert_eq!(lote[0].data_sorteio, "1998-03-08");
        assert_eq!(ocorrencias, vec![Ocorrencia::Aplicada(CORRECAO)]);
    }

    #[test]
    fn data_ja_corrigida_nao_gera_ocorrencia() {
        let mut lote = vec![res("quina", 380, "1998-03-12")];
        assert!(aplicar_lista(&[CORRECAO], "quina", &mut lote).is_empty());
        assert_eq!(lote[0].data_sorteio, "1998-03-12");
    }

    #[test]
    fn data_inesperada_gera_aviso_sem_alterar() {
        let mut lote = vec![res("quina", 380, "1998-03-13")];
        let ocorrencias = aplicar_lista(&[CORRECAO], "quina", &mut lote);
        assert_eq!(lote[0].data_sorteio, "1998-03-13");
        assert_eq!(
            ocorrencias,
            vec![Ocorrencia::Divergente {
                correcao: CORRECAO,
                data_recebida: "1998-03-13".to_string(),
            }]
        );
        assert!(ocorrencias[0].to_string().starts_with("aviso:"));
    }

    #[test]
    fn outra_loteria_nao_e_afetada() {
        let mut lote = vec![res("megasena", 380, "1997-03-12")];
        assert!(aplicar_lista(&[CORRECAO], "megasena", &mut lote).is_empty());
        assert_eq!(lote[0].data_sorteio, "1997-03-12");
    }

    #[test]
    fn concurso_ausente_no_lote_nao_gera_ocorrencia() {
        let mut lote = vec![res("quina", 7131, "2026-09-30")];
        assert!(aplicar_lista(&[CORRECAO], "quina", &mut lote).is_empty());
    }

    #[test]
    fn correcoes_configuradas_sao_validas() {
        let mut chaves = std::collections::BTreeSet::new();
        for c in CORRECOES_DATA {
            assert!(
                chaves.insert((c.loteria, c.concurso)),
                "mais de uma correção para {} {}",
                c.loteria,
                c.concurso
            );
            assert!(crate::config::buscar(c.loteria).is_some(), "{c:?}");
            assert!(data_iso_valida(c.data_original), "{c:?}");
            assert!(data_iso_valida(c.data_corrigida), "{c:?}");
            assert_ne!(c.data_original, c.data_corrigida, "{c:?}");
            assert!(!c.motivo.is_empty(), "{c:?}");
        }
    }
}
