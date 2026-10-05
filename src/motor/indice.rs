//! Índice de força: total ponderado, posição e variação de posição.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Índice de força" → "Regras do cálculo"
//! (decisões de 2026-10-04). Função pura: recebe as linhas de
//! `features_dezena` e os parâmetros do perfil e devolve as linhas de
//! `indice`.
//!
//! Ponto no tempo: o índice do concurso `t` usa só as features de `t` (que já
//! olham apenas `1 … t−1`) e a posição de `t−1`. A normalização é por
//! concurso, entre as dezenas daquele concurso, nunca sobre o histórico; e o
//! alvo `saiu` não é lido.

use super::{FeaturesDezena, Normalizacao, Pesos};
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// O total é arredondado a múltiplos de `1 / ESCALA_TOTAL` (1e-9) antes de
/// ordenar, para ruído de arredondamento do `f64` não trocar posições.
/// `1e9` é exato em `f64`, então `(x * 1e9).round() / 1e9` é o `f64` mais
/// próximo de `k × 1e-9`.
const ESCALA_TOTAL: f64 = 1e9;

/// Uma linha da tabela `indice`, sem a coluna `perfil` (quem grava a
/// acrescenta).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndiceDezena {
    pub concurso: u32,
    pub dezena: u32,
    /// Σ (pesoₖ × componenteₖ normalizado), **já arredondado** a 1e-9 e sem
    /// `-0.0`: é o valor que decide a posição, então gravar este garante que
    /// a ordem da tabela pode ser refeita a partir da coluna `total`.
    pub total: f64,
    /// 1 … N pelo total decrescente; empate → menor dezena à frente.
    pub posicao: u32,
    /// Posição no concurso anterior − posição atual (positivo = subiu);
    /// `None` no primeiro concurso da entrada.
    pub variacao_posicao: Option<i64>,
}

/// Calcula o índice de força de todos os concursos presentes em `features`,
/// usando só as linhas da `janela` do perfil. Devolve uma linha por
/// (concurso, dezena), ordenadas por (concurso, dezena) — a ordem da chave
/// primária de `indice`.
///
/// A entrada pode vir em qualquer ordem (é ordenada aqui), mas tem de formar
/// uma série completa; senão é [`SpiderError::MotorIndiceEntradaInvalida`]:
/// - linha (concurso, dezena) repetida na janela;
/// - buraco entre o primeiro e o último concurso da janela;
/// - concurso com conjunto de dezenas diferente do primeiro;
/// - concurso presente em outra janela fora da faixa coberta pela `janela`
///   do perfil (inclui a janela não ter linha nenhuma);
/// - peso não finito, ou total não finito (pesos absurdamente grandes).
///
/// Entrada vazia devolve vazio. O primeiro concurso da entrada tem
/// `variacao_posicao = None`; o snapshot do próximo concurso (`saiu = None`)
/// é um concurso como outro qualquer, medido contra o último sorteado.
pub fn calcular_indice(
    features: &[FeaturesDezena],
    janela: u32,
    normalizacao: Normalizacao,
    pesos: &Pesos,
) -> Result<Vec<IndiceDezena>> {
    let componentes = resolver_componentes(pesos)?;

    let mut linhas: Vec<&FeaturesDezena> = features.iter().filter(|f| f.janela == janela).collect();
    let Some(faixa_geral) = faixa_de_concursos(features.iter()) else {
        return Ok(Vec::new());
    };
    let Some(faixa_janela) = faixa_de_concursos(linhas.iter().copied()) else {
        return Err(invalida(format!(
            "nenhuma linha da janela {janela} (há features dos concursos {}..={})",
            faixa_geral.0, faixa_geral.1
        )));
    };
    if faixa_janela != faixa_geral {
        return Err(invalida(format!(
            "a janela {janela} cobre os concursos {}..={}, mas há features dos concursos {}..={}",
            faixa_janela.0, faixa_janela.1, faixa_geral.0, faixa_geral.1
        )));
    }

    // A ordenação instável é determinística: se (concurso, dezena) se repetir,
    // o laço abaixo devolve erro antes de produzir qualquer saída; sem
    // repetidas, as chaves são únicas e a ordem final não depende da entrada.
    linhas.sort_unstable_by_key(|f| (f.concurso, f.dezena));

    let mut saida = Vec::with_capacity(linhas.len());
    let mut referencia: Option<(u32, Vec<u32>)> = None;
    let mut posicoes_anteriores: Option<Vec<u32>> = None;

    for grupo in linhas.chunk_by(|a, b| a.concurso == b.concurso) {
        let concurso = grupo[0].concurso;
        let dezenas: Vec<u32> = grupo.iter().map(|f| f.dezena).collect();
        if let Some(par) = dezenas.windows(2).find(|par| par[0] == par[1]) {
            return Err(invalida(format!(
                "dezena {} repetida no concurso {concurso} (janela {janela})",
                par[0]
            )));
        }
        match &referencia {
            None => referencia = Some((concurso, dezenas.clone())),
            Some((primeiro, dezenas_ref)) => {
                if dezenas != *dezenas_ref {
                    return Err(invalida(format!(
                        "o concurso {concurso} tem {} dezena(s), {}, mas o concurso {primeiro} tem {}, {}",
                        dezenas.len(),
                        resumo(&dezenas),
                        dezenas_ref.len(),
                        resumo(dezenas_ref)
                    )));
                }
            }
        }
        if let Some(ultimo) = saida.last().map(|l: &IndiceDezena| l.concurso) {
            if concurso != ultimo + 1 {
                let faltando = if concurso - 1 == ultimo + 1 {
                    format!("do concurso {}", ultimo + 1)
                } else {
                    format!("dos concursos {}..={}", ultimo + 1, concurso - 1)
                };
                return Err(invalida(format!(
                    "faltam as features {faltando} (janela {janela})"
                )));
            }
        }

        let totais = totais_do_concurso(grupo, &componentes, normalizacao, concurso)?;
        let posicoes = posicoes(&totais, &dezenas);
        for (i, f) in grupo.iter().enumerate() {
            saida.push(IndiceDezena {
                concurso,
                dezena: f.dezena,
                total: totais[i],
                posicao: posicoes[i],
                // Mesmo conjunto de dezenas em todos os concursos e grupos
                // ordenados por dezena: o índice `i` é a mesma dezena no
                // concurso anterior.
                variacao_posicao: posicoes_anteriores
                    .as_ref()
                    .map(|ant| i64::from(ant[i]) - i64::from(posicoes[i])),
            });
        }
        posicoes_anteriores = Some(posicoes);
    }

    Ok(saida)
}

/// Lê um componente de uma linha de features (`NULL` = `None`).
type Extrator = fn(&FeaturesDezena) -> Option<u32>;

/// Associa cada peso de [`Pesos::componentes`] ao campo de mesmo nome em
/// [`FeaturesDezena`], na ordem de `componentes` (a ordem fixa da soma).
/// Casar pelo nome, e não pela posição, impede que uma mudança de ordem num
/// dos dois lados troque pesos de componente em silêncio.
fn resolver_componentes(pesos: &Pesos) -> Result<Vec<(f64, Extrator)>> {
    pesos
        .componentes()
        .into_iter()
        .map(|(nome, peso)| {
            if !peso.is_finite() {
                return Err(pesos_invalidos(format!(
                    "peso `{nome}` = {peso} não é finito"
                )));
            }
            let extrator: Extrator = match nome {
                "frequencia" => |f| Some(f.frequencia),
                "maior_espaco" => |f| f.maior_espaco,
                "maior_espaco_ocorrencias" => |f| f.maior_espaco_ocorrencias,
                "menor_espaco" => |f| f.menor_espaco,
                "menor_espaco_ocorrencias" => |f| f.menor_espaco_ocorrencias,
                "soma_variacoes" => |f| f.soma_variacoes,
                "espaco_final" => |f| f.espaco_final,
                "repeticoes_total" => |f| Some(f.repeticoes_total),
                "repeticao_atual" => |f| Some(f.repeticao_atual),
                outro => {
                    return Err(pesos_invalidos(format!(
                        "componente `{outro}` dos pesos não existe nas features"
                    )))
                }
            };
            Ok((peso, extrator))
        })
        .collect()
}

