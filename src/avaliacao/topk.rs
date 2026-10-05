//! Ranking: acertos das top-k dezenas contra a hipergeométrica (1c.2).
//!
//! Regras em `docs/PLANEJAMENTO.md`, 1c, "Ranking" (decisões de 2026-10-05):
//! - top-k = dezenas com `posicao` ≤ k (o empate já vem resolvido no índice);
//! - k principal = *K* da loteria, mas qualquer k em `1..=N−1` é aceito;
//! - concursos: o mesmo trecho da 1c.1 (do segundo concurso da entrada até o
//!   corte, só os sorteados; nada acima do corte é lido);
//! - ao acaso, os acertos de um concurso seguem a hipergeométrica de *k*
//!   escolhidas entre *N* com *K* sorteadas: média *k·K/N*;
//! - p-valor exato e unilateral: P(soma de T hipergeométricas independentes
//!   ≥ total observado). A correlação serial do ranking não o invalida: sob
//!   o acaso, **dado** o histórico até *t−1* (que fixa o top-k de *t*), os
//!   acertos de *t* seguem H(k; K, N), a mesma distribuição seja qual for
//!   esse histórico; logo as parcelas são i.i.d. H(k; K, N) e a soma tem a
//!   distribuição usada aqui.
//!
//! **Condição de validade**: o p-valor só vale se o perfil (pesos) e o k
//! foram fixados **sem ver** os sorteios do trecho avaliado. Com pesos
//! buscados no mesmo trecho, ou escolhendo o melhor entre vários perfis ou
//! vários k, ele é otimista (pequeno demais): nesses casos, avaliar no
//! conjunto trancado ou corrigir por múltiplas comparações (Bonferroni ou
//! Holm, informando quantos perfis/k foram testados).
//!
//! Ponto no tempo: os acertos de *t* usam só a posição de *t* (que o índice
//! calculou com *t−1*) e o `saiu` de *t*; nada de outro concurso.
//!
//! # p-valor
//!
//! A soma de T hipergeométricas i.i.d. tem suporte com até `T·(máx−mín)+1`
//! valores (Lotofácil, k = 15: ~34 mil). A cauda pode ser muito menor que o
//! menor `f64` (acertar as 15 em 3400 concursos: ~10^−22000), então a
//! convolução direta da pmf zera a cauda. Por isso a conta usa a
//! **inclinação exponencial**: com `q_θ(i) = p(i)·e^{θi} / M(θ)`, onde
//! `M(θ) = Σ p(i)·e^{θi}`, vale exatamente
//! `p^{*T}(s) = q_θ^{*T}(s) · M(θ)^T · e^{−θs}` para qualquer θ. Escolhendo θ
//! de modo que a média de `q_θ` seja `total/T`, a distribuição inclinada fica
//! centrada no total observado e os termos que importam são da ordem de
//! `1/√T`, sem underflow; o fator `M^T·e^{−θs}` entra em log. O resultado é
//! exato a menos de arredondamento (erro relativo da ordem de `T·eps`, ~1e−12
//! no pior caso real), e θ precisa só centrar a conta, não ser exato.
//!
//! A convolução T vezes é feita passo a passo (uma parcela por vez, ~T²·L·w/2
//! operações, com `w` = máx−mín+1 ≤ 11 valores no suporte e `L` = w−1 a
//! largura de cada parcela), descartando as somas
//! parciais que não alcançam o total observado nem com todas as parcelas
//! restantes no máximo. Todas as contas são somas de termos não negativos,
//! na mesma ordem a cada execução: mesma entrada, mesmos bits.

