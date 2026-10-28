// Lógica da seção "Taxa histórica" das Estatísticas, sem Svelte: mostra o ranking_taxa.json
// como veio. O app não recalcula nada: notas, diferenças e posições vêm prontas; aqui só se
// ordena (pela Posição A, que o contrato define como a ordem pela Nota A) e se formata.
// Sem grau de evidência e sem faixa de componente: o arquivo não os traz.
import type { RankingTaxaV1, Slug } from '../dados/tipos';
import { formatarDecimal, formatarDiferenca } from '../lib/numeros';
import { textos } from '../lib/textos';
import { estadoEData, type EstadoTela, type SituacaoTela } from './estado-tela';

export interface LinhaRanking {
  readonly dezena: number;
  readonly posicaoA: number;
  readonly notaA: string;
  /** Com sinal ("+0,0178", "−0,0098", menos tipográfico); zero sem sinal. */
  readonly diferenca: string;
  readonly posicaoIndice: number;
  readonly posicaoB: number;
  readonly notaB: string;
}

const CASAS = 4;

/** Nota a 4 casas ("0,5902"), pelo formatador comum do app. */
const formatarNota = (n: number): string => formatarDecimal(n, CASAS);
const formatarDif = (n: number): string => formatarDiferenca(n, CASAS);

/** Dezenas na ordem do ranking A (Posição A crescente); a posição do contrato manda, não a nota. */
export function linhasDoRanking(ranking: RankingTaxaV1): LinhaRanking[] {
  return [...ranking.dezenas]
    .sort((a, b) => a.posicaoA - b.posicaoA)
    .map((d) => ({
      dezena: d.dezena,
      posicaoA: d.posicaoA,
      notaA: formatarNota(d.notaA),
      diferenca: formatarDif(d.diferencaAcaso),
      posicaoIndice: d.posicaoIndice,
      posicaoB: d.posicaoB,
      notaB: formatarNota(d.notaB),
    }));
}

export type VisaoRankingTaxa =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  /** O index.json não lista o arquivo: não foi publicado (não é erro). */
  | { readonly tipo: 'nao_publicado'; readonly nomeLoteria: string; readonly texto: string }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      readonly subtitulo: string;
      readonly perfilJanela: string;
      readonly taxaAcaso: string;
      readonly estado: EstadoTela;
      readonly texto: string;
      /** Dado recusado: a linha de estado vira Aviso de erro (o ranking anterior continua). */
      readonly erro: boolean;
      readonly linhas: readonly LinhaRanking[];
      readonly total: number;
    };

export function montarVisaoRankingTaxa(
  slug: Slug,
  situacao: SituacaoTela | null,
  agora: Date = new Date(),
): VisaoRankingTaxa {
  const t = textos.rankingTaxa;
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const ranking = entrada?.dados.ranking_taxa ?? null;

  if (situacao.cache === null || entrada === null || ranking === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.cache === null) {
      return situacao.falha === 'recusado'
        ? { tipo: 'sem_dados', texto: t.recusadoSemAnterior, erro: true }
        : { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
    }
    if (entrada === null) {
      return situacao.falha === 'nenhuma'
        ? { tipo: 'sem_dados', texto: t.semLoteria, erro: false }
        : { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
    }
    // Sem a chave no índice: não publicado. Só afirma isso se o índice acabou de ser conferido.
    if (!entrada.indice.arquivos.ranking_taxa) {
      if (situacao.falha === 'nenhuma') {
        return { tipo: 'nao_publicado', nomeLoteria: textos.loterias[slug], texto: t.naoPublicado };
      }
      return situacao.falha === 'recusado'
        ? { tipo: 'sem_dados', texto: t.recusadoSemAnterior, erro: true }
        : { tipo: 'sem_dados', texto: t.semRedeSemAnterior, erro: true };
    }
    // O índice lista o arquivo, mas não há ranking guardado.
    return {
      tipo: 'sem_dados',
      texto:
        situacao.falha === 'recusado'
          ? t.recusadoSemAnterior
          : situacao.falha === 'sem_rede'
            ? t.semRedeSemAnterior
            : textos.sincronizacao.semDados,
      erro: true,
    };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.forca.subtitulo(ranking.concurso, ranking.ultimoSorteado),
    perfilJanela: t.perfilJanela(ranking.perfil, ranking.janela),
    taxaAcaso: formatarNota(ranking.taxaAcaso),
    estado,
    texto,
    erro: estado === 'recusado',
    linhas: linhasDoRanking(ranking),
    total: ranking.dezenas.length,
  };
}
