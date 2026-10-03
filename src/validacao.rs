//! Regras de integridade dos resultados de uma loteria.
//!
//! Funções puras: não fazem I/O nem SQL. Recebem a configuração da loteria e
//! os resultados e devolvem a lista de problemas encontrados, em ordem estável
//! por concurso. A validação é separada em duas partes para ser reutilizada ao
//! gravar (tarefa 0.3):
//!
//! - [`validar_resultado`]: o que dá para checar olhando um concurso isolado
//!   (loteria, número do concurso, quantidade, faixa, repetidas, data ISO);
//! - [`validar_sequencia`]: o que depende do conjunto (duplicatas, buracos na
//!   numeração, datas fora de ordem).
//!
//! [`validar`] aplica as duas (comando `validar`); [`avaliar_gravacao`]
//! aplica as duas a um lote antes de gravá-lo (`importar`, `baixar`,
//! `atualizar`).

use crate::config::LoteriaConfig;
use crate::model::Resultado;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Um problema de integridade encontrado nos resultados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Problema {
    /// O campo `loteria` do resultado não corresponde à configuração usada.
    LoteriaDiferente {
        concurso: u32,
        esperada: String,
        encontrada: String,
    },
    /// Concursos são numerados a partir de 1; 0 indica erro de leitura.
    ConcursoZero,
    QuantidadeDezenas {
        concurso: u32,
        esperada: usize,
        encontrada: usize,
    },
    DezenaForaDaFaixa {
        concurso: u32,
        dezena: u32,
        min: u32,
        max: u32,
    },
    DezenaRepetida {
        concurso: u32,
        dezena: u32,
    },
    DataInvalida {
        concurso: u32,
        data: String,
    },
    /// O mesmo concurso aparece mais de uma vez na entrada. Não ocorre no
    /// banco (chave primária), mas pode ocorrer num lote a gravar.
    ConcursoDuplicado {
        concurso: u32,
        ocorrencias: usize,
    },
    /// Faixa inclusiva de concursos ausentes (`inicio..=fim`).
    ConcursosFaltando {
        inicio: u32,
        fim: u32,
    },
    /// A data do concurso é anterior à do concurso anterior presente.
    DataForaDeOrdem {
        concurso: u32,
        data: String,
        concurso_anterior: u32,
        data_anterior: String,
    },
}

/// Gravidade de um problema: erros reprovam a validação; avisos são
/// reportados mas não reprovam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severidade {
    Erro,
    Aviso,
}

impl Problema {
    /// Data fora de ordem é aviso: costuma ser erro de digitação na planilha
    /// da Caixa (ex.: Quina 380), tratado com uma correção conhecida em
    /// `config::CORRECOES_DATA`, e não impede o uso do histórico.
    pub fn severidade(&self) -> Severidade {
        match self {
            Problema::DataForaDeOrdem { .. } => Severidade::Aviso,
            _ => Severidade::Erro,
        }
    }

    /// Concurso usado para ordenar o relatório (início da faixa, no caso de
    /// buracos).
    pub fn concurso(&self) -> u32 {
        match self {
            Problema::ConcursoZero => 0,
            Problema::LoteriaDiferente { concurso, .. }
            | Problema::QuantidadeDezenas { concurso, .. }
            | Problema::DezenaForaDaFaixa { concurso, .. }
            | Problema::DezenaRepetida { concurso, .. }
            | Problema::DataInvalida { concurso, .. }
            | Problema::ConcursoDuplicado { concurso, .. }
            | Problema::DataForaDeOrdem { concurso, .. } => *concurso,
            Problema::ConcursosFaltando { inicio, .. } => *inicio,
        }
    }
}