use super::calibracao::{ordenar_e_validar, Observacao};
use super::metricas::{comparar, Comparacao};
use super::{ate_o_corte, concursos_avaliados};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// Coeficiente binomial `C(n, r)` em inteiros, ou `None` se não couber em
/// `u128` (não acontece nas loterias reais: o maior é `C(80, 40)` ≈ 1,1e23;
/// o primeiro `C(n, n/2)` que estoura é `C(132, 66)`). `C(n, r) = 0` com
/// `r > n`.
pub fn binomial(n: u32, r: u32) -> Option<u128> {
    if r > n {
        return Some(0);
    }
    let r = r.min(n - r);
    let mut c: u128 = 1;
    for i in 1..=u128::from(r) {
        // c = C(n, i−1) e C(n, i) = c·(n−i+1)/i. Com g = mdc(c, i), i/g é
        // primo com c/g e divide c·(n−i+1), logo divide (n−i+1): as duas
        // divisões são exatas e o produto é o próprio C(n, i), então só
        // estoura se o resultado (≤ C(n, r)) não couber.
        let g = mdc(c, i);
        c = (c / g).checked_mul((u128::from(n) - i + 1) / (i / g))?;
    }
    Some(c)
}

fn mdc(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Hipergeométrica: acertos ao escolher `escolhidas` de `n` dezenas quando
/// `sorteadas` delas saem. Guardada em inteiros:
/// `P(i) = C(sorteadas, i)·C(n − sorteadas, escolhidas − i) / C(n, escolhidas)`.
///
/// Sem serde de propósito: só se constrói por [`Hipergeometrica::nova`], que
/// garante `casos` coerentes com os parâmetros (não é struct de saída).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hipergeometrica {
    n: u32,
    sorteadas: u32,
    escolhidas: u32,
    /// `casos[i]` = numerador de `P(i)`, para `i` em `0..=maximo()`; zero
    /// abaixo de `minimo()`.
    casos: Vec<u128>,
    /// `C(n, escolhidas)`; os `casos` somam exatamente isto (Vandermonde).
    total: u128,
}

impl Hipergeometrica {
    /// Distribuição geral, com `sorteadas` e `escolhidas` em `0..=n`.
    pub fn nova(n: u32, sorteadas: u32, escolhidas: u32) -> Result<Self> {
        if sorteadas > n || escolhidas > n {
            return Err(invalida(format!(
                "hipergeométrica com {sorteadas} sorteada(s) e {escolhidas} escolhida(s) entre {n} dezenas"
            )));
        }
        let estouro = || {
            invalida(format!(
                "hipergeométrica com n = {n}: C({n}, {escolhidas}) não cabe em 128 bits"
            ))
        };
        let total = binomial(n, escolhidas).ok_or_else(estouro)?;
        let maximo = sorteadas.min(escolhidas);
        let mut casos = Vec::with_capacity(maximo as usize + 1);
        for i in 0..=maximo {
            // Cada produto é uma parcela de C(n, escolhidas) (Vandermonde),
            // então cabe se o total coube.
            let a = binomial(sorteadas, i).ok_or_else(estouro)?;
            let b = binomial(n - sorteadas, escolhidas - i).ok_or_else(estouro)?;
            casos.push(a.checked_mul(b).ok_or_else(estouro)?);
        }
        Ok(Self {
            n,
            sorteadas,
            escolhidas,
            casos,
            total,
        })
    }

    /// Acertos das top-`k` dezenas num sorteio ao acaso da loteria: `k` de
    /// *N* com *K* sorteadas, derivados do `LoteriaConfig`. `k` fora de
    /// `1..=N−1` é [`SpiderError::AvaliacaoEntradaInvalida`].
    pub fn da_loteria(cfg: &LoteriaConfig, k: u32) -> Result<Self> {
        validar_k(cfg, k)?;
        let sorteadas = u32::try_from(cfg.qtd_dezenas).map_err(|_| {
            invalida(format!(
                "a {} sorteia {} dezenas",
                cfg.nome_exibicao, cfg.qtd_dezenas
            ))
        })?;
        Self::nova(cfg.qtd_dezenas_possiveis(), sorteadas, k)
    }

    pub fn n(&self) -> u32 {
        self.n
    }

    pub fn sorteadas(&self) -> u32 {
        self.sorteadas
    }

    pub fn escolhidas(&self) -> u32 {
        self.escolhidas
    }

    /// Menor número de acertos possível: `max(0, escolhidas + sorteadas − n)`.
    pub fn minimo(&self) -> u32 {
        (self.escolhidas + self.sorteadas).saturating_sub(self.n)
    }

