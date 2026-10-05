//! Testes de aleatoriedade do histórico (1c.4): qui-quadrado de frequência e
//! runs test por dezena.
//!
//! Regras em `docs/PLANEJAMENTO.md`, 1c, "Testes de aleatoriedade do
//! histórico" (decisões de 2026-10-05):
//! - os testes rodam sobre o **histórico inteiro** de `resultados` (inclusive
//!   o conjunto trancado: aqui se testa o sorteio, não um perfil, e nada é
//!   escolhido a partir do resultado, então não há o que vazar);
//! - **significância**: α = 5% ([`ALFA`]) por família. O qui-quadrado de
//!   frequência é um teste global e usa α direto; o runs test é feito dezena
//!   a dezena e a família recebe a correção de **Holm** ([`holm`]); cada
//!   dezena mostra o p-valor original e o corrigido;
//! - rejeição depois da correção: conferir primeiro os dados (erros de
//!   planilha, `sync validar`), depois o sorteio.
//!
//! # Qui-quadrado de frequência
//!
//! Em *T* concursos, `O_i` = vezes que a dezena *i* saiu e `E = T·K/N`. Cada
//! concurso sorteia *K* das *N* dezenas **sem reposição**, então as contagens
//! não são multinomiais: com `p = K/N`, cada `O_i` tem variância
//! `T·p·(1−p)` e cada par tem covariância `−T·p·(1−p)/(N−1)` (num concurso,
//! saber que *i* saiu deixa *K−1* vagas para as outras *N−1*). Juntando,
//! `Cov(O) = T·p·(1−p)·N/(N−1)·(I − J/N)`, com `J` a matriz de uns: um
//! múltiplo da projeção de posto *N−1* que tira a média. Pelo TCL, então,
//!
//! ```text
//! X² = Σ(O_i − E)² / (T·p·(1−p)·N/(N−1)) = (N−1)/(N−K) · Σ(O_i − E)²/E
//! ```
//!
//! segue χ² com *N−1* graus de liberdade. A soma de Pearson pura,
//! `Σ(O_i − E)²/E`, tem média exata `N−K` (não `N−1`): usá-la contra
//! χ²(N−1) daria p-valores grandes demais (na Lotofácil, média 10 contra
//! 24 graus de liberdade — o teste quase nunca rejeitaria). As duas são
//! devolvidas; a decisão usa a corrigida, cuja média exata sob o acaso é
//! `N−1` para qualquer *T*.
//!
//! As somas são feitas em inteiros: `Σ(O_i − E)²/E = Σ(N·O_i − T·K)² /
//! (T·K·N)`, com numerador e denominador exatos em `u128`. Cada um vira
//! `f64` e a divisão arredonda: acima de 2^53 são até três arredondamentos;
//! nos tamanhos reais (milhares de concursos) os dois ficam abaixo de 2^53,
//! convertem sem perda e só a divisão arredonda (resultado correto até o
//! último bit).
//!
//! # Runs test (Wald–Wolfowitz), por dezena
//!
//! Sequência saiu (1) / não saiu (0) da dezena em todos os concursos, em
//! ordem. `R` = número de sequências (blocos de valores iguais), `n1` =
//! saídas, `n0` = não saídas, `n = n1 + n0`. Condicionado a `n1` (sob o
//! acaso, toda ordem dos `n1` uns é igualmente provável):
//!
//! ```text
//! μ = 2·n1·n0/n + 1      σ² = 2·n1·n0·(2·n1·n0 − n) / (n²·(n − 1))
//! z = (R − μ)/σ          p = 2·Φ(−|z|)   (bilateral, sem correção de continuidade)
//! ```
//!
//! Poucas sequências (blocos longos de saídas ou de ausências) dão `z < 0`;
//! alternância demais, `z > 0`. Com `n1` = 0, `n0` = 0 ou `σ² = 0` (só
//! acontece com `n1 = n0 = 1`), a sequência não tem variação a testar: a
//! dezena é **não testável**, fica com p-valor `None` e **fora** da família
//! do Holm (contá-la só inflaria o *m* sem poder rejeitar nada). Nas
//! loterias reais, com milhares de concursos, toda dezena é testável.
//!
//! **Limitação medida da normal**: com `n1` pequeno diante de `n` (Quina,
//! 6,25%; Mega-Sena, 10%), a distribuição de `R` é assimétrica e a normal
//! **subestima a cauda esquerda** (poucas sequências). Em 300 históricos
//! simulados do tamanho real, a fração de p-valores ≤ α/N ficou em ~1,5× o
//! nominal na Quina (quase toda com `z < 0`) e a chance de alguma rejeição
//! falsa depois do Holm em ~6–7% (nominal: ≤ 5%); com 500 concursos na
//! Mega-Sena, ~10%. Na Lotofácil (*p* = 0,6) o efeito é pequeno. Nos
//! valores do meio da distribuição (p-valor ~5%) a normal é boa. Um p-valor
//! exato pela distribuição condicional de `R` resolveria; fica como
//! proposta para decisão (não está no plano). Em séries pequenas (os
//! exemplos dos testes) o p-valor é só indicativo.
//!
//! # Holm
//!
//! p-valores ajustados *step-down*: ordenados crescentes
//! `p_(1) ≤ … ≤ p_(m)`, `p̃_(i) = max_{j ≤ i} min(1, (m − j + 1)·p_(j))`;
//! rejeita quando `p̃ ≤ α`. Controla a chance de alguma rejeição falsa na
//! família em `α` sob **qualquer** dependência entre os testes (as dezenas
//! de um concurso não são independentes).
//!
//! # Funções especiais
//!
//! Os p-valores vêm da gama incompleta regularizada superior `Q(a, x)`:
//! `P(χ²_gl ≥ x) = Q(gl/2, x/2)` e `2·Φ(−|z|) = erfc(|z|/√2) = Q(1/2, z²/2)`.
//! `Q` é calculada como em *Numerical Recipes* (série de `P` para
//! `x < a + 1`, fração contínua de Legendre por Lentz modificado no resto),
//! sem subtrair de 1 na cauda, então caudas pequenas (z = 8, χ² muito acima
//! dos graus de liberdade) mantêm precisão relativa. Como `a` é sempre
//! inteiro ou semi-inteiro (`gl/2`), `ln Γ(a)` sai exato por soma de
//! logaritmos (`Γ(a) = (a−1)!` ou `Γ(m + 1/2) = √π·Π(j + 1/2)`), sem
//! aproximação de Lanczos.
//!
//! Determinismo: todas as somas percorrem as dezenas e os concursos em ordem
//! crescente; mesma entrada (em qualquer ordem), mesmos bits.