impl fmt::Display for Problema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Problema::LoteriaDiferente {
                concurso,
                esperada,
                encontrada,
            } => write!(
                f,
                "concurso {concurso}: loteria `{encontrada}`, esperada `{esperada}`"
            ),
            Problema::ConcursoZero => write!(f, "concurso 0: numeração começa em 1"),
            Problema::QuantidadeDezenas {
                concurso,
                esperada,
                encontrada,
            } => write!(
                f,
                "concurso {concurso}: {encontrada} dezena(s), esperadas {esperada}"
            ),
            Problema::DezenaForaDaFaixa {
                concurso,
                dezena,
                min,
                max,
            } => write!(
                f,
                "concurso {concurso}: dezena {dezena} fora da faixa {min}..={max}"
            ),
            Problema::DezenaRepetida { concurso, dezena } => {
                write!(f, "concurso {concurso}: dezena {dezena} repetida")
            }
            Problema::DataInvalida { concurso, data } => write!(
                f,
                "concurso {concurso}: data `{data}` não é uma data ISO AAAA-MM-DD válida"
            ),
            Problema::ConcursoDuplicado {
                concurso,
                ocorrencias,
            } => write!(f, "concurso {concurso}: aparece {ocorrencias} vezes"),
            Problema::ConcursosFaltando { inicio, fim } if inicio == fim => {
                write!(f, "concurso {inicio} faltando")
            }
            Problema::ConcursosFaltando { inicio, fim } => write!(
                f,
                "concursos {inicio}–{fim} faltando ({} concursos)",
                fim - inicio + 1
            ),
            Problema::DataForaDeOrdem {
                concurso,
                data,
                concurso_anterior,
                data_anterior,
            } => write!(
                f,
                "concurso {concurso}: data {data} anterior à do concurso \
                 {concurso_anterior} ({data_anterior}) — confira as duas datas e, \
                 se for erro da planilha, cadastre em CORRECOES_DATA (config.rs)"
            ),
        }
    }
}

/// Aplica todas as regras (individuais e de sequência). A ordem de entrada
/// não importa; o relatório sai ordenado por concurso.
pub fn validar(cfg: &LoteriaConfig, resultados: &[Resultado]) -> Vec<Problema> {
    let mut problemas: Vec<Problema> = resultados
        .iter()
        .flat_map(|r| validar_resultado(cfg, r))
        .collect();
    problemas.extend(validar_sequencia(resultados));
    // Ordenação estável: dentro do mesmo concurso, mantém a ordem das regras.
    problemas.sort_by_key(Problema::concurso);
    problemas
}

/// Regras que dependem só do próprio concurso.
pub fn validar_resultado(cfg: &LoteriaConfig, r: &Resultado) -> Vec<Problema> {
    let mut problemas = Vec::new();
    let concurso = r.concurso;

    if r.loteria != cfg.slug {
        problemas.push(Problema::LoteriaDiferente {
            concurso,
            esperada: cfg.slug.to_string(),
            encontrada: r.loteria.clone(),
        });
    }
    if concurso == 0 {
        problemas.push(Problema::ConcursoZero);
    }
    if r.dezenas.len() != cfg.qtd_dezenas {
        problemas.push(Problema::QuantidadeDezenas {
            concurso,
            esperada: cfg.qtd_dezenas,
            encontrada: r.dezenas.len(),
        });
    }

    // BTreeSet para reportar cada dezena repetida uma única vez e em ordem.
    let mut vistas = BTreeSet::new();
    let mut repetidas = BTreeSet::new();
    let mut fora = BTreeSet::new();
    for &d in &r.dezenas {
        if !cfg.contem(d) {
            fora.insert(d);
        }
        if !vistas.insert(d) {
            repetidas.insert(d);
        }
    }
    problemas.extend(fora.into_iter().map(|dezena| Problema::DezenaForaDaFaixa {
        concurso,
        dezena,
        min: cfg.dezena_min,
        max: cfg.dezena_max,
    }));
    problemas.extend(
        repetidas
            .into_iter()
            .map(|dezena| Problema::DezenaRepetida { concurso, dezena }),
    );

    if !data_iso_valida(&r.data_sorteio) {
        problemas.push(Problema::DataInvalida {
            concurso,
            data: r.data_sorteio.clone(),
        });
    }

    problemas
}

