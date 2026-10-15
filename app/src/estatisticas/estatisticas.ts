// Estatísticas simples sobre historico.json (tarefa 7.1): funções puras, sem Svelte,
// sem rede e sem relação com o motor. Descrevem o PASSADO; não dizem nada sobre o
// próximo sorteio. Cada estatística vem junto do valor esperado ao acaso.
//
// Os valores "esperados" (paresEsperado, somaEsperada, repetidas.esperada) são fórmulas
// simples conferidas à mão nos testes; a conferência contra o back-end é da tarefa 7.3.
//
// Janela = últimos N concursos da lista (10, 50, 100 ou todos). Com menos concursos
// que N, usa todos e informa o tamanho efetivo em `tamanho`.
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA } from '../dados/tipos';

export const JANELAS_ESTATISTICA = [10, 50, 100, 'total'] as const;
export type JanelaEstatistica = (typeof JANELAS_ESTATISTICA)[number];

export interface AtrasoDezena {
  readonly dezena: number;
  /** Concursos desde a última aparição (0 = saiu no último concurso da janela). */
  readonly atraso: number;
}

export interface EstatisticasJanela {
  readonly slug: Slug;
  readonly janela: JanelaEstatistica;
  /** Quantidade de concursos efetivamente usados (pode ser menor que a janela pedida). */
  readonly tamanho: number;
  /** Primeiro e último concurso da janela; `null` com janela vazia. */
  readonly primeiroConcurso: number | null;
  readonly ultimoConcurso: number | null;

  /** `frequencia[d - 1]` = quantas vezes a dezena d saiu na janela. */
  readonly frequencia: readonly number[];
  /** Esperado ao acaso por dezena: tamanho · k / total (igual para todas). */
  readonly frequenciaEsperada: number;

  /**
   * `atraso[d - 1]` = concursos desde a última aparição dentro da janela.
   * `null` = a dezena não saiu na janela (o atraso real é pelo menos `tamanho`,
   * mas a janela não permite dizer quanto).
   */
  readonly atraso: readonly (number | null)[];
  /**
   * Dezenas com atraso conhecido, do maior atraso para o menor (empate: dezena menor).
   * Atenção (7.2): o atraso descreve o PASSADO; não indica dezena "devida" nem
   * "atrasada" para sair. Mostrar sempre junto de `semAparicao`, pois dezenas sem
   * aparição ficam fora deste ranking embora tenham atraso maior.
   */
  readonly rankingAtraso: readonly AtrasoDezena[];
  /**
   * Dezenas que não saíram nenhuma vez na janela, em ordem crescente.
   * Atenção (7.2): exibir SEMPRE junto do ranking de atraso, com o texto
   * "não saiu nos últimos N" (N = `tamanho`); omiti-las faz o ranking enganar.
   */
  readonly semAparicao: readonly number[];

  /** `distribuicaoPares[p]` = sorteios com p dezenas pares (p de 0 a k). */
  readonly distribuicaoPares: readonly number[];
  /** Média de pares por sorteio; `null` com janela vazia. */
  readonly mediaPares: number | null;
  /** Esperado ao acaso: k · (pares do volante) / total. */
  readonly paresEsperado: number;

  /** Soma das dezenas por sorteio; `null` com janela vazia. */
  readonly soma: {
    readonly media: number;
    readonly minimo: number;
    readonly maximo: number;
  } | null;
  /** Esperado ao acaso: k · (total + 1) / 2. */
  readonly somaEsperada: number;

  /**
   * Dezenas repetidas do concurso anterior. Só entram sorteios que têm o anterior
   * dentro da janela (o primeiro da janela não conta): `comparacoes` = tamanho − 1.
   */
  readonly repetidas: {
    readonly comparacoes: number;
    /** `distribuicao[r]` = sorteios com r repetidas (r de 0 a k). */
    readonly distribuicao: readonly number[];
    /** `null` com menos de 2 concursos na janela. */
    readonly media: number | null;
    /** Esperado ao acaso: k · k / total. */
    readonly esperada: number;
  };
}

/** Tamanho efetivo da janela para uma lista de `n` concursos. */
export function tamanhoEfetivo(janela: JanelaEstatistica, n: number): number {
  if (n <= 0) return 0;
  return janela === 'total' ? n : Math.min(janela, n);
}

/** Dezenas pares de 1..total. */
function paresDoVolante(total: number): number {
  return Math.floor(total / 2);
}

/**
 * Calcula todas as estatísticas de uma janela numa única passada sobre os concursos.
 * Não aloca nada por concurso. Dezena fora de 1..total (impossível após o validador)
 * é ignorada em vez de lançar.
 */