use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::validacao;
use serde::{Deserialize, Serialize};

/// Nível de significância de cada família de testes (decisão de
/// 2026-10-05): o qui-quadrado global usa direto; o runs test por dezena,
/// depois da correção de Holm.
pub const ALFA: f64 = 0.05;

/// Qui-quadrado de frequência de todas as dezenas (teste global).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuiQuadradoFrequencia {
    /// *T*: concursos contados.
    pub concursos: u64,
    /// `E = T·K/N`: saídas esperadas de cada dezena.
    pub esperado: f64,
    /// `Σ(O_i − E)²/E`, sem a correção do sorteio sem reposição (média `N−K`
    /// sob o acaso). Só para referência: **não** segue χ²(N−1).
    pub pearson: f64,
    /// `(N−1)/(N−K)·pearson`: a estatística do teste, ~χ²(N−1).
    pub estatistica: f64,
    /// *N−1*.
    pub graus_liberdade: u32,
    /// `P(χ²(N−1) ≥ estatistica)`.
    pub p_valor: f64,
    /// `p_valor ≤ ALFA` (sem correção: é um teste só).
    pub rejeita: bool,
}

/// Runs test de uma sequência saiu/não saiu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TesteRuns {
    /// `n1`: quantas vezes saiu (é também o `O_i` do qui-quadrado).
    pub saidas: u64,
    /// `n0`: quantas vezes não saiu.
    pub nao_saidas: u64,
    /// `R`: número de sequências de valores iguais.
    pub sequencias: u64,
    /// `μ = 2·n1·n0/n + 1` (1 quando `n1` ou `n0` é zero).
    pub media: f64,
    /// `σ`; zero quando a dezena não é testável.
    pub desvio_padrao: f64,
    /// `(R − μ)/σ`; `None` se não testável (`n1` = 0, `n0` = 0 ou `σ` = 0).
    pub z: Option<f64>,
    /// p-valor bilateral pela normal; `None` se não testável.
    pub p_valor: Option<f64>,
}