    /// Maior número de acertos possível: `min(escolhidas, sorteadas)`.
    pub fn maximo(&self) -> u32 {
        self.sorteadas.min(self.escolhidas)
    }

    /// Numerador inteiro de `P(i)`; zero fora de `minimo()..=maximo()`.
    pub fn casos(&self, i: u32) -> u128 {
        self.casos.get(i as usize).copied().unwrap_or(0)
    }

    /// Denominador comum: `C(n, escolhidas)`.
    pub fn total(&self) -> u128 {
        self.total
    }

    /// `P(i)` em `f64` (as duas conversões e a divisão arredondam: erro
    /// relativo de até ~3 eps); zero fora de `minimo()..=maximo()`.
    pub fn pmf(&self, i: u32) -> f64 {
        self.casos(i) as f64 / self.total as f64
    }

    /// Média `escolhidas·sorteadas/n` (o *k·K/N* do plano).
    pub fn media(&self) -> f64 {
        f64::from(self.escolhidas) * f64::from(self.sorteadas) / f64::from(self.n)
    }

    /// Variância `k·(K/N)·((N−K)/N)·((N−k)/(N−1))`; zero com `n` ≤ 1.
    pub fn variancia(&self) -> f64 {
        if self.n <= 1 {
            return 0.0;
        }
        let (n, k, s) = (
            f64::from(self.n),
            f64::from(self.escolhidas),
            f64::from(self.sorteadas),
        );
        k * (s / n) * ((n - s) / n) * ((n - k) / (n - 1.0))
    }

    /// `P(minimo() + j)` para `j` em `0..=maximo()−minimo()`: o suporte, onde
    /// todo valor é positivo (o suporte da hipergeométrica é contíguo).
    fn pmf_no_suporte(&self) -> Vec<f64> {
        (self.minimo()..=self.maximo())
            .map(|i| self.pmf(i))
            .collect()
    }
}

/// Probabilidade de cauda, com o log para quando ela é pequena demais para
/// `f64`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cauda {
    /// Em `[0, 1]`. Vira 0 (ou subnormal) quando a cauda é menor que o menor
    /// `f64` (~1e−308); `log10` continua finito nesses casos.
    pub probabilidade: f64,
    /// `log10(probabilidade)` calculado sem passar por `probabilidade`:
    /// finito (≤ 0) para todo total possível, mesmo com
    /// `probabilidade` = 0.
    pub log10: f64,
}

