// Lógica da tela Estatísticas, sem Svelte: visão por estado (carregando, sem dados,
// conteúdo) e os textos montados a partir de `EstatisticasJanela` (7.1). O app não
// recalcula nada aqui além de ordenar e formatar o que a 7.1 já devolve.
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA } from '../dados/tipos';
import {
  chanceDeAtrasoAoMenos,
  compararDistribuicao,
  dezenasEsperadasComAtraso,
  dezenasForaDaFaixa,
  distribuicaoParesAoAcaso,
  distribuicaoRepetidasAoAcaso,
  distribuicaoSomaAoAcaso,
  faixaDaFrequencia,
  linhasVisiveis,
  posicaoNaFaixa,
  sorteiosNaFaixaDaSoma,
  type DistribuicaoSoma,
  type PosicaoNaFaixa,
  type ComparacaoDistribuicao,
  type DezenasForaDaFaixa,
  type FaixaFrequencia,
  type LinhaComparacao,
} from '../estatisticas/acaso';
import { MAXIMO_EMPATADOS_MARCADOS, type ExtremosFrequencia } from '../estatisticas/grafico';
import type { AtrasoDezena, EstatisticasJanela } from '../estatisticas/estatisticas';
import { doisDigitos, formatarDecimal, formatarInteiro, formatarPercentual } from '../lib/numeros';
import { textos } from '../lib/textos';
import { estadoEData, type EstadoTela, type SituacaoTela } from './estado-tela';

export type VisaoEstatisticas =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      readonly estado: EstadoTela;
      readonly texto: string;
      readonly erro: boolean;
      readonly concursos: readonly ConcursoHistorico[];
    };

export function montarVisaoEstatisticas(
  slug: Slug,
  situacao: SituacaoTela | null,
  agora: Date = new Date(),
): VisaoEstatisticas {
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const historico = entrada?.dados.historico ?? null;

  if (situacao.cache === null || historico === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.falha === 'recusado') {
      return { tipo: 'sem_dados', texto: textos.estatisticas.recusadoSemAnterior, erro: true };
    }
    if (situacao.falha === 'nenhuma' && situacao.cache !== null && entrada === null) {
      return { tipo: 'sem_dados', texto: textos.estatisticas.semLoteria, erro: false };
    }
    return { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    estado,
    texto,
    erro: estado === 'recusado',
    concursos: historico.concursos,
  };
}

/** "Quina · últimos 50 concursos". */
export function subtituloDasEstatisticas(e: EstatisticasJanela): string {
  return textos.estatisticas.subtitulo(textos.loterias[e.slug], e.tamanho, e.janela === 'total');
}

/** Frase curta da região viva ao trocar loteria ou janela. */
export function anuncioDasEstatisticas(e: EstatisticasJanela): string {
  return textos.estatisticas.anuncio(textos.loterias[e.slug], e.tamanho, e.janela === 'total');
}

/** Dezenas do extremo que merecem nome (até o limite de empates marcados). */
function dezenasCitadas(dezenas: readonly number[]): string[] {
  return dezenas.length <= MAXIMO_EMPATADOS_MARCADOS ? dezenas.map(doisDigitos) : [];
}

export interface LinhasResumoFrequencia {
  /** "Todas as dezenas saíram 3 vezes."; só quando não há extremo a destacar. */
  readonly iguais: string | null;
  /** "Mais vezes: dezena 12, com 9 vezes." */
  readonly maior: string | null;
  /** "Menos vezes: dezena 40, com 1 vez." */
  readonly menor: string | null;
}

/** Resume os extremos sem ler as dezenas todas (empate grande vira "7 dezenas empatadas"). */
export function linhasDoResumoFrequencia(extremos: ExtremosFrequencia): LinhasResumoFrequencia {
  const r = textos.estatisticas.frequencia.resumo;
  if (extremos.todasIguais) return { iguais: r.iguais(extremos.maximo), maior: null, menor: null };
  return {
    iguais: null,
    maior: r.maior(
      dezenasCitadas(extremos.dezenasMaximo),
      extremos.dezenasMaximo.length,
      extremos.maximo,
    ),
    menor: r.menor(
      dezenasCitadas(extremos.dezenasMinimo),
      extremos.dezenasMinimo.length,
      extremos.minimo,
    ),
  };
}

/** Rótulo do `role="img"` do gráfico: só descrição e esperado; os extremos ficam na legenda (já lida). */
export function rotuloDoGrafico(
  e: EstatisticasJanela,
  faixa: { readonly inferior: number; readonly superior: number } | null = null,
): string {
  return textos.estatisticas.frequencia.graficoRotulo(
    textos.loterias[e.slug],
    PARAMETROS_LOTERIA[e.slug].totalDezenas,
    e.tamanho,
    formatarDecimal(e.frequenciaEsperada),
    faixa,
  );
}

/** Abaixo disto a faixa é tão larga que vale avisar. */
export const JANELA_PEQUENA = 30;
/** Dezenas citadas por extenso em cada lista de "fora da faixa". */
export const MAXIMO_FORA_CITADAS = 10;

