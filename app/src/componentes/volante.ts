// Geometria e faixas do volante. Puro: o app não recalcula o motor, só
// distribui as posições que já vêm de tabela_geral.json em faixas visuais.
import { PARAMETROS_LOTERIA, type Slug } from '../dados/tipos';

export const NUMERO_FAIXAS = 5;

const COLUNAS: Readonly<Record<Slug, number>> = { lotofacil: 5, megasena: 10, quina: 10 };

export interface LayoutVolante {
  readonly linhas: number;
  readonly colunas: number;
  readonly total: number;
}

/** Lotofácil 5x5, Mega-Sena 6x10, Quina 8x10. */
export function layoutVolante(slug: Slug): LayoutVolante {
  const total = PARAMETROS_LOTERIA[slug].totalDezenas;
  const colunas = COLUNAS[slug];
  return { linhas: total / colunas, colunas, total };
}

/** Dezenas do volante, de 1 a N. */
export function dezenasDoVolante(slug: Slug): number[] {
  return Array.from({ length: PARAMETROS_LOTERIA[slug].totalDezenas }, (_, i) => i + 1);
}

/**
 * Faixa (1 = topo do ranking) de uma posição entre `total`, em `faixas` partes
 * iguais. Posição fora de 1..total é limitada às pontas.
 */
export function faixaPorPosicao(posicao: number, total: number, faixas = NUMERO_FAIXAS): number {
  const p = Math.min(Math.max(Math.trunc(posicao), 1), total);
  return Math.floor(((p - 1) * faixas) / total) + 1;
}

/** Posições (inclusivas) que formam a faixa, para a legenda. */
export function limitesDaFaixa(
  faixa: number,
  total: number,
  faixas = NUMERO_FAIXAS,
): { de: number; ate: number } {
  const primeira = (f: number) => Math.ceil(((f - 1) * total) / faixas) + 1;
  return { de: primeira(faixa), ate: faixa >= faixas ? total : primeira(faixa + 1) - 1 };
}
