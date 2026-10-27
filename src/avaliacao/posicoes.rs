//! Taxas históricas por posição e por (componente, faixa) — Fase 1f, tarefa
//! 1f.1.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1f" (decisões de 2026-10-27).
//! Função pura: recebe, por concurso, as dezenas com a posição que o índice
//! lhes deu **antes** do sorteio, se saíram e os valores crus dos 9
//! componentes; devolve, para cada (concurso, dezena), as taxas que o passado
//! dava à posição e às faixas dela. Sem I/O e sem SQL. É descrição do passado:
//! sorteios são independentes e o esperado de qualquer ranking feito com estas
//! taxas é **não superar o acaso**.
//!
//! # Taxa por posição
//!
//! `p_t(r) = (saídas da posição r antes de t + m·K/N) / (concursos antes de t + m)`
//! com `m` = [`CONCURSOS_FICTICIOS`] e o histórico inteiro, a mesma da
//! calibração da 1c ([`super::calibrar`]); a conta inteira é a mesma, então o
//! `f64` sai igual bit a bit.
//!
//! # Faixas dos componentes
//!
//! Em cada concurso e para cada um dos 9 componentes, as dezenas são
//! repartidas em tercis ([`Faixa`]) **entre as dezenas daquele concurso**,
//! sobre os valores crus de `features_dezena` (z-score e minmax preservam a
//! ordem, então o resultado é o mesmo do componente normalizado):
//!
//! - `menores` = quantas dezenas têm valor **estritamente menor**; `n` =
//!   dezenas com valor não `NULL` no concurso; tercil = `⌊3·menores / n⌋`
//!   (0 = baixo, 1 = médio, 2 = alto), em inteiros;
//! - empatadas caem na mesma faixa;
//! - `NULL` é a faixa própria [`Faixa::Nulo`], fora da conta de `n`;
//! - componente **constante** no concurso (todos os valores não `NULL` iguais,
//!   inclusive uma única dezena com valor): todas em "médio", como o neutro do
//!   índice. A fórmula sozinha daria "baixo" (`menores` = 0); o caso é tratado
//!   à parte.
//!
//! # Taxa por (componente, faixa)
//!
//! `(saídas na faixa antes de t + m·K/N) / (pares concurso×dezena na faixa antes de t + m)`
//! com o mesmo `m`, contado em pares: a faixa não tem tamanho fixo por
//! concurso, então o prior vale `m` pares fictícios.
//!
//! # Janela dos componentes
//!
//! Quem monta a entrada escolhe as linhas de `features_dezena` da janela do
//! perfil ativo (a mesma do índice que dá a posição); aqui não há janela.
//!
//! # Ponto no tempo
//!
//! Ordem do laço: taxas de `t` (com as contagens de `1 … t−1`) → só então `t`
//! entra nas contagens. [`TaxasAcumuladas::linhas`] exige que o concurso seja
//! o seguinte ao último incorporado, então as taxas nunca vêm de um estado
//! que já viu `t` (ou que ainda não viu `t−1`). O snapshot do próximo concurso
//! (`saiu` ausente) usa todo o histórico sorteado.
//!
//! # Incremental
//!
//! [`TaxasAcumuladas`] acumula contagens inteiras concurso a concurso;
//! processar tudo de uma vez ([`calcular_taxas`]) e processar em partes dão o
//! mesmo estado. Contas em inteiros e uma única divisão no fim; sem
//! `HashMap`, a saída tem ordem fixa.

use super::calibracao::{ordenar_e_validar, CONCURSOS_FICTICIOS};
use super::Observacao;
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::motor::FeaturesDezena;
use serde::{Deserialize, Serialize};

/// Quantos componentes entram no índice.
pub const QTD_COMPONENTES: usize = 9;

/// Quantas faixas existem: baixo, médio, alto e `NULL`.
pub const QTD_FAIXAS: usize = 4;

/// Nomes dos componentes na ordem de [`ObservacaoPosicoes::componentes`]: a
/// das colunas de `features_dezena` e de `Pesos::componentes`.
pub const NOMES_COMPONENTES: [&str; QTD_COMPONENTES] = [
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
];