export function calcularEstatisticas(
  slug: Slug,
  concursos: readonly ConcursoHistorico[],
  janela: JanelaEstatistica,
): EstatisticasJanela {
  const { totalDezenas: total, dezenasSorteadas: k } = PARAMETROS_LOTERIA[slug];
  const tamanho = tamanhoEfetivo(janela, concursos.length);
  const inicio = concursos.length - tamanho;

  const frequencia = new Array<number>(total).fill(0);
  // Posição (índice relativo à janela) da última aparição; -1 = não saiu.
  const ultimaPosicao = new Array<number>(total).fill(-1);
  const distribuicaoPares = new Array<number>(k + 1).fill(0);
  const distribuicaoRepetidas = new Array<number>(k + 1).fill(0);
  // Marca das dezenas do sorteio anterior: valor = posição relativa + 1 (0 = nunca).
  const marcaAnterior = new Uint32Array(total + 1);

  let somaPares = 0;
  let somaTotal = 0;
  let somaMin = Number.POSITIVE_INFINITY;
  let somaMax = Number.NEGATIVE_INFINITY;
  let somaRepetidas = 0;

  for (let i = 0; i < tamanho; i++) {
    const dezenas = concursos[inicio + i]![2];
    const posicaoAnterior = i; // o sorteio anterior (i - 1) deixou a marca i
    let pares = 0;
    let soma = 0;
    let repetidas = 0;
    for (let j = 0; j < dezenas.length; j++) {
      const d = dezenas[j]!;
      if (!Number.isInteger(d) || d < 1 || d > total) continue;
      frequencia[d - 1]!++;
      ultimaPosicao[d - 1] = i;
      if (d % 2 === 0) pares++;
      soma += d;
      if (i > 0 && marcaAnterior[d] === posicaoAnterior) repetidas++;
    }
    // Marca as dezenas deste sorteio para o próximo (marca = i + 1).
    for (let j = 0; j < dezenas.length; j++) {
      const d = dezenas[j]!;
      if (!Number.isInteger(d) || d < 1 || d > total) continue;
      marcaAnterior[d] = i + 1;
    }

    if (pares <= k) distribuicaoPares[pares]!++;
    somaPares += pares;
    somaTotal += soma;
    if (soma < somaMin) somaMin = soma;
    if (soma > somaMax) somaMax = soma;
    if (i > 0) {
      if (repetidas <= k) distribuicaoRepetidas[repetidas]!++;
      somaRepetidas += repetidas;
    }
  }

  const atraso = new Array<number | null>(total);
  const ranking: AtrasoDezena[] = [];
  const semAparicao: number[] = [];
  for (let d = 1; d <= total; d++) {
    const ultima = ultimaPosicao[d - 1]!;
    if (ultima < 0) {
      atraso[d - 1] = null;
      semAparicao.push(d);
    } else {
      const a = tamanho - 1 - ultima;
      atraso[d - 1] = a;
      ranking.push({ dezena: d, atraso: a });
    }
  }
  ranking.sort((x, y) => y.atraso - x.atraso || x.dezena - y.dezena);

  const comparacoes = Math.max(0, tamanho - 1);
  return {
    slug,
    janela,
    tamanho,
    primeiroConcurso: tamanho > 0 ? concursos[inicio]![0] : null,
    ultimoConcurso: tamanho > 0 ? concursos[concursos.length - 1]![0] : null,
    frequencia,
    frequenciaEsperada: (tamanho * k) / total,
    atraso,
    rankingAtraso: ranking,
    semAparicao,
    distribuicaoPares,
    mediaPares: tamanho > 0 ? somaPares / tamanho : null,
    paresEsperado: (k * paresDoVolante(total)) / total,
    soma: tamanho > 0 ? { media: somaTotal / tamanho, minimo: somaMin, maximo: somaMax } : null,
    somaEsperada: (k * (total + 1)) / 2,
    repetidas: {
      comparacoes,
      distribuicao: distribuicaoRepetidas,
      media: comparacoes > 0 ? somaRepetidas / comparacoes : null,
      esperada: (k * k) / total,
    },
  };
}

/** As quatro janelas de uma vez (10, 50, 100, total). */
export function calcularTodasAsJanelas(
  slug: Slug,
  concursos: readonly ConcursoHistorico[],
): Readonly<Record<`${JanelaEstatistica}`, EstatisticasJanela>> {
  const saida = {} as Record<`${JanelaEstatistica}`, EstatisticasJanela>;
  for (const j of JANELAS_ESTATISTICA) saida[`${j}`] = calcularEstatisticas(slug, concursos, j);
  return saida;
}
