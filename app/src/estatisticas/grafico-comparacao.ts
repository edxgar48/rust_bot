// Mini-barras emparelhadas "observado × esperado ao acaso" (7.3): funções puras que só
// calculam posições e textos; o componente desenha. Uma linha por valor (quantos pares ou
// repetidas o sorteio teve), com duas barras horizontais comparáveis entre si: a de cima é
// o observado (cheia) e a de baixo, o esperado ao acaso (vazada com hachura). Os dois
// percentuais ficam escritos ao lado das barras, então nada depende de cor.
import { formatarPercentual } from '../lib/numeros';
import type { LinhaComparacao } from './acaso';

export const ALTURA_BARRA = 11;
export const FOLGA_ENTRE_BARRAS = 2;
export const ALTURA_LINHA = 34;
export const X_ROTULO = 22;
export const X_BARRAS = 30;
/** Espaço à direita da barra mais longa para o texto do percentual. */
export const RESERVA_TEXTO = 48;
const MARGEM_TOPO = 3;

export interface LinhaMiniBarras {
  readonly valor: number;
  readonly rotulo: string;
  /** Topo da linha. */
  readonly y: number;
  readonly yObservada: number;
  readonly yEsperada: number;
  readonly larguraObservada: number;
  readonly larguraEsperada: number;
  readonly textoObservado: string;
  readonly textoEsperado: string;
  readonly xTextoObservado: number;
  readonly xTextoEsperado: number;
}

export interface DadosMiniBarras {
  readonly largura: number;
  readonly altura: number;
  readonly xBarras: number;
  /** Largura (px) correspondente à fração `escalaMaxima`. */
  readonly larguraUtil: number;
  /** Maior fração desenhada (a barra mais longa ocupa `larguraUtil`). */
  readonly escalaMaxima: number;
  readonly linhas: readonly LinhaMiniBarras[];
}

export function montarMiniBarras(
  linhas: readonly LinhaComparacao[],
  larguraDisponivel: number,
): DadosMiniBarras {
  const largura = Math.max(Math.round(larguraDisponivel), 200);
  const larguraUtil = Math.max(largura - X_BARRAS - RESERVA_TEXTO, 40);
  const maior = linhas.reduce((m, l) => Math.max(m, l.fracaoObservada, l.fracaoEsperada), 0);
  const escalaMaxima = maior > 0 ? maior : 1;
  const comprimento = (fracao: number): number =>
    fracao <= 0 ? 0 : Math.max((fracao / escalaMaxima) * larguraUtil, 1);

  const saida = linhas.map((l, i): LinhaMiniBarras => {
    const y = MARGEM_TOPO + i * ALTURA_LINHA;
    const yObservada = y + 3;
    const yEsperada = yObservada + ALTURA_BARRA + FOLGA_ENTRE_BARRAS;
    const larguraObservada = comprimento(l.fracaoObservada);
    const larguraEsperada = comprimento(l.fracaoEsperada);
    return {
      valor: l.valor,
      rotulo: String(l.valor),
      y,
      yObservada,
      yEsperada,
      larguraObservada,
      larguraEsperada,
      textoObservado: formatarPercentual(l.fracaoObservada),
      textoEsperado: formatarPercentual(l.fracaoEsperada),
      xTextoObservado: X_BARRAS + larguraObservada + 5,
      xTextoEsperado: X_BARRAS + larguraEsperada + 5,
    };
  });
  return {
    largura,
    altura: MARGEM_TOPO + linhas.length * ALTURA_LINHA,
    xBarras: X_BARRAS,
    larguraUtil,
    escalaMaxima,
    linhas: saida,
  };
}