/// Faixa (tercil) de um componente num concurso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Faixa {
    Baixo,
    Medio,
    Alto,
    /// O componente é `NULL` para a dezena naquele concurso.
    Nulo,
}

impl Faixa {
    /// Na ordem de [`Faixa::indice`].
    pub const TODAS: [Faixa; QTD_FAIXAS] = [Faixa::Baixo, Faixa::Medio, Faixa::Alto, Faixa::Nulo];

    /// Posição da faixa em [`Faixa::TODAS`] (0 … 3).
    pub const fn indice(self) -> usize {
        match self {
            Faixa::Baixo => 0,
            Faixa::Medio => 1,
            Faixa::Alto => 2,
            Faixa::Nulo => 3,
        }
    }

    pub const fn nome(self) -> &'static str {
        match self {
            Faixa::Baixo => "baixo",
            Faixa::Medio => "médio",
            Faixa::Alto => "alto",
            Faixa::Nulo => "nulo",
        }
    }
}

/// Uma dezena num concurso: posição que o índice lhe deu (antes do sorteio),
/// se saiu e os valores crus dos 9 componentes. Montada por quem lê o banco a
/// partir de `indice` (posição) e `features_dezena` (componentes e `saiu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservacaoPosicoes {
    pub concurso: u32,
    pub dezena: u32,
    /// 1 … N no concurso (1 = mais forte), como em `IndiceDezena::posicao`.
    pub posicao: u32,
    /// `None` só no snapshot do próximo concurso, ainda não sorteado.
    pub saiu: Option<bool>,
    /// Valores crus dos componentes, na ordem de [`NOMES_COMPONENTES`];
    /// `None` = `NULL` em `features_dezena`.
    pub componentes: [Option<i64>; QTD_COMPONENTES],
}

impl ObservacaoPosicoes {
    /// Os 9 valores crus de uma linha de `features_dezena`, na ordem de
    /// [`NOMES_COMPONENTES`]. Os componentes que nunca são `NULL` entram como
    /// `Some`.
    pub fn componentes_de(f: &FeaturesDezena) -> [Option<i64>; QTD_COMPONENTES] {
        let opcional = |v: Option<u32>| v.map(i64::from);
        [
            Some(i64::from(f.frequencia)),
            opcional(f.maior_espaco),
            opcional(f.maior_espaco_ocorrencias),
            opcional(f.menor_espaco),
            opcional(f.menor_espaco_ocorrencias),
            opcional(f.soma_variacoes),
            opcional(f.espaco_final),
            Some(i64::from(f.repeticoes_total)),
            Some(i64::from(f.repeticao_atual)),
        ]
    }
}

/// Saídas e total (concursos para a posição, pares concurso×dezena para a
/// faixa) de uma posição ou faixa, só do passado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contagem {
    pub saidas: u64,
    pub total: u64,
}

/// O que o passado dava a uma dezena num concurso `t`: a taxa da posição e a
/// faixa e a taxa de cada componente. Insumo das notas A e B (1f.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinhaTaxas {
    pub concurso: u32,
    pub dezena: u32,
    pub posicao: u32,
    /// Copiado da entrada; não entra em nenhuma taxa.
    pub saiu: Option<bool>,
    /// `p_t(posicao)`, só com os concursos anteriores.
    pub taxa_posicao: f64,
    /// Faixa da dezena em cada componente, neste concurso.
    pub faixas: [Faixa; QTD_COMPONENTES],
    /// Taxa do (componente, faixa) acima, só com os concursos anteriores.
    pub taxas_faixa: [f64; QTD_COMPONENTES],
}

/// Resultado de [`calcular_taxas`].
#[derive(Debug, Clone)]
pub struct SerieTaxas {
    /// Uma linha por (concurso, dezena), ordenadas por (concurso, dezena),
    /// inclusive o snapshot ainda não sorteado.
    pub linhas: Vec<LinhaTaxas>,
    /// Contagens com todos os concursos sorteados da entrada: as taxas do
    /// próximo concurso (ou, se a entrada já traz o snapshot, as que ele usou).
    pub acumuladas: TaxasAcumuladas,
}

