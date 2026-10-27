//! Notas A e B, ordenação, evidência e esperado sob o acaso do ranking por
//! taxa histórica — Fase 1f, tarefa 1f.2.
//!
//! Regras em `docs/PLANEJAMENTO.md`, "Fase 1f". Funções puras (sem I/O, sem
//! SQL, determinísticas) sobre as taxas da 1f.1 ([`super::calcular_taxas`]).
//! É descrição do passado: sorteios são independentes e o esperado de
//! qualquer ranking feito com estas notas é **não superar o acaso**.
//!
//! # Notas
//!
//! - **Nota A (principal)** de uma dezena num concurso `t`: a taxa da posição
//!   que ela ocupava no índice antes do sorteio ([`LinhaTaxas::taxa_posicao`]),
//!   só com os concursos até `t−1`. É o mesmo número da calibração da 1c
//!   ([`super::calibrar`]), bit a bit.
//! - **Nota B (experimental)**: `ln(taxa_posição / (K/N)) + Σ_{c=0..8}
//!   ln(taxa_faixa[c] / (K/N))` ([`nota_b`]), isto é, naive Bayes sobre a
//!   posição e as 9 faixas (a faixa `NULL` entra com a própria taxa, como
//!   qualquer outra). **É experimental**: os componentes são correlacionados
//!   (colinearidade descrita em *Casos de borda* do plano), então B conta a
//!   mesma informação mais de uma vez e exagera a distância ao acaso; nunca
//!   deve ser apresentada como a nota principal. K/N vem de
//!   [`super::taxa_do_acaso`]. Com todas as taxas iguais a K/N, B vale 0.
//!
//! # Ordenação
//!
//! Por concurso e separadamente para A e para B ([`ranquear`]): nota
//! decrescente, **arredondada a 1e-9** antes de comparar, desempate pela menor
//! dezena; posições 1 … N. A regra espelha a do índice (`motor::indice`,
//! `arredondar` e `ESCALA_TOTAL`, privados lá): `(x · 1e9).round() / 1e9` com
//! `-0.0` trocado por `0.0`, senão um "zero negativo" ordenaria depois de um
//! zero positivo e quebraria o desempate. As colunas de nota da saída são os
//! valores **crus** (a nota A tem de ser igual à da 1c); só a comparação usa o
//! valor arredondado.
//!
//! # Ponto no tempo
//!
//! As notas de `t` herdam o ponto no tempo das taxas: nada do concurso `t`
//! entra nelas. A ordenação olha apenas as dezenas do próprio concurso. O
//! snapshot do próximo concurso (`saiu` ausente) vem das taxas com todo o
//! histórico. A evidência e o esperado descrevem o histórico **inteiro**
//! (como a 1d-A) e não são usados para avaliar nada.
//!
//! # Evidência
//!
//! [`evidencia_das_taxas`]: para cada posição `r` (x = saídas da posição, n =
//! concursos incorporados) e cada (componente, faixa), inclusive `NULL` (x =
//! saídas, n = pares concurso × dezena), o fator de Bayes e o grau da 1d
//! ([`log10_fator_bayes`], [`classificar`]) nas três prioris de
//! [`DESVIOS_PRIORI`] (índice 0 = a principal, 5 %), só na janela do histórico
//! inteiro. Células com n = 0 ficam fora (sem fator de Bayes) e não entram na
//! contagem de graus.
//!
//! **Ressalva**: nas faixas, os pares do mesmo concurso não são
//! independentes (sob o acaso, a quantidade de saídas dentro de uma faixa num
//! concurso é hipergeométrica, não binomial). O fator de Bayes binomial da 1d
//! é aplicado mesmo assim, o que desloca a distribuição dos graus; por isso a
//! comparação justa da contagem observada é com o esperado simulado abaixo,
//! que respeita essa dependência.
//!
//! # Esperado sob o acaso
//!
//! [`esperado_das_taxas`]: em cada réplica e para cada concurso sorteado da
//! série, sorteia *K* de *N* dezenas sem reposição (Fisher–Yates parcial com
//! [`SplitMix64`], semente fixa), **mantendo as posições e faixas reais** de
//! cada dezena naquele concurso, recalcula as contagens, os fatores de Bayes e
//! os graus e devolve média e erro padrão por grau e direção. Para as
//! posições o resultado equivale a [`super::esperado_sob_o_acaso`] com o mesmo
//! n (as posições são uma permutação das dezenas); quando a posição de cada
//! dezena é a mesma em todos os concursos, coincide bit a bit com ela (mesma
//! sequência de sorteios). Contagens inteiras, sem alocação por réplica.

