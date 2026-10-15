// Lógica da tela Estatísticas, sem Svelte: visão por estado (carregando, sem dados,
// conteúdo) e os textos montados a partir de `EstatisticasJanela` (7.1). O app não
// recalcula nada aqui além de ordenar e formatar o que a 7.1 já devolve.
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA } from '../dados/tipos';
import { MAXIMO_EMPATADOS_MARCADOS, type ExtremosFrequencia } from '../estatisticas/grafico';
import type { AtrasoDezena, EstatisticasJanela } from '../estatisticas/estatisticas';
import { doisDigitos, formatarDecimal } from '../lib/numeros';
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
export function rotuloDoGrafico(e: EstatisticasJanela): string {
  return textos.estatisticas.frequencia.graficoRotulo(
    textos.loterias[e.slug],
    PARAMETROS_LOTERIA[e.slug].totalDezenas,
    e.tamanho,
    formatarDecimal(e.frequenciaEsperada),
  );
}

export const MAXIMO_ATRASO_EXIBIDO = 5;

export interface ItemAtraso {
  readonly dezena: number;
  readonly rotulo: string;
  readonly atraso: number;
  readonly texto: string;
}

export interface CartaoAtraso {
  /** As dezenas de maior atraso conhecido (até 5). */
  readonly itens: readonly ItemAtraso[];
  /** Dezenas que não saíram em toda a janela: SEMPRE exibidas junto dos itens. */
  readonly semAparicao: readonly number[];
  /** "Não saíram nos últimos 10 concursos: 72 dezenas" (ou o texto de nenhuma). */
  readonly textoSemAparicao: string;
}

export function montarCartaoAtraso(e: EstatisticasJanela): CartaoAtraso {
  const itens = e.rankingAtraso.slice(0, MAXIMO_ATRASO_EXIBIDO).map((a: AtrasoDezena) => ({
    dezena: a.dezena,
    rotulo: doisDigitos(a.dezena),
    atraso: a.atraso,
    texto: textos.estatisticas.atraso.item(doisDigitos(a.dezena), a.atraso),
  }));
  const quantas = e.semAparicao.length;
  return {
    itens,
    semAparicao: e.semAparicao,
    textoSemAparicao:
      quantas === 0
        ? textos.estatisticas.atraso.semAparicaoNenhuma(e.tamanho)
        : textos.estatisticas.atraso.semAparicao(quantas, e.tamanho),
  };
}