/// p-valor exato unilateral: P(S ≥ `total`), com S a soma de `t`
/// hipergeométricas `h` independentes. Abordagem e precisão na documentação
/// do módulo.
///
/// - `total` ≤ `t·minimo()` (inclui `t` = 0 com `total` = 0): exatamente 1;
/// - `total` > `t·maximo()` (impossível) é
///   [`SpiderError::AvaliacaoEntradaInvalida`];
/// - monotônico (não crescente) em `total`, sempre em `[0, 1]`.
pub fn p_valor_soma(h: &Hipergeometrica, t: usize, total: u64) -> Result<Cauda> {
    let minimo = t as u128 * u128::from(h.minimo());
    let maximo = t as u128 * u128::from(h.maximo());
    let total_u = u128::from(total);
    if total_u > maximo {
        return Err(invalida(format!(
            "{total} acertos em {t} concurso(s) com top-{}: o máximo possível é {maximo}",
            h.escolhidas
        )));
    }
    if total_u <= minimo {
        return Ok(Cauda {
            probabilidade: 1.0,
            log10: 0.0,
        });
    }
    // Daqui em diante t ≥ 1 e o suporte tem pelo menos dois valores.
    let pmf = h.pmf_no_suporte();
    let largura = pmf.len() - 1;
    // Total deslocado: soma dos `j = acertos − minimo` de cada concurso.
    // Cabe em usize porque é ≤ t·largura, o tamanho do vetor da convolução.
    let alvo = (total_u - minimo) as usize;
    let topo = t * largura;

    // Até a média (total·N ≤ t·k·K, em inteiros), sem inclinação: a cauda é
    // ≥ ~1/2 e não há underflow que importe. A soma de cima para baixo, na
    // mesma ordem, deixa a cauda monotônica no total por construção.
    let media_vezes_n = t as u128 * u128::from(h.escolhidas) * u128::from(h.sorteadas);
    if total_u * u128::from(h.n) <= media_vezes_n {
        let dist = potencia_de_convolucao(&pmf, t, alvo);
        let p = dist[alvo..].iter().rev().sum::<f64>().min(1.0);
        return Ok(Cauda {
            probabilidade: p,
            log10: p.log10(),
        });
    }

    let ln_p: Vec<f64> = pmf.iter().map(|p| p.ln()).collect();
    let ln_cauda = if alvo == topo {
        // Todos os concursos no máximo: P = p(máx)^t, direto em log (θ seria
        // infinito).
        t as f64 * ln_p[largura]
    } else {
        let theta = inclinacao_para_media(&ln_p, alvo as f64 / t as f64);
        let (q, ln_m) = inclinar(&ln_p, theta);
        let dist = potencia_de_convolucao(&q, t, alvo);
        // Σ_{x ≥ alvo} q^{*t}(x)·e^{−θ(x−alvo)}, das parcelas menores (topo)
        // para as maiores (perto do alvo). É ≥ q^{*t}(alvo) > 0.
        let soma: f64 = (alvo..=topo)
            .rev()
            .map(|x| dist[x] * (-theta * (x - alvo) as f64).exp())
            .sum();
        soma.ln() + t as f64 * ln_m - theta * alvo as f64
    };
    // Arredondamento pode empurrar um pouco acima de 0 quando a cauda é ~1.
    let ln_cauda = ln_cauda.min(0.0);
    Ok(Cauda {
        probabilidade: ln_cauda.exp(),
        log10: ln_cauda / std::f64::consts::LN_10,
    })
}

/// Distribuição da soma de `t` hipergeométricas `h` independentes:
/// `dist[s]` = P(S = s), para `s` em `0..=t·maximo()`. Sem inclinação, então
/// os valores da cauda menores que ~1e−308 viram 0; serve para conferir média,
/// variância e casos pequenos, não para o p-valor de caudas extremas.
///
/// Só para testes: a saída da 1c.2/1c.5 é a distribuição de acertos **por
/// concurso** ([`FaixaAcertos`]), não a da soma (~34 mil valores na
/// Lotofácil).
#[cfg(test)]
pub(crate) fn distribuicao_soma(h: &Hipergeometrica, t: usize) -> Vec<f64> {
    let deslocamento = t * h.minimo() as usize;
    let mut saida = vec![0.0; t * h.maximo() as usize + 1];
    let pmf = h.pmf_no_suporte();
    let dist = potencia_de_convolucao(&pmf, t, 0);
    saida[deslocamento..].copy_from_slice(&dist);
    saida
}

/// `q^{*t}` (convolução de `t` cópias de `q`), com `q[j]` ≥ 0 e `q.len()` ≥ 1.
/// Só os índices ≥ `alvo` do resultado são calculados; os abaixo voltam 0.
///
/// Uma parcela por vez, no próprio vetor, de cima para baixo (o valor novo
/// em `y` só lê índices ≤ `y`, ainda não sobrescritos). No passo `p` só
/// importam as somas parciais que, com as `t−p−1` parcelas restantes no
/// máximo, ainda alcançam `alvo`; as abaixo disso ficam velhas e nunca mais
/// são lidas (o piso do passo seguinte, menos uma parcela, é o piso deste).
fn potencia_de_convolucao(q: &[f64], t: usize, alvo: usize) -> Vec<f64> {
    let largura = q.len() - 1;
    let mut dist = vec![0.0_f64; t * largura + 1];
    dist[0] = 1.0;
    for passo in 0..t {
        let topo_antes = passo * largura;
        let topo = topo_antes + largura;
        let piso = alvo.saturating_sub((t - passo - 1) * largura);
        for y in (piso..=topo).rev() {
            let j_min = y.saturating_sub(topo_antes);
            let j_max = largura.min(y);
            let mut soma = 0.0;
            for j in j_min..=j_max {
                soma += dist[y - j] * q[j];
            }
            dist[y] = soma;
        }
    }
    dist[..alvo].fill(0.0);
    dist
}

