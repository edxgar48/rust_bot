//! Escritor de JSON determinístico do contrato `v1`.
//!
//! O `serde_json` não serve aqui: ele escreve `f64` pelo menor texto que
//! volta ao mesmo número (`2.5`, não `2.500000000`) e não tem a mistura de
//! estilos que o contrato pede (arquivos indentados, mas um concurso por
//! linha no histórico). Então o JSON é montado como uma árvore [`Valor`] e
//! escrito por este módulo, que fixa cada byte:
//!
//! - UTF-8 sem BOM, fim de linha `\n`, um `\n` no fim do arquivo;
//! - indentação de 2 espaços; objeto e [`Valor::Lista`] com um item por linha;
//! - [`Valor::Linha`] (lista curta) em uma linha só, `[1, 2, 3]`;
//! - chaves de objeto na ordem em que foram inseridas (nunca ordenadas por
//!   um mapa de hash);
//! - números inteiros em decimal; `f64` só por [`Valor::fixo`], com casas
//!   fixas; textos escapados pelo `serde_json` (que mantém o UTF-8 como está).

use crate::error::{Result, SpiderError};
use std::borrow::Cow;

/// Árvore JSON com a ordem das chaves fixada por quem monta.
#[derive(Debug, Clone, PartialEq)]
pub enum Valor {
    Nulo,
    Inteiro(i64),
    /// Número já formatado (ver [`Valor::fixo`]); o texto vai direto ao arquivo.
    Numero(String),
    Texto(String),
    /// Lista com um item por linha.
    Lista(Vec<Valor>),
    /// Lista inteira numa linha só: `[a, b, c]`.
    Linha(Vec<Valor>),
    Objeto(Vec<(Cow<'static, str>, Valor)>),
}

impl Valor {
    /// Objeto de chaves fixas, na ordem dada.
    pub fn objeto(campos: Vec<(&'static str, Valor)>) -> Valor {
        Valor::Objeto(campos.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Objeto cujas chaves vêm dos dados (p.ex. os slugs das loterias), na
    /// ordem dada.
    pub fn objeto_de(campos: Vec<(String, Valor)>) -> Valor {
        Valor::Objeto(campos.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    pub fn inteiro(v: impl Into<i64>) -> Valor {
        Valor::Inteiro(v.into())
    }

    pub fn texto(v: &str) -> Valor {
        Valor::Texto(v.to_string())
    }

    pub fn opcional(v: Option<u32>) -> Valor {
        v.map_or(Valor::Nulo, |v| Valor::Inteiro(i64::from(v)))
    }

    pub fn opcional_texto(v: Option<&str>) -> Valor {
        v.map_or(Valor::Nulo, Valor::texto)
    }

    /// Lista de inteiros numa linha só.
    pub fn linha_de(v: &[u32]) -> Valor {
        Valor::Linha(v.iter().map(|&n| Valor::Inteiro(i64::from(n))).collect())
    }

    /// `f64` com exatamente `casas` casas decimais, ponto decimal e sem
    /// expoente. `-0` e arredondamentos que dão zero saem sem sinal
    /// (`0.000000000`). Valor não finito não é JSON: é recusado.
    pub fn fixo(valor: f64, casas: usize) -> Result<Valor> {
        if !valor.is_finite() {
            return Err(SpiderError::ExportacaoInvalida(format!(
                "número não finito ({valor}) não cabe em JSON"
            )));
        }
        let texto = format!("{valor:.casas$}");
        let sem_sinal = texto.trim_start_matches('-');
        let texto = if sem_sinal.chars().all(|c| c == '0' || c == '.') {
            sem_sinal.to_string()
        } else {
            texto
        };
        Ok(Valor::Numero(texto))
    }

    /// O arquivo inteiro: o valor, indentado, mais o `\n` final.
    pub fn para_arquivo(&self) -> String {
        let mut saida = String::new();
        self.escrever(&mut saida, 0);
        saida.push('\n');
        saida
    }

    fn escrever(&self, saida: &mut String, nivel: usize) {
        match self {
            Valor::Lista(itens) if !itens.is_empty() => {
                saida.push_str("[\n");
                for (i, item) in itens.iter().enumerate() {
                    recuo(saida, nivel + 1);
                    item.escrever(saida, nivel + 1);
                    saida.push_str(if i + 1 < itens.len() { ",\n" } else { "\n" });
                }
                recuo(saida, nivel);
                saida.push(']');
            }
            Valor::Objeto(campos) if !campos.is_empty() => {
                saida.push_str("{\n");
                for (i, (chave, valor)) in campos.iter().enumerate() {
                    recuo(saida, nivel + 1);
                    saida.push_str(&texto_json(chave.as_ref()));
                    saida.push_str(": ");
                    valor.escrever(saida, nivel + 1);
                    saida.push_str(if i + 1 < campos.len() { ",\n" } else { "\n" });
                }
                recuo(saida, nivel);
                saida.push('}');
            }
            outro => outro.escrever_em_linha(saida),
        }
    }

    fn escrever_em_linha(&self, saida: &mut String) {
        match self {
            Valor::Nulo => saida.push_str("null"),
            Valor::Inteiro(n) => saida.push_str(&n.to_string()),
            Valor::Numero(n) => saida.push_str(n),
            Valor::Texto(t) => saida.push_str(&texto_json(t)),
            Valor::Lista(itens) | Valor::Linha(itens) => {
                saida.push('[');
                for (i, item) in itens.iter().enumerate() {
                    if i > 0 {
                        saida.push_str(", ");
                    }
                    item.escrever_em_linha(saida);
                }
                saida.push(']');
            }
            Valor::Objeto(campos) => {
                saida.push('{');
                for (i, (chave, valor)) in campos.iter().enumerate() {
                    if i > 0 {
                        saida.push_str(", ");
                    }
                    saida.push_str(&texto_json(chave.as_ref()));
                    saida.push_str(": ");
                    valor.escrever_em_linha(saida);
                }
                saida.push('}');
            }
        }
    }
}

fn recuo(saida: &mut String, nivel: usize) {
    for _ in 0..nivel {
        saida.push_str("  ");
    }
}

/// Texto entre aspas, escapado pelo `serde_json` (nunca falha para `&str`).
fn texto_json(texto: &str) -> String {
    serde_json::to_string(texto).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixo_escreve_casas_fixas_sem_expoente_e_sem_menos_zero() {
        let f = |v: f64| match Valor::fixo(v, 9).expect("finito") {
            Valor::Numero(t) => t,
            outro => panic!("{outro:?}"),
        };
        assert_eq!(f(2.5), "2.500000000");
        assert_eq!(f(-1.234_567_891_4), "-1.234567891");
        assert_eq!(f(0.0), "0.000000000");
        assert_eq!(f(-0.0), "0.000000000");
        assert_eq!(f(-0.000_000_000_4), "0.000000000");
        assert_eq!(f(1e-7), "0.000000100");
        assert_eq!(f(123_456_789_012.5), "123456789012.500000000");
    }

    #[test]
    fn fixo_recusa_nan_e_infinito() {
        assert!(Valor::fixo(f64::NAN, 9).is_err());
        assert!(Valor::fixo(f64::INFINITY, 9).is_err());
    }

    #[test]
    fn objeto_lista_e_linha_seguem_o_estilo_do_contrato() {
        let v = Valor::objeto(vec![
            ("a", Valor::inteiro(1)),
            ("b", Valor::Nulo),
            ("c", Valor::linha_de(&[1, 2, 3])),
            (
                "d",
                Valor::Lista(vec![
                    Valor::Linha(vec![
                        Valor::inteiro(1),
                        Valor::texto("x"),
                        Valor::linha_de(&[4]),
                    ]),
                    Valor::objeto(vec![("k", Valor::texto("é\n\"q\""))]),
                ]),
            ),
            ("e", Valor::Lista(vec![])),
            ("f", Valor::objeto(vec![])),
        ]);
        let esperado = "{\n  \"a\": 1,\n  \"b\": null,\n  \"c\": [1, 2, 3],\n  \"d\": [\n    \
[1, \"x\", [4]],\n    {\n      \"k\": \"é\\n\\\"q\\\"\"\n    }\n  ],\n  \"e\": [],\n  \"f\": {}\n}\n";
        assert_eq!(v.para_arquivo(), esperado);
    }
}
