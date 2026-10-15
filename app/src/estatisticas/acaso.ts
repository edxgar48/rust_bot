// Comparação com o esperado ao acaso (tarefa 7.3): funções puras, sem Svelte, sem rede e
// sem relação com o motor. Respondem "quanto de desvio o acaso sozinho explica?" e descrevem
// o PASSADO; nada aqui diz algo sobre o próximo sorteio.
//
// Dois modelos, ambos exatos (sem aproximação normal):
//  - Frequência de UMA dezena em N concursos: binomial(N, p = K/total), porque cada concurso
//    sorteia K das `total` dezenas e a dezena entra com probabilidade K/total, independente
//    dos outros concursos. Faixa central por quantis acumulados.
//  - Pares por sorteio e repetidas do anterior: hipergeométrica, a mesma do back-end
//    (`avaliacao::Hipergeometrica`, src/avaliacao/topk.rs), em inteiros (BigInt, como o
//    `u128` do Rust) e só no fim convertida para número:
//    P(i) = C(S, i) · C(total − S, E − i) / C(total, E).
//      pares:     S = total/2 dezenas pares, E = K dezenas sorteadas;
//      repetidas: S = K marcadas (o sorteio anterior), E = K dezenas sorteadas
//                 (igual a `Hipergeometrica::da_loteria(cfg, K)`).
//
// Conferência: `testes/gerar-esperado-acaso.py` calcula os mesmos números de forma
// independente (e roda o back-end Rust); `acaso.test.ts` compara com tolerância de 1e-9.
import type { Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA } from '../dados/tipos';

/** Probabilidade deixada fora em CADA ponta da faixa central (2,5% + 2,5% = 5%). */
export const CAUDA_PADRAO = 0.025;
/** Cobertura nominal da faixa central (a cobertura real é sempre >= esta). */
export const COBERTURA_NOMINAL = 0.95;

// ------------------------------------------------------------------ logaritmo do fatorial

const LIMITE_TABELA = 30;
const LN_FATORIAL_PEQUENO: readonly number[] = (() => {
  const t = [0];
  for (let i = 1; i < LIMITE_TABELA; i++) t.push(t[i - 1]! + Math.log(i));
  return t;
})();

/**
 * ln(n!). Tabela exata até 29 e série de Stirling acima (erro < 1e-18 para n >= 30), de modo
 * que não há soma acumulada de milhares de logaritmos.
 */
export function logFatorial(n: number): number {
  if (!Number.isInteger(n) || n < 0) throw new RangeError(`logFatorial: n inválido (${n})`);
  if (n < LIMITE_TABELA) return LN_FATORIAL_PEQUENO[n]!;
  const n2 = n * n;
  return (
    n * Math.log(n) -
    n +
    0.5 * Math.log(2 * Math.PI * n) +
    1 / (12 * n) -
    1 / (360 * n * n2) +
    1 / (1260 * n * n2 * n2) -
    1 / (1680 * n * n2 * n2 * n2) +
    1 / (1188 * n * n2 * n2 * n2 * n2)
  );
}

// ------------------------------------------------------------------ binomial

/** pmf da binomial(n, p): `pmf[x]` = P(X = x), x de 0 a n. Soma 1 (erro ~1e-12 até n = 8000). */
export function pmfBinomial(n: number, p: number): number[] {
  if (!Number.isInteger(n) || n < 0) throw new RangeError(`pmfBinomial: n inválido (${n})`);
  if (!(p >= 0 && p <= 1)) throw new RangeError(`pmfBinomial: p inválido (${p})`);
  const pmf = new Array<number>(n + 1).fill(0);
  if (p === 0) {
    pmf[0] = 1;
    return pmf;
  }
  if (p === 1) {
    pmf[n] = 1;
    return pmf;
  }
  const lnP = Math.log(p);
  const lnQ = Math.log1p(-p);
  const lnN = logFatorial(n);
  for (let x = 0; x <= n; x++) {
    pmf[x] = Math.exp(lnN - logFatorial(x) - logFatorial(n - x) + x * lnP + (n - x) * lnQ);
  }
  return pmf;
}