impl TesteRuns {
    /// Se a sequência tem variação a testar (entra na família do Holm).
    pub fn testavel(&self) -> bool {
        self.p_valor.is_some()
    }
}

/// Runs test de uma dezena, com o p-valor corrigido pela família.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunsDezena {
    pub dezena: u32,
    pub teste: TesteRuns,
    /// p-valor ajustado por Holm entre as dezenas testáveis; `None` se a
    /// dezena não é testável.
    pub p_valor_holm: Option<f64>,
    /// `p_valor_holm ≤ ALFA`; `false` se não testável.
    pub rejeita: bool,
}

/// Resultado dos testes de aleatoriedade de uma loteria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumoAleatoriedade {
    pub primeiro_concurso: u32,
    pub ultimo_concurso: u32,
    /// *T*: concursos testados (`ultimo − primeiro + 1`; a série não tem
    /// buraco).
    pub concursos: u64,
    /// [`ALFA`], registrado junto para o relatório.
    pub alfa: f64,
    pub qui_quadrado: QuiQuadradoFrequencia,
    /// Uma entrada por dezena, em ordem crescente de dezena.
    pub runs: Vec<RunsDezena>,
    /// *m* da família do Holm: dezenas testáveis.
    pub dezenas_testadas_runs: usize,
    /// Dezenas com p-valor original ≤ α (sem correção; só para comparar).
    pub rejeicoes_runs_sem_correcao: usize,
    /// Dezenas rejeitadas depois do Holm (a decisão da família).
    pub rejeicoes_runs: usize,
}

/// Qui-quadrado de frequência e runs test por dezena sobre o histórico
/// `resultados` (qualquer ordem; ordenado aqui por concurso).
///
/// Erros: série vazia é [`SpiderError::AvaliacaoSemConcursos`]; concurso
/// repetido, buraco na numeração e concurso reprovado por
/// [`validacao::validar_resultado`] (outra loteria, concurso 0, quantidade de
/// dezenas diferente de *K*, dezena fora da faixa ou repetida, data inválida)
/// são [`SpiderError::AleatoriedadeEntradaInvalida`]; configuração sem nada a
/// testar (*K* = 0 ou *K* ≥ *N*) é [`SpiderError::AvaliacaoEntradaInvalida`].
/// A série não precisa começar no concurso 1: o trecho testado sai em
/// `primeiro_concurso`/`ultimo_concurso`.
pub fn testar_aleatoriedade(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
) -> Result<ResumoAleatoriedade> {
    validar_config(cfg)?;
    let (primeiro_concurso, ultimo_concurso, sequencias) = sequencias_por_dezena(cfg, resultados)?;
    let concursos = resultados.len() as u64;

    let testes = sequencias
        .iter()
        .map(|s| teste_de_runs(s))
        .collect::<Result<Vec<_>>>()?;
    let frequencias: Vec<u64> = testes.iter().map(|t| t.saidas).collect();
    let qui_quadrado = qui_quadrado_frequencia(cfg, &frequencias, concursos)?;

    // Família do Holm: só as dezenas testáveis, na ordem crescente de dezena
    // (o desempate do `holm` é pela posição, logo pela menor dezena).
    let p_testaveis: Vec<f64> = testes.iter().filter_map(|t| t.p_valor).collect();
    let mut ajustados = holm(&p_testaveis)?.into_iter();
    let runs: Vec<RunsDezena> = cfg
        .dezenas()
        .zip(testes)
        .map(|(dezena, teste)| {
            let p_valor_holm = if teste.testavel() {
                ajustados.next()
            } else {
                None
            };
            RunsDezena {
                dezena,
                rejeita: p_valor_holm.is_some_and(|p| p <= ALFA),
                p_valor_holm,
                teste,
            }
        })
        .collect();

    Ok(ResumoAleatoriedade {
        primeiro_concurso,
        ultimo_concurso,
        concursos,
        alfa: ALFA,
        qui_quadrado,
        dezenas_testadas_runs: p_testaveis.len(),
        rejeicoes_runs_sem_correcao: p_testaveis.iter().filter(|&&p| p <= ALFA).count(),
        rejeicoes_runs: runs.iter().filter(|r| r.rejeita).count(),
        runs,
    })
}

