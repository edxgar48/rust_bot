//! Evidência bayesiana por dezena (Fase 1d, tarefa 1d.2).
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1d — Índice de evidência
//! bayesiano", item A. Em vez de "prever", mostra **quanta evidência os dados
//! dão** de que cada dezena tem uma chance diferente de *K/N*, em cinco
//! graus. Funções puras: sem I/O, sem SQL.
//!
//! # Fator de Bayes
//!
//! Cada dezena, sozinha, é Bernoulli(*p*) por concurso, com *p* = *K/N*,
//! independente entre concursos (mesmo com sorteio sem reposição dentro do
//! concurso: a marginal de uma dezena é exata). Com `x` saídas em `n`
//! concursos:
//!
//! - H0: a chance é exatamente *p*; `P(dados | H0) = p^x (1−p)^(n−x)` (para a
//!   sequência observada; o coeficiente binomial seria o mesmo nos dois
//!   lados e se cancela);
//! - H1: a chance é desconhecida, com priori Beta(a, b) de média *p*;
//!   `P(dados | H1) = B(x + a, n − x + b) / B(a, b)`.
//!
//! `FB = P(dados | H1) / P(dados | H0)`, de fórmula fechada com [`ln_gamma`]:
//!
//! `ln FB = ln B(x+a, n−x+b) − ln B(a, b) − x·ln p − (n−x)·ln(1−p)`.
//!
//! O FB é sempre guardado e comparado em `log10` ([`log10_fator_bayes`]).
//!
//! # Priori
//!
//! Beta(a, b) com média *p* e desvio padrão `s·p` (`s` = 5 % no principal;
//! 2,5 % e 10 % na sensibilidade, porque o FB depende da largura da priori):
//! `a + b = (1 − p)/(s²·p) − 1`, `a = p·(a + b)`, `b = (1 − p)·(a + b)`
//! ([`Priori::nova`]).
//!
//! # Graus
//!
//! Sobre `F = max(FB, 1/FB)`, em `log10` e com limites fechados à esquerda
//! (Jeffreys adaptada): muito fraca `< log10 3`; fraca `[log10 3, 1)`;
//! moderada `[1, log10 30)`; forte `[log10 30, 2)`; muito forte `≥ 2`.
//! Direção ([`classificar`]): "a favor de viés" se `FB > 1`; "a favor do
//! acaso" se `FB < 1` **ou `FB = 1` exato**.
//!
//! # Ponto no tempo
//!
//! A evidência do concurso `t` ([`evidencia_por_dezena`]) usa só concursos
//! `≤ t−1`: o que é `≥ t` é descartado antes de qualquer leitura ou
//! validação. A janela principal é o histórico inteiro antes de `t`; a
//! secundária, os últimos `min(500, t−1)` concursos antes de `t`.
//!
//! # Esperado sob o acaso
//!
//! Com 25, 60 ou 80 dezenas, algumas saem "moderada" por acaso.
//! [`esperado_sob_o_acaso`] simula históricos com o mesmo `n` (sorteio sem
//! reposição de *K* entre *N* por concurso, [`SplitMix64`], semente fixa) e
//! devolve a média, por grau e direção, da contagem de dezenas.
//! [`esperado_exato`] dá o mesmo valor pela binomial (a esperança de uma
//! soma vale a soma das esperanças, qualquer que seja a dependência entre as
//! dezenas do concurso), e os testes conferem uma contra a outra.

use super::bootstrap::SplitMix64;
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::validacao;

/// Desvios padrão relativos da priori: o **principal** (5 % de *K/N*, índice
/// 0) e a sensibilidade (2,5 % e 10 %).
pub const DESVIOS_PRIORI: [f64; 3] = [0.05, 0.025, 0.10];

/// Quantas prioris o relatório carrega (a principal e duas de sensibilidade).
pub const QTD_PRIORIS: usize = DESVIOS_PRIORI.len();

/// Concursos da janela secundária (viés que muda com o equipamento).
pub const JANELA_SECUNDARIA: usize = 500;

/// Semente fixa da simulação do esperado sob o acaso (a data da decisão,
/// 2026-10-08).
pub const SEMENTE_ESPERADO: u64 = 20_261_008;

/// Réplicas da simulação do esperado sob o acaso.
pub const REPLICAS_ESPERADO: usize = 1_000;

fn invalida(detalhe: impl Into<String>) -> SpiderError {
    SpiderError::EvidenciaEntradaInvalida(detalhe.into())
}

