//! Componentes de uma dezena numa janela, calculados a partir da lista de
//! aparições (números de concurso, em ordem crescente).
//!
//! Todo componente sai de um único acumulador ([`Acumulado`]) que incorpora
//! uma aparição por vez. O estado do motor usa o mesmo acumulador de dois
//! jeitos: mantido de forma incremental para a janela total, e refeito a cada
//! snapshot sobre as aparições recentes para as janelas finitas. Assim não há
//! duas implementações da mesma fórmula.

use serde::{Deserialize, Serialize};

/// Snapshot de uma dezena numa janela, imediatamente antes do concurso
/// `concurso` (usa só os concursos até `concurso - 1`). Uma linha de
/// `features_dezena`; `None` corresponde a `NULL`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeaturesDezena {
    pub concurso: u32,
    pub dezena: u32,
    /// Tamanho da janela em concursos; `JANELA_TOTAL` (0) = todo o histórico.
    pub janela: u32,
    pub frequencia: u32,
    pub maior_espaco: Option<u32>,
    pub maior_espaco_ocorrencias: Option<u32>,
    pub menor_espaco: Option<u32>,
    pub menor_espaco_ocorrencias: Option<u32>,
    pub soma_variacoes: Option<u32>,
    /// Não é limitado pela janela: `(concurso - 1) - última aparição`.
    pub espaco_final: Option<u32>,
    pub repeticoes_total: u32,
    pub repeticao_atual: u32,
    /// Alvo: se a dezena saiu em `concurso`. `None` no snapshot do próximo
    /// concurso, ainda não sorteado.
    pub saiu: Option<bool>,
}

/// Acumulador dos componentes sobre uma sequência crescente de aparições.
///
/// Espaço entre aparições consecutivas `a` e `b` = `b - a - 1`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Acumulado {
    pub(crate) frequencia: u32,
    pub(crate) ultima: Option<u32>,
    ultimo_espaco: Option<u32>,
    maior_espaco: Option<u32>,
    maior_espaco_ocorrencias: u32,
    menor_espaco: Option<u32>,
    menor_espaco_ocorrencias: u32,
    /// Σ |espaçoᵢ − espaçoᵢ₋₁|; só tem significado com 3 ou mais aparições.
    soma_variacoes: u32,
    /// Quantos espaços valem 0 (pares de concursos seguidos).
    repeticoes_total: u32,
    /// Tamanho da sequência de concursos seguidos que termina na última
    /// aparição incorporada.
    sequencia_final: u32,
}

impl Acumulado {
    /// Incorpora a aparição no concurso `concurso`. Quem chama garante ordem
    /// estritamente crescente (o estado do motor recusa concurso repetido ou
    /// fora de ordem antes de chegar aqui); aparição fora de ordem é ignorada
    /// em vez de gerar espaço negativo.
    pub(crate) fn incorporar(&mut self, concurso: u32) {
        if let Some(ultima) = self.ultima {
            debug_assert!(
                concurso > ultima,
                "aparição fora de ordem: {concurso} após {ultima}"
            );
            if concurso <= ultima {
                return;
            }
            let espaco = concurso - ultima - 1;
            if let Some(anterior) = self.ultimo_espaco {
                self.soma_variacoes += espaco.abs_diff(anterior);
            }
            match self.maior_espaco {
                Some(m) if espaco < m => {}
                Some(m) if espaco == m => self.maior_espaco_ocorrencias += 1,
                _ => {
                    self.maior_espaco = Some(espaco);
                    self.maior_espaco_ocorrencias = 1;
                }
            }
            match self.menor_espaco {
                Some(m) if espaco > m => {}
                Some(m) if espaco == m => self.menor_espaco_ocorrencias += 1,
                _ => {
                    self.menor_espaco = Some(espaco);
                    self.menor_espaco_ocorrencias = 1;
                }
            }
            if espaco == 0 {
                self.repeticoes_total += 1;
                self.sequencia_final += 1;
            } else {
                self.sequencia_final = 1;
            }
            self.ultimo_espaco = Some(espaco);
        } else {
            self.sequencia_final = 1;
        }
        self.ultima = Some(concurso);
        self.frequencia += 1;
    }