/// Contagens acumuladas, concurso a concurso, de que saem as taxas.
#[derive(Debug, Clone)]
pub struct TaxasAcumuladas {
    cfg: LoteriaConfig,
    ultimo_concurso: Option<u32>,
    concursos: u64,
    /// `saidas_posicao[r−1]`: quantas vezes a posição `r` saiu.
    saidas_posicao: Vec<u64>,
    saidas_faixa: [[u64; QTD_FAIXAS]; QTD_COMPONENTES],
    pares_faixa: [[u64; QTD_FAIXAS]; QTD_COMPONENTES],
}

impl TaxasAcumuladas {
    /// Estado vazio: nenhum concurso incorporado, todas as taxas = *K/N*.
    pub fn novas(cfg: &LoteriaConfig) -> Self {
        Self {
            cfg: *cfg,
            ultimo_concurso: None,
            concursos: 0,
            saidas_posicao: vec![0; cfg.qtd_dezenas_possiveis() as usize],
            saidas_faixa: [[0; QTD_FAIXAS]; QTD_COMPONENTES],
            pares_faixa: [[0; QTD_FAIXAS]; QTD_COMPONENTES],
        }
    }

    /// Último concurso incorporado, se houver.
    pub fn ultimo_concurso(&self) -> Option<u32> {
        self.ultimo_concurso
    }

    /// Quantos concursos sorteados já entraram nas contagens.
    pub fn concursos_incorporados(&self) -> u64 {
        self.concursos
    }

    /// Contagem da posição `posicao` (1 … N); `None` fora disso.
    pub fn contagem_posicao(&self, posicao: u32) -> Option<Contagem> {
        let saidas = *self
            .saidas_posicao
            .get((posicao.checked_sub(1)?) as usize)?;
        Some(Contagem {
            saidas,
            total: self.concursos,
        })
    }

    /// Contagem do (componente, faixa); `None` se `componente` ≥ 9.
    pub fn contagem_faixa(&self, componente: usize, faixa: Faixa) -> Option<Contagem> {
        let f = faixa.indice();
        Some(Contagem {
            saidas: *self.saidas_faixa.get(componente)?.get(f)?,
            total: *self.pares_faixa.get(componente)?.get(f)?,
        })
    }

    /// Taxa da posição `posicao` com o histórico incorporado; `None` fora de
    /// 1 … N.
    pub fn taxa_posicao(&self, posicao: u32) -> Option<f64> {
        let c = self.contagem_posicao(posicao)?;
        Some(self.taxa(c))
    }

    /// Taxa do (componente, faixa) com o histórico incorporado; `None` se
    /// `componente` ≥ 9.
    pub fn taxa_faixa(&self, componente: usize, faixa: Faixa) -> Option<f64> {
        let c = self.contagem_faixa(componente, faixa)?;
        Some(self.taxa(c))
    }

    /// `(saídas·N + m·K) / ((total + m)·N)`: o mesmo valor de
    /// `(saídas + m·K/N) / (total + m)` com uma única divisão no fim. Os
    /// inteiros ficam muito abaixo de 2^53, então o `f64` sai correto até o
    /// último bit (e igual ao de `calibrar` para a posição).
    fn taxa(&self, c: Contagem) -> f64 {
        let n = u64::from(self.cfg.qtd_dezenas_possiveis());
        let k = self.cfg.qtd_dezenas as u64;
        let m = u64::from(CONCURSOS_FICTICIOS);
        (c.saidas * n + m * k) as f64 / ((c.total + m) * n) as f64
    }

    /// As taxas que o histórico incorporado dá a cada dezena de `concurso`,
    /// sem alterar o estado. O concurso tem de ser o seguinte ao último
    /// incorporado (qualquer um, se nada foi incorporado), para as taxas
    /// verem exatamente `… t−1`. Pode ser o snapshot ainda não sorteado.
    ///
    /// As linhas vêm ordenadas por dezena. Entrada inválida (vazia, de mais
    /// de um concurso, dezenas ou posições que não formam o concurso da
    /// loteria, `saiu` misto ou com ≠ *K* saídas) é
    /// [`SpiderError::AvaliacaoEntradaInvalida`].
    pub fn linhas(&self, concurso: &[ObservacaoPosicoes]) -> Result<Vec<LinhaTaxas>> {
        let grupo = self.validar_concurso(concurso)?;
        let faixas = faixas_do_grupo(&grupo);
        Ok(self.linhas_do_grupo(&grupo, &faixas))
    }