// ---------------------------------------------------------------------------
// ln Γ
// ---------------------------------------------------------------------------

/// A partir daqui vale a série de Stirling (o primeiro termo omitido,
/// `~7e-20` em `x = 15`, fica abaixo do arredondamento).
const LIMITE_STIRLING: f64 = 15.0;

/// `ln Γ(x)` para `x > 0` (fora disso, `NaN`). Série de Stirling para
/// `x ≥ 15`; abaixo, `Γ(x) = Γ(x + m) / (x·(x+1)·…·(x+m−1))` com `m` o
/// menor inteiro que leva `x + m` a 15. Erro relativo ~1e-15 (~1e-14 perto de
/// seus zeros, `x` = 1 e 2, onde o valor é pequeno; conferido nos
/// testes contra valores conhecidos); `Γ(1) = Γ(2) = 1` saem exatos (0).
pub fn ln_gamma(x: f64) -> f64 {
    if x.is_nan() || x <= 0.0 || x.is_infinite() {
        return f64::NAN;
    }
    if x == 1.0 || x == 2.0 {
        return 0.0;
    }
    let mut z = x;
    let mut produto = 1.0;
    while z < LIMITE_STIRLING {
        produto *= z;
        z += 1.0;
    }
    let inv = 1.0 / z;
    let inv2 = inv * inv;
    // 1/(12z) − 1/(360z³) + 1/(1260z⁵) − 1/(1680z⁷) + 1/(1188z⁹) − 691/(360360z¹¹) + 1/(156z¹³)
    let serie = inv
        * (1.0 / 12.0
            + inv2
                * (-1.0 / 360.0
                    + inv2
                        * (1.0 / 1260.0
                            + inv2
                                * (-1.0 / 1680.0
                                    + inv2
                                        * (1.0 / 1188.0
                                            + inv2 * (-691.0 / 360_360.0 + inv2 / 156.0))))));
    let stirling = (z - 0.5) * z.ln() - z + 0.5 * (2.0 * std::f64::consts::PI).ln() + serie;
    stirling - produto.ln()
}

/// `ln B(u, v) = ln Γ(u) + ln Γ(v) − ln Γ(u + v)`, para `u, v > 0`.
fn ln_beta(u: f64, v: f64) -> f64 {
    ln_gamma(u) + ln_gamma(v) - ln_gamma(u + v)
}

// ---------------------------------------------------------------------------
// Priori e fator de Bayes
// ---------------------------------------------------------------------------

/// Priori Beta(a, b) da chance de uma dezena sob H1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Priori {
    /// Desvio padrão relativo `s` (o desvio da Beta é `s·p`).
    pub desvio: f64,
    pub a: f64,
    pub b: f64,
}

impl Priori {
    /// Beta de média `p` e desvio padrão `desvio·p`:
    /// `a + b = (1 − p)/(desvio²·p) − 1`. Exige `0 < p < 1`, `desvio > 0`
    /// finito e `desvio² < (1 − p)/p` (senão a + b ≤ 0 e a Beta não existe).
    pub fn nova(p: f64, desvio: f64) -> Result<Self> {
        if !(p > 0.0 && p < 1.0) {
            return Err(invalida(format!("chance sob H0 fora de (0, 1): {p}")));
        }
        if !(desvio > 0.0 && desvio.is_finite()) {
            return Err(invalida(format!(
                "desvio da priori deve ser positivo e finito: {desvio}"
            )));
        }
        let soma = (1.0 - p) / (desvio * desvio * p) - 1.0;
        if !(soma > 0.0 && soma.is_finite()) {
            return Err(invalida(format!(
                "desvio {desvio} largo demais para uma Beta de média {p} (a + b = {soma})"
            )));
        }
        Ok(Self {
            desvio,
            a: p * soma,
            b: (1.0 - p) * soma,
        })
    }
}