/// Regras que dependem do conjunto: concursos duplicados, buracos na
/// numeração (o histórico deve começar no concurso 1 e ir até o maior
/// presente) e datas decrescentes. Datas iguais em concursos seguidos são
/// permitidas.
pub fn validar_sequencia(resultados: &[Resultado]) -> Vec<Problema> {
    let mut ordenados: Vec<&Resultado> = resultados.iter().collect();
    ordenados.sort_by_key(|r| r.concurso);

    let mut problemas = Vec::new();
    let mut proximo_esperado: u32 = 1;
    // Último concurso com data válida, para comparar a ordem. Datas inválidas
    // já são reportadas por `validar_resultado` e não entram na comparação.
    let mut anterior_com_data: Option<&Resultado> = None;

    let mut i = 0;
    while i < ordenados.len() {
        let atual = ordenados[i];
        let concurso = atual.concurso;

        let mut fim_grupo = i + 1;
        while fim_grupo < ordenados.len() && ordenados[fim_grupo].concurso == concurso {
            fim_grupo += 1;
        }
        let ocorrencias = fim_grupo - i;
        if ocorrencias > 1 {
            problemas.push(Problema::ConcursoDuplicado {
                concurso,
                ocorrencias,
            });
        }

        // Concurso 0 é reportado por `validar_resultado`; não afeta buracos.
        if concurso > proximo_esperado {
            problemas.push(Problema::ConcursosFaltando {
                inicio: proximo_esperado,
                fim: concurso - 1,
            });
        }
        if concurso >= proximo_esperado {
            proximo_esperado = concurso.saturating_add(1);
        }

        for r in &ordenados[i..fim_grupo] {
            if !data_iso_valida(&r.data_sorteio) {
                continue;
            }
            if let Some(ant) = anterior_com_data {
                // Em AAAA-MM-DD válido, a ordem lexicográfica é a cronológica.
                if ant.concurso < r.concurso && r.data_sorteio < ant.data_sorteio {
                    problemas.push(Problema::DataForaDeOrdem {
                        concurso: r.concurso,
                        data: r.data_sorteio.clone(),
                        concurso_anterior: ant.concurso,
                        data_anterior: ant.data_sorteio.clone(),
                    });
                }
            }
        }
        // O próximo concurso compara com a última data válida deste grupo.
        if let Some(r) = ordenados[i..fim_grupo]
            .iter()
            .rev()
            .find(|r| data_iso_valida(&r.data_sorteio))
        {
            anterior_com_data = Some(r);
        }

        i = fim_grupo;
    }

    problemas
}

/// Resultado de [`avaliar_gravacao`]: problemas já classificados em erros e
/// avisos. Se houver qualquer erro, nada do lote deve ser gravado.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisaoGravacao {
    pub erros: Vec<Problema>,
    pub avisos: Vec<Problema>,
}

impl DecisaoGravacao {
    /// Tudo ou nada: um único erro bloqueia o lote inteiro.
    pub fn pode_gravar(&self) -> bool {
        self.erros.is_empty()
    }
}