    /// Incorpora um concurso **sorteado** às contagens. Mesmas exigências de
    /// [`TaxasAcumuladas::linhas`], mais: todo `saiu` conhecido. Se der erro,
    /// o estado não muda.
    pub fn incorporar(&mut self, concurso: &[ObservacaoPosicoes]) -> Result<()> {
        let grupo = self.validar_concurso(concurso)?;
        if grupo[0].saiu.is_none() {
            return Err(invalida(format!(
                "concurso {} sem resultado (`saiu` NULL): só concursos sorteados entram nas contagens",
                grupo[0].concurso
            )));
        }
        let faixas = faixas_do_grupo(&grupo);
        self.incorporar_do_grupo(&grupo, &faixas);
        Ok(())
    }

    /// Valida um concurso isolado e o devolve ordenado por dezena.
    fn validar_concurso(&self, concurso: &[ObservacaoPosicoes]) -> Result<Vec<ObservacaoPosicoes>> {
        let Some(primeiro) = concurso.first() else {
            return Err(invalida("concurso sem nenhuma dezena".to_string()));
        };
        let numero = primeiro.concurso;
        if let Some(outro) = concurso.iter().find(|o| o.concurso != numero) {
            return Err(invalida(format!(
                "as dezenas misturam os concursos {numero} e {}: passe um concurso por vez",
                outro.concurso
            )));
        }
        if let Some(ultimo) = self.ultimo_concurso {
            if numero != ultimo + 1 {
                return Err(invalida(format!(
                    "esperado o concurso {} (o seguinte ao último incorporado, {ultimo}), mas veio o {numero}",
                    ultimo + 1
                )));
            }
        }
        let mut grupo = concurso.to_vec();
        grupo.sort_unstable_by_key(|o| o.dezena);
        // Reaproveita as regras da calibração da 1c: dezenas da loteria uma vez
        // cada, posições uma permutação de 1..=N, `saiu` todo conhecido (com K
        // saídas) ou todo ausente.
        ordenar_e_validar(&self.cfg, &projetar(&grupo))?;
        Ok(grupo)
    }

    fn linhas_do_grupo(
        &self,
        grupo: &[ObservacaoPosicoes],
        faixas: &[[Faixa; QTD_COMPONENTES]],
    ) -> Vec<LinhaTaxas> {
        grupo
            .iter()
            .zip(faixas)
            .map(|(o, f)| LinhaTaxas {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                saiu: o.saiu,
                // O grupo foi validado: posicao em 1..=N.
                taxa_posicao: self.taxa(Contagem {
                    saidas: self.saidas_posicao[(o.posicao - 1) as usize],
                    total: self.concursos,
                }),
                faixas: *f,
                taxas_faixa: std::array::from_fn(|c| {
                    let i = f[c].indice();
                    self.taxa(Contagem {
                        saidas: self.saidas_faixa[c][i],
                        total: self.pares_faixa[c][i],
                    })
                }),
            })
            .collect()
    }

    /// Grupo validado e sorteado.
    fn incorporar_do_grupo(
        &mut self,
        grupo: &[ObservacaoPosicoes],
        faixas: &[[Faixa; QTD_COMPONENTES]],
    ) {
        for (o, f) in grupo.iter().zip(faixas) {
            let saiu = u64::from(o.saiu == Some(true));
            self.saidas_posicao[(o.posicao - 1) as usize] += saiu;
            for (c, faixa) in f.iter().enumerate() {
                self.pares_faixa[c][faixa.indice()] += 1;
                self.saidas_faixa[c][faixa.indice()] += saiu;
            }
        }
        self.concursos += 1;
        self.ultimo_concurso = Some(grupo[0].concurso);
    }
}