export interface FaixaCentral {
  /** Menor valor x com P(X <= x) >= cauda. */
  readonly inferior: number;
  /** Menor valor x com P(X <= x) >= 1 − cauda. */
  readonly superior: number;
  /** P(inferior <= X <= superior): sempre >= 1 − 2·cauda (a distribuição é discreta). */
  readonly coberturaReal: number;
  /** 1 − coberturaReal: chance de UMA dezena cair fora da faixa só por acaso. */
  readonly foraPorDezena: number;
}

/**
 * Faixa central de uma pmf indexada por valor (0, 1, 2, ...), por quantis acumulados.
 *
 * Atenção: a decisão "acumulada >= cauda" é feita em ponto flutuante. Ela só está certa se
 * nenhum quantil cai a menos de ~1e-12 de um empate com 2,5% / 97,5%. Isso foi conferido nas
 * fixtures (binomial exata em inteiros e soma exata em BigInt, a 1e-9, em
 * `testes/gerar-esperado-acaso.py`), mas um N futuro (outro tamanho de janela, outra
 * loteria) pode cair perto de um empate e mudar um dos limites em 1; rode o gerador de novo.
 */
export function faixaCentral(pmf: readonly number[], cauda = CAUDA_PADRAO): FaixaCentral {
  if (pmf.length === 0) throw new RangeError('faixaCentral: pmf vazia');
  let acumulada = 0;
  let inferior = -1;
  let superior = pmf.length - 1;
  for (let x = 0; x < pmf.length; x++) {
    acumulada += pmf[x]!;
    if (inferior < 0 && acumulada >= cauda) inferior = x;
    if (acumulada >= 1 - cauda) {
      superior = x;
      break;
    }
  }
  if (inferior < 0) inferior = 0;
  let dentro = 0;
  for (let x = inferior; x <= superior; x++) dentro += pmf[x]!;
  const coberturaReal = Math.min(1, dentro);
  return { inferior, superior, coberturaReal, foraPorDezena: Math.max(0, 1 - coberturaReal) };
}

/** Média e desvio-padrão de uma pmf indexada por valor. */
export function mediaEDesvio(pmf: readonly number[]): { media: number; desvio: number } {
  let media = 0;
  for (let x = 0; x < pmf.length; x++) media += x * pmf[x]!;
  let variancia = 0;
  for (let x = 0; x < pmf.length; x++) variancia += (x - media) ** 2 * pmf[x]!;
  return { media, desvio: Math.sqrt(variancia) };
}

export interface FaixaFrequencia extends FaixaCentral {
  /** Concursos usados (o tamanho EFETIVO da janela, não o pedido). */
  readonly tamanho: number;
  /** p = K / total: chance de a dezena sair em um concurso. */
  readonly p: number;
  /** N · p. */
  readonly esperado: number;
  /** Desvio-padrão da contagem: √(N · p · (1 − p)). */
  readonly desvio: number;
}

const memoFaixa = new Map<string, FaixaFrequencia>();

/**
 * Faixa em que o acaso põe a frequência de uma dezena em `tamanho` concursos.
 * `null` com janela vazia. O mesmo número serve para todas as dezenas (p é igual).
 */
export function faixaDaFrequencia(slug: Slug, tamanho: number): FaixaFrequencia | null {
  if (tamanho <= 0) return null;
  const chave = `${slug}:${tamanho}`;
  const guardada = memoFaixa.get(chave);
  if (guardada) return guardada;
  const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[slug];
  const p = dezenasSorteadas / totalDezenas;
  const faixa = faixaCentral(pmfBinomial(tamanho, p));
  const resultado: FaixaFrequencia = {
    ...faixa,
    tamanho,
    p,
    esperado: tamanho * p,
    desvio: Math.sqrt(tamanho * p * (1 - p)),
  };
  if (memoFaixa.size > 64) memoFaixa.clear();
  memoFaixa.set(chave, resultado);
  return resultado;
}

export type PosicaoNaFaixa = 'dentro' | 'abaixo' | 'acima';

