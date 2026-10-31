// Lógica da tela Força, sem Svelte: converte o que há em cache (e o resultado da
// sincronização) no que a tela mostra. O app NÃO recalcula nada do motor: posição,
// índice, variação e evidência vêm prontos de tabela_geral.json. Testada sem navegador.
import type {
  DirecaoEvidencia,
  GrauEvidencia,
  RankingPosicoesV1,
  Slug,
  TabelaGeralV1,
  UltimoV1,
} from '../dados/tipos';
import type { OrdemForca } from '../lib/rotas';
import { textos } from '../lib/textos';
import { estadoEData, type EstadoTela, type SituacaoTela } from './estado-tela';

export interface LinhaForca {
  readonly posicao: number;
  readonly dezena: number;
  /** Posições ganhas (+) ou perdidas (−); `null` = sem concurso anterior. */
  readonly variacao: number | null;
  /** `total` do contrato (índice de força), valor bruto: a tela formata. */
  readonly indice: number;
  readonly grau: GrauEvidencia | null;
  readonly direcao: DirecaoEvidencia | null;
  /** G-1 da dezena (ranking_posicoes.json); `null` = sem G-1 (arquivo ausente ou de outro snapshot). */
  readonly g1?: number | null;
  /** Variação da dezena no G-1; `null`/ausente = sem G-1 ou sem concurso anterior. */
  readonly variacaoG1?: number | null;
}

/** G-1 de uma dezena, como veio do arquivo. */
export interface G1DaDezena {
  readonly g1: number;
  readonly variacao: number | null;
}

export type VisaoForca =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      /** "Próximo concurso N, com dados até o M". */
      readonly subtitulo: string;
      /** "Perfil padrao_v1 · janela de 50 concursos". */
      readonly perfilJanela: string;
      readonly estado: EstadoTela;
      /** Texto da linha de estado, com a data do arquivo quando não está atualizado. */
      readonly texto: string;
      /** Dado recusado: a linha de estado vira Aviso de erro (a tabela anterior continua abaixo). */
      readonly erro: boolean;
      readonly linhas: readonly LinhaForca[];
      /** Posição de cada dezena (volante). */
      readonly posicoes: Readonly<Record<number, number>>;
      /** Posições (do ranking atual) que o último concurso sorteou; ver `posicoesSorteadasNoAnterior`. */
      readonly posicoesSorteadas: ReadonlySet<number>;
      /** Tabela validada, de onde sai o detalhe da dezena. */
      readonly tabela: TabelaGeralV1;
      /** G-1 por dezena; `null` = sem G-1 (some o selo, a chave de ordem e o bloco do detalhe). */
      readonly g1: ReadonlyMap<number, G1DaDezena> | null;
    };

/**
 * Junção por dezena entre tabela_geral.json e ranking_posicoes.json. Só vale se os dois são do MESMO
 * snapshot (mesmo próximo concurso e mesma posição de cada dezena); do contrário devolve `null` e a
 * tela segue sem G-1, em vez de misturar dois concursos (por exemplo, a tabela atualizou e o G-1
 * guardado é o do concurso anterior).
 */
export function juntarG1(
  tabela: TabelaGeralV1,
  ranking: RankingPosicoesV1 | null,
): ReadonlyMap<number, G1DaDezena> | null {
  if (ranking === null) return null;
  if (ranking.loteria !== tabela.loteria) return null;
  if (ranking.concurso !== tabela.concurso || ranking.ultimoSorteado !== tabela.ultimoSorteado) {
    return null;
  }
  if (ranking.dezenas.length !== tabela.dezenas.length) return null;
  const porDezena = new Map(ranking.dezenas.map((d) => [d.dezena, d]));
  const juncao = new Map<number, G1DaDezena>();
  for (const d of tabela.dezenas) {
    const g = porDezena.get(d.dezena);
    if (g === undefined || g.posicaoIndice !== d.posicao) return null;
    juncao.set(d.dezena, { g1: g.g1, variacao: g.variacaoG1 });
  }
  return juncao;
}

