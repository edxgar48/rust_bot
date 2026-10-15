// Lógica da tela Conferir jogo, sem Svelte: quantidades válidas, conferência de um
// jogo contra o histórico e visão da tela. O app só LÊ historico.json; nada do motor
// é recalculado. A conferência descreve o passado e não diz nada sobre o próximo sorteio.
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA } from '../dados/tipos';
import { textos } from '../lib/textos';
import { estadoEData, type EstadoTela, type SituacaoTela } from './estado-tela';

// ---------------------------------------------------------------- quantidade do jogo

export interface QuantidadeJogo {
  /** Menos dezenas que isto não é um jogo válido. */
  readonly minimo: number;
  /** Mais dezenas que isto não é aceito pela loteria. */
  readonly maximo: number;
}

/** Quantidade de dezenas que se pode marcar num jogo de cada loteria (regra da Caixa). */
export const QUANTIDADE_JOGO: Readonly<Record<Slug, QuantidadeJogo>> = {
  lotofacil: { minimo: 15, maximo: 20 },
  megasena: { minimo: 6, maximo: 20 },
  quina: { minimo: 5, maximo: 15 },
};

export type SituacaoQuantidade =
  | { readonly tipo: 'vazia' }
  | { readonly tipo: 'faltam'; readonly faltam: number }
  | { readonly tipo: 'valida' }
  | { readonly tipo: 'excede'; readonly sobram: number };

/** Só `valida` libera a conferência. */
export function avaliarQuantidade(slug: Slug, marcadas: number): SituacaoQuantidade {
  const { minimo, maximo } = QUANTIDADE_JOGO[slug];
  if (marcadas <= 0) return { tipo: 'vazia' };
  if (marcadas < minimo) return { tipo: 'faltam', faltam: minimo - marcadas };
  if (marcadas > maximo) return { tipo: 'excede', sobram: marcadas - maximo };
  return { tipo: 'valida' };
}

/** Alterna uma dezena no jogo: mantém a ordem crescente e respeita o máximo da loteria. */
export function alternarDezenaDoJogo(
  slug: Slug,
  jogo: readonly number[],
  dezena: number,
): number[] {
  if (jogo.includes(dezena)) return jogo.filter((d) => d !== dezena);
  if (jogo.length >= QUANTIDADE_JOGO[slug].maximo) return [...jogo];
  return [...jogo, dezena].sort((a, b) => a - b);
}

// ---------------------------------------------------------------- conferência

export interface MelhorResultado {
  readonly acertos: number;
  /** Concursos em que esse número de acertos ocorreu, do mais recente ao mais antigo. */
  readonly concursos: readonly number[];
}

export interface ResultadoConferencia {
  readonly totalConcursos: number;
  /**
   * `porAcertos[k]` = quantos concursos tiveram exatamente k acertos. Vai de 0 até
   * min(dezenas distintas do jogo, dezenas sorteadas por concurso).
   */
  readonly porAcertos: readonly number[];
  /**
   * Menor número de acertos possível: max(0, distintas + sorteadas − total). Abaixo disso
   * a faixa é impossível (ex.: 15 dezenas na Lotofácil acertam ao menos 5) e não é listada.
   */
  readonly minimoAcertos: number;
  /** `null` se não há concursos. */
  readonly melhor: MelhorResultado | null;
}

/** Tabela de presença do jogo (dezenas fora de 1..total ou repetidas são ignoradas). */
function marcarJogo(
  jogo: readonly number[],
  total: number,
): { marca: Uint8Array; distintas: number } {
  const marca = new Uint8Array(total + 1);
  let distintas = 0;
  for (const d of jogo) {
    if (Number.isInteger(d) && d >= 1 && d <= total && marca[d] === 0) {
      marca[d] = 1;
      distintas++;
    }
  }
  return { marca, distintas };
}

/** Quantas dezenas sorteadas estão na tabela de presença. */
function contarAcertos(marca: Uint8Array, sorteadas: readonly number[]): number {
  let acertos = 0;
  for (let j = 0; j < sorteadas.length; j++) {
    if (marca[sorteadas[j] as number] === 1) acertos++;
  }
  return acertos;
}

/**
 * Confere o jogo contra todos os concursos, numa passada e sem alocar nada por
 * concurso (a tabela de presença é única; o custo é o total de dezenas sorteadas).
 * Empate no melhor resultado: guarda todos os concursos, do mais recente ao mais antigo.
 */