export function posicaoNaFaixa(
  vezes: number,
  faixa: Pick<FaixaCentral, 'inferior' | 'superior'>,
): PosicaoNaFaixa {
  if (vezes < faixa.inferior) return 'abaixo';
  if (vezes > faixa.superior) return 'acima';
  return 'dentro';
}

export interface DezenasForaDaFaixa {
  /** `posicao[d - 1]` = situação da dezena d. */
  readonly posicao: readonly PosicaoNaFaixa[];
  readonly abaixo: readonly number[];
  readonly acima: readonly number[];
  /** abaixo + acima. */
  readonly fora: number;
  /**
   * Quantas dezenas se esperaria fora da faixa só por acaso: total · foraPorDezena. Vale
   * exatamente (linearidade da esperança), mesmo com as dezenas não sendo independentes.
   */
  readonly foraEsperado: number;
}

export function dezenasForaDaFaixa(
  frequencia: readonly number[],
  faixa: FaixaCentral,
): DezenasForaDaFaixa {
  const abaixo: number[] = [];
  const acima: number[] = [];
  const posicao = frequencia.map((vezes, i) => {
    const pos = posicaoNaFaixa(vezes, faixa);
    if (pos === 'abaixo') abaixo.push(i + 1);
    else if (pos === 'acima') acima.push(i + 1);
    return pos;
  });
  return {
    posicao,
    abaixo,
    acima,
    fora: abaixo.length + acima.length,
    foraEsperado: frequencia.length * faixa.foraPorDezena,
  };
}

// ------------------------------------------------------------------ hipergeométrica

/** C(n, r) exato; 0 com r > n (igual a `binomial` de topk.rs). */
export function combinacao(n: number, r: number): bigint {
  if (r < 0 || r > n) return 0n;
  const m = Math.min(r, n - r);
  let c = 1n;
  for (let i = 1; i <= m; i++) {
    // c = C(n, i−1); C(n, i) = c·(n−i+1)/i é exato em inteiros.
    c = (c * BigInt(n - i + 1)) / BigInt(i);
  }
  return c;
}

export interface Hipergeometrica {
  readonly n: number;
  readonly sorteadas: number;
  readonly escolhidas: number;
  /** Menor/maior número de acertos possível. */
  readonly minimo: number;
  readonly maximo: number;
  /** `casos[i]` = numerador inteiro de P(i), i de 0 a `escolhidas` (zero fora do suporte). */
  readonly casos: readonly bigint[];
  /** C(n, escolhidas). */
  readonly total: bigint;
  /** `pmf[i]` = casos[i] / total, i de 0 a `escolhidas`. */
  readonly pmf: readonly number[];
  /** escolhidas · sorteadas / n. */
  readonly media: number;
  /** k·(K/N)·((N−K)/N)·((N−k)/(N−1)); 0 com n <= 1. */
  readonly variancia: number;
}

/** Igual a `Hipergeometrica::nova(n, sorteadas, escolhidas)` do back-end. */
export function hipergeometrica(n: number, sorteadas: number, escolhidas: number): Hipergeometrica {
  if (![n, sorteadas, escolhidas].every((v) => Number.isInteger(v) && v >= 0)) {
    throw new RangeError('hipergeometrica: parâmetros devem ser inteiros não negativos');
  }
  if (sorteadas > n || escolhidas > n) {
    throw new RangeError('hipergeometrica: sorteadas e escolhidas não podem passar de n');
  }
  const total = combinacao(n, escolhidas);
  const casos: bigint[] = [];
  for (let i = 0; i <= escolhidas; i++) {
    casos.push(combinacao(sorteadas, i) * combinacao(n - sorteadas, escolhidas - i));
  }
  const totalNumero = Number(total);
  const pmf = casos.map((c) => Number(c) / totalNumero);
  const variancia =
    n <= 1
      ? 0
      : escolhidas * (sorteadas / n) * ((n - sorteadas) / n) * ((n - escolhidas) / (n - 1));
  return {
    n,
    sorteadas,
    escolhidas,
    minimo: Math.max(0, escolhidas + sorteadas - n),
    maximo: Math.min(escolhidas, sorteadas),
    casos,
    total,
    pmf,
    media: (escolhidas * sorteadas) / n,
    variancia,
  };
}