/// Média de `q_θ` (pmf inclinada) com `ln_p[j]` = ln P(mínimo + j).
fn media_inclinada(ln_p: &[f64], theta: f64) -> f64 {
    let (q, _) = inclinar(ln_p, theta);
    q.iter().enumerate().map(|(j, q)| j as f64 * q).sum()
}

/// `q_θ(j) = p(j)·e^{θj} / M(θ)` e `ln M(θ)`, em log para não estourar.
fn inclinar(ln_p: &[f64], theta: f64) -> (Vec<f64>, f64) {
    let l: Vec<f64> = ln_p
        .iter()
        .enumerate()
        .map(|(j, lp)| lp + theta * j as f64)
        .collect();
    let maior = l.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let soma: f64 = l.iter().map(|x| (x - maior).exp()).sum();
    let ln_m = maior + soma.ln();
    (l.iter().map(|x| (x - ln_m).exp()).collect(), ln_m)
}

/// θ ≥ 0 com média inclinada ≈ `media_alvo` (estritamente entre a média de
/// `p` e o máximo `ln_p.len() − 1`). A média inclinada cresce com θ (a
/// derivada é a variância), então bissecção com número fixo de passos:
/// determinística. Precisão de θ não importa para a exatidão do p-valor, só
/// para centrar a conta.
fn inclinacao_para_media(ln_p: &[f64], media_alvo: f64) -> f64 {
    let mut baixo = 0.0_f64;
    let mut alto = 1.0_f64;
    // Acima de ~1e3 as parcelas abaixo do máximo já somem em e^{−θ}.
    while media_inclinada(ln_p, alto) < media_alvo && alto < 1024.0 {
        baixo = alto;
        alto *= 2.0;
    }
    for _ in 0..100 {
        let meio = 0.5 * (baixo + alto);
        if meio <= baixo || meio >= alto {
            break;
        }
        if media_inclinada(ln_p, meio) < media_alvo {
            baixo = meio;
        } else {
            alto = meio;
        }
    }
    0.5 * (baixo + alto)
}

/// Acertos das top-k dezenas num concurso avaliado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcertosConcurso {
    pub concurso: u32,
    /// Quantas dezenas com `posicao` ≤ k saíram.
    pub acertos: u32,
}

/// Uma linha da distribuição de acertos: observado ao lado do acaso.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaixaAcertos {
    pub acertos: u32,
    /// Concursos avaliados com exatamente `acertos` acertos.
    pub observados: usize,
    /// T × P(`acertos`) pela hipergeométrica.
    pub esperados: f64,
}

/// Resumo do top-k de um perfil num trecho da série.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumoTopK {
    pub k: u32,
    /// T: quantos concursos entraram.
    pub concursos_avaliados: usize,
    pub primeiro_concurso: u32,
    pub ultimo_concurso: u32,
    /// Soma dos acertos nos T concursos (a estatística do p-valor).
    pub total_acertos: u64,
    /// Acertos por concurso: média do perfil, *k·K/N* do acaso, diferença e
    /// incerteza por concurso, como na 1c.1. Aqui **maior é melhor**.
    /// Mesma ressalva da 1c.1: com a correlação serial do ranking, o IC normal
    /// tende a sair estreito (otimista); o veredito da 1c.5 usa teste robusto.
    pub acertos: Comparacao,
    /// Uma linha para cada número de acertos de 0 a min(k, K).
    pub distribuicao: Vec<FaixaAcertos>,
    /// P(acaso dar ≥ `total_acertos` em T concursos), exato e unilateral.
    pub p_valor: Cauda,
}