/// Qui-quadrado de frequência com a correção do sorteio sem reposição
/// (derivação na documentação do módulo). `frequencias[i]` = saídas da
/// dezena `dezena_min + i` em `concursos` concursos.
///
/// Exige `frequencias.len() = N`, cada uma ≤ `concursos` e soma
/// `concursos·K` (cada concurso sorteia *K*); `concursos` = 0 é
/// [`SpiderError::AvaliacaoSemConcursos`].
pub fn qui_quadrado_frequencia(
    cfg: &LoteriaConfig,
    frequencias: &[u64],
    concursos: u64,
) -> Result<QuiQuadradoFrequencia> {
    validar_config(cfg)?;
    if concursos == 0 {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "qui-quadrado de frequência da {} com 0 concursos",
            cfg.nome_exibicao
        )));
    }
    let n = u64::from(cfg.qtd_dezenas_possiveis());
    let k = cfg.qtd_dezenas as u64;
    if frequencias.len() as u64 != n {
        return Err(invalida(format!(
            "{} frequência(s) no qui-quadrado da {}, esperadas {n} (uma por dezena)",
            frequencias.len(),
            cfg.nome_exibicao
        )));
    }
    if let Some(o) = frequencias.iter().find(|&&o| o > concursos) {
        return Err(invalida(format!(
            "frequência {o} maior que o número de concursos ({concursos}) no qui-quadrado da {}",
            cfg.nome_exibicao
        )));
    }
    let estouro = || {
        invalida(format!(
            "qui-quadrado da {} com {concursos} concursos: conta não cabe em 128 bits",
            cfg.nome_exibicao
        ))
    };
    let soma: u128 = frequencias.iter().map(|&o| u128::from(o)).sum();
    let t_k = u128::from(concursos)
        .checked_mul(u128::from(k))
        .ok_or_else(estouro)?;
    if soma != t_k {
        return Err(invalida(format!(
            "as frequências somam {soma}, mas {concursos} concurso(s) de {k} dezenas somam {t_k} no qui-quadrado da {}",
            cfg.nome_exibicao
        )));
    }

    // Σ(O − E)²/E = Σ(N·O − T·K)² / (T·K·N), tudo em inteiros, na ordem das
    // dezenas.
    let mut soma_quadrados: u128 = 0;
    for &o in frequencias {
        let desvio = (u128::from(n) * u128::from(o)).abs_diff(t_k);
        let quadrado = desvio.checked_mul(desvio).ok_or_else(estouro)?;
        soma_quadrados = soma_quadrados.checked_add(quadrado).ok_or_else(estouro)?;
    }
    let denominador = t_k.checked_mul(u128::from(n)).ok_or_else(estouro)?;
    // Duas conversões u128 → f64 e uma divisão: exatas abaixo de 2^53 (o caso
    // das loterias reais), então só a divisão arredonda; acima disso, até três
    // arredondamentos.
    let pearson = soma_quadrados as f64 / denominador as f64;
    let estatistica = soma_quadrados
        .checked_mul(u128::from(n - 1))
        .ok_or_else(estouro)? as f64
        / denominador
            .checked_mul(u128::from(n - k))
            .ok_or_else(estouro)? as f64;

    // `validar_config` garante N ≥ 2, então gl ≥ 1.
    let graus_liberdade = cfg.qtd_dezenas_possiveis() - 1;
    let p_valor = p_valor_qui_quadrado(estatistica, graus_liberdade)?;
    Ok(QuiQuadradoFrequencia {
        concursos,
        esperado: t_k as f64 / n as f64,
        pearson,
        estatistica,
        graus_liberdade,
        p_valor,
        rejeita: p_valor <= ALFA,
    })
}