/// Totais (já arredondados) das dezenas de um concurso, na ordem de `grupo`.
fn totais_do_concurso(
    grupo: &[&FeaturesDezena],
    componentes: &[(f64, Extrator)],
    normalizacao: Normalizacao,
    concurso: u32,
) -> Result<Vec<f64>> {
    let mut totais = vec![0.0_f64; grupo.len()];
    let mut valores = Vec::with_capacity(grupo.len());
    for (peso, extrator) in componentes {
        valores.clear();
        valores.extend(grupo.iter().map(|f| extrator(f)));
        let normalizados = normalizar(&valores, normalizacao);
        for (total, n) in totais.iter_mut().zip(normalizados) {
            *total += peso * n;
        }
    }
    totais
        .into_iter()
        .zip(grupo)
        .map(|(total, f)| {
            let arredondado = arredondar(total);
            if arredondado.is_finite() {
                Ok(arredondado)
            } else {
                Err(pesos_invalidos(format!(
                    "total da dezena {} no concurso {concurso} não é finito ({total}): pesos grandes demais",
                    f.dezena
                )))
            }
        })
        .collect()
}

/// Normaliza um componente entre as dezenas de um concurso.
///
/// Estatísticas só sobre as dezenas com valor; `NULL` recebe o neutro (0 no
/// z-score, 0,5 no minmax). Sem variação (todos `NULL`, um valor só ou
/// máx = mín) → todas recebem o neutro, sem divisão por zero.
fn normalizar(valores: &[Option<u32>], normalizacao: Normalizacao) -> Vec<f64> {
    let neutro = match normalizacao {
        Normalizacao::Zscore => 0.0,
        Normalizacao::Minmax => 0.5,
    };
    let presentes = valores.iter().flatten().copied();
    let (min, max) = match presentes.clone().fold(None, |acc, x| match acc {
        None => Some((x, x)),
        Some((lo, hi)) => Some((u32::min(lo, x), u32::max(hi, x))),
    }) {
        // Máx = mín nos inteiros é exatamente "desvio 0": decide sem
        // depender de ponto flutuante.
        Some((min, max)) if min < max => (min, max),
        _ => return vec![neutro; valores.len()],
    };

    match normalizacao {
        Normalizacao::Zscore => {
            // Soma em u64 é exata e não depende da ordem; a variância soma na
            // ordem das dezenas (fixa). Desvio populacional (÷N).
            let n = presentes.clone().count() as f64;
            let soma: u64 = presentes.clone().map(u64::from).sum();
            let media = soma as f64 / n;
            let variancia = presentes
                .map(|x| {
                    let d = f64::from(x) - media;
                    d * d
                })
                .sum::<f64>()
                / n;
            let desvio = variancia.sqrt();
            // Com mín < máx o desvio é > 0; a guarda só evita NaN se algum dia
            // não for.
            if desvio <= 0.0 || !desvio.is_finite() {
                return vec![neutro; valores.len()];
            }
            valores
                .iter()
                .map(|v| v.map_or(neutro, |x| (f64::from(x) - media) / desvio))
                .collect()
        }
        Normalizacao::Minmax => {
            let amplitude = f64::from(max - min);
            valores
                .iter()
                .map(|v| v.map_or(neutro, |x| f64::from(x - min) / amplitude))
                .collect()
        }
    }
}

/// Arredonda a 1e-9 e troca `-0.0` por `0.0`: senão um total "zero
/// negativo" ordenaria depois de um zero positivo e quebraria o desempate
/// pela menor dezena.
fn arredondar(total: f64) -> f64 {
    let r = (total * ESCALA_TOTAL).round() / ESCALA_TOTAL;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// Posição (1 … N) de cada dezena, na ordem de `totais`/`dezenas`: total
/// decrescente, empate → menor dezena à frente.
fn posicoes(totais: &[f64], dezenas: &[u32]) -> Vec<u32> {
    let mut ordem: Vec<usize> = (0..totais.len()).collect();
    // Totais finitos e sem -0.0 (ver `arredondar`): `total_cmp` coincide com
    // a ordem numérica.
    ordem.sort_by(|&a, &b| {
        totais[b]
            .total_cmp(&totais[a])
            .then(dezenas[a].cmp(&dezenas[b]))
    });
    let mut posicoes = vec![0_u32; totais.len()];
    for (pos, &i) in (1_u32..).zip(&ordem) {
        posicoes[i] = pos;
    }
    posicoes
}

/// Menor e maior concurso das linhas (`None` se não houver linha).
fn faixa_de_concursos<'a>(linhas: impl Iterator<Item = &'a FeaturesDezena>) -> Option<(u32, u32)> {
    linhas.fold(None, |acc, f| match acc {
        None => Some((f.concurso, f.concurso)),
        Some((lo, hi)) => Some((lo.min(f.concurso), hi.max(f.concurso))),
    })
}

/// Lista curta de dezenas para mensagens de erro.
fn resumo(dezenas: &[u32]) -> String {
    const MAX: usize = 10;
    let mut texto: Vec<String> = dezenas.iter().take(MAX).map(u32::to_string).collect();
    if dezenas.len() > MAX {
        texto.push("…".to_string());
    }
    format!("[{}]", texto.join(", "))
}

/// Problema nas features de entrada: a saída é regenerá-las.
fn invalida(detalhe: String) -> SpiderError {
    SpiderError::MotorIndiceEntradaInvalida(format!(
        "{detalhe} — regenere as features (`sync features`)"
    ))
}

