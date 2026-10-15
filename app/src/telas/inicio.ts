// Lógica da tela Início, sem Svelte: converte a situação dos dados (abertura.ts)
// no que a tela mostra. Testada sem navegador.
import type { SituacaoDados } from '../dados/abertura';
import type { TipoEstadoSincronizacao } from '../dados/estado-sincronizacao';
import { formatarDataSorteio } from '../lib/datas';
import { SLUGS, type Slug } from '../dados/tipos';
import { FAIXA_PREMIO, type NovidadeLoteria } from '../jogos/conferencia-automatica';
import { hashDosJogos } from '../lib/rotas';
import { textos } from '../lib/textos';

export interface CartaoInicio {
  readonly slug: Slug;
  readonly nome: string;
  /** `sem_loteria`: o índice não tem esta loteria; `sem_dezenas`: há concurso no índice, mas o ultimo.json não está guardado. */
  readonly tipo: 'sem_loteria' | 'sem_dezenas' | 'resultado';
  readonly concurso: number | null;
  /** dd/mm/aaaa. */
  readonly data: string | null;
  readonly dezenas: readonly number[];
  /** Lotofácil: 15 bolas em grade de 5 colunas; Mega-Sena e Quina: em linha. */
  readonly disposicao: 'grade' | 'linha';
}

export type VisaoInicio =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string }
  | {
      readonly tipo: 'conteudo';
      /** Estado de sincronização por trás da linha de estado. */
      readonly estado: TipoEstadoSincronizacao;
      readonly texto: string;
      /** Dado recusado: a linha de estado vira Aviso de erro (os dados anteriores continuam abaixo). */
      readonly erro: boolean;
      readonly cartoes: readonly CartaoInicio[];
    };

function montarCartao(slug: Slug, situacao: SituacaoDados): CartaoInicio {
  const nome = textos.loterias[slug];
  const disposicao = slug === 'lotofacil' ? 'grade' : 'linha';
  const entrada = situacao.cache?.indice.loterias[slug] ?? null;
  const ultimo = situacao.cache?.loterias[slug]?.dados.ultimo ?? null;
  if (ultimo) {
    // Concurso, data e dezenas vêm do MESMO arquivo: se o ultimo.json guardado
    // é mais antigo que o índice (atualização recusada), não se mistura.
    return {
      slug,
      nome,
      tipo: 'resultado',
      concurso: ultimo.concurso,
      data: formatarDataSorteio(ultimo.data),
      dezenas: ultimo.dezenas,
      disposicao,
    };
  }
  if (entrada) {
    return {
      slug,
      nome,
      tipo: 'sem_dezenas',
      concurso: entrada.ultimoConcurso,
      data: formatarDataSorteio(entrada.data),
      dezenas: [],
      disposicao,
    };
  }
  return { slug, nome, tipo: 'sem_loteria', concurso: null, data: null, dezenas: [], disposicao };
}

export function montarVisaoInicio(situacao: SituacaoDados | null): VisaoInicio {
  // Ainda não leu o cache, ou não há cache e a primeira sincronização roda.
  if (situacao === null) return { tipo: 'carregando' };
  if (situacao.cache === null) {
    return situacao.estado.tipo === 'sem_dados'
      ? { tipo: 'sem_dados', texto: situacao.estado.texto }
      : { tipo: 'carregando' };
  }
  return {
    tipo: 'conteudo',
    estado: situacao.estado.tipo,
    texto: situacao.estado.texto,
    erro: situacao.estado.tipo === 'recusado',
    cartoes: SLUGS.map((slug) => montarCartao(slug, situacao)),
  };
}

/** Aviso discreto "Concurso N saiu: seus jogos salvos" (6.4), já com os textos prontos. */
export interface AvisoNovidade {
  readonly slug: Slug;
  readonly titulo: string;
  /** "2 jogos conferidos; melhor: 13 acertos"; `null` se nenhum jogo pôde ser conferido ainda. */
  readonly resumo: string | null;
  /** Quando há mais de um concurso novo. */
  readonly concursos: string | null;
  /** Jogos que atingiram a faixa com prêmio; `null` se nenhum. */
  readonly faixa: string | null;
  /** Jogos que esperam o histórico; `null` se nenhum. */
  readonly pendente: string | null;
  /** Link para Meus jogos da loteria. */
  readonly href: string;
}

/** Um aviso por loteria com novidade (nada se não há jogos ou concurso novo), na ordem das loterias. */
export function montarAvisosNovidade(novidades: readonly NovidadeLoteria[]): AvisoNovidade[] {
  return SLUGS.flatMap((slug) => {
    const n = novidades.find((x) => x.slug === slug);
    if (!n || n.jogos.length === 0) return [];
    return [
      {
        slug,
        titulo: textos.novidade.titulo(textos.loterias[slug], n.ultimoConcurso),
        resumo:
          n.conferidos > 0 && n.melhorAcertos !== null
            ? textos.novidade.resumo(n.conferidos, n.melhorAcertos)
            : null,
        concursos: n.concursosNovos > 1 ? textos.novidade.concursosNovos(n.concursosNovos) : null,
        faixa:
          n.naFaixaPremio > 0 ? textos.novidade.faixa(n.naFaixaPremio, FAIXA_PREMIO[slug]) : null,
        pendente: n.pendentes > 0 ? textos.novidade.pendente(n.pendentes) : null,
        href: hashDosJogos(slug),
      },
    ];
  });
}
