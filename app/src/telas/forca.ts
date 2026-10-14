// Lógica da tela Força, sem Svelte: converte o que há em cache (e o resultado da
// sincronização) no que a tela mostra. O app NÃO recalcula nada do motor: posição,
// índice, variação e evidência vêm prontos de tabela_geral.json. Testada sem navegador.
import type { ResultadoCache } from '../dados/cliente';
import { formatarDataHora } from '../dados/estado-sincronizacao';
import type { FalhaDaGarantia } from '../dados/sob-demanda';
import type { DirecaoEvidencia, GrauEvidencia, Slug, TabelaGeralV1 } from '../dados/tipos';
import { textos } from '../lib/textos';

export interface LinhaForca {
  readonly posicao: number;
  readonly dezena: number;
  /** Posições ganhas (+) ou perdidas (−); `null` = sem concurso anterior. */
  readonly variacao: number | null;
  /** `total` do contrato (índice de força), valor bruto: a tela formata. */
  readonly indice: number;
  readonly grau: GrauEvidencia | null;
  readonly direcao: DirecaoEvidencia | null;
}

/** O que a tela sabe: o cache lido e em que ponto da sincronização está. */
export interface SituacaoForca {
  readonly cache: ResultadoCache | null;
  /** `true` enquanto `garantir` ainda não respondeu. */
  readonly sincronizando: boolean;
  /** Resultado de `garantir` (só vale com `sincronizando` falso). */
  readonly falha: FalhaDaGarantia;
}

export type EstadoForca = 'sincronizando' | 'atualizado' | 'sem_rede' | 'recusado';

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
      readonly estado: EstadoForca;
      /** Texto da linha de estado, com a data do arquivo quando não está atualizado. */
      readonly texto: string;
      /** Dado recusado: a linha de estado vira Aviso de erro (a tabela anterior continua abaixo). */
      readonly erro: boolean;
      readonly linhas: readonly LinhaForca[];
      /** Posição de cada dezena (volante). */
      readonly posicoes: Readonly<Record<number, number>>;
      /** Tabela validada, de onde sai o detalhe da dezena. */
      readonly tabela: TabelaGeralV1;
    };

/** Linhas em ordem de posição (1 = mais forte); não confia na ordem do arquivo. */
export function linhasDaTabela(tabela: TabelaGeralV1): LinhaForca[] {
  return tabela.dezenas
    .map((d) => ({
      posicao: d.posicao,
      dezena: d.dezena,
      variacao: d.variacaoPosicao,
      indice: d.total,
      grau: d.evidenciaGrau,
      direcao: d.evidenciaDirecao,
    }))
    .sort((a, b) => a.posicao - b.posicao || a.dezena - b.dezena);
}

/** Dezena -> posição no ranking, como o volante a recebe. */
export function posicoesDaTabela(tabela: TabelaGeralV1): Record<number, number> {
  return Object.fromEntries(tabela.dezenas.map((d) => [d.dezena, d.posicao]));
}

function estadoEData(
  situacao: SituacaoForca,
  cache: ResultadoCache,
  agora: Date,
): { estado: EstadoForca; texto: string } {
  const iso = cache.sincronizadoEm ?? cache.indiceSincronizadoEm;
  const data = iso === null ? null : formatarDataHora(iso);
  if (situacao.sincronizando) {
    return { estado: 'sincronizando', texto: textos.sincronizacao.sincronizando(data) };
  }
  if (situacao.falha === 'recusado') {
    return { estado: 'recusado', texto: textos.sincronizacao.recusado(data) };
  }
  if (situacao.falha === 'sem_rede') {
    return { estado: 'sem_rede', texto: textos.sincronizacao.semRede(data) };
  }
  return {
    estado: 'atualizado',
    texto: textos.sincronizacao.atualizado(data ?? formatarDataHora(agora.toISOString()) ?? ''),
  };
}

export function montarVisaoForca(
  slug: Slug,
  situacao: SituacaoForca | null,
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
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.forca.subtitulo(tabela.concurso, tabela.ultimoSorteado),
    perfilJanela: textos.forca.perfilJanela(tabela.perfil, tabela.janela),
    estado,
    texto,
    erro: estado === 'recusado',
    linhas: linhasDaTabela(tabela),
    posicoes: posicoesDaTabela(tabela),
    tabela,
  };
}