/// Runs test (Wald–Wolfowitz) de uma sequência saiu/não saiu, na ordem dada.
/// Fórmulas e o caso não testável na documentação do módulo. Sequência vazia
/// é [`SpiderError::AvaliacaoSemConcursos`].
pub fn teste_de_runs(sequencia: &[bool]) -> Result<TesteRuns> {
    let Some((&primeiro, resto)) = sequencia.split_first() else {
        return Err(SpiderError::AvaliacaoSemConcursos(
            "runs test com sequência vazia".to_string(),
        ));
    };
    let n = sequencia.len() as u64;
    let saidas = sequencia.iter().filter(|&&s| s).count() as u64;
    let nao_saidas = n - saidas;
    let mut sequencias: u64 = 1;
    let mut anterior = primeiro;
    for &s in resto {
        if s != anterior {
            sequencias += 1;
            anterior = s;
        }
    }

    // Em inteiros (cabem: n ≤ 2^32 concursos dá 2·n1·n0 ≤ 2^63):
    // 2·n1·n0, A = 2·n1·n0·(2·n1·n0 − n) (≥ 0, zero só com n1·n0 = 0 ou
    // n1 = n0 = 1) e o numerador de R − μ = (n·(R−1) − 2·n1·n0)/n.
    let dobro_produto = 2 * u128::from(saidas) * u128::from(nao_saidas);
    let n128 = u128::from(n);
    let a = dobro_produto * dobro_produto.saturating_sub(n128);
    let media = dobro_produto as f64 / n as f64 + 1.0;
    let (desvio_padrao, z, p_valor) = if a == 0 {
        (0.0, None, None)
    } else {
        // σ = √A / (n·√(n−1)) e z = (n·(R−1) − 2·n1·n0)·√((n−1)/A). A > 0
        // implica n ≥ 3.
        let numerador = (n128 * u128::from(sequencias - 1)) as f64 - dobro_produto as f64;
        let z = numerador * ((n - 1) as f64 / a as f64).sqrt();
        let desvio = (a as f64).sqrt() / (n as f64 * ((n - 1) as f64).sqrt());
        (desvio, Some(z), Some(p_valor_normal_bilateral(z)?))
    };
    Ok(TesteRuns {
        saidas,
        nao_saidas,
        sequencias,
        media,
        desvio_padrao,
        z,
        p_valor,
    })
}

/// p-valores ajustados por Holm (*step-down*), na mesma ordem da entrada:
/// `p̃_(i) = max_{j ≤ i} min(1, (m − j + 1)·p_(j))` com os p-valores em ordem
/// crescente e empate desfeito pela posição na entrada (determinístico; com
/// p-valores iguais o ajustado também é igual, seja qual for o desempate).
/// Rejeita-se a hipótese *i* quando `p̃_i ≤ α`.
///
/// Os ajustados são ≥ os originais, ≤ 1 e respeitam a ordem dos originais.
/// Entrada vazia devolve vazio; p-valor fora de `[0, 1]` (ou NaN) é
/// [`SpiderError::AvaliacaoEntradaInvalida`].
pub fn holm(p_valores: &[f64]) -> Result<Vec<f64>> {
    if let Some((i, p)) = p_valores
        .iter()
        .enumerate()
        .find(|(_, p)| !(0.0..=1.0).contains(*p))
    {
        return Err(invalida(format!(
            "p-valor {p} na posição {i} da correção de Holm (deve estar em [0, 1])"
        )));
    }
    let m = p_valores.len();
    let mut ordem: Vec<usize> = (0..m).collect();
    // Ordenação estável: empates ficam na ordem da entrada.
    ordem.sort_by(|&i, &j| p_valores[i].total_cmp(&p_valores[j]));
    let mut ajustados = vec![0.0; m];
    let mut maximo: f64 = 0.0;
    for (posicao, &i) in ordem.iter().enumerate() {
        let fator = (m - posicao) as f64;
        maximo = maximo.max((fator * p_valores[i]).min(1.0));
        ajustados[i] = maximo;
    }
    Ok(ajustados)
}

/// `P(χ² ≥ estatistica)` com `graus_liberdade` graus de liberdade:
/// `Q(gl/2, estatistica/2)`. `+∞` dá 0. Estatística negativa ou NaN e
/// `graus_liberdade` = 0 são [`SpiderError::AvaliacaoEntradaInvalida`].
pub fn p_valor_qui_quadrado(estatistica: f64, graus_liberdade: u32) -> Result<f64> {
    if graus_liberdade == 0 {
        return Err(invalida(
            "qui-quadrado com 0 graus de liberdade".to_string(),
        ));
    }
    if estatistica.is_nan() || estatistica < 0.0 {
        return Err(invalida(format!(
            "estatística qui-quadrado {estatistica} (deve ser ≥ 0)"
        )));
    }
    gama_q(graus_liberdade, estatistica / 2.0)
}