/// Decide se o lote `novos` pode ser gravado sobre `existentes` (o que já
/// está no banco).
///
/// - Cada registro novo passa por [`validar_resultado`]; concurso repetido
///   dentro do próprio lote também é erro.
/// - A sequência é validada sobre o estado que o banco teria depois da
///   gravação (existentes + novos, com o novo substituindo o existente de
///   mesmo concurso), porque é esse estado que precisa ficar íntegro.
/// - Data fora de ordem é aviso, e só é reportada se envolver algum concurso
///   do lote (como atual ou como anterior): avisos antigos do banco não se
///   repetem a cada gravação.
/// - Buraco na numeração é sempre aviso na gravação (decisão do usuário,
///   2026-10-03): não impede gravar o que chegou, e pode ser fechado depois
///   com `sync inserir`. No `validar`, continua sendo erro.
///
/// Os registros existentes não são revalidados individualmente: problemas
/// antigos são assunto do `validar`, não do lote atual.
pub fn avaliar_gravacao(
    cfg: &LoteriaConfig,
    existentes: &[Resultado],
    novos: &[Resultado],
) -> DecisaoGravacao {
    let mut problemas: Vec<Problema> = novos
        .iter()
        .flat_map(|r| validar_resultado(cfg, r))
        .collect();
    problemas.extend(
        validar_sequencia(novos)
            .into_iter()
            .filter(|p| matches!(p, Problema::ConcursoDuplicado { .. })),
    );

    // BTreeMap: estado resultante em ordem de concurso, sem duplicatas (as
    // do lote já foram reportadas acima).
    let mut estado: BTreeMap<u32, &Resultado> =
        existentes.iter().map(|r| (r.concurso, r)).collect();
    estado.extend(novos.iter().map(|r| (r.concurso, r)));
    let estado: Vec<Resultado> = estado.into_values().cloned().collect();
    let concursos_novos: BTreeSet<u32> = novos.iter().map(|r| r.concurso).collect();
    problemas.extend(validar_sequencia(&estado).into_iter().filter(|p| match p {
        Problema::DataForaDeOrdem {
            concurso,
            concurso_anterior,
            ..
        } => concursos_novos.contains(concurso) || concursos_novos.contains(concurso_anterior),
        _ => true,
    }));

    problemas.sort_by_key(Problema::concurso);

    let (erros, avisos) = problemas.into_iter().partition(|p| {
        p.severidade() == Severidade::Erro && !matches!(p, Problema::ConcursosFaltando { .. })
    });
    DecisaoGravacao { erros, avisos }
}