/// `log10 FB` de `x` saídas em `n` concursos: H1 (chance ~ `priori`) contra
/// H0 (chance exatamente `p`). Positivo = os dados favorecem o viés.
/// `n = 0` dá exatamente 0 (FB = 1).
pub fn log10_fator_bayes(x: u64, n: u64, p: f64, priori: &Priori) -> Result<f64> {
    if x > n {
        return Err(invalida(format!("{x} saídas em apenas {n} concursos")));
    }
    if !(p > 0.0 && p < 1.0) {
        return Err(invalida(format!("chance sob H0 fora de (0, 1): {p}")));
    }
    if !(priori.a > 0.0 && priori.b > 0.0 && priori.a.is_finite() && priori.b.is_finite()) {
        return Err(invalida(format!(
            "priori Beta({}, {}) inválida",
            priori.a, priori.b
        )));
    }
    let (xf, nf) = (x as f64, n as f64);
    let ln_fb = ln_beta(xf + priori.a, nf - xf + priori.b)
        - ln_beta(priori.a, priori.b)
        - xf * p.ln()
        - (nf - xf) * (-p).ln_1p();
    if !ln_fb.is_finite() {
        return Err(invalida(format!(
            "fator de Bayes não finito (x = {x}, n = {n}, p = {p})"
        )));
    }
    Ok(ln_fb * std::f64::consts::LOG10_E)
}

// ---------------------------------------------------------------------------
// Graus e direção
// ---------------------------------------------------------------------------

/// Um dos cinco graus de evidência (Jeffreys adaptada).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grau {
    MuitoFraca,
    Fraca,
    Moderada,
    Forte,
    MuitoForte,
}

impl Grau {
    /// Do mais fraco ao mais forte.
    pub const TODOS: [Grau; 5] = [
        Grau::MuitoFraca,
        Grau::Fraca,
        Grau::Moderada,
        Grau::Forte,
        Grau::MuitoForte,
    ];

    /// Posição em [`Grau::TODOS`].
    pub const fn indice(self) -> usize {
        self as usize
    }

    pub const fn nome(self) -> &'static str {
        match self {
            Grau::MuitoFraca => "muito fraca",
            Grau::Fraca => "fraca",
            Grau::Moderada => "moderada",
            Grau::Forte => "forte",
            Grau::MuitoForte => "muito forte",
        }
    }
}

/// Para que lado os dados apontam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direcao {
    /// FB > 1.
    Vies,
    /// FB < 1 ou FB = 1 exato.
    Acaso,
}

impl Direcao {
    pub const fn nome(self) -> &'static str {
        match self {
            Direcao::Vies => "a favor de viés",
            Direcao::Acaso => "a favor do acaso",
        }
    }
}

/// Grau e direção de um fator de Bayes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Classificacao {
    pub grau: Grau,
    pub direcao: Direcao,
}

impl Classificacao {
    /// Posição na grade de 10 células: viés (0..5) e depois acaso (5..10).
    const fn celula(self) -> usize {
        let base = match self.direcao {
            Direcao::Vies => 0,
            Direcao::Acaso => 5,
        };
        base + self.grau.indice()
    }
}

/// Classifica um `log10 FB` finito. Os limites são comparados em `log10`,
/// fechados à esquerda: `|log10 FB|` em `[log10 3, 1)` é fraca etc.; FB = 1
/// exato (`log10 FB = 0`) cai em "muito fraca, a favor do acaso".
pub fn classificar(log10_fb: f64) -> Result<Classificacao> {
    if !log10_fb.is_finite() {
        return Err(invalida(format!(
            "log10 do fator de Bayes não finito: {log10_fb}"
        )));
    }
    let f = log10_fb.abs();
    let grau = if f < 3f64.log10() {
        Grau::MuitoFraca
    } else if f < 1.0 {
        Grau::Fraca
    } else if f < 30f64.log10() {
        Grau::Moderada
    } else if f < 2.0 {
        Grau::Forte
    } else {
        Grau::MuitoForte
    };
    let direcao = if log10_fb > 0.0 {
        Direcao::Vies
    } else {
        Direcao::Acaso
    };
    Ok(Classificacao { grau, direcao })
}

/// Um valor por (direção, grau): contagem de dezenas (observada) ou o seu
/// valor esperado/erro padrão.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Grade<T> {
    /// Índice por [`Grau::indice`].
    pub vies: [T; 5],
    pub acaso: [T; 5],
}

impl<T: Copy> Grade<T> {
    pub fn get(&self, c: Classificacao) -> T {
        self.celulas()[c.celula()]
    }

    pub fn do_grau(&self, grau: Grau, direcao: Direcao) -> T {
        self.get(Classificacao { grau, direcao })
    }

    fn celulas(&self) -> [T; 10] {
        let (v, a) = (self.vies, self.acaso);
        [v[0], v[1], v[2], v[3], v[4], a[0], a[1], a[2], a[3], a[4]]
    }