export interface ResumoAcaso {
  readonly faixa: FaixaFrequencia;
  readonly fora: DezenasForaDaFaixa;
  /** "2 dezenas ficaram fora da faixa; ao acaso se esperam cerca de 1,2. ..." */
  readonly texto: string;
  /** "Abaixo da faixa: 08, 16." ou `null` se nenhuma. */
  readonly abaixo: string | null;
  readonly acima: string | null;
  /** Aviso de faixa larga em janela pequena, ou `null`. */
  readonly aviso: string | null;
}

function listaDeDezenas(dezenas: readonly number[]): string {
  const t = textos.estatisticas.frequencia;
  const citadas = dezenas.slice(0, MAXIMO_FORA_CITADAS).map(doisDigitos).join(', ');
  const resto = dezenas.length - MAXIMO_FORA_CITADAS;
  return resto > 0 ? `${citadas} ${t.foraMais(resto)}` : citadas;
}

/**
 * Faixa do acaso e dezenas fora dela para a janela efetiva (`e.tamanho`, não a pedida).
 * `null` com janela vazia.
 */
export function montarResumoAcaso(e: EstatisticasJanela): ResumoAcaso | null {
  const faixa = faixaDaFrequencia(e.slug, e.tamanho);
  if (faixa === null) return null;
  const t = textos.estatisticas.frequencia;
  const fora = dezenasForaDaFaixa(e.frequencia, faixa);
  return {
    faixa,
    fora,
    texto: t.resumoAcaso(fora.fora, formatarDecimal(fora.foraEsperado)),
    abaixo: fora.abaixo.length > 0 ? t.foraAbaixo(listaDeDezenas(fora.abaixo)) : null,
    acima: fora.acima.length > 0 ? t.foraAcima(listaDeDezenas(fora.acima)) : null,
    aviso: e.tamanho < JANELA_PEQUENA ? t.faixaLarga(e.tamanho) : null,
  };
}

export interface LinhaTabelaSoma {
  readonly medida: string;
  readonly valor: string;
  readonly situacao: string;
}

export interface BlocoSoma {
  readonly acaso: DistribuicaoSoma;
  readonly sorteios: number;
  readonly media: number;
  readonly minimo: number;
  readonly maximo: number;
  readonly dentro: number;
  readonly legendaFaixa: string;
  readonly legendaEsperado: string;
  readonly legendaMedia: string;
  readonly legendaExtremos: string;
  readonly textoDentro: string;
  readonly explicacao: string;
  readonly rotulo: string;
  readonly tabela: readonly LinhaTabelaSoma[];
  readonly legendaTabela: string;
}

/** "ao acaso" da soma: `null` sem sorteios na janela. */
export function montarBlocoSoma(e: EstatisticasJanela): BlocoSoma | null {
  if (e.soma === null || e.tamanho === 0) return null;
  const t = textos.estatisticas.soma.acaso;
  const acaso = distribuicaoSomaAoAcaso(e.slug);
  const { inferior, superior, coberturaReal, foraPorDezena } = acaso.faixa;
  const dentro = sorteiosNaFaixaDaSoma(e.distribuicaoSoma, acaso.faixa);
  const posicao = (v: number): string => {
    const p: PosicaoNaFaixa = posicaoNaFaixa(v, acaso.faixa);
    return t.posicao[p];
  };
  const esperadoTexto = formatarDecimal(acaso.media);
  const mediaTexto = formatarDecimal(e.soma.media);
  const minimoTexto = formatarInteiro(e.soma.minimo);
  const maximoTexto = formatarInteiro(e.soma.maximo);
  return {
    acaso,
    sorteios: e.tamanho,
    media: e.soma.media,
    minimo: e.soma.minimo,
    maximo: e.soma.maximo,
    dentro,
    legendaFaixa: t.legendaFaixa(inferior, superior),
    legendaEsperado: t.legendaEsperado(esperadoTexto),
    legendaMedia: t.legendaMedia(mediaTexto),
    legendaExtremos: t.legendaExtremos(minimoTexto, maximoTexto),
    textoDentro: t.dentro(
      dentro,
      e.tamanho,
      formatarPercentual(dentro / e.tamanho),
      formatarPercentual(coberturaReal),
    ),
    explicacao: t.explicacao(formatarPercentual(foraPorDezena)),
    rotulo: t.rotulo(inferior, superior, esperadoTexto, e.tamanho),
    legendaTabela: t.tabelaLegenda(e.tamanho),
    tabela: [
      { medida: t.linhaFaixa, valor: t.faixaValor(inferior, superior), situacao: t.semSituacao },
      { medida: t.linhaEsperado, valor: esperadoTexto, situacao: t.semSituacao },
      { medida: t.linhaMedia, valor: mediaTexto, situacao: posicao(e.soma.media) },
      { medida: t.linhaMinimo, valor: minimoTexto, situacao: posicao(e.soma.minimo) },
      { medida: t.linhaMaximo, valor: maximoTexto, situacao: posicao(e.soma.maximo) },
      {
        medida: t.linhaDentro,
        valor: t.dentroValor(dentro, e.tamanho, formatarPercentual(dentro / e.tamanho)),
        situacao: t.dentroSituacao(formatarPercentual(coberturaReal)),
      },
    ],
  };
}