/// `2·Φ(−|z|)`, o p-valor bilateral da normal padrão: `Q(1/2, z²/2)`.
/// `±∞` dá 0; NaN é [`SpiderError::AvaliacaoEntradaInvalida`].
pub fn p_valor_normal_bilateral(z: f64) -> Result<f64> {
    if z.is_nan() {
        return Err(invalida("z = NaN no p-valor da normal".to_string()));
    }
    gama_q(1, z * z / 2.0)
}

/// Tolerância relativa das iterações da gama incompleta: o próprio `eps` do
/// `f64`.
const EPS: f64 = f64::EPSILON;
/// Perto do menor `f64` normal, para o Lentz não dividir por zero.
const QUASE_ZERO: f64 = f64::MIN_POSITIVE / f64::EPSILON;
/// Teto de iterações. A série converge em ~√a·poucos termos e a fração
/// contínua em dezenas com `x ≥ a + 1`; com os graus de liberdade das
/// loterias (*N*−1 < 100) não passa de ~100.
const MAX_ITERACOES: u32 = 100_000;

/// Gama incompleta regularizada superior `Q(a, x) = Γ(a, x)/Γ(a)` com
/// `a = dobro_a/2` (inteiro ou semi-inteiro, `dobro_a` ≥ 1) e `x` ≥ 0
/// (`+∞` dá 0). Algoritmo na documentação do módulo.
fn gama_q(dobro_a: u32, x: f64) -> Result<f64> {
    debug_assert!(dobro_a >= 1 && x >= 0.0);
    if x == 0.0 {
        return Ok(1.0);
    }
    if x.is_infinite() {
        return Ok(0.0);
    }
    let a = f64::from(dobro_a) / 2.0;
    // e^{−x}·x^a/Γ(a), em log para não estourar no meio da conta.
    let prefator = (-x + a * x.ln() - ln_gama_meio(dobro_a)).exp();
    let nao_convergiu = || {
        invalida(format!(
            "gama incompleta Q({a}, {x}) não convergiu em {MAX_ITERACOES} iterações"
        ))
    };

    if x < a + 1.0 {
        // Série: P(a, x) = prefator · Σ_{n≥0} x^n / (a·(a+1)·…·(a+n)).
        // Aqui P < 0,92 (o pior caso é a = 1/2 perto de x = 3/2, onde
        // P = erf(√1,5)), então 1 − P perde no máximo um dígito; as caudas
        // pequenas caem sempre no ramo da fração contínua.
        let mut termo = 1.0 / a;
        let mut soma = termo;
        let mut ap = a;
        for _ in 0..MAX_ITERACOES {
            ap += 1.0;
            termo *= x / ap;
            soma += termo;
            if termo.abs() < soma.abs() * EPS {
                return Ok(1.0 - prefator * soma);
            }
        }
        Err(nao_convergiu())
    } else {
        // Fração contínua de Legendre para Q(a, x), por Lentz modificado:
        // Q = prefator · 1/(x+1−a− 1·(1−a)/(x+3−a− 2·(2−a)/(x+5−a− …))).
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / QUASE_ZERO;
        let mut d = 1.0 / b;
        let mut h = d;
        for i in 1..=MAX_ITERACOES {
            let i = f64::from(i);
            let an = -i * (i - a);
            b += 2.0;
            d = an * d + b;
            if d.abs() < QUASE_ZERO {
                d = QUASE_ZERO;
            }
            c = b + an / c;
            if c.abs() < QUASE_ZERO {
                c = QUASE_ZERO;
            }
            d = 1.0 / d;
            let delta = d * c;
            h *= delta;
            if (delta - 1.0).abs() < EPS {
                return Ok(prefator * h);
            }
        }
        Err(nao_convergiu())
    }
}

