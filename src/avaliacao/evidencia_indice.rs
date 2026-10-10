//! Evidência bayesiana do índice de força (Fase 1d, tarefa 1d.3).
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1d — Índice de evidência
//! bayesiano", item B. Função pura: sem I/O, sem SQL.
//!
//! # Fator de Bayes prequencial
//!
//! O FB do índice (H1: o índice prevê, com a probabilidade calibrada pela
//! posição; H0: o acaso, *K/N* em toda dezena) é a razão das
//! verossimilhanças **prequenciais**: concurso a concurso, a probabilidade que
//! cada hipótese dava ao que saiu, calculada só com o passado. Cada par
//! (concurso, dezena) é uma Bernoulli (*p* se saiu, 1−*p* se não saiu):
//!
//! `ln FB = Σ_(t, d) [ln p_modelo(t, d) − ln p_acaso]`
//!
//! sobre os pares das **dobras de teste 2 a 5** do walk-forward (as mesmas
//! dobras do `sync avaliar`; a dobra 1 só treina na busca de pesos). As duas
//! probabilidades têm o **mesmo limite** `[1e-15, 1 − 1e-15]` do log loss
//! ([`log_loss`]), então vale exatamente
//!
//! `ln FB = −(nº de pares) × (log loss do modelo − log loss do acaso)`
//!
//! nessas dobras: o mesmo número da diferença de log loss da 1c, em outra
//! escala (conferido nos testes contra o `walk_forward`). O resultado é
//! guardado como `log10_fb = ln FB ÷ ln 10` e classificado pela escala de A
//! ([`classificar`]: mesmos graus e limites).
//!
//! # Régua (Fase 1e)
//!
//! A calibração é uma [`Regua`]. A **linear** da 1e ([`super::calibrar_linear`], um
//! parâmetro) é a régua principal de B e do índice inerte; a **por posição** da
//! 1c ([`super::calibrar`], *N* parâmetros) segue disponível para a linha informativa
//! do relatório, sem inerte. Tudo o mais (dobras, limite de probabilidade,
//! graus, direção, conferência com o `sync avaliar`) é igual nas duas.
//! [`evidencia_do_indice`] mantém a régua por posição da 1d.3;
//! [`evidencia_do_indice_com`] escolhe.
//!
//! # Direção
//!
//! "A favor do índice" se FB > 1; "a favor do acaso" se FB ≤ 1 (FB = 1 exato
//! fica a favor do acaso). A direção de A, por dezena, continua "a favor de
//! viés" ([`super::Direcao`]).
//!
//! # Ressalva
//!
//! As dezenas de um concurso são tratadas como Bernoullis **independentes**
//! (pseudo-verossimilhança), igual para o modelo e para o acaso; na verdade
//! somam exatamente *K*. O FB resultante serve como régua de evidência, não
//! como probabilidade exata do sorteio.
//!
//! # Índice inerte (decisão de 2026-10-10, achado da 1d.3)
//!
//! A calibração pela posição estima uma taxa por posição, e esse aprendizado
//! custa evidência mesmo quando o índice não carrega informação nenhuma: um
//! índice inerte sai, em média, "a favor do acaso" com grau alto (ln FB médio
//! ≈ −17, −40 e −60). A linear da 1e custa da ordem de ½·ln T, e por isso é a
//! régua principal; o inerte continua impresso ao lado. O FB observado vem ao
//! lado do esperado de um índice inerte
//! ([`esperado_indice_inerte`]): as mesmas posições do perfil diante de
//! históricos simulados sob o acaso (a cada concurso, *K* de *N* sem
//! reposição, [`SplitMix64`] com semente fixa). Sob o acaso a posição não
//! informa nada, então cada réplica mede só o custo da calibração. O índice
//! só mostra algo se o observado ficar acima do que os inertes dão.
//!
//! # Ponto no tempo
//!
//! As probabilidades vêm da [`Regua`] (a de `t` usa só concursos `< t`, nas
//! duas) e
//! a entrada é cortada no último concurso da última dobra antes de qualquer
//! cálculo: nada acima do corte da busca (o conjunto trancado) é lido.

use super::bootstrap::SplitMix64;
use super::calibracao::{ConstantesLineares, Observacao, Regua};
use super::evidencia::{classificar, Direcao, Grau};
use super::metricas::log_loss;
use super::walk_forward::{dobras_da_busca, FaixaConcursos};
use super::{ate_o_corte, taxa_do_acaso};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};

/// Número (a partir de 1) da primeira dobra de teste do walk-forward: a dobra
/// 1 só treina na busca de pesos e fica fora de B.
pub const PRIMEIRA_DOBRA_DE_TESTE: u32 = 2;

/// Para que lado os dados apontam na evidência do índice (B).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirecaoIndice {
    /// FB > 1.
    Indice,
    /// FB ≤ 1 (FB = 1 exato inclusive).
    Acaso,
}