use super::{
    classificar, log10_fator_bayes, taxa_do_acaso, Classificacao, Contagem, Direcao,
    EsperadoPriori, Faixa, Grade, LinhaTaxas, Priori, SplitMix64, TaxasAcumuladas, DESVIOS_PRIORI,
    QTD_COMPONENTES, QTD_FAIXAS, QTD_PRIORIS,
};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};

/// O total é arredondado a múltiplos de `1 / ESCALA_NOTA` (1e-9) antes de
/// ordenar, como no índice (`motor::indice::ESCALA_TOTAL`).
const ESCALA_NOTA: f64 = 1e9;

/// Células por dezena num concurso: a posição e uma faixa por componente.
const CELULAS_POR_DEZENA: usize = 1 + QTD_COMPONENTES;

/// Quantas (componente, faixa) existem.
const QTD_CELULAS_FAIXA: usize = QTD_COMPONENTES * QTD_FAIXAS;

fn invalida(detalhe: impl Into<String>) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe.into())
}

// ---------------------------------------------------------------------------
// Notas e ordenação
// ---------------------------------------------------------------------------

/// Uma linha do ranking: as duas notas de uma dezena num concurso e a posição
/// que cada uma lhe dá.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinhaRanking {
    pub concurso: u32,
    pub dezena: u32,
    /// Posição que o índice deu à dezena antes do sorteio (1 … N).
    pub posicao_indice: u32,
    /// Copiado da entrada (`None` no snapshot do próximo concurso).
    pub saiu: Option<bool>,
    /// Nota A: a taxa da posição, crua (igual à `probabilidade` da 1c).
    pub nota_a: f64,
    /// Nota B (experimental), crua; ver [`nota_b`].
    pub nota_b: f64,
    /// Posição pela nota A (1 … N; 1 = maior nota).
    pub posicao_ranking_a: u32,
    /// Posição pela nota B (1 … N; 1 = maior nota).
    pub posicao_ranking_b: u32,
}