/// `ln Γ(a)` exato (a menos do arredondamento da soma) para `a = dobro_a/2`:
/// `Γ(a) = (a−1)!` com `a` inteiro e `Γ(m + 1/2) = √π·Π_{j<m}(j + 1/2)` com
/// `a` semi-inteiro. Soma em ordem crescente (determinística).
fn ln_gama_meio(dobro_a: u32) -> f64 {
    let m = dobro_a / 2;
    if dobro_a.is_multiple_of(2) {
        (1..m).map(|j| f64::from(j).ln()).sum()
    } else {
        // ln √π
        let ln_raiz_pi = 0.5 * std::f64::consts::PI.ln();
        (0..m).fold(ln_raiz_pi, |soma, j| soma + (f64::from(j) + 0.5).ln())
    }
}

/// *K* e *N* com algo a testar: `1 ≤ K < N` (com *K* = *N* todas as dezenas
/// saem sempre e o qui-quadrado dividiria por zero).
fn validar_config(cfg: &LoteriaConfig) -> Result<()> {
    let n = u64::from(cfg.qtd_dezenas_possiveis());
    let k = cfg.qtd_dezenas as u64;
    if k == 0 || k >= n {
        return Err(invalida(format!(
            "a {} sorteia {k} de {n} dezenas: os testes de aleatoriedade exigem 1 ≤ K < N",
            cfg.nome_exibicao
        )));
    }
    Ok(())
}

/// Ordena por concurso, valida e devolve `(primeiro, último, sequências)`,
/// com `sequencias[i][t]` = a dezena `dezena_min + i` saiu no `t`-ésimo
/// concurso da série.
fn sequencias_por_dezena(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
) -> Result<(u32, u32, Vec<Vec<bool>>)> {
    let mut ordenados: Vec<&Resultado> = resultados.iter().collect();
    ordenados.sort_by_key(|r| r.concurso);
    let (Some(primeiro), Some(ultimo)) = (ordenados.first(), ordenados.last()) else {
        return Err(SpiderError::AvaliacaoSemConcursos(format!(
            "nenhum resultado da {} para os testes de aleatoriedade",
            cfg.nome_exibicao
        )));
    };
    let (primeiro, ultimo) = (primeiro.concurso, ultimo.concurso);

    let n = cfg.qtd_dezenas_possiveis() as usize;
    let mut sequencias = vec![Vec::with_capacity(ordenados.len()); n];
    let mut anterior: Option<u32> = None;
    for r in ordenados {
        let concurso = r.concurso;
        // Regras de um concurso isolado: as mesmas do `sync validar` (e do
        // motor), todas como erro.
        let problemas = validacao::validar_resultado(cfg, r);
        if !problemas.is_empty() {
            let detalhe = problemas
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(resultados_invalidos(detalhe));
        }
        // A sequência não usa `validacao::validar_sequencia`: ela exige que o
        // histórico comece no concurso 1 (aqui a série pode começar depois) e
        // trata data fora de ordem, que não afeta estes testes (a ordem é a do
        // número do concurso). Só repetido e buraco importam.
        if let Some(ant) = anterior {
            if concurso == ant {
                return Err(resultados_invalidos(format!(
                    "concurso {concurso} repetido nos resultados da {}",
                    cfg.nome_exibicao
                )));
            }
            if concurso != ant + 1 {
                return Err(resultados_invalidos(format!(
                    "faltam os concursos {}..={} nos resultados da {}",
                    ant + 1,
                    concurso - 1,
                    cfg.nome_exibicao
                )));
            }
        }
        anterior = Some(concurso);
        // `validar_resultado` passou: K dezenas distintas, todas na faixa.
        let mut saiu = vec![false; n];
        for &d in &r.dezenas {
            saiu[(d - cfg.dezena_min) as usize] = true;
        }
        for (sequencia, s) in sequencias.iter_mut().zip(saiu) {
            sequencia.push(s);
        }
    }
    Ok((primeiro, ultimo, sequencias))
}

/// Problema no histórico `resultados` (não nos argumentos das funções
/// numéricas nem na configuração): orienta para `sync validar`/`buracos`.
fn resultados_invalidos(detalhe: String) -> SpiderError {
    SpiderError::AleatoriedadeEntradaInvalida(detalhe)
}

/// Argumento inválido das funções numéricas públicas ou configuração sem
/// nada a testar: mesma variante dos outros blocos de `avaliacao` (o
/// `validar_k` do top-k recusa *k* fora de `1..N` assim).
fn invalida(detalhe: String) -> SpiderError {
    SpiderError::AvaliacaoEntradaInvalida(detalhe)
}

#[cfg(test)]
mod tests;