impl DirecaoIndice {
    pub const fn nome(self) -> &'static str {
        match self {
            DirecaoIndice::Indice => "a favor do índice",
            DirecaoIndice::Acaso => "a favor do acaso",
        }
    }
}

/// Grau e direção de um `log10 FB` do índice: a mesma escala de A
/// ([`classificar`]), com o rótulo de B na direção.
pub fn classificar_indice(log10_fb: f64) -> Result<(Grau, DirecaoIndice)> {
    let c = classificar(log10_fb)?;
    let direcao = match c.direcao {
        Direcao::Vies => DirecaoIndice::Indice,
        Direcao::Acaso => DirecaoIndice::Acaso,
    };
    Ok((c.grau, direcao))
}

/// Uma dobra de teste (informativa: o grau sai do total).
#[derive(Debug, Clone, PartialEq)]
pub struct DobraEvidenciaIndice {
    /// Número da dobra no walk-forward (2 a 5).
    pub dobra: u32,
    pub faixa: FaixaConcursos,
    pub concursos: usize,
    /// Pares (concurso, dezena): `concursos × N`.
    pub pares: u64,
    pub ln_fb: f64,
    pub log10_fb: f64,
}

/// Evidência do índice (B) no conjunto das dobras de teste.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaIndice {
    /// A calibração que traduziu a posição em probabilidade.
    pub regua: Regua,
    /// Da primeira dobra de teste ao fim da última (consecutivas).
    pub faixa: FaixaConcursos,
    pub concursos: usize,
    pub pares: u64,
    /// `Σ` de `ln p_modelo − ln p_acaso`, soma das dobras em ordem.
    pub ln_fb: f64,
    pub log10_fb: f64,
    pub grau: Grau,
    pub direcao: DirecaoIndice,
    /// As dobras de teste, em ordem (2 a 5).
    pub por_dobra: Vec<DobraEvidenciaIndice>,
}

/// Evidência bayesiana do índice (B) para o trecho de busca de `cfg`.
///
/// `observacoes` é a série desde o primeiro concurso do índice (como no
/// `sync avaliar`): a calibração parte do primeiro concurso da entrada e usa
/// todo o passado. As dobras são as de [`dobras_da_busca`] (do segundo
/// concurso da entrada ao corte do config, só os sorteados); entram as de
/// número [`PRIMEIRA_DOBRA_DE_TESTE`] em diante. Nada acima do último
/// concurso da última dobra é lido, nem para validar.
///
/// Erros: série inválida é [`SpiderError::AvaliacaoEntradaInvalida`]; menos
/// concursos que dobras do walk-forward é [`SpiderError::AvaliacaoSemConcursos`].
pub fn evidencia_do_indice(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
) -> Result<EvidenciaIndice> {
    evidencia_do_indice_com(cfg, observacoes, Regua::Posicao)
}

/// [`evidencia_do_indice`] com a [`Regua`] à escolha: a por posição da 1c ou a
/// linear da 1e. Mesmas dobras, mesmo corte, mesmas garantias de ponto no
/// tempo e mesmos erros.
pub fn evidencia_do_indice_com(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    regua: Regua,
) -> Result<EvidenciaIndice> {
    let todas = dobras_da_busca(cfg, observacoes)?;
    let primeira = PRIMEIRA_DOBRA_DE_TESTE as usize - 1;
    let (Some(ultima), Some(inicial)) = (todas.last(), todas.get(primeira)) else {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "o walk-forward tem {} dobra(s), menos que a primeira dobra de teste ({PRIMEIRA_DOBRA_DE_TESTE})",
            todas.len()
        )));
    };
    let faixa = FaixaConcursos {
        primeiro: inicial.primeiro,
        ultimo: ultima.ultimo,
    };
    let teste = &todas[primeira..];

    // Nada acima do fim da última dobra chega ao cálculo.
    let probabilidades = regua.calibrar(cfg, &ate_o_corte(observacoes, faixa.ultimo))?;
    let acaso = taxa_do_acaso(cfg);

    let mut ln_dobras = vec![0.0_f64; teste.len()];
    let mut concursos_dobras = vec![0_usize; teste.len()];
    for grupo in probabilidades.chunk_by(|a, b| a.concurso == b.concurso) {
        let concurso = grupo[0].concurso;
        let Some(i) = teste.iter().position(|d| d.contem(concurso)) else {
            continue;
        };
        let mut soma = 0.0_f64;
        for p in grupo {
            let saiu = p.saiu.ok_or_else(|| {
                SpiderError::AvaliacaoEntradaInvalida(format!(
                    "o concurso {concurso} está numa dobra de teste mas ainda não foi sorteado"
                ))
            })?;
            // ln p_modelo − ln p_acaso, com o limite do log loss nos dois.
            soma += log_loss(acaso, saiu) - log_loss(p.probabilidade, saiu);
        }
        ln_dobras[i] += soma;
        concursos_dobras[i] += 1;
    }

    let dezenas = u64::from(cfg.qtd_dezenas_possiveis());
    let mut por_dobra = Vec::with_capacity(teste.len());
    let mut ln_fb = 0.0_f64;
    let mut concursos = 0_usize;
    for (i, dobra) in teste.iter().enumerate() {
        // Defesa: as dobras vêm da mesma série, então cobrem todos os concursos.
        if concursos_dobras[i] as u64 != dobra.tamanho() {
            return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
                "dobra {}..={}: {} concurso(s) calibrados, esperados {}",
                dobra.primeiro,
                dobra.ultimo,
                concursos_dobras[i],
                dobra.tamanho()
            )));
        }
        ln_fb += ln_dobras[i];
        concursos += concursos_dobras[i];
        por_dobra.push(DobraEvidenciaIndice {
            dobra: PRIMEIRA_DOBRA_DE_TESTE + i as u32,
            faixa: *dobra,
            concursos: concursos_dobras[i],
            pares: concursos_dobras[i] as u64 * dezenas,
            ln_fb: ln_dobras[i],
            log10_fb: ln_dobras[i] * std::f64::consts::LOG10_E,
        });
    }
    let log10_fb = ln_fb * std::f64::consts::LOG10_E;
    let (grau, direcao) = classificar_indice(log10_fb)?;
    Ok(EvidenciaIndice {
        regua,
        faixa,
        concursos,
        pares: concursos as u64 * dezenas,
        ln_fb,
        log10_fb,
        grau,
        direcao,
        por_dobra,
    })
}