/// Arredonda a 1e-9 e troca `-0.0` por `0.0` (ver "Ordenação" no topo).
fn arredondar(nota: f64) -> f64 {
    let r = (nota * ESCALA_NOTA).round() / ESCALA_NOTA;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// Nota B (**experimental**) de uma linha de taxas:
/// `ln(taxa_posição / (K/N)) + Σ_{c=0..8} ln(taxa_faixa[c] / (K/N))`, somando
/// primeiro a posição e depois os componentes na ordem de
/// [`super::NOMES_COMPONENTES`]. Naive Bayes sobre atributos correlacionados:
/// conta a mesma informação mais de uma vez (ver o topo do módulo). Com todas
/// as taxas iguais a *K/N* vale exatamente 0.
pub fn nota_b(cfg: &LoteriaConfig, linha: &LinhaTaxas) -> f64 {
    let acaso = taxa_do_acaso(cfg);
    let mut soma = (linha.taxa_posicao / acaso).ln();
    for taxa in linha.taxas_faixa {
        soma += (taxa / acaso).ln();
    }
    soma
}

/// Posição no ranking (1 … N) de cada nota, na ordem de `notas`/`dezenas`:
/// nota arredondada decrescente, empate → menor dezena à frente. Notas
/// finitas (já conferidas por quem chama).
fn posicoes_do_ranking(notas: &[f64], dezenas: &[u32]) -> Vec<u32> {
    let arredondadas: Vec<f64> = notas.iter().map(|&x| arredondar(x)).collect();
    let mut ordem: Vec<usize> = (0..notas.len()).collect();
    // Sem NaN e sem -0.0 (ver `arredondar`): `total_cmp` coincide com a ordem
    // numérica.
    ordem.sort_by(|&a, &b| {
        arredondadas[b]
            .total_cmp(&arredondadas[a])
            .then(dezenas[a].cmp(&dezenas[b]))
    });
    let mut posicoes = vec![0_u32; notas.len()];
    for (pos, &i) in (1_u32..).zip(&ordem) {
        posicoes[i] = pos;
    }
    posicoes
}

/// Linhas ordenadas por (concurso, dezena) e agrupadas por concurso, cada
/// grupo conferido: as *N* dezenas da loteria uma vez cada, posição em
/// 1 … N e `saiu` todo conhecido ou todo ausente.
fn agrupar<'a>(cfg: &LoteriaConfig, linhas: &'a [LinhaTaxas]) -> Result<Vec<Vec<&'a LinhaTaxas>>> {
    let n = cfg.qtd_dezenas_possiveis() as usize;
    let mut ordenadas: Vec<&LinhaTaxas> = linhas.iter().collect();
    ordenadas.sort_by_key(|l| (l.concurso, l.dezena));
    let mut grupos = Vec::new();
    for grupo in ordenadas.chunk_by(|a, b| a.concurso == b.concurso) {
        let concurso = grupo[0].concurso;
        if grupo.len() != n {
            return Err(invalida(format!(
                "o concurso {concurso} tem {} linhas, esperadas {n} (uma por dezena)",
                grupo.len()
            )));
        }
        for (i, l) in grupo.iter().enumerate() {
            let esperada = cfg.dezena_min + i as u32;
            if l.dezena != esperada {
                return Err(invalida(format!(
                    "concurso {concurso}: esperada a dezena {esperada}, veio a {} \
                     (dezena faltando, repetida ou fora da loteria)",
                    l.dezena
                )));
            }
            if l.posicao == 0 || l.posicao as usize > n {
                return Err(invalida(format!(
                    "concurso {concurso}, dezena {}: posição {} fora de 1..={n}",
                    l.dezena, l.posicao
                )));
            }
            if l.saiu.is_some() != grupo[0].saiu.is_some() {
                return Err(invalida(format!(
                    "concurso {concurso}: `saiu` conhecido em umas dezenas e ausente em outras"
                )));
            }
        }
        grupos.push(grupo.to_vec());
    }
    Ok(grupos)
}

/// Notas A e B e a posição de cada dezena por nota, concurso a concurso, a
/// partir das linhas de [`super::calcular_taxas`] (`SerieTaxas::linhas`).
///
/// Uma linha por (concurso, dezena) da entrada, ordenadas por (concurso,
/// dezena), inclusive o snapshot do próximo concurso. A entrada pode vir em
/// qualquer ordem, mas cada concurso tem de trazer as *N* dezenas da loteria
/// uma vez cada, com posição em 1 … N e `saiu` todo conhecido ou todo ausente;
/// nota não finita também é recusada. Tudo isso é
/// [`SpiderError::AvaliacaoEntradaInvalida`]. Entrada vazia devolve vazio.
pub fn ranquear(cfg: &LoteriaConfig, linhas: &[LinhaTaxas]) -> Result<Vec<LinhaRanking>> {
    let mut saida = Vec::with_capacity(linhas.len());
    for grupo in agrupar(cfg, linhas)? {
        let dezenas: Vec<u32> = grupo.iter().map(|l| l.dezena).collect();
        let notas_a: Vec<f64> = grupo.iter().map(|l| l.taxa_posicao).collect();
        let notas_b: Vec<f64> = grupo.iter().map(|l| nota_b(cfg, l)).collect();
        for (rotulo, notas) in [("A", &notas_a), ("B", &notas_b)] {
            if let Some(i) = notas.iter().position(|&x| !arredondar(x).is_finite()) {
                return Err(invalida(format!(
                    "concurso {}, dezena {}: nota {rotulo} não finita ({})",
                    grupo[i].concurso, grupo[i].dezena, notas[i]
                )));
            }
        }
        let ranking_a = posicoes_do_ranking(&notas_a, &dezenas);
        let ranking_b = posicoes_do_ranking(&notas_b, &dezenas);
        for (i, l) in grupo.iter().enumerate() {
            saida.push(LinhaRanking {
                concurso: l.concurso,
                dezena: l.dezena,
                posicao_indice: l.posicao,
                saiu: l.saiu,
                nota_a: notas_a[i],
                nota_b: notas_b[i],
                posicao_ranking_a: ranking_a[i],
                posicao_ranking_b: ranking_b[i],
            });
        }
    }
    Ok(saida)
}