const memoHiper = new Map<string, Hipergeometrica>();
function memorizada(slug: Slug, tipo: 'pares' | 'repetidas'): Hipergeometrica {
  const chave = `${slug}:${tipo}`;
  const guardada = memoHiper.get(chave);
  if (guardada) return guardada;
  const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[slug];
  const h =
    tipo === 'pares'
      ? hipergeometrica(totalDezenas, Math.floor(totalDezenas / 2), dezenasSorteadas)
      : hipergeometrica(totalDezenas, dezenasSorteadas, dezenasSorteadas);
  memoHiper.set(chave, h);
  return h;
}

/** Pares por sorteio ao acaso: `total/2` pares entre `total` dezenas, sorteio de K. */
export function distribuicaoParesAoAcaso(slug: Slug): Hipergeometrica {
  return memorizada(slug, 'pares');
}

/** Repetidas do anterior ao acaso: K marcadas (o anterior) entre `total`, sorteio de K. */
export function distribuicaoRepetidasAoAcaso(slug: Slug): Hipergeometrica {
  return memorizada(slug, 'repetidas');
}

// ------------------------------------------------------------------ observado × esperado

export interface LinhaComparacao {
  /** Quantos pares (ou repetidas) o sorteio teve. */
  readonly valor: number;
  /** Sorteios observados com este valor. */
  readonly observado: number;
  /** observado / sorteios (0 com janela vazia). */
  readonly fracaoObservada: number;
  /** P(valor) ao acaso. */
  readonly fracaoEsperada: number;
  /** sorteios · P(valor): quantos sorteios se esperariam com este valor. */
  readonly esperado: number;
}

export interface ComparacaoDistribuicao {
  /** Sorteios contados (soma da distribuição observada). */
  readonly sorteios: number;
  /** Todos os valores possíveis (suporte da hipergeométrica), em ordem crescente. */
  readonly linhas: readonly LinhaComparacao[];
}

/**
 * Compara a distribuição observada (`observada[v]` = sorteios com v) com a hipergeométrica.
 * Devolve todo o suporte; `linhasVisiveis` corta as pontas desprezíveis do gráfico.
 */
export function compararDistribuicao(
  observada: readonly number[],
  esperada: Hipergeometrica,
): ComparacaoDistribuicao {
  const sorteios = observada.reduce((a, b) => a + b, 0);
  const linhas: LinhaComparacao[] = [];
  for (let v = esperada.minimo; v <= esperada.maximo; v++) {
    const obs = observada[v] ?? 0;
    const fe = esperada.pmf[v] ?? 0;
    linhas.push({
      valor: v,
      observado: obs,
      fracaoObservada: sorteios > 0 ? obs / sorteios : 0,
      fracaoEsperada: fe,
      esperado: sorteios * fe,
    });
  }
  return { sorteios, linhas };
}

/** Abaixo disto (0,05%) e sem nenhuma ocorrência, o valor só aparece na tabela. */
export const LIMITE_VALOR_RARO = 0.0005;

/** Faixa contínua de valores que merece barra: do primeiro ao último não raro. */
export function linhasVisiveis(comparacao: ComparacaoDistribuicao): LinhaComparacao[] {
  const merece = (l: LinhaComparacao): boolean =>
    l.fracaoEsperada >= LIMITE_VALOR_RARO || l.observado > 0;
  const primeiro = comparacao.linhas.findIndex(merece);
  if (primeiro < 0) return [...comparacao.linhas];
  let ultimo = primeiro;
  comparacao.linhas.forEach((l, i) => {
    if (merece(l)) ultimo = i;
  });
  return comparacao.linhas.slice(primeiro, ultimo + 1);
}

// ------------------------------------------------------------------ soma das dezenas