/// Réplicas da simulação do índice inerte.
pub const REPLICAS_INERTE: usize = 200;

/// Semente fixa da simulação do índice inerte (a data da decisão,
/// 2026-10-10): trocar muda o esperado impresso.
pub const SEMENTE_INERTE: u64 = 20_261_010;

/// Distribuição do `log10 FB` de um índice inerte (ver o módulo), com o
/// observado situado nela.
#[derive(Debug, Clone, PartialEq)]
pub struct IndiceInerte {
    pub replicas: usize,
    pub semente: u64,
    /// Média do `log10 FB` entre as réplicas.
    pub media_log10: f64,
    /// Percentis 2,5 % e 97,5 % (ordem das réplicas, posição arredondada
    /// para baixo).
    pub p025_log10: f64,
    pub p975_log10: f64,
    /// Réplicas com `log10 FB` ≥ o observado (quanto menos, mais o índice se
    /// destaca de um inerte).
    pub replicas_iguais_ou_acima: usize,
}

/// Série já cortada e ordenada, pronta para as réplicas do inerte: só os
/// concursos sorteados, cada um com as *N* posições (0 … N−1) na ordem das
/// dezenas.
struct SerieInerte {
    /// Só na régua linear: as constantes inteiras de [`super::calibrar_linear`], as
    /// mesmas da calibração direta.
    linear: Option<ConstantesLineares>,
    /// `concursos × N` posições, concurso a concurso.
    posicoes: Vec<usize>,
    /// Por concurso: se está nas dobras de teste (entra no ln FB).
    no_teste: Vec<bool>,
}

impl SerieInerte {
    /// Corta em `faixa.ultimo`, ordena por (concurso, dezena) e confere que
    /// cada concurso tem as *N* dezenas (a série completa já foi validada pela
    /// [`super::calibrar`] de [`evidencia_do_indice`]; aqui é só defesa).
    fn nova(
        cfg: &LoteriaConfig,
        observacoes: &[Observacao],
        faixa: FaixaConcursos,
        regua: Regua,
    ) -> Result<Self> {
        let mut linhas = ate_o_corte(observacoes, faixa.ultimo);
        linhas.sort_unstable_by_key(|o| (o.concurso, o.dezena));
        let n = cfg.qtd_dezenas_possiveis() as usize;
        let mut posicoes = Vec::with_capacity(linhas.len());
        let mut no_teste = Vec::new();
        for grupo in linhas.chunk_by(|a, b| a.concurso == b.concurso) {
            if grupo[0].saiu.is_none() {
                continue;
            }
            if grupo.len() != n
                || grupo
                    .iter()
                    .any(|o| o.posicao == 0 || o.posicao as usize > n)
            {
                return Err(SpiderError::AvaliacaoEntradaInvalida(format!(
                    "concurso {}: série incompleta para a simulação do índice inerte",
                    grupo[0].concurso
                )));
            }
            posicoes.extend(grupo.iter().map(|o| (o.posicao - 1) as usize));
            no_teste.push(grupo[0].concurso >= faixa.primeiro);
        }
        let linear = match regua {
            Regua::Posicao => None,
            Regua::Linear => Some(ConstantesLineares::novas(cfg)?),
        };
        Ok(Self {
            linear,
            posicoes,
            no_teste,
        })
    }