// ---------------------------------------------------------------------------
// Evidência (graus da 1d)
// ---------------------------------------------------------------------------

/// *K/N* e as prioris da 1d, com `1 ≤ K < N` (espelha `chance_e_prioris` da
/// 1d, privada lá).
fn chance_e_prioris(cfg: &LoteriaConfig) -> Result<(f64, [Priori; QTD_PRIORIS])> {
    let n = cfg.qtd_dezenas_possiveis() as usize;
    if cfg.qtd_dezenas == 0 || cfg.qtd_dezenas >= n {
        return Err(SpiderError::EvidenciaEntradaInvalida(format!(
            "a {} sorteia {} de {n} dezenas: a evidência exige 1 ≤ K < N",
            cfg.nome_exibicao, cfg.qtd_dezenas
        )));
    }
    let p = taxa_do_acaso(cfg);
    let nova = |i: usize| Priori::nova(p, DESVIOS_PRIORI[i]);
    Ok((p, [nova(0)?, nova(1)?, nova(2)?]))
}

/// Posição (0 … 9) de uma classificação na grade de 10 células: viés (0 … 4)
/// e depois acaso (5 … 9), como na 1d.
fn celula(c: Classificacao) -> usize {
    let base = match c.direcao {
        Direcao::Vies => 0,
        Direcao::Acaso => 5,
    };
    base + c.grau.indice()
}

fn grade_de<T: Copy>(c: [T; 10]) -> Grade<T> {
    Grade {
        vies: [c[0], c[1], c[2], c[3], c[4]],
        acaso: [c[5], c[6], c[7], c[8], c[9]],
    }
}

/// Fator de Bayes e grau de uma célula, nas três prioris.
#[derive(Debug, Clone, PartialEq)]
pub struct AvaliacaoFb {
    /// Na ordem de [`DESVIOS_PRIORI`] (0 = principal).
    pub log10_fb: [f64; QTD_PRIORIS],
    pub classificacao: [Classificacao; QTD_PRIORIS],
}

/// Evidência de uma posição do índice.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaPosicao {
    /// 1 … N.
    pub posicao: u32,
    /// `x`: quantas vezes a posição saiu.
    pub saidas: u64,
    /// `n`: concursos incorporados.
    pub total: u64,
    /// `None` se `total` = 0 (sem fator de Bayes; fora da contagem de graus).
    pub avaliacao: Option<AvaliacaoFb>,
}

/// Evidência de um (componente, faixa).
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaFaixa {
    /// 0 … 8, na ordem de [`super::NOMES_COMPONENTES`].
    pub componente: usize,
    pub faixa: Faixa,
    /// `x`: saídas dentro da faixa.
    pub saidas: u64,
    /// `n`: pares concurso × dezena na faixa.
    pub total: u64,
    /// `None` se `total` = 0 (sem fator de Bayes; fora da contagem de graus).
    pub avaliacao: Option<AvaliacaoFb>,
}