export interface DistribuicaoSoma {
  readonly total: number;
  readonly k: number;
  /** Menor e maior soma possíveis: 1+2+...+k e (total−k+1)+...+total. */
  readonly minimo: number;
  readonly maximo: number;
  /** `casos[s]` = quantos subconjuntos de k dezenas distintas de 1..total somam s (BigInt). */
  readonly casos: readonly bigint[];
  /** C(total, k): a soma de `casos`. */
  readonly totalCasos: bigint;
  /** `pmf[s]` = casos[s] / totalCasos, s de 0 a k · total. */
  readonly pmf: readonly number[];
  /** k · (total + 1) / 2, exato. */
  readonly media: number;
  readonly faixa: FaixaCentral;
}

/**
 * Contagem exata das somas de k dezenas distintas de 1..total, por programação dinâmica em
 * BigInt: `c[j][s]` = subconjuntos de j dezenas (entre as já vistas) com soma s. Cada
 * sorteio é equiprovável, então a pmf é casos / C(total, k): exato, sem aproximação normal.
 */
export function contarSomas(total: number, k: number): bigint[] {
  if (!Number.isInteger(total) || !Number.isInteger(k) || k < 0 || k > total) {
    throw new RangeError(`contarSomas: parâmetros inválidos (${total}, ${k})`);
  }
  const maxSoma = k * total;
  const c: bigint[][] = Array.from({ length: k + 1 }, () =>
    new Array<bigint>(maxSoma + 1).fill(0n),
  );
  c[0]![0] = 1n;
  for (let v = 1; v <= total; v++) {
    for (let j = Math.min(k, v); j >= 1; j--) {
      const de = c[j - 1]!;
      const para = c[j]!;
      for (let s = maxSoma; s >= v; s--) para[s] = para[s]! + de[s - v]!;
    }
  }
  return c[k]!;
}

const memoSoma = new Map<Slug, DistribuicaoSoma>();

/** Soma de uma dezena-sorteio ao acaso: distribuição exata e faixa central de 95%. */
export function distribuicaoSomaAoAcaso(slug: Slug): DistribuicaoSoma {
  const guardada = memoSoma.get(slug);
  if (guardada) return guardada;
  const { totalDezenas: total, dezenasSorteadas: k } = PARAMETROS_LOTERIA[slug];
  const casos = contarSomas(total, k);
  const totalCasos = combinacao(total, k);
  const pmf = casos.map((c) => Number(c) / Number(totalCasos));
  const d: DistribuicaoSoma = {
    total,
    k,
    minimo: (k * (k + 1)) / 2,
    maximo: (k * (2 * total - k + 1)) / 2,
    casos,
    totalCasos,
    pmf,
    media: (k * (total + 1)) / 2,
    faixa: faixaCentral(pmf),
  };
  memoSoma.set(slug, d);
  return d;
}

/** Sorteios da janela com soma de `inferior` a `superior` (inclusive). */
export function sorteiosNaFaixaDaSoma(
  distribuicaoObservada: readonly number[],
  faixa: Pick<FaixaCentral, 'inferior' | 'superior'>,
): number {
  let dentro = 0;
  for (let s = faixa.inferior; s <= faixa.superior; s++) dentro += distribuicaoObservada[s] ?? 0;
  return dentro;
}

// ------------------------------------------------------------------ atraso

/**
 * P(uma dezena ficar pelo menos `atraso` concursos seguidos sem sair) = (1 − k/total)^atraso.
 * Cada concurso sorteia k das `total` dezenas de forma independente, então a dezena fica de
 * fora de um concurso com probabilidade (total − k)/total. Descreve o que o acaso produz;
 * não diz nada sobre o próximo concurso (a chance de sair segue k/total, qualquer atraso).
 */
export function chanceDeAtrasoAoMenos(slug: Slug, atraso: number): number {
  if (!Number.isInteger(atraso) || atraso < 0) {
    throw new RangeError(`chanceDeAtrasoAoMenos: atraso inválido (${atraso})`);
  }
  const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[slug];
  return Math.pow((totalDezenas - dezenasSorteadas) / totalDezenas, atraso);
}

/** Quantas dezenas, entre todas, se esperam com atraso >= `atraso` ao acaso (linearidade). */
export function dezenasEsperadasComAtraso(slug: Slug, atraso: number): number {
  return PARAMETROS_LOTERIA[slug].totalDezenas * chanceDeAtrasoAoMenos(slug, atraso);
}