/// Problema nos pesos (não nas features): a saída é corrigir o perfil.
fn pesos_invalidos(detalhe: String) -> SpiderError {
    SpiderError::MotorIndiceEntradaInvalida(format!("{detalhe} — corrija os pesos do perfil"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linha de features com todos os componentes zerados/`NULL`; os testes
    /// preenchem só o que interessa.
    fn linha(concurso: u32, dezena: u32, janela: u32) -> FeaturesDezena {
        FeaturesDezena {
            concurso,
            dezena,
            janela,
            frequencia: 0,
            maior_espaco: None,
            maior_espaco_ocorrencias: None,
            menor_espaco: None,
            menor_espaco_ocorrencias: None,
            soma_variacoes: None,
            espaco_final: None,
            repeticoes_total: 0,
            repeticao_atual: 0,
            saiu: None,
        }
    }

    /// Pesos do golden: todos diádicos (exatos em binário), para as contas
    /// à mão darem o mesmo `f64` que o código.
    const PESOS_GOLDEN: Pesos = Pesos {
        frequencia: 1.0,
        maior_espaco: 0.5,
        maior_espaco_ocorrencias: 0.75,
        menor_espaco: 0.25,
        menor_espaco_ocorrencias: 0.5,
        soma_variacoes: -2.0,
        espaco_final: -0.125,
        repeticoes_total: 3.0,
        repeticao_atual: 0.0,
    };

    /// Dois concursos (10 e 11) de uma loteria com as dezenas 1..=4, janela 0.
    ///
    /// Concurso 10, por componente (dezenas 1, 2, 3, 4):
    /// - frequencia               [1, 1, 3, 3]          média 2, desvio 1
    /// - maior_espaco             [NULL, 2, 4, NULL]    só {2, 4}: média 3, desvio 1
    /// - maior_espaco_ocorrencias [1, 1, 1, 1]          constante → neutro
    /// - menor_espaco             [2, 0, 2, 0]          média 1, desvio 1
    /// - menor_espaco_ocorrencias todo NULL             → neutro
    /// - soma_variacoes           [NULL, NULL, NULL, 7] um valor só (máx = mín) → neutro
    /// - espaco_final             [6, 2, 2, 6]          média 4, desvio 2
    /// - repeticoes_total         [5, 5, 5, 5]          constante → neutro
    /// - repeticao_atual          [0, 1, 0, 1]          varia, mas peso 0
    ///
    /// Concurso 11: só `frequencia` varia, [3, 3, 1, 1]; o resto é constante
    /// ou todo NULL. `saiu` preenchido de propósito (o índice não o lê).
    fn golden() -> Vec<FeaturesDezena> {
        let mut v = Vec::new();
        let freq = [1, 1, 3, 3];
        let maior = [None, Some(2), Some(4), None];
        let menor = [2, 0, 2, 0];
        let soma = [None, None, None, Some(7)];
        let fim = [6, 2, 2, 6];
        let rep = [0, 1, 0, 1];
        for i in 0..4 {
            let mut f = linha(10, i as u32 + 1, 0);
            f.frequencia = freq[i];
            f.maior_espaco = maior[i];
            f.maior_espaco_ocorrencias = Some(1);
            f.menor_espaco = Some(menor[i]);
            f.soma_variacoes = soma[i];
            f.espaco_final = Some(fim[i]);
            f.repeticoes_total = 5;
            f.repeticao_atual = rep[i];
            f.saiu = Some(i == 0);
            v.push(f);
        }
        let freq11 = [3, 3, 1, 1];
        for (i, &freq) in freq11.iter().enumerate() {
            let mut f = linha(11, i as u32 + 1, 0);
            f.frequencia = freq;
            f.menor_espaco = Some(3);
            f.espaco_final = Some(0);
            f.repeticao_atual = 2;
            f.saiu = Some(i == 3);
            v.push(f);
        }
        v
    }

    fn ind(
        concurso: u32,
        dezena: u32,
        total: f64,
        posicao: u32,
        variacao_posicao: Option<i64>,
    ) -> IndiceDezena {
        IndiceDezena {
            concurso,
            dezena,
            total,
            posicao,
            variacao_posicao,
        }
    }

    #[test]
    fn golden_zscore() {
        // z de cada componente no concurso 10 (dezenas 1, 2, 3, 4):
        //   frequencia     (x−2)/1 = [−1, −1, 1, 1]          × 1     = [−1, −1, 1, 1]
        //   maior_espaco   [neutro 0, −1, 1, neutro 0]        × 0,5   = [0, −0,5, 0,5, 0]
        //   menor_espaco   (x−1)/1 = [1, −1, 1, −1]           × 0,25  = [0,25, −0,25, 0,25, −0,25]
        //   espaco_final   (x−4)/2 = [1, −1, −1, 1]           × −0,125 = [−0,125, 0,125, 0,125, −0,125]
        //   demais: neutro 0 (constante, todo NULL ou valor único) ou peso 0.
        // Totais: d1 = −1 + 0,25 − 0,125 = −0,875
        //         d2 = −1 − 0,5 − 0,25 + 0,125 = −1,625
        //         d3 = 1 + 0,5 + 0,25 + 0,125 = 1,875
        //         d4 = 1 − 0,25 − 0,125 = 0,625
        // Posições: d3 1º, d4 2º, d1 3º, d2 4º; primeiro concurso → variação NULL.
        //
        // Concurso 11: frequencia [3, 3, 1, 1], média 2, desvio 1 → z [1, 1, −1, −1];
        // o resto é neutro. Totais [1, 1, −1, −1]; empate d1/d2 e d3/d4 → menor
        // dezena à frente: d1 1º, d2 2º, d3 3º, d4 4º.
        // Variação = anterior − atual: d1 3−1 = 2, d2 4−2 = 2, d3 1−3 = −2, d4 2−4 = −2.
        let indice =
            calcular_indice(&golden(), 0, Normalizacao::Zscore, &PESOS_GOLDEN).expect("calcula");
        assert_eq!(
            indice,
            vec![
                ind(10, 1, -0.875, 3, None),
                ind(10, 2, -1.625, 4, None),
                ind(10, 3, 1.875, 1, None),
                ind(10, 4, 0.625, 2, None),
                ind(11, 1, 1.0, 1, Some(2)),
                ind(11, 2, 1.0, 2, Some(2)),
                ind(11, 3, -1.0, 3, Some(-2)),
                ind(11, 4, -1.0, 4, Some(-2)),
            ]
        );
    }

    #[test]
    fn golden_minmax() {
        // Concurso 10, (x − mín)/(máx − mín); NULL e componente sem variação → 0,5:
        //   frequencia               [0, 0, 1, 1]        × 1      = [0, 0, 1, 1]
        //   maior_espaco             [0,5, 0, 1, 0,5]    × 0,5    = [0,25, 0, 0,5, 0,25]
        //   maior_espaco_ocorrencias constante 0,5       × 0,75   = 0,375 em todas
        //   menor_espaco             [1, 0, 1, 0]        × 0,25   = [0,25, 0, 0,25, 0]
        //   menor_espaco_ocorrencias todo NULL 0,5       × 0,5    = 0,25 em todas
        //   soma_variacoes           valor único 0,5     × −2     = −1 em todas
        //   espaco_final             [1, 0, 0, 1]        × −0,125 = [−0,125, 0, 0, −0,125]
        //   repeticoes_total         constante 0,5       × 3      = 1,5 em todas
        //   repeticao_atual          [0, 1, 0, 1]        × 0      = 0
        // Parte comum: 0,375 + 0,25 − 1 + 1,5 = 1,125.
        // Totais: d1 = 1,125 + 0 + 0,25 + 0,25 − 0,125 = 1,5
        //         d2 = 1,125 + 0 + 0 + 0 + 0 = 1,125
        //         d3 = 1,125 + 1 + 0,5 + 0,25 + 0 = 2,875
        //         d4 = 1,125 + 1 + 0,25 + 0 − 0,125 = 2,25
        // Posições: d3 1º, d4 2º, d1 3º, d2 4º.
        //
        // Concurso 11: frequencia [1, 1, 0, 0]; os outros 8 componentes são
        // neutros (0,5): 0,5 × (0,5 + 0,75 + 0,25 + 0,5 − 2 − 0,125 + 3 + 0) = 1,4375.
        // Totais [2,4375, 2,4375, 1,4375, 1,4375] → d1, d2, d3, d4;
        // variação igual à do z-score: [2, 2, −2, −2].
        let indice =
            calcular_indice(&golden(), 0, Normalizacao::Minmax, &PESOS_GOLDEN).expect("calcula");
        assert_eq!(
            indice,
            vec![
                ind(10, 1, 1.5, 3, None),
                ind(10, 2, 1.125, 4, None),
                ind(10, 3, 2.875, 1, None),
                ind(10, 4, 2.25, 2, None),
                ind(11, 1, 2.4375, 1, Some(2)),
                ind(11, 2, 2.4375, 2, Some(2)),
                ind(11, 3, 1.4375, 3, Some(-2)),
                ind(11, 4, 1.4375, 4, Some(-2)),
            ]
        );
    }

    fn pesos_zerados() -> Pesos {
        Pesos {
            frequencia: 0.0,
            maior_espaco: 0.0,
            maior_espaco_ocorrencias: 0.0,
            menor_espaco: 0.0,
            menor_espaco_ocorrencias: 0.0,
            soma_variacoes: 0.0,
            espaco_final: 0.0,
            repeticoes_total: 0.0,
            repeticao_atual: 0.0,
        }
    }

    #[test]
    fn ruido_de_arredondamento_menor_que_1e9_e_empate() {
        // Minmax com 0/1: d1 só tem frequencia = 1 (peso 0,3), d2 só tem
        // maior_espaco = 1 (0,1) e maior_espaco_ocorrencias = 1 (0,2), d3 nada.
        // Sem arredondar: d1 = 0,3 e d2 = 0,1 + 0,2 = 0,30000000000000004 e
        // a d2 passaria à frente. Arredondado a 1e-9, os dois valem 0,3 →
        // empate → menor dezena (d1) à frente.
        assert!(std::hint::black_box(0.1_f64) + 0.2 > 0.3);
        let mut v = Vec::new();
        for (dezena, freq, maior) in [(1, 1, 0), (2, 0, 1), (3, 0, 0)] {
            let mut f = linha(5, dezena, 0);
            f.frequencia = freq;
            f.maior_espaco = Some(maior);
            f.maior_espaco_ocorrencias = Some(maior);
            v.push(f);
        }
        let pesos = Pesos {
            frequencia: 0.3,
            maior_espaco: 0.1,
            maior_espaco_ocorrencias: 0.2,
            ..pesos_zerados()
        };
        let indice = calcular_indice(&v, 0, Normalizacao::Minmax, &pesos).expect("calcula");
        assert_eq!(indice[0].total.to_bits(), indice[1].total.to_bits());
        assert_eq!(indice[0].total, 0.3);
        let posicoes: Vec<u32> = indice.iter().map(|l| l.posicao).collect();
        assert_eq!(posicoes, [1, 2, 3]);
    }

    #[test]
    fn zero_negativo_empata_com_zero() {
        // d1: −0,1 − 0,2 + 0,3 = −5,55e−17 → arredonda para −0,0; d2: 0,0.
        // Com −0,0 a d2 (0,0) passaria à frente em `total_cmp`; o total
        // gravado tem de ser +0,0 e o empate vai para a menor dezena.
        let mut v = Vec::new();
        for (dezena, x) in [(1, 1), (2, 0)] {
            let mut f = linha(5, dezena, 0);
            f.frequencia = x;
            f.maior_espaco = Some(x);
            f.maior_espaco_ocorrencias = Some(x);
            v.push(f);
        }
        let pesos = Pesos {
            frequencia: -0.1,
            maior_espaco: -0.2,
            maior_espaco_ocorrencias: 0.3,
            ..pesos_zerados()
        };
        assert!(std::hint::black_box(-0.1_f64) + -0.2 + 0.3 < 0.0);
        let indice = calcular_indice(&v, 0, Normalizacao::Minmax, &pesos).expect("calcula");
        for l in &indice {
            assert_eq!(l.total.to_bits(), 0.0_f64.to_bits(), "{l:?}");
        }
        assert_eq!(indice[0].posicao, 1);
        assert_eq!(indice[1].posicao, 2);
    }

    // ------------------------------------------------------------------
    // Helpers dos testes de comportamento
    // ------------------------------------------------------------------

    const NORMALIZACOES: [Normalizacao; 2] = [Normalizacao::Zscore, Normalizacao::Minmax];

    fn so_frequencia() -> Pesos {
        Pesos {
            frequencia: 1.0,
            ..pesos_zerados()
        }
    }

    fn so_maior_espaco() -> Pesos {
        Pesos {
            maior_espaco: 1.0,
            ..pesos_zerados()
        }
    }

    /// Exige `MotorIndiceEntradaInvalida` e devolve a mensagem.
    fn erro_entrada(r: Result<Vec<IndiceDezena>>) -> String {
        match r {
            Err(SpiderError::MotorIndiceEntradaInvalida(m)) => m,
            outro => panic!("esperava MotorIndiceEntradaInvalida, veio {outro:?}"),
        }
    }

    /// Um concurso em que só `frequencia` é preenchida (dezenas e valores
    /// na mesma ordem).
    fn concurso_com_frequencia(
        concurso: u32,
        janela: u32,
        dezenas: &[u32],
        freqs: &[u32],
    ) -> Vec<FeaturesDezena> {
        dezenas
            .iter()
            .zip(freqs)
            .map(|(&d, &fr)| {
                let mut f = linha(concurso, d, janela);
                f.frequencia = fr;
                f
            })
            .collect()
    }

    fn do_concurso(indice: &[IndiceDezena], concurso: u32) -> Vec<IndiceDezena> {
        indice
            .iter()
            .filter(|l| l.concurso == concurso)
            .cloned()
            .collect()
    }

    fn totais_de(indice: &[IndiceDezena], concurso: u32) -> Vec<f64> {
        do_concurso(indice, concurso)
            .iter()
            .map(|l| l.total)
            .collect()
    }

    fn posicoes_de(indice: &[IndiceDezena], concurso: u32) -> Vec<u32> {
        do_concurso(indice, concurso)
            .iter()
            .map(|l| l.posicao)
            .collect()
    }

    fn variacoes_de(indice: &[IndiceDezena], concurso: u32) -> Vec<Option<i64>> {
        do_concurso(indice, concurso)
            .iter()
            .map(|l| l.variacao_posicao)
            .collect()
    }

    /// Igualdade bit a bit (`total` inclusive), não só `PartialEq`.
    fn assert_identico(a: &[IndiceDezena], b: &[IndiceDezena]) {
        assert_eq!(a, b);
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.total.to_bits(), y.total.to_bits(), "{x:?} vs {y:?}");
        }
    }

    /// Todos os pesos diferentes de zero (diádicos), para a série variada.
    const PESOS_VARIADOS: Pesos = Pesos {
        frequencia: 1.0,
        maior_espaco: 0.5,
        maior_espaco_ocorrencias: 0.25,
        menor_espaco: -0.5,
        menor_espaco_ocorrencias: 0.75,
        soma_variacoes: 0.125,
        espaco_final: -0.25,
        repeticoes_total: 1.5,
        repeticao_atual: 2.0,
    };

    const DEZENAS_VARIADAS: [u32; 6] = [2, 3, 5, 8, 13, 21];

    /// Features pseudo-variadas, determinísticas, que dependem só de
    /// (concurso, dezena) e têm alguns `NULL`.
    fn features_variadas(c: u32, d: u32, janela: u32) -> FeaturesDezena {
        let mut f = linha(c, d, janela);
        f.frequencia = (c * 7 + d * d * 3 + c * d) % 11;
        f.maior_espaco = if (c + d).is_multiple_of(4) {
            None
        } else {
            Some((c * d) % 9)
        };
        f.maior_espaco_ocorrencias = Some((c + d) % 3);
        f.menor_espaco = if d.is_multiple_of(5) && c.is_multiple_of(2) {
            None
        } else {
            Some((c * 3 + d) % 6)
        };
        f.menor_espaco_ocorrencias = Some((c + 2 * d) % 4);
        f.soma_variacoes = Some((d * c) % 7);
        f.espaco_final = Some((d * 5 + c * 3) % 7);
        f.repeticoes_total = (c + 2 * d) % 5;
        f.repeticao_atual = (c * c + d) % 3;
        f.saiu = Some(d.is_multiple_of(2));
        f
    }

    fn serie_variada(concursos: std::ops::RangeInclusive<u32>, janela: u32) -> Vec<FeaturesDezena> {
        let mut v = Vec::new();
        for c in concursos {
            for d in DEZENAS_VARIADAS {
                v.push(features_variadas(c, d, janela));
            }
        }
        v
    }

    fn calcular_variado(v: &[FeaturesDezena], n: Normalizacao) -> Vec<IndiceDezena> {
        calcular_indice(v, 0, n, &PESOS_VARIADOS).expect("calcula")
    }

    // ------------------------------------------------------------------
    // 1-3. NULL e componentes sem variação
    // ------------------------------------------------------------------

    #[test]
    fn zscore_estatisticas_ignoram_null_e_null_recebe_zero() {
        // maior_espaco [NULL, 1, 1, 5, 5]: só os 4 presentes entram na conta:
        // média 3, variância (4+4+4+4)/4 = 4, desvio 2 → z [NULL→0, −1, −1, 1, 1].
        // (Se o NULL contasse como 0 a média seria 2,4 e os z seriam outros.)
        // Peso 1 só nesse componente → total = z.
        let mut v = Vec::new();
        for (d, m) in [
            (1, None),
            (2, Some(1)),
            (3, Some(1)),
            (4, Some(5)),
            (5, Some(5)),
        ] {
            let mut f = linha(7, d, 0);
            f.maior_espaco = m;
            v.push(f);
        }
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&r, 7), [0.0, -1.0, -1.0, 1.0, 1.0]);
        // Posições: d4 (1) e d5 (1) empatam → d4 1º, d5 2º; d1 (0) 3º; d2, d3 → 4º, 5º.
        assert_eq!(posicoes_de(&r, 7), [3, 4, 5, 1, 2]);
    }

    #[test]
    fn minmax_estatisticas_ignoram_null_e_null_recebe_meio() {
        // maior_espaco [NULL, 2, 4, 6, NULL]: mín 2 e máx 6 só dos presentes →
        // [NULL→0,5, 0, 0,5, 1, NULL→0,5]. (Contando o NULL como 0, mín seria 0.)
        let mut v = Vec::new();
        for (d, m) in [
            (1, None),
            (2, Some(2)),
            (3, Some(4)),
            (4, Some(6)),
            (5, None),
        ] {
            let mut f = linha(7, d, 0);
            f.maior_espaco = m;
            v.push(f);
        }
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&r, 7), [0.5, 0.0, 0.5, 1.0, 0.5]);
    }

    #[test]
    fn componente_constante_recebe_neutro_no_zscore() {
        // frequencia = 7 em todas: desvio 0 → neutro 0 (sem NaN).
        let v = concurso_com_frequencia(3, 0, &[1, 2, 3], &[7, 7, 7]);
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia()).expect("calcula");
        for l in &r {
            assert_eq!(l.total.to_bits(), 0.0_f64.to_bits(), "{l:?}");
        }
    }

    #[test]
    fn componente_constante_recebe_neutro_no_minmax() {
        // frequencia = 7 em todas: máx = mín → neutro 0,5; peso 1 → total 0,5.
        let v = concurso_com_frequencia(3, 0, &[1, 2, 3], &[7, 7, 7]);
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &so_frequencia()).expect("calcula");
        assert_eq!(totais_de(&r, 3), [0.5, 0.5, 0.5]);
    }

    #[test]
    fn componente_todo_null_recebe_neutro_sem_nan() {
        // maior_espaco todo NULL (linha() já deixa None): zscore 0, minmax 0,5.
        let v: Vec<_> = [1, 2, 3, 4].iter().map(|&d| linha(3, d, 0)).collect();
        let z = calcular_indice(&v, 0, Normalizacao::Zscore, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&z, 3), [0.0; 4]);
        let m = calcular_indice(&v, 0, Normalizacao::Minmax, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&m, 3), [0.5; 4]);
        for l in z.iter().chain(&m) {
            assert!(l.total.is_finite(), "{l:?}");
        }
    }

    #[test]
    fn componente_com_um_unico_valor_nao_null_recebe_neutro() {
        // maior_espaco [NULL, 7, NULL, NULL]: um valor só → todos neutros,
        // inclusive a dezena que tem o 7 (não vira 0 nem 1 no minmax).
        let v: Vec<_> = [None, Some(7), None, None]
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let mut f = linha(3, i as u32 + 1, 0);
                f.maior_espaco = m;
                f
            })
            .collect();
        let z = calcular_indice(&v, 0, Normalizacao::Zscore, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&z, 3), [0.0; 4]);
        let m = calcular_indice(&v, 0, Normalizacao::Minmax, &so_maior_espaco()).expect("calcula");
        assert_eq!(totais_de(&m, 3), [0.5; 4]);
    }

    #[test]
    fn todos_os_componentes_sem_variacao_empatam_e_posicoes_seguem_a_dezena() {
        // Todos os 9 componentes constantes em cada concurso, dezenas não
        // contíguas. Zscore: tudo neutro 0 → total 0. Minmax: tudo 0,5 →
        // 0,5 × Σ pesos = 0,5 × 3,875 = 1,9375 (Σ: 1 + 0,5 + 0,75 + 0,25 + 0,5
        // − 2 − 0,125 + 3 + 0 = 3,875).
        let mut v = Vec::new();
        for c in 1..=2 {
            for d in [2, 5, 9] {
                let mut f = linha(c, d, 0);
                f.frequencia = 4;
                f.maior_espaco = Some(3);
                f.maior_espaco_ocorrencias = Some(3);
                f.menor_espaco = Some(3);
                f.menor_espaco_ocorrencias = Some(3);
                f.soma_variacoes = Some(3);
                f.espaco_final = Some(3);
                f.repeticoes_total = 2;
                f.repeticao_atual = 1;
                v.push(f);
            }
        }
        let z = calcular_indice(&v, 0, Normalizacao::Zscore, &PESOS_GOLDEN).expect("calcula");
        let m = calcular_indice(&v, 0, Normalizacao::Minmax, &PESOS_GOLDEN).expect("calcula");
        for c in 1..=2 {
            assert_eq!(totais_de(&z, c), [0.0; 3]);
            assert_eq!(totais_de(&m, c), [1.9375; 3]);
            assert_eq!(posicoes_de(&z, c), [1, 2, 3]);
            assert_eq!(posicoes_de(&m, c), [1, 2, 3]);
        }
        // Posições iguais nos dois concursos → variação 0 no segundo.
        assert_eq!(variacoes_de(&z, 2), [Some(0); 3]);
    }

    // ------------------------------------------------------------------
    // 5-8. Posição e variação
    // ------------------------------------------------------------------

    #[test]
    fn empate_exato_vai_para_a_menor_dezena_com_dezenas_nao_contiguas() {
        // frequencia [d3 = 0, d7 = 1, d20 = 1] em minmax → totais [0, 1, 1].
        // d7 e d20 empatam em 1º: a menor dezena (7) fica na frente; d3 é
        // o último, apesar de ser a menor dezena e o índice 0.
        let v = concurso_com_frequencia(4, 0, &[3, 7, 20], &[0, 1, 1]);
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &so_frequencia()).expect("calcula");
        assert_eq!(totais_de(&r, 4), [0.0, 1.0, 1.0]);
        assert_eq!(posicoes_de(&r, 4), [3, 1, 2]);
    }

    #[test]
    fn empate_exato_no_ultimo_lugar_tambem_vai_para_a_menor_dezena() {
        // frequencia [d3 = 1, d7 = 0, d20 = 0] → totais [1, 0, 0]:
        // d3 1º; d7 e d20 empatam em último → d7 2º, d20 3º.
        let v = concurso_com_frequencia(4, 0, &[3, 7, 20], &[1, 0, 0]);
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &so_frequencia()).expect("calcula");
        assert_eq!(posicoes_de(&r, 4), [1, 2, 3]);
    }

    #[test]
    fn variacao_de_posicao_tem_sinal_certo_em_tres_concursos() {
        // Peso só em frequencia, valores todos distintos → posição = ordem
        // decrescente da frequência. Dezenas 1, 2, 3.
        //  c1 freq [3, 2, 1] → posições [1, 2, 3]            variação NULL
        //  c2 freq [1, 2, 3] → posições [3, 2, 1]
        //     variação = ant − atual: d1 1−3 = −2 (desceu), d2 2−2 = 0, d3 3−1 = +2 (subiu)
        //  c3 freq [2, 3, 1] → posições [2, 1, 3]
        //     d1 3−2 = +1, d2 2−1 = +1, d3 1−3 = −2
        let mut v = concurso_com_frequencia(1, 0, &[1, 2, 3], &[3, 2, 1]);
        v.extend(concurso_com_frequencia(2, 0, &[1, 2, 3], &[1, 2, 3]));
        v.extend(concurso_com_frequencia(3, 0, &[1, 2, 3], &[2, 3, 1]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia()).expect("calcula");
        assert_eq!(posicoes_de(&r, 1), [1, 2, 3]);
        assert_eq!(posicoes_de(&r, 2), [3, 2, 1]);
        assert_eq!(posicoes_de(&r, 3), [2, 1, 3]);
        assert_eq!(variacoes_de(&r, 1), [None, None, None]);
        assert_eq!(variacoes_de(&r, 2), [Some(-2), Some(0), Some(2)]);
        assert_eq!(variacoes_de(&r, 3), [Some(1), Some(1), Some(-2)]);
    }

    #[test]
    fn snapshot_do_proximo_concurso_com_saiu_none_e_concurso_normal() {
        // Concursos 1..=3 sorteados (saiu = Some), o 4 é o snapshot (None).
        // c3 freq [1, 2, 3] → posições [3, 2, 1]; c4 freq [3, 2, 1] → [1, 2, 3].
        // Variação do c4 contra o c3: d1 3−1 = +2, d2 0, d3 1−3 = −2.
        let mut v = Vec::new();
        for (c, freqs) in [(1, [1, 1, 2]), (2, [2, 1, 3]), (3, [1, 2, 3])] {
            let mut linhas = concurso_com_frequencia(c, 0, &[1, 2, 3], &freqs);
            for f in &mut linhas {
                f.saiu = Some(f.dezena == 1);
            }
            v.extend(linhas);
        }
        let mut c4 = concurso_com_frequencia(4, 0, &[1, 2, 3], &[3, 2, 1]);
        for f in &mut c4 {
            assert_eq!(f.saiu, None);
        }
        v.extend(c4);
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia()).expect("calcula");
        assert_eq!(do_concurso(&r, 4).len(), 3);
        assert_eq!(posicoes_de(&r, 3), [3, 2, 1]);
        assert_eq!(posicoes_de(&r, 4), [1, 2, 3]);
        assert_eq!(variacoes_de(&r, 4), [Some(2), Some(0), Some(-2)]);
    }

    #[test]
    fn primeiro_concurso_da_entrada_nao_precisa_ser_1() {
        // Série 100..=101: o 100 é o primeiro da entrada → variação NULL; o
        // 101 é medido contra o 100.
        // c100 freq [2, 1] → posições [1, 2]; c101 freq [1, 2] → [2, 1];
        // variação c101: d1 1−2 = −1, d2 2−1 = +1.
        let mut v = concurso_com_frequencia(100, 0, &[4, 9], &[2, 1]);
        v.extend(concurso_com_frequencia(101, 0, &[4, 9], &[1, 2]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia()).expect("calcula");
        assert_eq!(variacoes_de(&r, 100), [None, None]);
        assert_eq!(variacoes_de(&r, 101), [Some(-1), Some(1)]);
        assert_eq!(r.iter().map(|l| l.concurso).min(), Some(100));
    }

    // ------------------------------------------------------------------
    // 9-11. Determinismo, ordem da entrada, janelas, `saiu`
    // ------------------------------------------------------------------

    #[test]
    fn duas_chamadas_dao_saida_identica_bit_a_bit() {
        for n in NORMALIZACOES {
            let v = serie_variada(1..=5, 0);
            let a = calcular_variado(&v, n);
            let b = calcular_variado(&v, n);
            assert_identico(&a, &b);
            let ga = calcular_indice(&golden(), 0, n, &PESOS_GOLDEN).expect("calcula");
            let gb = calcular_indice(&golden(), 0, n, &PESOS_GOLDEN).expect("calcula");
            assert_identico(&ga, &gb);
        }
    }

    #[test]
    fn entrada_embaralhada_da_a_mesma_saida_da_ordenada() {
        for n in NORMALIZACOES {
            let v = serie_variada(1..=5, 0);
            let esperado = calcular_variado(&v, n);

            let mut invertida = v.clone();
            invertida.reverse();
            assert_identico(&calcular_variado(&invertida, n), &esperado);

            // Permutação fixa (sem rand): rotação + trocas.
            let mut permutada = v.clone();
            permutada.rotate_left(11);
            permutada.swap(0, 29);
            permutada.swap(3, 17);
            permutada.swap(5, 22);
            assert_ne!(permutada, v, "a permutação tem de mudar a ordem");
            assert_identico(&calcular_variado(&permutada, n), &esperado);
        }
    }

    #[test]
    fn linhas_de_outras_janelas_nao_mudam_o_resultado_da_janela_pedida() {
        for n in NORMALIZACOES {
            let base = calcular_indice(&golden(), 0, n, &PESOS_GOLDEN).expect("calcula");
            // Mesmos concursos e dezenas na janela 10, com valores malucos.
            let mut v = golden();
            for g in golden() {
                let mut f = linha(g.concurso, g.dezena, 10);
                f.frequencia = 4_000_000_000 - g.dezena * 1000;
                f.maior_espaco = Some(g.dezena * 999_983);
                f.menor_espaco = Some(u32::MAX - g.dezena);
                f.espaco_final = if g.dezena == 2 { None } else { Some(1) };
                f.repeticoes_total = g.dezena * g.concurso * 77;
                f.repeticao_atual = 5 - g.dezena;
                v.push(f);
            }
            let com_ruido = calcular_indice(&v, 0, n, &PESOS_GOLDEN).expect("calcula");
            assert_identico(&com_ruido, &base);
        }
    }

    #[test]
    fn pedir_outra_janela_usa_so_as_linhas_dela() {
        // A janela 10 = valores da série variada; a janela 0 = golden. O
        // resultado da janela 10 tem de ser o mesmo de uma entrada só com as
        // linhas da janela 10 (rotuladas como janela 0).
        let mut v = golden();
        for c in 10..=11 {
            for d in 1..=4 {
                v.push(features_variadas(c, d, 10));
            }
        }
        let so_janela_10: Vec<_> = v
            .iter()
            .filter(|f| f.janela == 10)
            .cloned()
            .map(|mut f| {
                f.janela = 0;
                f
            })
            .collect();
        for n in NORMALIZACOES {
            let pedida = calcular_indice(&v, 10, n, &PESOS_VARIADOS).expect("calcula");
            let esperada = calcular_indice(&so_janela_10, 0, n, &PESOS_VARIADOS).expect("calcula");
            assert_identico(&pedida, &esperada);
            let da_janela_0 = calcular_indice(&v, 0, n, &PESOS_VARIADOS).expect("calcula");
            assert_ne!(
                pedida, da_janela_0,
                "as janelas têm de dar resultados diferentes"
            );
        }
    }

    #[test]
    fn saiu_nao_e_lido() {
        for n in NORMALIZACOES {
            let base = calcular_indice(&golden(), 0, n, &PESOS_GOLDEN).expect("calcula");
            let mut invertido = golden();
            for f in &mut invertido {
                f.saiu = f.saiu.map(|b| !b);
            }
            assert_identico(
                &calcular_indice(&invertido, 0, n, &PESOS_GOLDEN).expect("calcula"),
                &base,
            );
            let mut todos_none = golden();
            for f in &mut todos_none {
                f.saiu = None;
            }
            assert_identico(
                &calcular_indice(&todos_none, 0, n, &PESOS_GOLDEN).expect("calcula"),
                &base,
            );
            let mut todos_true = golden();
            for f in &mut todos_true {
                f.saiu = Some(true);
            }
            assert_identico(
                &calcular_indice(&todos_true, 0, n, &PESOS_GOLDEN).expect("calcula"),
                &base,
            );
            // Os casos acima mudam `saiu` igual em todas as dezenas, e uma
            // constante some na normalização. Aqui `saiu` varia entre as
            // dezenas (dois padrões opostos): uma leitura dele mudaria a
            // ordem relativa e, portanto, o total ou a posição. Todos os pesos
            // são diferentes de 0, para nenhum componente ficar invisível.
            let pesos = Pesos {
                frequencia: 1.0,
                maior_espaco: 0.9,
                maior_espaco_ocorrencias: 0.8,
                menor_espaco: 0.7,
                menor_espaco_ocorrencias: 0.6,
                soma_variacoes: -0.5,
                espaco_final: 0.4,
                repeticoes_total: 0.3,
                repeticao_atual: 0.2,
            };
            let base = calcular_indice(&golden(), 0, n, &pesos).expect("calcula");
            for par_saiu in [true, false] {
                let mut alternado = golden();
                for f in &mut alternado {
                    f.saiu = Some((f.dezena % 2 == 0) == par_saiu);
                }
                assert_identico(
                    &calcular_indice(&alternado, 0, n, &pesos).expect("calcula"),
                    &base,
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // 12. Ponto no tempo
    // ------------------------------------------------------------------

    #[test]
    fn mudar_qualquer_componente_do_concurso_seguinte_nao_altera_concursos_anteriores() {
        type Mutador = fn(&mut FeaturesDezena, u32);
        let mutadores: [(&str, Mutador); 9] = [
            ("frequencia", |f, x| f.frequencia = x),
            ("maior_espaco", |f, x| f.maior_espaco = Some(x)),
            ("maior_espaco_ocorrencias", |f, x| {
                f.maior_espaco_ocorrencias = Some(x)
            }),
            ("menor_espaco", |f, x| f.menor_espaco = Some(x)),
            ("menor_espaco_ocorrencias", |f, x| {
                f.menor_espaco_ocorrencias = Some(x)
            }),
            ("soma_variacoes", |f, x| f.soma_variacoes = Some(x)),
            ("espaco_final", |f, x| f.espaco_final = Some(x)),
            ("repeticoes_total", |f, x| f.repeticoes_total = x),
            ("repeticao_atual", |f, x| f.repeticao_atual = x),
        ];
        for n in NORMALIZACOES {
            let v = serie_variada(1..=4, 0);
            let base = calcular_variado(&v, n);
            let ate_3: Vec<_> = base.iter().filter(|l| l.concurso <= 3).cloned().collect();
            for (nome, mutar) in mutadores {
                let mut alt = v.clone();
                for f in alt.iter_mut().filter(|f| f.concurso == 4) {
                    mutar(f, 100 + f.dezena * 37 % 17);
                }
                let novo = calcular_variado(&alt, n);
                let novo_ate_3: Vec<_> = novo.iter().filter(|l| l.concurso <= 3).cloned().collect();
                assert_identico(&novo_ate_3, &ate_3);
                // Sanidade: o concurso 4 em si mudou (o teste tem força).
                assert_ne!(
                    do_concurso(&novo, 4),
                    do_concurso(&base, 4),
                    "mutar `{nome}` deveria mudar o concurso 4"
                );
            }
        }
    }

    #[test]
    fn acrescentar_ou_retirar_o_concurso_seguinte_nao_altera_concursos_anteriores() {
        for n in NORMALIZACOES {
            let curta = calcular_variado(&serie_variada(1..=3, 0), n);
            let longa = calcular_variado(&serie_variada(1..=5, 0), n);
            let longa_ate_3: Vec<_> = longa.iter().filter(|l| l.concurso <= 3).cloned().collect();
            assert_identico(&longa_ate_3, &curta);
            assert_eq!(do_concurso(&longa, 4).len(), DEZENAS_VARIADAS.len());
            assert_eq!(do_concurso(&longa, 5).len(), DEZENAS_VARIADAS.len());
        }
    }

    #[test]
    fn indice_de_t_depende_so_das_features_de_t_e_da_posicao_de_t_menos_1() {
        for n in NORMALIZACOES {
            for t in [3_u32, 5] {
                let inteira = calcular_variado(&serie_variada(1..=5, 0), n);
                let so_dois = calcular_variado(&serie_variada(t - 1..=t, 0), n);
                // total, posição e variação de t idênticos.
                assert_identico(&do_concurso(&so_dois, t), &do_concurso(&inteira, t));
                // E t−1 sozinho é o primeiro da entrada curta → variação NULL.
                assert_eq!(
                    variacoes_de(&so_dois, t - 1),
                    vec![None; DEZENAS_VARIADAS.len()]
                );
            }
        }
    }

    #[test]
    fn mudar_a_ordem_de_t_menos_1_altera_so_a_variacao_de_t() {
        // t = 3, t−1 = 2. Inverte a frequência do concurso 2 (20 − x), o que
        // muda a ordem de 2 (verificado abaixo). O total e a posição de 3
        // dependem só das features de 3; só a variação de 3 pode mudar.
        for n in NORMALIZACOES {
            let v = serie_variada(1..=5, 0);
            let base = calcular_variado(&v, n);
            let mut alt = v.clone();
            for f in alt.iter_mut().filter(|f| f.concurso == 2) {
                f.frequencia = 20 - f.frequencia;
            }
            let novo = calcular_variado(&alt, n);
            assert_ne!(
                posicoes_de(&base, 2),
                posicoes_de(&novo, 2),
                "a alteração tem de mudar a ordem do concurso 2"
            );

            let (b3, n3) = (do_concurso(&base, 3), do_concurso(&novo, 3));
            for (b, x) in b3.iter().zip(&n3) {
                assert_eq!(b.total.to_bits(), x.total.to_bits(), "{b:?} vs {x:?}");
                assert_eq!(b.posicao, x.posicao, "{b:?} vs {x:?}");
            }
            // Variação nova = posição nova de 2 − posição de 3.
            let pos2 = posicoes_de(&novo, 2);
            for (i, x) in n3.iter().enumerate() {
                assert_eq!(
                    x.variacao_posicao,
                    Some(i64::from(pos2[i]) - i64::from(x.posicao)),
                    "{x:?}"
                );
            }
            assert_ne!(variacoes_de(&base, 3), variacoes_de(&novo, 3));
            // Concursos depois de 3 dependem de 3 (inalterado): iguais.
            for c in 4..=5 {
                assert_identico(&do_concurso(&novo, c), &do_concurso(&base, c));
            }
            // Concurso 1 intocado.
            assert_identico(&do_concurso(&novo, 1), &do_concurso(&base, 1));
        }
    }

    // ------------------------------------------------------------------
    // 13 e 15. Entrada vazia, formato da saída
    // ------------------------------------------------------------------

    #[test]
    fn entrada_vazia_devolve_vazio() {
        let r = calcular_indice(&[], 0, Normalizacao::Zscore, &PESOS_GOLDEN).expect("vazio é Ok");
        assert!(r.is_empty());
        let r = calcular_indice(&[], 50, Normalizacao::Minmax, &PESOS_GOLDEN).expect("vazio é Ok");
        assert_eq!(r, vec![]);
    }

    #[test]
    fn saida_ordenada_por_concurso_e_dezena_com_n_linhas_por_concurso() {
        let mut v = serie_variada(1..=5, 0);
        v.reverse(); // entrada fora de ordem de propósito
        let r = calcular_variado(&v, Normalizacao::Zscore);
        assert_eq!(r.len(), 5 * DEZENAS_VARIADAS.len());
        for par in r.windows(2) {
            assert!(
                (par[0].concurso, par[0].dezena) < (par[1].concurso, par[1].dezena),
                "fora de ordem: {:?} antes de {:?}",
                par[0],
                par[1]
            );
        }
        for c in 1..=5 {
            let linhas = do_concurso(&r, c);
            let dezenas: Vec<u32> = linhas.iter().map(|l| l.dezena).collect();
            assert_eq!(dezenas, DEZENAS_VARIADAS);
            // Posições do concurso são uma permutação de 1..=N.
            let mut posicoes: Vec<u32> = linhas.iter().map(|l| l.posicao).collect();
            posicoes.sort_unstable();
            assert_eq!(posicoes, [1, 2, 3, 4, 5, 6]);
        }
    }

    #[test]
    fn posicao_segue_o_total_decrescente_em_cada_concurso() {
        // Coerência entre colunas: refazer a ordem a partir de `total`
        // (decrescente, desempate pela menor dezena) dá a `posicao` gravada.
        for n in NORMALIZACOES {
            let r = calcular_variado(&serie_variada(1..=5, 0), n);
            for c in 1..=5 {
                let mut linhas = do_concurso(&r, c);
                linhas.sort_by(|a, b| b.total.total_cmp(&a.total).then(a.dezena.cmp(&b.dezena)));
                for (i, l) in linhas.iter().enumerate() {
                    assert_eq!(l.posicao, i as u32 + 1, "{l:?}");
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 14. Erros de entrada
    // ------------------------------------------------------------------

    #[test]
    fn erro_a_dezena_repetida_no_concurso() {
        let mut v = concurso_com_frequencia(5, 0, &[1, 2, 3], &[1, 2, 3]);
        v.extend(concurso_com_frequencia(6, 0, &[1, 2, 3], &[1, 2, 3]));
        v.push(linha(6, 2, 0)); // (6, 2) duplicada
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("dezena 2"), "{msg}");
        assert!(msg.contains("concurso 6"), "{msg}");
    }

    #[test]
    fn erro_a_dezena_repetida_no_primeiro_concurso() {
        let mut v = concurso_com_frequencia(5, 0, &[1, 2, 3], &[1, 2, 3]);
        v.push(linha(5, 3, 0));
        let msg = erro_entrada(calcular_indice(
            &v,
            0,
            Normalizacao::Minmax,
            &so_frequencia(),
        ));
        assert!(msg.contains("dezena 3"), "{msg}");
        assert!(msg.contains("concurso 5"), "{msg}");
    }

    #[test]
    fn erro_b_buraco_entre_concursos() {
        // Concursos 10 e 12, sem o 11.
        let mut v = concurso_com_frequencia(10, 0, &[1, 2], &[1, 2]);
        v.extend(concurso_com_frequencia(12, 0, &[1, 2], &[2, 1]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(
            msg.contains("11"),
            "a mensagem deve citar o concurso 11: {msg}"
        );
    }

    #[test]
    fn erro_c_concurso_com_dezena_a_menos() {
        let mut v = concurso_com_frequencia(1, 0, &[1, 2, 3], &[1, 2, 3]);
        v.extend(concurso_com_frequencia(2, 0, &[1, 2], &[1, 2]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("concurso 2"), "{msg}");
    }

    #[test]
    fn erro_c_concurso_com_dezena_a_mais() {
        let mut v = concurso_com_frequencia(1, 0, &[1, 2, 3], &[1, 2, 3]);
        v.extend(concurso_com_frequencia(2, 0, &[1, 2, 3, 4], &[1, 2, 3, 4]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("concurso 2"), "{msg}");
    }

    #[test]
    fn erro_c_concurso_com_dezena_trocada() {
        // Mesmo tamanho (3), mas {1, 2, 4} no lugar de {1, 2, 3}.
        let mut v = concurso_com_frequencia(1, 0, &[1, 2, 3], &[1, 2, 3]);
        v.extend(concurso_com_frequencia(2, 0, &[1, 2, 4], &[1, 2, 3]));
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("concurso 2"), "{msg}");
    }

    #[test]
    fn erro_d_nenhuma_linha_da_janela_pedida_havendo_outra_janela() {
        let v = concurso_com_frequencia(3, 10, &[1, 2, 3], &[1, 2, 3]);
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("janela 0"), "{msg}");
    }

    #[test]
    fn erro_e_janela_pedida_cobre_faixa_menor_que_a_das_outras() {
        // Janela 0 tem 10..=11; janela 50 tem 10..=12.
        let mut v = Vec::new();
        for c in 10..=11 {
            v.extend(concurso_com_frequencia(c, 0, &[1, 2], &[1, 2]));
        }
        for c in 10..=12 {
            v.extend(concurso_com_frequencia(c, 50, &[1, 2], &[1, 2]));
        }
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("10..=11"), "{msg}");
        assert!(msg.contains("10..=12"), "{msg}");
        // A janela que cobre tudo (50) é aceita.
        let ok = calcular_indice(&v, 50, Normalizacao::Zscore, &so_frequencia()).expect("calcula");
        assert_eq!(ok.len(), 3 * 2);
    }

    #[test]
    fn erro_e_janela_pedida_sem_o_primeiro_concurso_das_outras() {
        // Janela 0 tem 11..=12; janela 50 tem 10..=12 (faltam no começo).
        let mut v = Vec::new();
        for c in 11..=12 {
            v.extend(concurso_com_frequencia(c, 0, &[1, 2], &[1, 2]));
        }
        for c in 10..=12 {
            v.extend(concurso_com_frequencia(c, 50, &[1, 2], &[1, 2]));
        }
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &so_frequencia());
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
    }

    #[test]
    fn erro_f_peso_nan() {
        let v = concurso_com_frequencia(1, 0, &[1, 2], &[1, 2]);
        let pesos = Pesos {
            frequencia: f64::NAN,
            ..pesos_zerados()
        };
        let r = calcular_indice(&v, 0, Normalizacao::Zscore, &pesos);
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("frequencia"), "{msg}");
    }

    #[test]
    fn erro_f_peso_infinito() {
        let v = concurso_com_frequencia(1, 0, &[1, 2], &[1, 2]);
        let pesos = Pesos {
            repeticao_atual: f64::INFINITY,
            ..pesos_zerados()
        };
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &pesos);
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("repeticao_atual"), "{msg}");
    }

    #[test]
    fn erro_g_total_nao_finito_por_overflow() {
        // Minmax: a dezena 2 tem frequencia = 1 e maior_espaco = 1 (normalizados
        // 1 e 1); com peso 1e308 em cada, 1e308 + 1e308 = 2e308 > f64::MAX → inf.
        let grande = 1e308_f64;
        let soma = std::hint::black_box(grande) + grande;
        assert!(soma.is_infinite(), "premissa: 1e308 + 1e308 estoura f64");

        let mut v = Vec::new();
        for (d, x) in [(1, 0), (2, 1)] {
            let mut f = linha(9, d, 0);
            f.frequencia = x;
            f.maior_espaco = Some(x);
            v.push(f);
        }
        let pesos = Pesos {
            frequencia: grande,
            maior_espaco: grande,
            ..pesos_zerados()
        };
        let r = calcular_indice(&v, 0, Normalizacao::Minmax, &pesos);
        assert!(matches!(
            &r,
            Err(SpiderError::MotorIndiceEntradaInvalida(_))
        ));
        let msg = erro_entrada(r);
        assert!(msg.contains("dezena 2"), "{msg}");
        assert!(msg.contains("concurso 9"), "{msg}");
    }
}