    fn de_celulas(c: [T; 10]) -> Self {
        Self {
            vies: [c[0], c[1], c[2], c[3], c[4]],
            acaso: [c[5], c[6], c[7], c[8], c[9]],
        }
    }
}

impl Grade<usize> {
    /// Soma de todas as células (o número de dezenas contadas).
    pub fn total(&self) -> usize {
        self.vies.iter().chain(&self.acaso).sum()
    }

    /// Dezenas com grau pelo menos `minimo`, nas duas direções.
    pub fn a_partir_de(&self, minimo: Grau) -> usize {
        Grau::TODOS
            .iter()
            .filter(|&&g| g >= minimo)
            .map(|g| self.vies[g.indice()] + self.acaso[g.indice()])
            .sum()
    }
}

impl Grade<f64> {
    /// Soma de todas as células, em ordem fixa.
    pub fn total(&self) -> f64 {
        self.vies.iter().chain(&self.acaso).sum()
    }

    /// Esperado de dezenas com grau pelo menos `minimo`, nas duas direções.
    pub fn a_partir_de(&self, minimo: Grau) -> f64 {
        Grau::TODOS
            .iter()
            .filter(|&&g| g >= minimo)
            .map(|g| self.vies[g.indice()] + self.acaso[g.indice()])
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Evidência por dezena num ponto no tempo
// ---------------------------------------------------------------------------

/// Quais concursos (anteriores a `t`) entram na evidência.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Janela {
    /// Todo o histórico antes de `t` (principal).
    Inteira,
    /// Os últimos `min(J, t−1)` concursos antes de `t`.
    Ultimos(usize),
}

/// Evidência de uma dezena. Os vetores seguem [`DESVIOS_PRIORI`]: o índice 0
/// é a priori principal (5 %).
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaDezena {
    pub dezena: u32,
    /// `x`: quantas vezes saiu na janela.
    pub saidas: u64,
    /// `n`: concursos da janela.
    pub concursos: u64,
    pub log10_fb: [f64; QTD_PRIORIS],
    pub classificacao: [Classificacao; QTD_PRIORIS],
}

/// Evidência de todas as dezenas numa janela, para o concurso `t`.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaJanela {
    pub janela: Janela,
    /// O concurso para o qual vale (só concursos `< t` foram lidos).
    pub t: u32,
    /// `n`: concursos usados.
    pub concursos: usize,
    pub primeiro_concurso: u32,
    pub ultimo_concurso: u32,
    /// *K/N*, derivado do `LoteriaConfig`.
    pub p: f64,
    pub prioris: [Priori; QTD_PRIORIS],
    /// Em ordem crescente de dezena.
    pub dezenas: Vec<EvidenciaDezena>,
    /// Quantas dezenas em cada (direção, grau), por priori.
    pub observada: [Grade<usize>; QTD_PRIORIS],
}

/// *K/N* e as prioris, com `1 ≤ K < N`.
fn chance_e_prioris(cfg: &LoteriaConfig) -> Result<(f64, [Priori; QTD_PRIORIS])> {
    let n = cfg.qtd_dezenas_possiveis() as usize;
    if cfg.qtd_dezenas == 0 || cfg.qtd_dezenas >= n {
        return Err(invalida(format!(
            "a {} sorteia {} de {n} dezenas: a evidência exige 1 ≤ K < N",
            cfg.nome_exibicao, cfg.qtd_dezenas
        )));
    }
    let p = cfg.qtd_dezenas as f64 / n as f64;
    let prioris = [
        Priori::nova(p, DESVIOS_PRIORI[0])?,
        Priori::nova(p, DESVIOS_PRIORI[1])?,
        Priori::nova(p, DESVIOS_PRIORI[2])?,
    ];
    Ok((p, prioris))
}