    /// Uma réplica: sorteia *K* de *N* em cada concurso (Fisher–Yates
    /// parcial) e devolve o ln FB da mesma conta de [`evidencia_do_indice_com`]
    /// (a fórmula da régua em inteiros, limite do [`log_loss`]), num laço só,
    /// sem refazer ordenação e validação a cada réplica.
    fn ln_fb_de_uma_replica(&self, cfg: &LoteriaConfig, gerador: &mut SplitMix64) -> Result<f64> {
        let n = cfg.qtd_dezenas_possiveis() as usize;
        let k = cfg.qtd_dezenas;
        let m = u64::from(super::CONCURSOS_FICTICIOS);
        let acaso = taxa_do_acaso(cfg);
        let mut saidas_por_posicao = vec![0_u64; n];
        // Só na régua linear: S, a soma dos pesos das posições que saíram.
        let mut soma_pesos = 0_i128;
        let mut baralho: Vec<usize> = (0..n).collect();
        let mut saiu = vec![false; n];
        let mut ln_fb = 0.0_f64;
        for (c, (posicoes, &no_teste)) in self.posicoes.chunks(n).zip(&self.no_teste).enumerate() {
            saiu.iter_mut().for_each(|s| *s = false);
            for i in 0..k {
                let j = i + gerador.abaixo_de((n - i) as u64)? as usize;
                baralho.swap(i, j);
                saiu[baralho[i]] = true;
            }
            match &self.linear {
                Some(constantes) => {
                    // `posicoes` guarda r − 1; o peso é o de r.
                    let peso = |r: usize| constantes.peso(r as u32 + 1);
                    if no_teste {
                        let passo = constantes.passo(c as i128, soma_pesos);
                        for (&r, &s) in posicoes.iter().zip(&saiu) {
                            ln_fb += log_loss(acaso, s) - log_loss(passo.probabilidade(peso(r)), s);
                        }
                    }
                    for (&r, &s) in posicoes.iter().zip(&saiu) {
                        if s {
                            soma_pesos += peso(r);
                        }
                    }
                }
                None => {
                    if no_teste {
                        let denominador = ((c as u64 + m) * n as u64) as f64;
                        for (&r, &s) in posicoes.iter().zip(&saiu) {
                            let p = (saidas_por_posicao[r] * n as u64 + m * k as u64) as f64
                                / denominador;
                            ln_fb += log_loss(acaso, s) - log_loss(p, s);
                        }
                    }
                    for (&r, &s) in posicoes.iter().zip(&saiu) {
                        saidas_por_posicao[r] += u64::from(s);
                    }
                }
            }
        }
        Ok(ln_fb)
    }
}

/// Simula `replicas` vezes o mesmo cálculo de [`evidencia_do_indice_com`], na
/// régua de `observado` ([`EvidenciaIndice::regua`]), com as posições de `observacoes` e o resultado de cada concurso sorteado ao acaso
/// (*K* de *N* sem reposição, Fisher–Yates parcial com [`SplitMix64`] em
/// `semente`), e situa `observado` nessa distribuição. Mesma entrada, mesmos
/// bits. Nada acima do fim da última dobra é usado; o snapshot ainda não
/// sorteado fica de fora. `observacoes` é a mesma série passada a
/// [`evidencia_do_indice`] para obter `observado`.
pub fn esperado_indice_inerte(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    observado: &EvidenciaIndice,
    replicas: usize,
    semente: u64,
) -> Result<IndiceInerte> {
    if replicas < 2 {
        return Err(SpiderError::AvaliacaoEntradaInvalida(
            "a simulação do índice inerte precisa de ao menos 2 réplicas".to_string(),
        ));
    }
    let serie = SerieInerte::nova(cfg, observacoes, observado.faixa, observado.regua)?;
    let mut gerador = SplitMix64::nova(semente);
    let mut valores = Vec::with_capacity(replicas);
    for _ in 0..replicas {
        let ln_fb = serie.ln_fb_de_uma_replica(cfg, &mut gerador)?;
        valores.push(ln_fb * std::f64::consts::LOG10_E);
    }

    let media_log10 = valores.iter().sum::<f64>() / replicas as f64;
    let replicas_iguais_ou_acima = valores.iter().filter(|&&v| v >= observado.log10_fb).count();
    valores.sort_by(f64::total_cmp);
    let percentil = |q: f64| valores[((replicas - 1) as f64 * q) as usize];
    Ok(IndiceInerte {
        replicas,
        semente,
        media_log10,
        p025_log10: percentil(0.025),
        p975_log10: percentil(0.975),
        replicas_iguais_ou_acima,
    })
}

#[cfg(test)]
mod tests;
