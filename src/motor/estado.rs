//! Estado incremental do motor: tudo o que é preciso saber do histórico até
//! o último concurso incorporado para produzir o snapshot do próximo.
//!
//! Ordem do laço, que é o que impede vazamento do futuro: o snapshot do
//! concurso `t` é calculado com o estado até `t - 1`; só depois o alvo
//! `saiu` é preenchido com as dezenas de `t` e `t` é incorporado ao estado.

use super::features::{Acumulado, FeaturesDezena};
use super::{FEATURE_VERSION, JANELAS, JANELA_MAX, JANELA_TOTAL};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::validacao;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Estado de uma dezena.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct EstadoDezena {
    /// Acumulado de todas as aparições (janela total).
    total: Acumulado,
    /// Aparições nos últimos `JANELA_MAX` concursos, em ordem crescente. As
    /// janelas finitas são refeitas sobre o fim desta fila a cada snapshot.
    recentes: VecDeque<u32>,
}

/// Estado do motor para uma loteria, serializável para retomar o cálculo do
/// último concurso incorporado (tabela `motor_estado`, tarefa 1a.2).
///
/// A numeração tem de ser contínua **a partir do concurso 1** — a mesma
/// regra do `sync validar` — porque a janela conta pelo número do concurso:
/// um histórico que começasse depois do 1 daria janelas e `espaco_final`
/// diferentes dos do histórico completo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoMotor {
    /// Versão das fórmulas com que o estado foi construído; estado de outra
    /// versão não pode ser retomado.
    feature_version: u32,
    loteria: String,
    dezena_min: u32,
    dezena_max: u32,
    /// Último concurso incorporado; `None` antes do primeiro.
    ultimo_concurso: Option<u32>,
    /// Índice = `dezena - dezena_min`.
    dezenas: Vec<EstadoDezena>,
}

impl EstadoMotor {
    /// Estado vazio (antes do concurso 1).
    pub fn novo(cfg: &LoteriaConfig) -> Self {
        EstadoMotor {
            feature_version: FEATURE_VERSION,
            loteria: cfg.slug.to_string(),
            dezena_min: cfg.dezena_min,
            dezena_max: cfg.dezena_max,
            ultimo_concurso: None,
            dezenas: vec![EstadoDezena::default(); cfg.qtd_dezenas_possiveis() as usize],
        }
    }

    pub fn feature_version(&self) -> u32 {
        self.feature_version
    }

    pub fn loteria(&self) -> &str {
        &self.loteria
    }

    /// Último concurso incorporado (`None` se nenhum).
    pub fn ultimo_concurso(&self) -> Option<u32> {
        self.ultimo_concurso
    }

    /// Concurso cujo snapshot o estado produz agora: último incorporado + 1.
    pub fn proximo_concurso(&self) -> u32 {
        self.ultimo_concurso.map_or(1, |u| u.saturating_add(1))
    }

    /// Confere se o estado pode ser usado com `cfg` (mesma loteria, faixa e
    /// versão das fórmulas). Útil para um estado lido do banco.
    pub fn verificar_compatibilidade(&self, cfg: &LoteriaConfig) -> Result<()> {
        let incompativel = |motivo: String| Err(SpiderError::MotorEstadoIncompativel(motivo));
        if self.feature_version != FEATURE_VERSION {
            return incompativel(format!(
                "estado na versão {} das features, código na versão {FEATURE_VERSION}",
                self.feature_version
            ));
        }
        if self.loteria != cfg.slug {
            return incompativel(format!(
                "estado da loteria `{}`, configuração de `{}`",
                self.loteria, cfg.slug
            ));
        }
        if (self.dezena_min, self.dezena_max) != (cfg.dezena_min, cfg.dezena_max)
            || self.dezenas.len() != cfg.qtd_dezenas_possiveis() as usize
        {
            return incompativel(format!(
                "estado com dezenas {}..={} ({} posições), configuração com {}..={}",
                self.dezena_min,
                self.dezena_max,
                self.dezenas.len(),
                cfg.dezena_min,
                cfg.dezena_max
            ));
        }
        // Coerência interna, para um estado lido do banco: nenhuma aparição
        // depois do último concurso incorporado (seria dado "do futuro" nas
        // janelas), fila em ordem crescente e coerente com o acumulado total.
        let limite = self.ultimo_concurso.unwrap_or(0);
        for (dezena, estado) in (self.dezena_min..).zip(&self.dezenas) {
            let fila_ok = estado
                .recentes
                .iter()
                .zip(estado.recentes.iter().skip(1))
                .all(|(a, b)| a < b)
                && estado
                    .recentes
                    .back()
                    .is_none_or(|&u| Some(u) == estado.total.ultima)
                && estado.recentes.len() <= estado.total.frequencia as usize;
            let total_ok = estado.total.ultima.is_none_or(|u| u >= 1 && u <= limite)
                && (estado.total.ultima.is_some() == (estado.total.frequencia > 0));
            if !fila_ok || !total_ok {
                return incompativel(format!(
                    "dados da dezena {dezena} incoerentes com o último concurso {limite}"
                ));
            }
        }
        Ok(())
    }

