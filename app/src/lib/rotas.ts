// Roteamento mínimo por hash: #/inicio, #/forca, #/conferir, #/estatisticas,
// #/entenda. Funções puras (sem window), para testar sem navegador.
import { SLUGS, type Slug } from '../dados/tipos';
import { JANELAS_ESTATISTICA, type JanelaEstatistica } from '../estatisticas/estatisticas';
import { idsAbas, type IdAba } from './textos';

export const ROTA_PADRAO: IdAba = 'inicio';

/** Hash da rota ("#/forca"). */
export function hashDaRota(rota: IdAba): string {
  return `#/${rota}`;
}

/**
 * Rota correspondente a um `location.hash`. Vazio, "#/", desconhecido ou
 * malformado cai na rota padrão. Barra final e parâmetros (`?x`) são ignorados.
 */
export function rotaDoHash(hash: string): IdAba {
  const m = /^#\/([^/?#]*)\/?(?:[?#].*)?$/.exec(hash);
  const nome = m?.[1] ?? '';
  return (idsAbas as readonly string[]).includes(nome) ? (nome as IdAba) : ROTA_PADRAO;
}

/** Loteria padrão das telas que têm seletor de loteria. */
export const LOTERIA_PADRAO: Slug = 'lotofacil';

export function ehSlug(valor: unknown): valor is Slug {
  return SLUGS.some((s) => s === valor);
}

/** Parâmetro `loteria` de um hash como "#/forca?loteria=quina"; ausente ou inválido = `null`. */
export function loteriaDoHash(hash: string): Slug | null {
  const m = /[?&]loteria=([^&#]*)/.exec(hash);
  const valor = m?.[1] ?? null;
  return ehSlug(valor) ? valor : null;
}

/** Formas de exibição da Força (alternador Lista/Volante). */
export const EXIBICOES_FORCA = ['lista', 'volante'] as const;
export type ExibicaoForca = (typeof EXIBICOES_FORCA)[number];
export const EXIBICAO_PADRAO: ExibicaoForca = 'lista';

export function ehExibicao(valor: unknown): valor is ExibicaoForca {
  return EXIBICOES_FORCA.some((e) => e === valor);
}

/** Parâmetro `visao` ("lista" ou "volante"); ausente ou inválido = `null`. */
export function exibicaoDoHash(hash: string): ExibicaoForca | null {
  const m = /[?&]visao=([^&#]*)/.exec(hash);
  const valor = m?.[1] ?? null;
  return ehExibicao(valor) ? valor : null;
}

/** Parâmetro `dezena` (inteiro positivo de até 3 dígitos); ausente ou inválido = `null`. */
export function dezenaDoHash(hash: string): number | null {
  const m = /[?&]dezena=(\d{1,3})(?:[&#]|$)/.exec(hash);
  const n = m?.[1] === undefined ? null : Number(m[1]);
  return n !== null && n >= 1 ? n : null;
}

export interface OpcoesHashForca {
  readonly visao?: ExibicaoForca;
  /** Dezena com o detalhe aberto. */
  readonly dezena?: number | null;
}

/**
 * Hash da Força: "#/forca?loteria=quina", com "&visao=volante" e "&dezena=20"
 * quando informados.
 */
export function hashDaForca(slug: Slug, opcoes: OpcoesHashForca = {}): string {
  let hash = `${hashDaRota('forca')}?loteria=${slug}`;
  if (opcoes.visao !== undefined) hash += `&visao=${opcoes.visao}`;
  if (opcoes.dezena !== undefined && opcoes.dezena !== null) hash += `&dezena=${opcoes.dezena}`;
  return hash;
}

/** Seções da aba Conferir: conferir um jogo (padrão), o histórico de concursos e os jogos salvos. */
export const SECOES_CONFERIR = ['jogo', 'historico', 'jogos'] as const;
export type SecaoConferir = (typeof SECOES_CONFERIR)[number];
export const SECAO_CONFERIR_PADRAO: SecaoConferir = 'jogo';

/** Parâmetro `secao` do hash; ausente ou inválido = Conferir jogo. */
export function secaoConferirDoHash(hash: string): SecaoConferir {
  const m = /[?&]secao=([^&#]*)/.exec(hash);
  const valor = m?.[1] ?? null;
  return SECOES_CONFERIR.find((s) => s === valor) ?? SECAO_CONFERIR_PADRAO;
}

/** Hash da aba Conferir na seção Conferir jogo: "#/conferir?loteria=quina". */
export function hashDoJogo(slug: Slug): string {
  return `${hashDaRota('conferir')}?loteria=${slug}`;
}

/** Hash da aba Conferir na seção Histórico: "#/conferir?loteria=quina&secao=historico". */
export function hashDoHistorico(slug: Slug): string {
  return `${hashDoJogo(slug)}&secao=historico`;
}

/** Hash da aba Conferir na seção Meus jogos: "#/conferir?loteria=quina&secao=jogos". */
export function hashDosJogos(slug: Slug): string {
  return `${hashDoJogo(slug)}&secao=jogos`;
}

export function hashDaSecaoConferir(secao: SecaoConferir, slug: Slug): string {
  if (secao === 'historico') return hashDoHistorico(slug);
  if (secao === 'jogos') return hashDosJogos(slug);
  return hashDoJogo(slug);
}

/** Janela padrão das Estatísticas: últimos 50 concursos. */
export const JANELA_ESTATISTICA_PADRAO: JanelaEstatistica = 50;

/** Parâmetro `janela` ("10", "50", "100" ou "total"); ausente ou inválido = `null`. */
export function janelaDoHash(hash: string): JanelaEstatistica | null {
  const m = /[?&]janela=([^&#]*)/.exec(hash);
  const valor = m?.[1] ?? null;
  return JANELAS_ESTATISTICA.find((j) => String(j) === valor) ?? null;
}

/** Hash das Estatísticas: "#/estatisticas?loteria=quina&janela=50". */
export function hashDasEstatisticas(slug: Slug, janela: JanelaEstatistica): string {
  return `${hashDaRota('estatisticas')}?loteria=${slug}&janela=${janela}`;
}