/// Taxas históricas de uma série inteira num só passe: as de cada concurso
/// (só com o passado), inclusive o snapshot ainda não sorteado, e as
/// contagens finais.
///
/// A entrada pode vir em qualquer ordem, mas tem de formar uma série completa
/// (concursos seguidos, as dezenas da loteria em cada um, posições 1 … N,
/// `saiu` todo conhecido com *K* saídas ou todo ausente e este só no último
/// concurso); senão é [`SpiderError::AvaliacaoEntradaInvalida`]. Entrada
/// vazia devolve vazio. O primeiro concurso da entrada recebe a taxa do
/// acaso.
pub fn calcular_taxas(
    cfg: &LoteriaConfig,
    observacoes: &[ObservacaoPosicoes],
) -> Result<SerieTaxas> {
    let mut linhas_entrada = observacoes.to_vec();
    // Instável e determinística: chaves repetidas são recusadas na validação
    // antes de qualquer saída; sem repetidas, a ordem final não depende da
    // entrada.
    linhas_entrada.sort_unstable_by_key(|o| (o.concurso, o.dezena));
    // Chaves únicas (garantido pela validação), então esta projeção tem a
    // mesma ordem de `linhas_entrada`.
    ordenar_e_validar(cfg, &projetar(&linhas_entrada))?;

    let mut acumuladas = TaxasAcumuladas::novas(cfg);
    let mut linhas = Vec::with_capacity(linhas_entrada.len());
    for grupo in linhas_entrada.chunk_by(|a, b| a.concurso == b.concurso) {
        let faixas = faixas_do_grupo(grupo);
        linhas.extend(acumuladas.linhas_do_grupo(grupo, &faixas));
        // Só agora o concurso entra nas contagens: as taxas acima não viram o
        // próprio resultado.
        if grupo[0].saiu.is_some() {
            acumuladas.incorporar_do_grupo(grupo, &faixas);
        }
    }
    Ok(SerieTaxas { linhas, acumuladas })
}

/// Faixa de cada valor entre os valores do mesmo concurso (ver "Faixas dos
/// componentes" no topo do módulo). Devolve uma faixa por entrada, na mesma
/// ordem.
pub fn faixas_do_componente(valores: &[Option<i64>]) -> Vec<Faixa> {
    let mut ordenados: Vec<i64> = valores.iter().flatten().copied().collect();
    ordenados.sort_unstable();
    let n = ordenados.len();
    let constante = n > 0 && ordenados[0] == ordenados[n - 1];
    valores
        .iter()
        .map(|valor| match valor {
            None => Faixa::Nulo,
            Some(_) if constante => Faixa::Medio,
            Some(v) => {
                // Dezenas com valor estritamente menor. `n > 0` aqui.
                let menores = ordenados.partition_point(|x| x < v);
                match 3 * menores / n {
                    0 => Faixa::Baixo,
                    1 => Faixa::Medio,
                    _ => Faixa::Alto,
                }
            }
        })
        .collect()
}

/// Faixas dos 9 componentes de cada dezena de um concurso (mesma ordem do
/// grupo).
fn faixas_do_grupo(grupo: &[ObservacaoPosicoes]) -> Vec<[Faixa; QTD_COMPONENTES]> {
    let mut por_dezena = vec![[Faixa::Medio; QTD_COMPONENTES]; grupo.len()];
    for c in 0..QTD_COMPONENTES {
        let valores: Vec<Option<i64>> = grupo.iter().map(|o| o.componentes[c]).collect();
        for (faixas, faixa) in por_dezena.iter_mut().zip(faixas_do_componente(&valores)) {
            faixas[c] = faixa;
        }
    }
    por_dezena
}

/// O que a validação da calibração enxerga: posição e `saiu`.
fn projetar(grupo: &[ObservacaoPosicoes]) -> Vec<Observacao> {
    grupo
        .iter()
        .map(|o| Observacao {
            concurso: o.concurso,
            dezena: o.dezena,
            posicao: o.posicao,
            saiu: o.saiu,
        })
        .collect()
}

fn invalida(detalhe: String) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe)
}

#[cfg(test)]
mod tests;