/// Evidência por posição e por (componente, faixa), histórico inteiro.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenciaTaxas {
    /// *K/N*, derivado do `LoteriaConfig`.
    pub p: f64,
    pub prioris: [Priori; QTD_PRIORIS],
    /// Concursos incorporados nas contagens.
    pub concursos: u64,
    /// As *N* posições, em ordem crescente.
    pub posicoes: Vec<EvidenciaPosicao>,
    /// Os 9 × 4 (componente, faixa), por componente e, dentro dele, na ordem
    /// de [`Faixa::TODAS`].
    pub faixas: Vec<EvidenciaFaixa>,
    /// Quantas posições em cada (direção, grau), por priori; só as de n > 0.
    pub observada_posicoes: [Grade<usize>; QTD_PRIORIS],
    /// Idem para as faixas.
    pub observada_faixas: [Grade<usize>; QTD_PRIORIS],
}

/// Fator de Bayes e grau de uma contagem; `None` se `n` = 0.
fn avaliar_contagem(
    c: Contagem,
    p: f64,
    prioris: &[Priori; QTD_PRIORIS],
) -> Result<Option<AvaliacaoFb>> {
    if c.total == 0 {
        return Ok(None);
    }
    let mut log10_fb = [0.0; QTD_PRIORIS];
    let mut classificacao = [classificar(0.0)?; QTD_PRIORIS];
    for (k, priori) in prioris.iter().enumerate() {
        log10_fb[k] = log10_fator_bayes(c.saidas, c.total, p, priori)?;
        classificacao[k] = classificar(log10_fb[k])?;
    }
    Ok(Some(AvaliacaoFb {
        log10_fb,
        classificacao,
    }))
}

fn contar(observada: &mut [[usize; 10]; QTD_PRIORIS], avaliacao: &Option<AvaliacaoFb>) {
    if let Some(a) = avaliacao {
        for (k, &c) in a.classificacao.iter().enumerate() {
            observada[k][celula(c)] += 1;
        }
    }
}

/// Grau e direção da 1d para cada posição e cada (componente, faixa), com as
/// contagens acumuladas (normalmente `SerieTaxas::acumuladas`, o histórico
/// inteiro sorteado). Ver "Evidência" no topo para a ressalva sobre as faixas.
///
/// `cfg` precisa ser o da loteria das contagens (mesmo *N*) e valer
/// `1 ≤ K < N`; senão, [`SpiderError::AvaliacaoEntradaInvalida`] ou
/// [`SpiderError::EvidenciaEntradaInvalida`].
pub fn evidencia_das_taxas(
    cfg: &LoteriaConfig,
    acumuladas: &TaxasAcumuladas,
) -> Result<EvidenciaTaxas> {
    let (p, prioris) = chance_e_prioris(cfg)?;
    let n = cfg.qtd_dezenas_possiveis();
    if acumuladas.contagem_posicao(n).is_none() || acumuladas.contagem_posicao(n + 1).is_some() {
        return Err(invalida(format!(
            "as contagens não são de uma loteria com {n} dezenas"
        )));
    }

    let mut obs_pos = [[0_usize; 10]; QTD_PRIORIS];
    let mut posicoes = Vec::with_capacity(n as usize);
    for posicao in 1..=n {
        // Dentro de 1..=N, conferido acima.
        let c = acumuladas
            .contagem_posicao(posicao)
            .ok_or_else(|| invalida(format!("posição {posicao} sem contagem")))?;
        let avaliacao = avaliar_contagem(c, p, &prioris)?;
        contar(&mut obs_pos, &avaliacao);
        posicoes.push(EvidenciaPosicao {
            posicao,
            saidas: c.saidas,
            total: c.total,
            avaliacao,
        });
    }

    let mut obs_fx = [[0_usize; 10]; QTD_PRIORIS];
    let mut faixas = Vec::with_capacity(QTD_CELULAS_FAIXA);
    for componente in 0..QTD_COMPONENTES {
        for faixa in Faixa::TODAS {
            let c = acumuladas
                .contagem_faixa(componente, faixa)
                .ok_or_else(|| invalida(format!("componente {componente} sem contagem")))?;
            let avaliacao = avaliar_contagem(c, p, &prioris)?;
            contar(&mut obs_fx, &avaliacao);
            faixas.push(EvidenciaFaixa {
                componente,
                faixa,
                saidas: c.saidas,
                total: c.total,
                avaliacao,
            });
        }
    }

    Ok(EvidenciaTaxas {
        p,
        prioris,
        concursos: acumuladas.concursos_incorporados(),
        posicoes,
        faixas,
        observada_posicoes: obs_pos.map(grade_de),
        observada_faixas: obs_fx.map(grade_de),
    })
}

