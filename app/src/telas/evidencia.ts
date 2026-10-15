// Lógica da seção "Por evidência" das Estatísticas, sem Svelte: agrupa as dezenas da
// tabela_geral.json pelo grau e pela direção de evidência que o MOTOR já calculou. O app
// não recalcula nada: posição, variação e evidência vêm prontas; aqui só se reordena.
import {
  DIRECOES_EVIDENCIA,
  GRAUS_EVIDENCIA,
  type DirecaoEvidencia,
  type GrauEvidencia,
  type Slug,
  type TabelaGeralV1,
} from '../dados/tipos';
import { rotuloEvidenciaDirecao, rotuloEvidenciaGrau, textos } from '../lib/textos';
import { estadoEData, type EstadoTela, type SituacaoTela } from './estado-tela';
import { linhasDaTabela, type LinhaForca } from './forca';

export interface GrupoEvidencia {
  /** "fraca|vies"; "sem" para dezenas sem grau ou sem direção. Estável, serve de chave. */
  readonly chave: string;
  readonly grau: GrauEvidencia | null;
  readonly direcao: DirecaoEvidencia | null;
  /** "Fraca · viés" (igual ao da Força) ou "Sem evidência calculada". */
  readonly rotulo: string;
  /** Em ordem de posição (1 = mais forte). */
  readonly dezenas: readonly LinhaForca[];
}

export const CHAVE_SEM_EVIDENCIA = 'sem';

export function rotuloDoGrupo(
  grau: GrauEvidencia | null,
  direcao: DirecaoEvidencia | null,
): string {
  return grau === null || direcao === null
    ? textos.evidencia.grupo.semEvidencia
    : `${rotuloEvidenciaGrau(grau)} · ${rotuloEvidenciaDirecao(direcao)}`;
}

/**
 * Agrupa por grau x direção. Ordem dos grupos: grau do mais forte ao mais fraco; dentro do
 * grau, "viés" antes de "acaso"; "Sem evidência calculada" por último. Só existem grupos
 * com pelo menos uma dezena. Dentro de cada grupo vale a ordem de posição, e uma dezena
 * aparece em exatamente um grupo. Grau ou direção nulos nunca são inventados.
 */
export function agruparPorEvidencia(linhas: readonly LinhaForca[]): GrupoEvidencia[] {
  const ordenadas = [...linhas].sort((a, b) => a.posicao - b.posicao || a.dezena - b.dezena);
  const grupos: GrupoEvidencia[] = [];
  const graus = [...GRAUS_EVIDENCIA].reverse();
  for (const grau of graus) {
    for (const direcao of DIRECOES_EVIDENCIA) {
      const dezenas = ordenadas.filter((l) => l.grau === grau && l.direcao === direcao);
      if (dezenas.length === 0) continue;
      grupos.push({
        chave: `${grau}|${direcao}`,
        grau,
        direcao,
        rotulo: rotuloDoGrupo(grau, direcao),
        dezenas,
      });
    }
  }
  const sem = ordenadas.filter((l) => l.grau === null || l.direcao === null);
  if (sem.length > 0) {
    grupos.push({
      chave: CHAVE_SEM_EVIDENCIA,
      grau: null,
      direcao: null,
      rotulo: rotuloDoGrupo(null, null),
      dezenas: sem,
    });
  }
  return grupos;
}

/** Dezenas nos graus "muito fraca" ou "fraca" (qualquer direção), contadas nos grupos. */
export function contarGrausFracos(grupos: readonly GrupoEvidencia[]): number {
  return grupos
    .filter((g) => g.grau === 'muito_fraca' || g.grau === 'fraca')
    .reduce((soma, g) => soma + g.dezenas.length, 0);
}

export type VisaoEvidencia =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      /** "Próximo concurso N, com dados até o M" (igual ao da Força). */
      readonly subtitulo: string;
      /** Perfil e janela do motor (não são a janela do seletor de Frequência). */
      readonly perfilJanela: string;
      readonly estado: EstadoTela;
      readonly texto: string;
      /** Dado recusado: a linha de estado vira Aviso de erro (a tabela anterior continua abaixo). */
      readonly erro: boolean;
      readonly total: number;
      readonly grupos: readonly GrupoEvidencia[];
      /** Dezenas nos graus muito fraca ou fraca. */
      readonly fracos: number;
      /** Tabela validada, de onde sai o detalhe da dezena. */
      readonly tabela: TabelaGeralV1;
    };

export function montarVisaoEvidencia(
  slug: Slug,
  situacao: SituacaoTela | null,
  agora: Date = new Date(),
): VisaoEvidencia {
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const tabela = entrada?.dados.tabela_geral ?? null;

  if (situacao.cache === null || tabela === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.falha === 'recusado') {
      return { tipo: 'sem_dados', texto: textos.evidencia.recusadoSemAnterior, erro: true };
    }
    if (situacao.falha === 'nenhuma' && situacao.cache !== null && entrada === null) {
      return { tipo: 'sem_dados', texto: textos.evidencia.semLoteria, erro: false };
    }
    return { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  const grupos = agruparPorEvidencia(linhasDaTabela(tabela));
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.forca.subtitulo(tabela.concurso, tabela.ultimoSorteado),
    perfilJanela: textos.evidencia.perfilJanela(tabela.perfil, tabela.janela),
    estado,
    texto,
    erro: estado === 'recusado',
    total: tabela.dezenas.length,
    grupos,
    fracos: contarGrausFracos(grupos),
    tabela,
  };
}
