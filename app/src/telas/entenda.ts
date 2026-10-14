// Lógica da tela Entenda, sem Svelte: converte o que há em cache (cabeçalho da
// tabela_geral de cada loteria, data da última atualização, último concurso) no
// que a tela mostra. Nada aqui é recalculado do motor: os valores vêm do arquivo.
import { formatarIndice } from '../componentes/ranking';
import type { SituacaoDados } from '../dados/abertura';
import { formatarDataHora } from '../dados/estado-sincronizacao';
import { SLUGS, type Slug, type TabelaGeralV1 } from '../dados/tipos';
import { SCHEMA_VERSION_SUPORTADA } from '../dados/config';
import { formatarDataSorteio } from '../lib/datas';
import { rotuloEvidenciaGrau, rotuloNormalizacao, textos } from '../lib/textos';

/** Quantos caracteres do hash_toml aparecem na tela (o completo fica um toque adiante). */
export const TAMANHO_HASH_ABREVIADO = 8;

export interface PerfilEntenda {
  readonly perfil: string;
  /** "histórico inteiro" ou "últimos 50 concursos". */
  readonly janela: string;
  readonly normalizacao: string;
  readonly featureVersion: number;
  readonly hashAbreviado: string;
  readonly hashCompleto: string;
}

export interface LoteriaEntenda {
  readonly slug: Slug;
  readonly nome: string;
  /** "3066 (03/10/2026)", "3066" sem data, ou `null` se nada se sabe. */
  readonly ultimoConcurso: string | null;
  /** `null` = tabela_geral ainda não está em cache. */
  readonly perfil: PerfilEntenda | null;
}

export interface VisaoEntenda {
  /** "14/10/2026 09:30", ou `null` se nunca atualizou. */
  readonly atualizadoEm: string | null;
  readonly formatoDados: number;
  readonly versaoApp: string;
  readonly atualizacaoDoApp: boolean;
  readonly loterias: readonly LoteriaEntenda[];
}

export interface EntradaEntenda {
  readonly situacao: SituacaoDados | null;
  /** tabela_geral de cada loteria já guardada no aparelho (`null`/ausente = ainda não baixada). */
  readonly tabelas: Readonly<Partial<Record<Slug, TabelaGeralV1 | null>>>;
  readonly versaoApp: string;
  readonly atualizacaoDoApp: boolean;
}

/** Primeiros caracteres do hash; hash curto demais volta inteiro. */
export function abreviarHash(hash: string, tamanho: number = TAMANHO_HASH_ABREVIADO): string {
  return hash.slice(0, tamanho);
}

export function textoJanela(janela: number): string {
  return janela === 0 ? textos.entenda.dados.janelaInteira : textos.entenda.dados.janela(janela);
}

function montarPerfil(tabela: TabelaGeralV1): PerfilEntenda {
  return {
    perfil: tabela.perfil,
    janela: textoJanela(tabela.janela),
    normalizacao: rotuloNormalizacao(tabela.normalizacao),
    featureVersion: tabela.featureVersion,
    hashAbreviado: abreviarHash(tabela.hashToml),
    hashCompleto: tabela.hashToml,
  };
}

function ultimoConcurso(
  slug: Slug,
  situacao: SituacaoDados | null,
  tabela: TabelaGeralV1 | null,
): string | null {
  const dados = situacao?.cache?.loterias[slug]?.dados.ultimo ?? null;
  if (dados)
    return textos.entenda.dados.ultimoConcurso(dados.concurso, formatarDataSorteio(dados.data));
  const entrada = situacao?.cache?.indice.loterias[slug] ?? null;
  if (entrada) {
    return textos.entenda.dados.ultimoConcurso(
      entrada.ultimoConcurso,
      formatarDataSorteio(entrada.data),
    );
  }
  // Sem ultimo.json nem índice, a própria tabela sabe qual foi o último sorteado (sem data).
  return tabela ? textos.entenda.dados.ultimoConcurso(tabela.ultimoSorteado, null) : null;
}

export function montarVisaoEntenda(entrada: EntradaEntenda): VisaoEntenda {
  const cache = entrada.situacao?.cache ?? null;
  const iso = cache ? (cache.sincronizadoEm ?? cache.indiceSincronizadoEm) : null;
  return {
    atualizadoEm: iso === null ? null : formatarDataHora(iso),
    formatoDados: SCHEMA_VERSION_SUPORTADA,
    versaoApp: entrada.versaoApp,
    atualizacaoDoApp: entrada.atualizacaoDoApp,
    loterias: SLUGS.map((slug) => {
      const tabela = entrada.tabelas[slug] ?? null;
      return {
        slug,
        nome: textos.loterias[slug],
        ultimoConcurso: ultimoConcurso(slug, entrada.situacao, tabela),
        perfil: tabela ? montarPerfil(tabela) : null,
      };
    }),
  };
}

/** Exemplo ilustrativo de linha da tabela (valores inventados, marcados como exemplo na tela). */
export const EXEMPLO_LINHA = {
  dezena: 7,
  posicao: 3,
  variacao: 2,
  indice: 0.85,
  grau: 'moderada',
  direcao: 'vies',
} as const;

export function textoDoExemplo(): string {
  const e = EXEMPLO_LINHA;
  return [
    textos.ranking.posicao(e.posicao),
    textos.ranking.subiu(e.variacao),
    `${textos.ranking.indice} ${formatarIndice(e.indice)}`,
    `${textos.ranking.evidencia}: ${rotuloEvidenciaGrau(e.grau)}`,
  ].join(' · ');
}