/** Linhas em ordem de posição (1 = mais forte); não confia na ordem do arquivo. */
export function linhasDaTabela(
  tabela: TabelaGeralV1,
  g1: ReadonlyMap<number, G1DaDezena> | null = null,
): LinhaForca[] {
  return tabela.dezenas
    .map((d) => ({
      posicao: d.posicao,
      dezena: d.dezena,
      variacao: d.variacaoPosicao,
      indice: d.total,
      grau: d.evidenciaGrau,
      direcao: d.evidenciaDirecao,
      // Sem G-1 a linha fica exatamente como antes (sem os campos).
      ...(g1 === null
        ? {}
        : { g1: g1.get(d.dezena)?.g1 ?? null, variacaoG1: g1.get(d.dezena)?.variacao ?? null }),
    }))
    .sort((a, b) => a.posicao - b.posicao || a.dezena - b.dezena);
}

/**
 * Reordena a lista. Por força: posição da tabela (1 = mais forte). Por G-1: lugar no índice das
 * posições (1 = o melhor); empate ou G-1 ausente desempata pela posição de força. Não altera a entrada.
 */
export function ordenarLinhas(linhas: readonly LinhaForca[], ordem: OrdemForca): LinhaForca[] {
  const copia = [...linhas];
  if (ordem === 'g1') {
    copia.sort(
      (a, b) =>
        (a.g1 ?? Number.POSITIVE_INFINITY) - (b.g1 ?? Number.POSITIVE_INFINITY) ||
        a.posicao - b.posicao ||
        a.dezena - b.dezena,
    );
  } else {
    copia.sort((a, b) => a.posicao - b.posicao || a.dezena - b.dezena);
  }
  return copia;
}

/** Dezena -> posição no ranking, como o volante a recebe. */
export function posicoesDaTabela(tabela: TabelaGeralV1): Record<number, number> {
  return Object.fromEntries(tabela.dezenas.map((d) => [d.dezena, d.posicao]));
}

/**
 * Posições da tabela atual que o último concurso sorteou: para cada dezena sorteada, a posição
 * que ela ocupava ANTES do sorteio (`posição atual + variação`). É a posição que sai, não a
 * dezena. Sem `ultimo` (ou de outro concurso) ou sem variação, não marca nada.
 */
export function posicoesSorteadasNoAnterior(
  tabela: TabelaGeralV1,
  ultimo: UltimoV1 | null,
): ReadonlySet<number> {
  const marcadas = new Set<number>();
  if (ultimo === null || ultimo.concurso !== tabela.ultimoSorteado) return marcadas;
  const sorteadas = new Set(ultimo.dezenas);
  for (const d of tabela.dezenas) {
    if (!sorteadas.has(d.dezena) || d.variacaoPosicao === null) continue;
    marcadas.add(d.posicao + d.variacaoPosicao);
  }
  return marcadas;
}

export function montarVisaoForca(
  slug: Slug,
  situacao: SituacaoTela | null,
  agora: Date = new Date(),
): VisaoForca {
  // Ainda não leu o cache.
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const tabela = entrada?.dados.tabela_geral ?? null;

  if (situacao.cache === null || tabela === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.falha === 'recusado') {
      return { tipo: 'sem_dados', texto: textos.forca.recusadoSemAnterior, erro: true };
    }
    if (situacao.falha === 'nenhuma' && situacao.cache !== null && entrada === null) {
      // O índice não tem esta loteria (tolerado pelo contrato).
      return { tipo: 'sem_dados', texto: textos.forca.semLoteria, erro: false };
    }
    return { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  const g1 = juntarG1(tabela, entrada?.dados.ranking_posicoes ?? null);
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.forca.subtitulo(tabela.concurso, tabela.ultimoSorteado),
    perfilJanela: textos.forca.perfilJanela(tabela.perfil, tabela.janela),
    estado,
    texto,
    erro: estado === 'recusado',
    linhas: linhasDaTabela(tabela, g1),
    posicoes: posicoesDaTabela(tabela),
    posicoesSorteadas: posicoesSorteadasNoAnterior(tabela, entrada?.dados.ultimo ?? null),
    tabela,
    g1,
  };
}