    /// Monta a linha de features do snapshot do concurso `concurso`.
    ///
    /// `self` deve conter exatamente as aparições da janela (concursos
    /// `concurso - janela` … `concurso - 1`); `ultima_aparicao` é a última
    /// aparição em todo o histórico até `concurso - 1`, porque `espaco_final`
    /// não é limitado pela janela.
    pub(crate) fn features(
        &self,
        concurso: u32,
        dezena: u32,
        janela: u32,
        ultima_aparicao: Option<u32>,
    ) -> FeaturesDezena {
        // Menos de 2 aparições → sem espaços; menos de 3 → sem variações.
        let com_espacos = self.frequencia >= 2;
        let com_variacoes = self.frequencia >= 3;
        let anterior = concurso.checked_sub(1);
        // A sequência só está "em curso" se terminou em `concurso - 1`; como
        // `self` só tem aparições da janela, ela já sai limitada a `janela`.
        let repeticao_atual = if self.ultima.is_some() && self.ultima == anterior {
            self.sequencia_final
        } else {
            0
        };
        FeaturesDezena {
            concurso,
            dezena,
            janela,
            frequencia: self.frequencia,
            maior_espaco: self.maior_espaco.filter(|_| com_espacos),
            maior_espaco_ocorrencias: com_espacos.then_some(self.maior_espaco_ocorrencias),
            menor_espaco: self.menor_espaco.filter(|_| com_espacos),
            menor_espaco_ocorrencias: com_espacos.then_some(self.menor_espaco_ocorrencias),
            soma_variacoes: com_variacoes.then_some(self.soma_variacoes),
            espaco_final: match (anterior, ultima_aparicao) {
                (Some(ant), Some(u)) if u <= ant => Some(ant - u),
                _ => None,
            },
            repeticoes_total: self.repeticoes_total,
            repeticao_atual,
            saiu: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acumular(aparicoes: &[u32]) -> Acumulado {
        let mut a = Acumulado::default();
        for &c in aparicoes {
            a.incorporar(c);
        }
        a
    }

    #[test]
    fn golden_do_plano_no_acumulador() {
        // docs/PLANEJAMENTO.md, "Exemplo de referência": aparições 3, 4, 8,
        // 9, 10, 15; snapshot em t = 20, janela total.
        let a = acumular(&[3, 4, 8, 9, 10, 15]);
        let f = a.features(20, 7, 0, a.ultima);
        assert_eq!(
            f,
            FeaturesDezena {
                concurso: 20,
                dezena: 7,
                janela: 0,
                frequencia: 6,
                maior_espaco: Some(4),
                maior_espaco_ocorrencias: Some(1),
                menor_espaco: Some(0),
                menor_espaco_ocorrencias: Some(3),
                soma_variacoes: Some(10),
                espaco_final: Some(4),
                repeticoes_total: 3,
                repeticao_atual: 0,
                saiu: None,
            }
        );
    }

    #[test]
    fn sem_aparicoes() {
        let a = Acumulado::default();
        let f = a.features(5, 1, 0, None);
        assert_eq!(f.frequencia, 0);
        assert_eq!(f.maior_espaco, None);
        assert_eq!(f.maior_espaco_ocorrencias, None);
        assert_eq!(f.menor_espaco, None);
        assert_eq!(f.menor_espaco_ocorrencias, None);
        assert_eq!(f.soma_variacoes, None);
        assert_eq!(f.espaco_final, None);
        assert_eq!(f.repeticoes_total, 0);
        assert_eq!(f.repeticao_atual, 0);
    }

    #[test]
    fn uma_aparicao_nao_tem_espacos() {
        let a = acumular(&[4]);
        let f = a.features(5, 1, 0, a.ultima);
        assert_eq!(f.frequencia, 1);
        assert_eq!(f.maior_espaco, None);
        assert_eq!(f.menor_espaco_ocorrencias, None);
        assert_eq!(f.soma_variacoes, None);
        assert_eq!(f.espaco_final, Some(0));
        assert_eq!(f.repeticao_atual, 1);
    }

    #[test]
    fn duas_aparicoes_tem_espaco_mas_nao_variacao() {
        let a = acumular(&[2, 6]);
        let f = a.features(10, 1, 0, a.ultima);
        assert_eq!(f.maior_espaco, Some(3));
        assert_eq!(f.maior_espaco_ocorrencias, Some(1));
        assert_eq!(f.menor_espaco, Some(3));
        assert_eq!(f.menor_espaco_ocorrencias, Some(1));
        assert_eq!(f.soma_variacoes, None);
        assert_eq!(f.espaco_final, Some(3));
        assert_eq!(f.repeticoes_total, 0);
        assert_eq!(f.repeticao_atual, 0);
    }

    #[test]
    fn sequencia_em_curso() {
        let a = acumular(&[1, 3, 4, 5]);
        let f = a.features(6, 1, 0, a.ultima);
        assert_eq!(f.repeticao_atual, 3);
        assert_eq!(f.repeticoes_total, 2);
        assert_eq!(f.espaco_final, Some(0));
        // Espaços [1, 0, 0]: variações |0−1| + |0−0| = 1.
        assert_eq!(f.soma_variacoes, Some(1));
    }
}
