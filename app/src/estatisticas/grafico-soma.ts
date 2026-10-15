// Escala horizontal da soma das dezenas (7.3): funções puras que só calculam posições; o
// componente desenha. Mostra, numa régua de somas, a faixa do acaso (hachura), a média
// esperada (linha tracejada) e, do observado, a média (losango) e a menor e a maior soma
// (círculos vazados). Os valores também ficam escritos na legenda e numa tabela, então nada
// depende de cor.
import type { DistribuicaoSoma } from './acaso';

export const ALTURA_REGUA = 64;
const MARGEM_X = 18;
const Y_FAIXA_TOPO = 8;
const Y_FAIXA_BASE = 34;
export const Y_MARCAS = (Y_FAIXA_TOPO + Y_FAIXA_BASE) / 2;
const Y_ROTULOS = 56;
/** Fração de cada ponta da distribuição que fica fora da régua (a menos que haja observado). */
const CORTE_PONTA = 0.0005;

export interface ObservadoSoma {
  readonly media: number;
  readonly minimo: number;
  readonly maximo: number;
}

export interface TickSoma {
  readonly valor: number;
  readonly x: number;
  readonly rotulo: string;
  readonly ancora: 'start' | 'middle' | 'end';
}

export interface DadosReguaSoma {
  readonly largura: number;
  readonly altura: number;
  readonly dominio: readonly [number, number];
  readonly yFaixaTopo: number;
  readonly yFaixaBase: number;
  readonly yMarcas: number;
  readonly yRotulos: number;
  readonly faixaX: number;
  readonly faixaLargura: number;
  readonly esperadoX: number;
  readonly mediaX: number;
  readonly minimoX: number;
  readonly maximoX: number;
  readonly ticks: readonly TickSoma[];
}

/** Menor e maior soma com chance não desprezível, para o domínio da régua. */
export function dominioDaSoma(d: DistribuicaoSoma, observado: ObservadoSoma): [number, number] {
  let acumulada = 0;
  let baixo = d.minimo;
  for (let s = 0; s < d.pmf.length; s++) {
    acumulada += d.pmf[s]!;
    if (acumulada >= CORTE_PONTA) {
      baixo = s;
      break;
    }
  }
  acumulada = 0;
  let alto = d.maximo;
  for (let s = d.pmf.length - 1; s >= 0; s--) {
    acumulada += d.pmf[s]!;
    if (acumulada >= CORTE_PONTA) {
      alto = s;
      break;
    }
  }
  baixo = Math.min(baixo, d.faixa.inferior, observado.minimo);
  alto = Math.max(alto, d.faixa.superior, observado.maximo);
  return [baixo, alto];
}

export function montarReguaSoma(
  d: DistribuicaoSoma,
  observado: ObservadoSoma,
  larguraDisponivel: number,
): DadosReguaSoma {
  const largura = Math.max(Math.round(larguraDisponivel), 240);
  const [baixo, alto] = dominioDaSoma(d, observado);
  const util = largura - 2 * MARGEM_X;
  const x = (valor: number): number => MARGEM_X + ((valor - baixo) / (alto - baixo || 1)) * util;
  const inferiorX = x(d.faixa.inferior);
  const superiorX = x(d.faixa.superior);
  const esperadoX = x(d.media);
  const rotulo = (valor: number): string => valor.toLocaleString('pt-BR');
  // Três rótulos: limites da faixa e média esperada (sempre bem separados entre si).
  const ticks: TickSoma[] = [
    { valor: d.faixa.inferior, x: inferiorX, rotulo: rotulo(d.faixa.inferior), ancora: 'middle' },
    { valor: d.media, x: esperadoX, rotulo: rotulo(d.media), ancora: 'middle' },
    { valor: d.faixa.superior, x: superiorX, rotulo: rotulo(d.faixa.superior), ancora: 'middle' },
  ];
  return {
    largura,
    altura: ALTURA_REGUA,
    dominio: [baixo, alto],
    yFaixaTopo: Y_FAIXA_TOPO,
    yFaixaBase: Y_FAIXA_BASE,
    yMarcas: Y_MARCAS,
    yRotulos: Y_ROTULOS,
    faixaX: inferiorX,
    faixaLargura: Math.max(superiorX - inferiorX, 1),
    esperadoX,
    mediaX: x(observado.media),
    minimoX: x(observado.minimo),
    maximoX: x(observado.maximo),
    ticks,
  };
}