/// Evidência por dezena para o concurso `t`, **só com concursos `< t`**
/// (`resultados` pode trazer qualquer coisa a partir de `t`: é descartado
/// antes de qualquer leitura). Na janela [`Janela::Ultimos`], só os últimos
/// concursos entram (e só eles são conferidos).
///
/// Os concursos usados precisam ser seguidos (sem buraco nem repetição) e
/// válidos como no `sync validar`; senão, [`SpiderError::EvidenciaEntradaInvalida`].
/// Nenhum concurso antes de `t` é [`SpiderError::AvaliacaoSemConcursos`].
pub fn evidencia_por_dezena(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
    t: u32,
    janela: Janela,
) -> Result<EvidenciaJanela> {
    let (p, prioris) = chance_e_prioris(cfg)?;
    if janela == Janela::Ultimos(0) {
        return Err(invalida("janela de 0 concursos"));
    }

    let mut anteriores: Vec<&Resultado> = resultados.iter().filter(|r| r.concurso < t).collect();
    anteriores.sort_by_key(|r| r.concurso);
    let usados: &[&Resultado] = match janela {
        Janela::Inteira => &anteriores,
        Janela::Ultimos(j) => &anteriores[anteriores.len().saturating_sub(j)..],
    };
    let (Some(primeiro), Some(ultimo)) = (usados.first(), usados.last()) else {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso da {} antes do {t} para a evidência",
            cfg.nome_exibicao
        )));
    };
    let (primeiro, ultimo) = (primeiro.concurso, ultimo.concurso);

    let mut saidas = vec![0u64; cfg.qtd_dezenas_possiveis() as usize];
    let mut anterior: Option<u32> = None;
    for r in usados {
        let problemas = validacao::validar_resultado(cfg, r);
        if !problemas.is_empty() {
            let detalhe = problemas
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(invalida(format!("concurso {}: {detalhe}", r.concurso)));
        }
        if let Some(ant) = anterior {
            if r.concurso == ant {
                return Err(invalida(format!("concurso {ant} repetido")));
            }
            if r.concurso != ant + 1 {
                return Err(invalida(format!(
                    "faltam os concursos {}..={}",
                    ant + 1,
                    r.concurso - 1
                )));
            }
        }
        anterior = Some(r.concurso);
        for &d in &r.dezenas {
            saidas[(d - cfg.dezena_min) as usize] += 1;
        }
    }

    let n = usados.len() as u64;
    let mut observada = [Grade::<usize>::default(); QTD_PRIORIS];
    let mut dezenas = Vec::with_capacity(saidas.len());
    for (i, &x) in saidas.iter().enumerate() {
        let mut log10_fb = [0.0; QTD_PRIORIS];
        let mut classificacao = [Classificacao {
            grau: Grau::MuitoFraca,
            direcao: Direcao::Acaso,
        }; QTD_PRIORIS];
        for (k, priori) in prioris.iter().enumerate() {
            log10_fb[k] = log10_fator_bayes(x, n, p, priori)?;
            classificacao[k] = classificar(log10_fb[k])?;
            let c = classificacao[k];
            match c.direcao {
                Direcao::Vies => observada[k].vies[c.grau.indice()] += 1,
                Direcao::Acaso => observada[k].acaso[c.grau.indice()] += 1,
            }
        }
        dezenas.push(EvidenciaDezena {
            dezena: cfg.dezena_min + i as u32,
            saidas: x,
            concursos: n,
            log10_fb,
            classificacao,
        });
    }

    Ok(EvidenciaJanela {
        janela,
        t,
        concursos: usados.len(),
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        p,
        prioris,
        dezenas,
        observada,
    })
}

// ---------------------------------------------------------------------------
// Esperado sob o acaso
// ---------------------------------------------------------------------------

/// Esperado sob o acaso para uma priori.
#[derive(Debug, Clone, PartialEq)]
pub struct EsperadoPriori {
    pub desvio: f64,
    /// Média, entre as réplicas, de dezenas em cada (direção, grau).
    pub media: Grade<f64>,
    /// Erro padrão dessa média (`desvio amostral / √réplicas`).
    pub erro_padrao: Grade<f64>,
}

/// Resultado de [`esperado_sob_o_acaso`].
#[derive(Debug, Clone, PartialEq)]
pub struct EsperadoAcaso {
    /// `n` de cada histórico simulado.
    pub concursos: usize,
    pub replicas: usize,
    pub semente: u64,
    /// Na ordem dos `desvios` recebidos.
    pub por_priori: Vec<EsperadoPriori>,
}

/// Célula de cada `x` em `0..=n`, para a priori dada.
fn tabela_de_celulas(n: u64, p: f64, priori: &Priori) -> Result<Vec<usize>> {
    (0..=n)
        .map(|x| Ok(classificar(log10_fator_bayes(x, n, p, priori)?)?.celula()))
        .collect()
}