export function conferirJogo(
  slug: Slug,
  jogo: readonly number[],
  concursos: readonly ConcursoHistorico[],
): ResultadoConferencia {
  const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[slug];
  const { marca, distintas } = marcarJogo(jogo, totalDezenas);
  const porAcertos = new Array<number>(Math.min(distintas, dezenasSorteadas) + 1).fill(0);
  let melhorAcertos = -1;
  let melhores: number[] = [];

  for (let i = 0; i < concursos.length; i++) {
    const concurso = concursos[i] as ConcursoHistorico;
    // Índice fora da tabela (dezena inválida no arquivo) lê `undefined`, que não é 1.
    const acertos = contarAcertos(marca, concurso[2]);
    const faixa = Math.min(acertos, porAcertos.length - 1);
    porAcertos[faixa] = (porAcertos[faixa] as number) + 1;
    if (acertos > melhorAcertos) {
      melhorAcertos = acertos;
      melhores = [concurso[0]];
    } else if (acertos === melhorAcertos) {
      melhores.push(concurso[0]);
    }
  }

  return {
    totalConcursos: concursos.length,
    porAcertos,
    minimoAcertos: Math.max(0, distintas + dezenasSorteadas - totalDezenas),
    melhor: melhorAcertos < 0 ? null : { acertos: melhorAcertos, concursos: melhores.reverse() },
  };
}

/**
 * Concursos com exatamente `acertos` acertos, do mais recente ao mais antigo
 * (até `limite`). Segunda passada, só quando a tela precisa listar uma faixa.
 */
export function concursosComAcertos(
  slug: Slug,
  jogo: readonly number[],
  concursos: readonly ConcursoHistorico[],
  acertos: number,
  limite = Infinity,
): number[] {
  const { marca } = marcarJogo(jogo, PARAMETROS_LOTERIA[slug].totalDezenas);
  const saida: number[] = [];
  for (let i = concursos.length - 1; i >= 0 && saida.length < limite; i--) {
    const concurso = concursos[i] as ConcursoHistorico;
    if (contarAcertos(marca, concurso[2]) === acertos) saida.push(concurso[0]);
  }
  return saida;
}

/** Acertos do jogo no concurso mais recente do histórico (o último do arquivo); `null` se vazio. */
export function acertosNoUltimoConcurso(
  slug: Slug,
  jogo: readonly number[],
  concursos: readonly ConcursoHistorico[],
): { concurso: number; acertos: number } | null {
  const ultimo = concursos[concursos.length - 1];
  if (ultimo === undefined) return null;
  const { marca } = marcarJogo(jogo, PARAMETROS_LOTERIA[slug].totalDezenas);
  return { concurso: ultimo[0], acertos: contarAcertos(marca, ultimo[2]) };
}

export interface FaixaDeAcertos {
  readonly acertos: number;
  readonly concursos: number;
  /** 0..1, em relação à faixa mais cheia (largura da barra). */
  readonly proporcao: number;
}

/** Faixas do maior número de acertos para o menor (sem as impossíveis), como a tela as lista. */
export function faixasDoResultado(resultado: ResultadoConferencia): FaixaDeAcertos[] {
  const maior = Math.max(0, ...resultado.porAcertos);
  const faixas: FaixaDeAcertos[] = [];
  for (let k = resultado.porAcertos.length - 1; k >= resultado.minimoAcertos; k--) {
    const concursos = resultado.porAcertos[k] as number;
    faixas.push({ acertos: k, concursos, proporcao: maior === 0 ? 0 : concursos / maior });
  }
  return faixas;
}

// ---------------------------------------------------------------- visão da tela

export type VisaoConferir =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      /** "Quina · escolha de 5 a 15 dezenas". */
      readonly subtitulo: string;
      readonly estado: EstadoTela;
      readonly texto: string;
      readonly erro: boolean;
      readonly concursos: readonly ConcursoHistorico[];
    };

export function montarVisaoConferir(
  slug: Slug,
  situacao: SituacaoTela | null,
  agora: Date = new Date(),
): VisaoConferir {
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const historico = entrada?.dados.historico ?? null;

  if (situacao.cache === null || historico === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.falha === 'recusado') {
      return { tipo: 'sem_dados', texto: textos.conferir.recusadoSemAnterior, erro: true };
    }
    if (situacao.falha === 'nenhuma' && situacao.cache !== null && entrada === null) {
      return { tipo: 'sem_dados', texto: textos.conferir.semLoteria, erro: false };
    }
    return { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  const { minimo, maximo } = QUANTIDADE_JOGO[slug];
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.conferir.subtitulo(textos.loterias[slug], minimo, maximo),
    estado,
    texto,
    erro: estado === 'recusado',
    concursos: historico.concursos,
  };
}