/// `true` se `data` está no formato `AAAA-MM-DD` e representa um dia que
/// existe no calendário gregoriano (inclui 29/02 em anos bissextos).
pub fn data_iso_valida(data: &str) -> bool {
    let b = data.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let numero = |faixa: std::ops::Range<usize>| -> Option<u32> {
        let parte = &b[faixa];
        if !parte.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(
            parte
                .iter()
                .fold(0u32, |acc, &c| acc * 10 + u32::from(c - b'0')),
        )
    };
    let (Some(ano), Some(mes), Some(dia)) = (numero(0..4), numero(5..7), numero(8..10)) else {
        return false;
    };
    if ano == 0 || !(1..=12).contains(&mes) {
        return false;
    }
    let bissexto = (ano % 4 == 0 && ano % 100 != 0) || ano % 400 == 0;
    let dias_no_mes = match mes {
        2 if bissexto => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=dias_no_mes).contains(&dia)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    fn mega() -> &'static LoteriaConfig {
        config::buscar("megasena").expect("loteria configurada")
    }

    fn res(concurso: u32, data: &str, dezenas: &[u32]) -> Resultado {
        Resultado {
            loteria: "megasena".to_string(),
            concurso,
            data_sorteio: data.to_string(),
            dezenas: dezenas.to_vec(),
        }
    }

    fn validos() -> Vec<Resultado> {
        vec![
            res(1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(2, "1996-03-18", &[9, 37, 39, 41, 43, 49]),
            res(3, "1996-03-25", &[10, 11, 29, 30, 36, 47]),
        ]
    }

    #[test]
    fn historico_valido_nao_tem_problemas() {
        assert_eq!(validar(mega(), &validos()), vec![]);
    }

    #[test]
    fn entrada_vazia_nao_tem_problemas() {
        assert_eq!(validar(mega(), &[]), vec![]);
    }

    #[test]
    fn ordem_de_entrada_nao_importa() {
        let mut v = validos();
        v.reverse();
        assert_eq!(validar(mega(), &v), vec![]);
        v.swap(0, 1);
        assert_eq!(validar(mega(), &v), vec![]);
    }

    #[test]
    fn quantidade_de_dezenas_errada() {
        let r = res(1, "1996-03-11", &[4, 5, 30, 33, 41]);
        assert_eq!(
            validar_resultado(mega(), &r),
            vec![Problema::QuantidadeDezenas {
                concurso: 1,
                esperada: 6,
                encontrada: 5
            }]
        );
        let r = res(1, "1996-03-11", &[1, 2, 3, 4, 5, 6, 7]);
        assert!(matches!(
            validar_resultado(mega(), &r)[..],
            [Problema::QuantidadeDezenas { encontrada: 7, .. }]
        ));
    }

    #[test]
    fn dezena_fora_da_faixa() {
        let r = res(1, "1996-03-11", &[0, 5, 30, 33, 41, 61]);
        assert_eq!(
            validar_resultado(mega(), &r),
            vec![
                Problema::DezenaForaDaFaixa {
                    concurso: 1,
                    dezena: 0,
                    min: 1,
                    max: 60
                },
                Problema::DezenaForaDaFaixa {
                    concurso: 1,
                    dezena: 61,
                    min: 1,
                    max: 60
                },
            ]
        );
    }

    #[test]
    fn bordas_da_faixa_sao_validas() {
        let r = res(1, "1996-03-11", &[1, 5, 30, 33, 41, 60]);
        assert_eq!(validar_resultado(mega(), &r), vec![]);
    }

    #[test]
    fn dezena_repetida_reportada_uma_vez() {
        let r = res(1, "1996-03-11", &[5, 5, 5, 33, 41, 52]);
        assert_eq!(
            validar_resultado(mega(), &r),
            vec![Problema::DezenaRepetida {
                concurso: 1,
                dezena: 5
            }]
        );
    }

    #[test]
    fn data_invalida() {
        for data in [
            "11/03/1996",
            "1996-3-11",
            "1996-13-01",
            "1996-00-10",
            "1996-04-31",
            "1997-02-29",
            "1900-02-29",
            "1996-03-00",
            "",
            "1996-03-1a",
            "0000-01-01",
        ] {
            let r = res(1, data, &[4, 5, 30, 33, 41, 52]);
            assert_eq!(
                validar_resultado(mega(), &r),
                vec![Problema::DataInvalida {
                    concurso: 1,
                    data: data.to_string()
                }],
                "{data:?}"
            );
        }
    }

    #[test]
    fn datas_validas_incluindo_bissextos() {
        for data in ["1996-02-29", "2000-02-29", "2024-12-31", "2026-01-01"] {
            assert!(data_iso_valida(data), "{data}");
        }
    }

    #[test]
    fn loteria_diferente_e_concurso_zero() {
        let mut r = res(0, "1996-03-11", &[4, 5, 30, 33, 41, 52]);
        r.loteria = "quina".to_string();
        assert_eq!(
            validar_resultado(mega(), &r),
            vec![
                Problema::LoteriaDiferente {
                    concurso: 0,
                    esperada: "megasena".to_string(),
                    encontrada: "quina".to_string()
                },
                Problema::ConcursoZero,
            ]
        );
    }

    #[test]
    fn buraco_no_meio_reportado_como_faixa() {
        let v = vec![
            res(1, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
            res(2, "2000-01-02", &[1, 2, 3, 4, 5, 6]),
            res(6, "2000-01-06", &[1, 2, 3, 4, 5, 6]),
            res(8, "2000-01-08", &[1, 2, 3, 4, 5, 6]),
        ];
        assert_eq!(
            validar_sequencia(&v),
            vec![
                Problema::ConcursosFaltando { inicio: 3, fim: 5 },
                Problema::ConcursosFaltando { inicio: 7, fim: 7 },
            ]
        );
    }

    #[test]
    fn buraco_no_inicio_sem_concurso_1() {
        let v = vec![
            res(5, "2000-01-05", &[1, 2, 3, 4, 5, 6]),
            res(4, "2000-01-04", &[1, 2, 3, 4, 5, 6]),
        ];
        assert_eq!(
            validar_sequencia(&v),
            vec![Problema::ConcursosFaltando { inicio: 1, fim: 3 }]
        );
    }

    #[test]
    fn data_fora_de_ordem() {
        let v = vec![
            res(3, "2000-01-02", &[1, 2, 3, 4, 5, 6]),
            res(1, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
            res(2, "2000-01-05", &[1, 2, 3, 4, 5, 6]),
        ];
        assert_eq!(
            validar_sequencia(&v),
            vec![Problema::DataForaDeOrdem {
                concurso: 3,
                data: "2000-01-02".to_string(),
                concurso_anterior: 2,
                data_anterior: "2000-01-05".to_string(),
            }]
        );
    }

    #[test]
    fn data_fora_de_ordem_e_aviso_e_o_resto_e_erro() {
        let aviso = Problema::DataForaDeOrdem {
            concurso: 380,
            data: "1997-03-12".to_string(),
            concurso_anterior: 379,
            data_anterior: "1998-03-08".to_string(),
        };
        assert_eq!(aviso.severidade(), Severidade::Aviso);
        assert!(aviso.to_string().contains("CORRECOES_DATA"));
        assert_eq!(
            Problema::ConcursosFaltando { inicio: 1, fim: 1 }.severidade(),
            Severidade::Erro
        );
        assert_eq!(
            Problema::DezenaRepetida {
                concurso: 1,
                dezena: 7
            }
            .severidade(),
            Severidade::Erro
        );
    }

    #[test]
    fn data_igual_e_permitida() {
        let v = vec![
            res(1, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
            res(2, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
        ];
        assert_eq!(validar_sequencia(&v), vec![]);
    }

    #[test]
    fn data_invalida_nao_entra_na_comparacao_de_ordem() {
        let v = vec![
            res(1, "2000-01-05", &[1, 2, 3, 4, 5, 6]),
            res(2, "lixo", &[1, 2, 3, 4, 5, 6]),
            res(3, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
        ];
        // O concurso 3 é comparado com o 1 (último com data válida).
        assert_eq!(
            validar_sequencia(&v),
            vec![Problema::DataForaDeOrdem {
                concurso: 3,
                data: "2000-01-01".to_string(),
                concurso_anterior: 1,
                data_anterior: "2000-01-05".to_string(),
            }]
        );
    }

    #[test]
    fn concurso_duplicado() {
        let v = vec![
            res(1, "2000-01-01", &[1, 2, 3, 4, 5, 6]),
            res(2, "2000-01-02", &[1, 2, 3, 4, 5, 6]),
            res(2, "2000-01-02", &[7, 8, 9, 10, 11, 12]),
            res(2, "2000-01-02", &[1, 2, 3, 4, 5, 6]),
        ];
        assert_eq!(
            validar_sequencia(&v),
            vec![Problema::ConcursoDuplicado {
                concurso: 2,
                ocorrencias: 3
            }]
        );
    }

    #[test]
    fn relatorio_ordenado_por_concurso() {
        let v = vec![
            res(9, "2000-01-01", &[1, 2, 3, 4, 5]),
            res(1, "2000-01-05", &[1, 1, 3, 4, 5, 6]),
        ];
        let problemas = validar(mega(), &v);
        let concursos: Vec<u32> = problemas.iter().map(Problema::concurso).collect();
        assert_eq!(concursos, vec![1, 2, 9, 9]);
        assert_eq!(
            problemas,
            vec![
                Problema::DezenaRepetida {
                    concurso: 1,
                    dezena: 1
                },
                Problema::ConcursosFaltando { inicio: 2, fim: 8 },
                Problema::QuantidadeDezenas {
                    concurso: 9,
                    esperada: 6,
                    encontrada: 5
                },
                Problema::DataForaDeOrdem {
                    concurso: 9,
                    data: "2000-01-01".to_string(),
                    concurso_anterior: 1,
                    data_anterior: "2000-01-05".to_string(),
                },
            ]
        );
    }

    #[test]
    fn gravacao_valida_sobre_banco_vazio() {
        let d = avaliar_gravacao(mega(), &[], &validos());
        assert!(d.pode_gravar());
        assert_eq!(d, DecisaoGravacao::default());
    }

    #[test]
    fn gravacao_de_lote_vazio() {
        let d = avaliar_gravacao(mega(), &[], &[]);
        assert!(d.pode_gravar());
        assert_eq!(d, DecisaoGravacao::default());
        let d = avaliar_gravacao(mega(), &validos(), &[]);
        assert!(d.pode_gravar());
        assert_eq!(d, DecisaoGravacao::default());
    }

    #[test]
    fn um_registro_invalido_bloqueia_o_lote_inteiro() {
        let mut novos = validos();
        novos[1].dezenas = vec![9, 37, 39, 41, 43, 61];
        {
            let d = avaliar_gravacao(mega(), &[], &novos);
            assert!(!d.pode_gravar());
            assert_eq!(
                d.erros,
                vec![Problema::DezenaForaDaFaixa {
                    concurso: 2,
                    dezena: 61,
                    min: 1,
                    max: 60
                }]
            );
            assert_eq!(d.avisos, vec![]);
        }
    }

    #[test]
    fn registro_de_outra_loteria_bloqueia() {
        let mut novos = validos();
        novos[2].loteria = "quina".to_string();
        let d = avaliar_gravacao(mega(), &[], &novos);
        assert!(!d.pode_gravar());
        assert!(matches!(
            d.erros[..],
            [Problema::LoteriaDiferente { concurso: 3, .. }]
        ));
    }

    #[test]
    fn concurso_duplicado_no_lote_bloqueia() {
        let mut novos = validos();
        novos.push(res(2, "1996-03-18", &[1, 2, 3, 4, 5, 6]));
        let d = avaliar_gravacao(mega(), &[], &novos);
        assert_eq!(
            d.erros,
            vec![Problema::ConcursoDuplicado {
                concurso: 2,
                ocorrencias: 2
            }]
        );
    }

    #[test]
    fn buraco_no_lote_so_avisa() {
        let novos = vec![
            res(1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(3, "1996-03-25", &[10, 11, 29, 30, 36, 47]),
        ];
        let d = avaliar_gravacao(mega(), &[], &novos);
        assert!(d.pode_gravar());
        assert_eq!(d.erros, vec![]);
        assert_eq!(
            d.avisos,
            vec![Problema::ConcursosFaltando { inicio: 2, fim: 2 }]
        );
    }

    #[test]
    fn buraco_entre_banco_e_lote_so_avisa() {
        let existentes = &validos()[..2];
        let novos = [res(5, "1996-04-15", &[1, 2, 3, 4, 5, 6])];
        let d = avaliar_gravacao(mega(), existentes, &novos);
        assert!(d.pode_gravar());
        assert_eq!(d.erros, vec![]);
        assert_eq!(
            d.avisos,
            vec![Problema::ConcursosFaltando { inicio: 3, fim: 4 }]
        );
    }

    #[test]
    fn buraco_nao_tolera_outros_erros() {
        let novos = [res(5, "1996-04-15", &[1, 2, 3, 4, 5])];
        let d = avaliar_gravacao(mega(), &validos(), &novos);
        assert!(!d.pode_gravar());
        assert!(matches!(
            d.erros[..],
            [Problema::QuantidadeDezenas { concurso: 5, .. }]
        ));
        assert_eq!(
            d.avisos,
            vec![Problema::ConcursosFaltando { inicio: 4, fim: 4 }]
        );
    }

    #[test]
    fn lote_que_fecha_buraco_do_banco_e_aceito() {
        let existentes = vec![
            res(1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(3, "1996-03-25", &[10, 11, 29, 30, 36, 47]),
        ];
        let novos = [res(2, "1996-03-18", &[9, 37, 39, 41, 43, 49])];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert_eq!(d, DecisaoGravacao::default());
    }

    #[test]
    fn data_fora_de_ordem_so_avisa() {
        let mut novos = validos();
        novos[2].data_sorteio = "1996-03-01".to_string();
        {
            let d = avaliar_gravacao(mega(), &[], &novos);
            assert!(d.pode_gravar());
            assert_eq!(d.erros, vec![]);
            assert!(matches!(
                d.avisos[..],
                [Problema::DataForaDeOrdem { concurso: 3, .. }]
            ));
        }
    }

    #[test]
    fn novo_substitui_existente_de_mesmo_concurso() {
        // No banco, o 3 tem data fora de ordem; o lote traz o 3 corrigido.
        let mut existentes = validos();
        existentes[2].data_sorteio = "1996-03-01".to_string();
        let novos = [res(3, "1996-03-25", &[10, 11, 29, 30, 36, 47])];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert_eq!(d, DecisaoGravacao::default());

        // E o contrário: o lote traz a data errada sobre um banco correto.
        let novos = [res(3, "1996-03-01", &[10, 11, 29, 30, 36, 47])];
        let d = avaliar_gravacao(mega(), &validos(), &novos);
        assert!(d.pode_gravar());
        assert!(matches!(
            d.avisos[..],
            [Problema::DataForaDeOrdem { concurso: 3, .. }]
        ));
    }

    #[test]
    fn existente_igual_ao_novo_nao_conta_como_duplicado() {
        let d = avaliar_gravacao(mega(), &validos(), &validos());
        assert_eq!(d, DecisaoGravacao::default());
    }

    #[test]
    fn primeiro_concurso_em_banco_vazio_avisa_buraco() {
        let d = avaliar_gravacao(mega(), &[], &[res(5, "1996-04-15", &[1, 2, 3, 4, 5, 6])]);
        assert!(d.pode_gravar());
        assert_eq!(d.erros, vec![]);
        assert_eq!(
            d.avisos,
            vec![Problema::ConcursosFaltando { inicio: 1, fim: 4 }]
        );
    }

    #[test]
    fn buraco_antigo_do_banco_so_avisa() {
        // Um buraco antigo do banco, fora do lote, não impede gravar o lote.
        let existentes = vec![
            res(1, "1996-03-11", &[4, 5, 30, 33, 41, 52]),
            res(3, "1996-03-25", &[10, 11, 29, 30, 36, 47]),
        ];
        let novos = [res(4, "1996-04-01", &[1, 2, 3, 4, 5, 6])];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert!(d.pode_gravar());
        assert_eq!(d.erros, vec![]);
        assert_eq!(
            d.avisos,
            vec![Problema::ConcursosFaltando { inicio: 2, fim: 2 }]
        );
    }

    #[test]
    fn novo_de_outra_loteria_substituindo_existente_valido_bloqueia() {
        let mut novo = validos()[1].clone();
        novo.loteria = "quina".to_string();
        {
            let d = avaliar_gravacao(mega(), &validos(), std::slice::from_ref(&novo));
            assert!(!d.pode_gravar());
            assert!(matches!(
                d.erros[..],
                [Problema::LoteriaDiferente { concurso: 2, .. }]
            ));
        }
    }

    #[test]
    fn data_fora_de_ordem_antiga_do_banco_nao_se_repete() {
        // No banco, o 2 tem data anterior à do 1 (aviso antigo).
        let mut existentes = validos();
        existentes[1].data_sorteio = "1996-03-01".to_string();

        // Lote que não toca o 1 nem o 2: o aviso antigo não aparece.
        let novos = [res(4, "1996-04-01", &[1, 2, 3, 4, 5, 6])];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert_eq!(d, DecisaoGravacao::default());

        // Lote cujo concurso novo fica fora de ordem: aparece.
        let novos = [res(4, "1996-03-20", &[1, 2, 3, 4, 5, 6])];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert!(d.pode_gravar());
        assert_eq!(
            d.avisos,
            vec![Problema::DataForaDeOrdem {
                concurso: 4,
                data: "1996-03-20".to_string(),
                concurso_anterior: 3,
                data_anterior: "1996-03-25".to_string(),
            }]
        );

        // Lote que regrava o concurso anterior ao par fora de ordem (o 1)
        // também o envolve: aparece.
        let novos = [validos()[0].clone()];
        let d = avaliar_gravacao(mega(), &existentes, &novos);
        assert!(matches!(
            d.avisos[..],
            [Problema::DataForaDeOrdem {
                concurso: 2,
                concurso_anterior: 1,
                ..
            }]
        ));
    }

    #[test]
    fn mensagens_em_portugues() {
        assert_eq!(
            Problema::ConcursosFaltando {
                inicio: 120,
                fim: 125
            }
            .to_string(),
            "concursos 120–125 faltando (6 concursos)"
        );
        assert_eq!(
            Problema::ConcursosFaltando { inicio: 7, fim: 7 }.to_string(),
            "concurso 7 faltando"
        );
    }
}