    /// Snapshot de todas as dezenas e janelas para o próximo concurso
    /// (`proximo_concurso()`), com `saiu = None`. Ordenado por (dezena,
    /// janela). Só lê o estado, que contém concursos até o último
    /// incorporado.
    pub fn snapshot(&self) -> Vec<FeaturesDezena> {
        let t = self.proximo_concurso();
        let mut saida = Vec::with_capacity(self.dezenas.len() * JANELAS.len());
        for (dezena, estado) in (self.dezena_min..).zip(&self.dezenas) {
            let ultima = estado.total.ultima;
            for &janela in &JANELAS {
                let linha = if janela == JANELA_TOTAL {
                    estado.total.features(t, dezena, janela, ultima)
                } else {
                    let mut acumulado = Acumulado::default();
                    // Janela J em t = concursos t−J … t−1 (`a + J >= t`).
                    for &a in estado
                        .recentes
                        .iter()
                        .filter(|&&a| a.saturating_add(janela) >= t)
                    {
                        acumulado.incorporar(a);
                    }
                    acumulado.features(t, dezena, janela, ultima)
                };
                saida.push(linha);
            }
        }
        saida
    }

    /// Calcula o snapshot do concurso `r` (com `saiu` preenchido) e depois o
    /// incorpora ao estado. Exige `r.concurso == proximo_concurso()` e um
    /// resultado válido para `cfg`; em caso de erro o estado não muda.
    pub fn adicionar(&mut self, cfg: &LoteriaConfig, r: &Resultado) -> Result<Vec<FeaturesDezena>> {
        self.verificar_compatibilidade(cfg)?;
        verificar_resultado(cfg, r)?;
        self.verificar_sequencia(r.concurso)?;

        // 1. Snapshot com o estado até t − 1.
        let mut linhas = self.snapshot();
        // 2. Alvo. `verificar_resultado` garantiu faixa e ausência de
        //    repetidas, então o índice é válido.
        let mut saiu = vec![false; self.dezenas.len()];
        for &d in &r.dezenas {
            saiu[(d - self.dezena_min) as usize] = true;
        }
        for linha in &mut linhas {
            linha.saiu = Some(saiu[(linha.dezena - self.dezena_min) as usize]);
        }
        // 3. Só agora t entra no estado.
        self.incorporar(r.concurso, &saiu);
        Ok(linhas)
    }

    /// Adiciona uma sequência de concursos, em ordem, e devolve os snapshots
    /// de cada um, ordenados por (concurso, dezena, janela). Tudo ou nada: em
    /// caso de erro o estado não muda.
    pub fn avancar(
        &mut self,
        cfg: &LoteriaConfig,
        resultados: &[Resultado],
    ) -> Result<Vec<FeaturesDezena>> {
        let mut novo = self.clone();
        let mut saida = Vec::with_capacity(resultados.len() * self.dezenas.len() * JANELAS.len());
        for r in resultados {
            saida.extend(novo.adicionar(cfg, r)?);
        }
        *self = novo;
        Ok(saida)
    }

    fn verificar_sequencia(&self, recebido: u32) -> Result<()> {
        let esperado = self.proximo_concurso();
        match self.ultimo_concurso {
            Some(ultimo) if recebido == ultimo => Err(SpiderError::MotorConcursoRepetido(recebido)),
            Some(ultimo) if recebido < ultimo => {
                Err(SpiderError::MotorForaDeOrdem { ultimo, recebido })
            }
            _ if recebido != esperado => Err(SpiderError::MotorBuraco { esperado, recebido }),
            _ => Ok(()),
        }
    }

    fn incorporar(&mut self, concurso: u32, saiu: &[bool]) {
        // Depois deste concurso, o próximo snapshot é o de `concurso + 1`;
        // a maior janela finita precisa das aparições a partir de
        // `concurso + 1 − JANELA_MAX`.
        let t = concurso.saturating_add(1);
        for (estado, &saiu) in self.dezenas.iter_mut().zip(saiu) {
            if saiu {
                estado.total.incorporar(concurso);
                estado.recentes.push_back(concurso);
            }
            while estado
                .recentes
                .front()
                .is_some_and(|&a| a.saturating_add(JANELA_MAX) < t)
            {
                estado.recentes.pop_front();
            }
        }
        self.ultimo_concurso = Some(concurso);
    }
}

/// Reaproveita as regras de `validacao` para um concurso isolado (loteria,
/// concurso 0, quantidade, faixa, repetidas, data). Todas são erro.
fn verificar_resultado(cfg: &LoteriaConfig, r: &Resultado) -> Result<()> {
    let problemas = validacao::validar_resultado(cfg, r);
    if problemas.is_empty() {
        return Ok(());
    }
    let detalhe = problemas
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    Err(SpiderError::MotorResultadoInvalido {
        concurso: r.concurso,
        detalhe,
    })
}