/// Simula `replicas` históricos de `concursos` concursos sob o acaso (a cada
/// concurso, *K* dezenas de *N* sem reposição, por Fisher–Yates parcial com
/// [`SplitMix64`] iniciado em `semente`) e devolve, para cada desvio de
/// priori, a média e o erro padrão da contagem de dezenas por (direção,
/// grau). Mesma entrada, mesmos bits.
pub fn esperado_sob_o_acaso(
    cfg: &LoteriaConfig,
    concursos: usize,
    desvios: &[f64],
    replicas: usize,
    semente: u64,
) -> Result<EsperadoAcaso> {
    let (p, _) = chance_e_prioris(cfg)?;
    if concursos == 0 {
        return Err(invalida("simulação sem concursos"));
    }
    if replicas < 2 {
        return Err(invalida("a simulação precisa de ao menos 2 réplicas"));
    }
    if desvios.is_empty() {
        return Err(invalida("nenhuma priori para simular"));
    }
    let tabelas: Vec<Vec<usize>> = desvios
        .iter()
        .map(|&s| tabela_de_celulas(concursos as u64, p, &Priori::nova(p, s)?))
        .collect::<Result<_>>()?;

    let n_dezenas = cfg.qtd_dezenas_possiveis() as usize;
    let k = cfg.qtd_dezenas;
    let mut gerador = SplitMix64::nova(semente);
    let mut baralho: Vec<usize> = (0..n_dezenas).collect();
    let mut saidas = vec![0usize; n_dezenas];
    let mut soma = vec![[0u64; 10]; desvios.len()];
    let mut soma_quadrados = vec![[0u64; 10]; desvios.len()];
    for _ in 0..replicas {
        saidas.iter_mut().for_each(|s| *s = 0);
        for _ in 0..concursos {
            // Fisher–Yates parcial: os K primeiros são um subconjunto
            // uniforme qualquer que seja a ordem anterior do baralho.
            for i in 0..k {
                let j = i + gerador.abaixo_de((n_dezenas - i) as u64)? as usize;
                baralho.swap(i, j);
                saidas[baralho[i]] += 1;
            }
        }
        for (idx, tabela) in tabelas.iter().enumerate() {
            let mut celulas = [0u64; 10];
            for &x in &saidas {
                celulas[tabela[x]] += 1;
            }
            for (c, &v) in celulas.iter().enumerate() {
                soma[idx][c] += v;
                soma_quadrados[idx][c] += v * v;
            }
        }
    }

    let r = replicas as f64;
    let por_priori = desvios
        .iter()
        .enumerate()
        .map(|(idx, &desvio)| {
            let mut media = [0.0; 10];
            let mut erro = [0.0; 10];
            for c in 0..10 {
                let (s, q) = (soma[idx][c] as f64, soma_quadrados[idx][c] as f64);
                media[c] = s / r;
                let variancia = ((q - s * s / r) / (r - 1.0)).max(0.0);
                erro[c] = (variancia / r).sqrt();
            }
            EsperadoPriori {
                desvio,
                media: Grade::de_celulas(media),
                erro_padrao: Grade::de_celulas(erro),
            }
        })
        .collect();

    Ok(EsperadoAcaso {
        concursos,
        replicas,
        semente,
        por_priori,
    })
}

/// Esperado **exato** de dezenas por (direção, grau) sob o acaso, para `n`
/// concursos e a priori de desvio `desvio`: *N* · `P(x cai na célula)` com
/// `x ~ Binomial(n, p)`. Vale para a contagem de dezenas do concurso inteiro
/// pela linearidade da esperança, apesar de as dezenas serem dependentes
/// (sorteio sem reposição). Referência da simulação.
pub fn esperado_exato(cfg: &LoteriaConfig, n: u64, desvio: f64) -> Result<Grade<f64>> {
    let (p, _) = chance_e_prioris(cfg)?;
    let tabela = tabela_de_celulas(n, p, &Priori::nova(p, desvio)?)?;
    let nf = n as f64;
    let ln_fatorial_n = ln_gamma(nf + 1.0);
    let ln_q = (-p).ln_1p();
    let mut celulas = [0.0; 10];
    for (x, &c) in tabela.iter().enumerate() {
        let xf = x as f64;
        let ln_pmf = ln_fatorial_n - ln_gamma(xf + 1.0) - ln_gamma(nf - xf + 1.0)
            + xf * p.ln()
            + (nf - xf) * ln_q;
        celulas[c] += ln_pmf.exp();
    }
    let n_dezenas = f64::from(cfg.qtd_dezenas_possiveis());
    Ok(Grade::de_celulas(celulas.map(|v| v * n_dezenas)))
}

#[cfg(test)]
mod tests;