export type TipoComparacao = 'pares' | 'repetidas';

export interface BlocoComparacao {
  readonly tipo: TipoComparacao;
  /** "pares" ou "repetidas", para os textos. */
  readonly nome: string;
  readonly comparacao: ComparacaoDistribuicao;
  /** Linhas das mini-barras (sem as pontas raras). */
  readonly visiveis: readonly LinhaComparacao[];
  /** Há valores só na tabela. */
  readonly temRaros: boolean;
  readonly rotulo: string;
}

/** Observado × esperado ao acaso de pares ou repetidas; `null` sem sorteios para comparar. */
export function montarBlocoComparacao(
  tipo: TipoComparacao,
  e: EstatisticasJanela,
): BlocoComparacao | null {
  const observada = tipo === 'pares' ? e.distribuicaoPares : e.repetidas.distribuicao;
  const esperada =
    tipo === 'pares' ? distribuicaoParesAoAcaso(e.slug) : distribuicaoRepetidasAoAcaso(e.slug);
  const comparacao = compararDistribuicao(observada, esperada);
  if (comparacao.sorteios === 0) return null;
  const t = textos.estatisticas.comparacao;
  const nome = t[tipo];
  const visiveis = linhasVisiveis(comparacao);
  return {
    tipo,
    nome,
    comparacao,
    visiveis,
    temRaros: visiveis.length < comparacao.linhas.length,
    rotulo: t.rotulo(nome, comparacao.sorteios),
  };
}

export const MAXIMO_ATRASO_EXIBIDO = 5;

export interface ItemAtraso {
  readonly dezena: number;
  readonly rotulo: string;
  readonly atraso: number;
  readonly texto: string;
  /** Chance ao acaso de ficar `atraso` concursos ou mais sem sair; `null` com atraso 0. */
  readonly chance: string | null;
}

export interface CartaoAtraso {
  /** As dezenas de maior atraso conhecido (até 5). */
  readonly itens: readonly ItemAtraso[];
  /** Dezenas que não saíram em toda a janela: SEMPRE exibidas junto dos itens. */
  readonly semAparicao: readonly number[];
  /** "Não saíram nos últimos 10 concursos: 72 dezenas" (ou o texto de nenhuma). */
  readonly textoSemAparicao: string;
  /** Número esperado de dezenas com o maior atraso da lista, ao acaso; `null` sem itens. */
  readonly resumoMaior: string | null;
  /** Chance ao acaso de uma dezena não sair em toda a janela e quantas se esperam assim. */
  readonly chanceSemAparicao: string;
  /** "Cada sorteio é independente..." com a chance por concurso (K/total). */
  readonly independente: string;
}

/** Percentual de chance; abaixo de 0,05% vira "menos de 0,1%" (nunca "0%"). */
function chanceEmTexto(p: number): string {
  const t = textos.estatisticas.atraso.acaso;
  return p < 0.0005 ? t.menosDe : formatarPercentual(p);
}

/** Número esperado de dezenas; abaixo de 0,05 vira "menos de 0,1". */
function esperadoEmTexto(v: number): string {
  const t = textos.estatisticas.atraso.acaso;
  return v < 0.05 ? t.menosDeEsperado : formatarDecimal(v);
}

export function montarCartaoAtraso(e: EstatisticasJanela): CartaoAtraso {
  const ta = textos.estatisticas.atraso.acaso;
  const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[e.slug];
  const itens = e.rankingAtraso
    .slice(0, MAXIMO_ATRASO_EXIBIDO)
    .map((a: AtrasoDezena): ItemAtraso => ({
      dezena: a.dezena,
      rotulo: doisDigitos(a.dezena),
      atraso: a.atraso,
      texto: textos.estatisticas.atraso.item(doisDigitos(a.dezena), a.atraso),
      chance:
        a.atraso > 0
          ? ta.chance(a.atraso, chanceEmTexto(chanceDeAtrasoAoMenos(e.slug, a.atraso)))
          : null,
    }));
  const maior = itens[0];
  const quantas = e.semAparicao.length;
  return {
    itens,
    resumoMaior:
      maior && maior.atraso > 0
        ? ta.resumoMaior(
            totalDezenas,
            maior.atraso,
            esperadoEmTexto(dezenasEsperadasComAtraso(e.slug, maior.atraso)),
          )
        : null,
    chanceSemAparicao: ta.semAparicao(
      e.tamanho,
      chanceEmTexto(chanceDeAtrasoAoMenos(e.slug, e.tamanho)),
      totalDezenas,
      esperadoEmTexto(dezenasEsperadasComAtraso(e.slug, e.tamanho)),
    ),
    independente: ta.independente(formatarPercentual(dezenasSorteadas / totalDezenas)),
    semAparicao: e.semAparicao,
    textoSemAparicao:
      quantas === 0
        ? textos.estatisticas.atraso.semAparicaoNenhuma(e.tamanho)
        : textos.estatisticas.atraso.semAparicao(quantas, e.tamanho),
  };
}