// ---------------------------------------------------------------------------
// Esperado sob o acaso
// ---------------------------------------------------------------------------

/// Resultado de [`esperado_das_taxas`]: o esperado da contagem de graus de
/// [`evidencia_das_taxas`] se os sorteios fossem ao acaso, mantidas as
/// posições e faixas reais de cada dezena em cada concurso.
#[derive(Debug, Clone, PartialEq)]
pub struct EsperadoTaxas {
    /// Concursos sorteados da série, cada um sorteado de novo em cada réplica.
    pub concursos: usize,
    pub replicas: usize,
    pub semente: u64,
    /// Posições com n > 0 (as contadas; todas as *N*, se há algum concurso).
    pub celulas_posicoes: usize,
    /// (componente, faixa) com n > 0 (as contadas).
    pub celulas_faixas: usize,
    /// Na ordem de [`DESVIOS_PRIORI`]: média e erro padrão por (direção,
    /// grau) do número de posições.
    pub posicoes: Vec<EsperadoPriori>,
    /// Idem para as faixas.
    pub faixas: Vec<EsperadoPriori>,
}

/// Média e erro padrão da média (`desvio amostral / √réplicas`) por célula, a
/// partir das somas e somas de quadrados inteiras (mesma conta de
/// [`super::esperado_sob_o_acaso`]).
fn resumir_esperado(
    soma: &[[u64; 10]; QTD_PRIORIS],
    soma_quadrados: &[[u64; 10]; QTD_PRIORIS],
    replicas: usize,
) -> Vec<EsperadoPriori> {
    let r = replicas as f64;
    DESVIOS_PRIORI
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
                media: grade_de(media),
                erro_padrao: grade_de(erro),
            }
        })
        .collect()
}

