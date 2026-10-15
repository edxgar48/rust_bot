// Dados do gráfico de barras das Estatísticas (7.2): funções puras, sem Svelte e sem DOM.
// O componente só desenha o que sai daqui. O eixo vertical sempre começa em zero (barra
// truncada exagera diferenças), e a linha do "esperado ao acaso" entra na escala.
//
// Largura em px reais: o viewBox tem o tamanho do contêiner medido, então o texto do SVG
// não encolhe nem estoura quando a tela muda, e as barras se ajustam ao espaço.
import { doisDigitos, formatarInteiro } from '../lib/numeros';
import { posicaoNaFaixa, type PosicaoNaFaixa } from './acaso';

export type MarcaBarra = 'maior' | 'menor';

export interface BarraGrafico {
  readonly dezena: number;
  readonly vezes: number;
  readonly x: number;
  readonly y: number;
  readonly largura: number;
  readonly altura: number;
  /** Extremo marcado com símbolo (nunca só cor); `null` = sem marca. */
  readonly marca: MarcaBarra | null;
  /** Fora da faixa do acaso (7.3): ganha contorno, além da coluna da tabela e do texto. */
  readonly foraDaFaixa: boolean;
}

export interface TickX {
  readonly dezena: number;
  /** Centro da barra. */
  readonly x: number;
  readonly rotulo: string;
}

export interface TickY {
  readonly valor: number;
  readonly y: number;
  readonly rotulo: string;
}

export interface DadosGrafico {
  readonly largura: number;
  readonly altura: number;
  readonly esquerda: number;
  readonly direita: number;
  readonly topo: number;
  /** Coordenada y do eixo (valor zero). */
  readonly base: number;
  readonly barras: readonly BarraGrafico[];
  readonly ticksX: readonly TickX[];
  readonly ticksY: readonly TickY[];
  /** Coordenada y da linha tracejada do esperado. */
  readonly esperadoY: number;
  /** Faixa sombreada do acaso (7.3): `topo` < `base`; `null` sem faixa. */
  readonly faixa: { readonly topo: number; readonly base: number } | null;
  /** Topo da escala vertical (em vezes). */
  readonly escalaMaxima: number;
  /** Meia-largura das marcas de extremo: encolhe com barras estreitas (Quina) para não invadir as vizinhas. */
  readonly raioMarca: number;
}

export interface EntradaGrafico {
  /** `frequencia[d - 1]` = vezes que a dezena d saiu. */
  readonly frequencia: readonly number[];
  readonly esperado: number;
  /** Faixa em que o acaso põe a frequência (limites inclusivos, em vezes); opcional. */
  readonly faixa?: { readonly inferior: number; readonly superior: number } | null;
  /** Largura disponível, em px. */
  readonly largura: number;
  readonly altura?: number;
}

export const ALTURA_GRAFICO_PADRAO = 200;
export const LARGURA_MINIMA_GRAFICO = 200;
/** Largura usada antes da primeira medição do contêiner (e fora de um navegador). */
export const LARGURA_ANTES_DA_MEDICAO = 320;
export const RAIO_MARCA_MAXIMO = 4.5;
export const RAIO_MARCA_MINIMO = 2.5;

/** Largura do viewBox: 0 = ainda não medido; medida pequena demais vira o mínimo do gráfico. */
export function larguraDoGrafico(medida: number): number {
  return medida === 0 ? LARGURA_ANTES_DA_MEDICAO : Math.max(medida, LARGURA_MINIMA_GRAFICO);
}
const MARGEM_TOPO = 16;
const MARGEM_BASE = 24;
const MARGEM_DIREITA = 8;
const LARGURA_CARACTERE = 6.5;
/** Mais empatados que isto no extremo: nenhuma barra é marcada (marcar todas não informa). */
export const MAXIMO_EMPATADOS_MARCADOS = 3;

/** Passo entre rótulos do eixo das dezenas: de 5 em 5 até 25 dezenas, de 10 em 10 acima. */
export function passoRotulos(totalDezenas: number): number {
  return totalDezenas <= 25 ? 5 : 10;
}

/** Dezenas que levam rótulo: a 1 e os múltiplos do passo (1, 10, 20, ... 80). */
export function dezenasRotuladas(totalDezenas: number): number[] {
  const passo = passoRotulos(totalDezenas);
  const saida: number[] = [];
  if (totalDezenas >= 1) saida.push(1);
  for (let d = passo; d <= totalDezenas; d += passo) saida.push(d);
  return saida;
}

/**
 * Escala vertical "redonda": topo e valores dos ticks (0 incluído), com no máximo 4 intervalos
 * e passos 1, 2 ou 5 vezes uma potência de 10. O topo nunca fica abaixo de `maximo`.
 */
export function escalaVertical(maximo: number): { topo: number; ticks: number[] } {
  const alvo = Math.max(maximo, 1);
  // Contagens são inteiras: o passo nunca é menor que 1.
  const potencia = Math.max(1, 10 ** Math.floor(Math.log10(alvo / 4)));
  let passo = potencia;
  for (const m of [1, 2, 5, 10]) {
    passo = m * potencia;
    if (passo * 4 >= alvo) break;
  }
  const intervalos = Math.ceil(alvo / passo - 1e-9);
  const ticks: number[] = [];
  for (let i = 0; i <= intervalos; i++) ticks.push(i * passo);
  return { topo: intervalos * passo, ticks };
}