/// Acertos das top-`k` por concurso avaliado, em ordem de concurso.
///
/// Mesmo trecho e mesma validação de [`super::metricas_por_concurso`]:
/// descarta antes de tudo as observações acima de `corte` (nada acima do
/// corte é lido, nem para validar), valida a série até o corte e avalia do
/// segundo concurso da entrada até o corte, só os já sorteados. `k` fora de
/// `1..=N−1` é [`SpiderError::AvaliacaoEntradaInvalida`].
pub fn acertos_por_concurso(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
    corte: u32,
) -> Result<Vec<AcertosConcurso>> {
    validar_k(cfg, k)?;
    let linhas = ordenar_e_validar(cfg, &ate_o_corte(observacoes, corte))?;
    Ok(
        concursos_avaliados(&linhas, |o| o.concurso, |o| o.saiu.is_some())
            .map(|grupo| AcertosConcurso {
                concurso: grupo[0].concurso,
                // Só a posição e o `saiu` do próprio concurso.
                acertos: grupo
                    .iter()
                    .filter(|o| o.posicao <= k && o.saiu == Some(true))
                    .count() as u32,
            })
            .collect(),
    )
}

/// Resume uma série de acertos por concurso. Série vazia é
/// [`SpiderError::AvaliacaoSemConcursos`]; `k` fora de `1..=N−1` ou acertos
/// impossíveis para o top-`k` são [`SpiderError::AvaliacaoEntradaInvalida`].
pub fn resumir_topk(cfg: &LoteriaConfig, k: u32, serie: &[AcertosConcurso]) -> Result<ResumoTopK> {
    let h = Hipergeometrica::da_loteria(cfg, k)?;
    let (Some(primeiro), Some(ultimo)) = (
        serie.iter().map(|a| a.concurso).min(),
        serie.iter().map(|a| a.concurso).max(),
    ) else {
        return Err(SpiderError::AvaliacaoSemConcursos(
            "a série de acertos por concurso está vazia".to_string(),
        ));
    };
    if let Some(fora) = serie
        .iter()
        .find(|a| a.acertos < h.minimo() || a.acertos > h.maximo())
    {
        return Err(invalida(format!(
            "concurso {}: {} acerto(s) no top-{k}, mas o possível é {}..={}",
            fora.concurso,
            fora.acertos,
            h.minimo(),
            h.maximo()
        )));
    }

    let t = serie.len();
    let esperado = h.media();
    let pares: Vec<(f64, f64)> = serie
        .iter()
        .map(|a| (f64::from(a.acertos), esperado))
        .collect();
    let total_acertos: u64 = serie.iter().map(|a| u64::from(a.acertos)).sum();

    let mut observados = vec![0_usize; h.maximo() as usize + 1];
    for a in serie {
        observados[a.acertos as usize] += 1;
    }
    let distribuicao = observados
        .into_iter()
        .zip(0_u32..)
        .map(|(observados, acertos)| FaixaAcertos {
            acertos,
            observados,
            esperados: t as f64 * h.pmf(acertos),
        })
        .collect();

    Ok(ResumoTopK {
        k,
        concursos_avaliados: t,
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        total_acertos,
        acertos: comparar(&pares),
        distribuicao,
        p_valor: p_valor_soma(&h, t, total_acertos)?,
    })
}

/// [`acertos_por_concurso`] seguido de [`resumir_topk`]. Sem nenhum concurso
/// sorteado entre o segundo da entrada e `corte` é
/// [`SpiderError::AvaliacaoSemConcursos`].
pub fn avaliar_topk(
    cfg: &LoteriaConfig,
    observacoes: &[Observacao],
    k: u32,
    corte: u32,
) -> Result<ResumoTopK> {
    let serie = acertos_por_concurso(cfg, observacoes, k, corte)?;
    if serie.is_empty() {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado entre o segundo concurso do índice e o corte {corte}"
        )));
    }
    resumir_topk(cfg, k, &serie)
}

/// top-k precisa pegar ao menos uma dezena e deixar ao menos uma de fora.
fn validar_k(cfg: &LoteriaConfig, k: u32) -> Result<()> {
    let n = cfg.qtd_dezenas_possiveis();
    if k == 0 || k >= n {
        return Err(invalida(format!(
            "k = {k} fora de 1..={} no top-k da {} (N = {n})",
            n - 1,
            cfg.nome_exibicao
        )));
    }
    Ok(())
}

fn invalida(detalhe: String) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe)
}

#[cfg(test)]
mod tests;