/// Esperado sob o acaso da contagem de graus por posição e por (componente,
/// faixa), por simulação (ver "Esperado sob o acaso" no topo).
///
/// `linhas` são as de [`super::calcular_taxas`]: só os concursos **sorteados**
/// entram (o snapshot não sorteado é ignorado), e de cada dezena usa-se a
/// posição e as faixas do próprio concurso. Mesma entrada, mesma semente e
/// mesmas réplicas dão os mesmos bits, qualquer que seja a ordem das linhas.
/// Use [`super::REPLICAS_ESPERADO`] e [`super::SEMENTE_ESPERADO`] como padrão.
///
/// Recusa menos de 2 réplicas, entrada que [`ranquear`] também recusaria e
/// série sem nenhum concurso sorteado ([`SpiderError::AvaliacaoSemConcursos`]).
pub fn esperado_das_taxas(
    cfg: &LoteriaConfig,
    linhas: &[LinhaTaxas],
    replicas: usize,
    semente: u64,
) -> Result<EsperadoTaxas> {
    let (p, prioris) = chance_e_prioris(cfg)?;
    if replicas < 2 {
        return Err(SpiderError::EvidenciaEntradaInvalida(
            "a simulação precisa de ao menos 2 réplicas".to_string(),
        ));
    }
    let n = cfg.qtd_dezenas_possiveis() as usize;
    let k = cfg.qtd_dezenas;
    let grupos: Vec<Vec<&LinhaTaxas>> = agrupar(cfg, linhas)?
        .into_iter()
        .filter(|g| g[0].saiu.is_some())
        .collect();
    if grupos.is_empty() {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum concurso sorteado da {} para o esperado sob o acaso",
            cfg.nome_exibicao
        )));
    }
    let concursos = grupos.len();

    // Contadores: [0, N) = posições; depois 4 faixas por componente.
    let n_contadores = n + QTD_CELULAS_FAIXA;
    // Para cada (concurso, dezena em ordem crescente): o contador de cada uma
    // das 10 células a que ela pertence naquele concurso.
    let mut contadores_de: Vec<[u16; CELULAS_POR_DEZENA]> = Vec::with_capacity(concursos * n);
    // Quantos pares há em cada contador (o `n` do fator de Bayes de cada um).
    let mut totais = vec![0_u64; n_contadores];
    totais[..n].fill(concursos as u64);
    for grupo in &grupos {
        for l in grupo {
            let mut linha = [0_u16; CELULAS_POR_DEZENA];
            linha[0] = (l.posicao - 1) as u16;
            for c in 0..QTD_COMPONENTES {
                let contador = n + c * QTD_FAIXAS + l.faixas[c].indice();
                linha[1 + c] = contador as u16;
                totais[contador] += 1;
            }
            contadores_de.push(linha);
        }
    }
    let celulas_posicoes = n;
    let celulas_faixas = totais[n..].iter().filter(|&&t| t > 0).count();

    let mut gerador = SplitMix64::nova(semente);
    let mut baralho: Vec<usize> = (0..n).collect();
    let mut saidas = vec![0_u64; n_contadores];
    let mut soma_pos = [[0_u64; 10]; QTD_PRIORIS];
    let mut soma_q_pos = [[0_u64; 10]; QTD_PRIORIS];
    let mut soma_fx = [[0_u64; 10]; QTD_PRIORIS];
    let mut soma_q_fx = [[0_u64; 10]; QTD_PRIORIS];
    for _ in 0..replicas {
        saidas.fill(0);
        for c in 0..concursos {
            let base = c * n;
            // Fisher–Yates parcial: os K primeiros são um subconjunto uniforme
            // qualquer que seja a ordem anterior do baralho.
            for i in 0..k {
                let j = i + gerador.abaixo_de((n - i) as u64)? as usize;
                baralho.swap(i, j);
                for &contador in &contadores_de[base + baralho[i]] {
                    saidas[contador as usize] += 1;
                }
            }
        }
        for (idx, priori) in prioris.iter().enumerate() {
            let mut celulas_pos = [0_u64; 10];
            let mut celulas_fx = [0_u64; 10];
            for (contador, (&x, &total)) in saidas.iter().zip(&totais).enumerate() {
                if total == 0 {
                    continue;
                }
                let classe = classificar(log10_fator_bayes(x, total, p, priori)?)?;
                if contador < n {
                    celulas_pos[celula(classe)] += 1;
                } else {
                    celulas_fx[celula(classe)] += 1;
                }
            }
            for c in 0..10 {
                soma_pos[idx][c] += celulas_pos[c];
                soma_q_pos[idx][c] += celulas_pos[c] * celulas_pos[c];
                soma_fx[idx][c] += celulas_fx[c];
                soma_q_fx[idx][c] += celulas_fx[c] * celulas_fx[c];
            }
        }
    }

    Ok(EsperadoTaxas {
        concursos,
        replicas,
        semente,
        celulas_posicoes,
        celulas_faixas,
        posicoes: resumir_esperado(&soma_pos, &soma_q_pos, replicas),
        faixas: resumir_esperado(&soma_fx, &soma_q_fx, replicas),
    })
}

#[cfg(test)]
mod tests;