export interface ExtremosFrequencia {
  readonly maximo: number;
  readonly minimo: number;
  readonly dezenasMaximo: readonly number[];
  readonly dezenasMinimo: readonly number[];
  /** Todas as dezenas com o mesmo valor (ou lista vazia): não há extremo a destacar. */
  readonly todasIguais: boolean;
}

export function extremosFrequencia(frequencia: readonly number[]): ExtremosFrequencia {
  if (frequencia.length === 0) {
    return { maximo: 0, minimo: 0, dezenasMaximo: [], dezenasMinimo: [], todasIguais: true };
  }
  let maximo = Number.NEGATIVE_INFINITY;
  let minimo = Number.POSITIVE_INFINITY;
  for (const v of frequencia) {
    if (v > maximo) maximo = v;
    if (v < minimo) minimo = v;
  }
  const dezenasMaximo: number[] = [];
  const dezenasMinimo: number[] = [];
  frequencia.forEach((v, i) => {
    if (v === maximo) dezenasMaximo.push(i + 1);
    if (v === minimo) dezenasMinimo.push(i + 1);
  });
  return { maximo, minimo, dezenasMaximo, dezenasMinimo, todasIguais: maximo === minimo };
}

export function montarGrafico(entrada: EntradaGrafico): DadosGrafico {
  const { frequencia, esperado } = entrada;
  const faixa = entrada.faixa ?? null;
  const n = frequencia.length;
  const largura = Math.max(Math.round(entrada.largura), LARGURA_MINIMA_GRAFICO);
  const altura = entrada.altura ?? ALTURA_GRAFICO_PADRAO;
  const extremos = extremosFrequencia(frequencia);
  const escala = escalaVertical(Math.max(extremos.maximo, esperado, faixa?.superior ?? 0, 0));

  const rotulosY = escala.ticks.map((t) => formatarInteiro(t));
  const maiorRotulo = rotulosY.reduce((m, r) => Math.max(m, r.length), 1);
  const esquerda = Math.ceil(maiorRotulo * LARGURA_CARACTERE) + 12;
  const direita = largura - MARGEM_DIREITA;
  const base = altura - MARGEM_BASE;
  const topo = MARGEM_TOPO;
  const utilY = base - topo;
  const yDe = (valor: number): number => base - (valor / escala.topo) * utilY;

  const passoX = n > 0 ? (direita - esquerda) / n : 0;
  const larguraBarra = Math.max(passoX * 0.74, 1);
  const folga = (passoX - larguraBarra) / 2;

  const marcarMaior =
    !extremos.todasIguais && extremos.dezenasMaximo.length <= MAXIMO_EMPATADOS_MARCADOS;
  const marcarMenor =
    !extremos.todasIguais && extremos.dezenasMinimo.length <= MAXIMO_EMPATADOS_MARCADOS;

  const barras: BarraGrafico[] = frequencia.map((vezes, i) => {
    const alturaBarra = vezes > 0 ? Math.max(base - yDe(vezes), 1) : 0;
    const marca: MarcaBarra | null =
      marcarMaior && vezes === extremos.maximo
        ? 'maior'
        : marcarMenor && vezes === extremos.minimo
          ? 'menor'
          : null;
    return {
      dezena: i + 1,
      vezes,
      x: esquerda + i * passoX + folga,
      y: base - alturaBarra,
      largura: larguraBarra,
      altura: alturaBarra,
      marca,
      foraDaFaixa: faixa !== null && posicaoNaFaixa(vezes, faixa) !== 'dentro',
    };
  });

  return {
    largura,
    altura,
    esquerda,
    direita,
    topo,
    base,
    barras,
    ticksX: dezenasRotuladas(n).map((dezena) => ({
      dezena,
      x: esquerda + (dezena - 0.5) * passoX,
      rotulo: doisDigitos(dezena),
    })),
    ticksY: escala.ticks.map((valor, i) => ({
      valor,
      y: yDe(valor),
      rotulo: rotulosY[i] ?? '',
    })),
    esperadoY: yDe(esperado),
    faixa: faixa === null ? null : { topo: yDe(faixa.superior), base: yDe(faixa.inferior) },
    escalaMaxima: escala.topo,
    raioMarca: Math.min(RAIO_MARCA_MAXIMO, Math.max(RAIO_MARCA_MINIMO, passoX * 0.75)),
  };
}

export interface LinhaTabelaFrequencia {
  readonly dezena: number;
  readonly vezes: number;
  readonly diferenca: number;
  /** Situação na faixa do acaso; `null` sem faixa (7.3). */
  readonly posicao: PosicaoNaFaixa | null;
}

/** Linhas da tabela alternativa ao gráfico: uma por dezena, com a diferença para o esperado (que é igual em todas e vai na legenda da tabela) e, havendo faixa do acaso, se ficou dentro. */
export function linhasTabelaFrequencia(
  frequencia: readonly number[],
  esperado: number,
  faixa: { readonly inferior: number; readonly superior: number } | null = null,
): LinhaTabelaFrequencia[] {
  return frequencia.map((vezes, i) => ({
    dezena: i + 1,
    vezes,
    diferenca: vezes - esperado,
    posicao: faixa === null ? null : posicaoNaFaixa(vezes, faixa),
  }));
}
