// Roteamento mínimo por hash: #/inicio, #/forca, #/conferir, #/estatisticas,
// #/entenda. Funções puras (sem window), para testar sem navegador.
import { SLUGS, type Slug } from '../dados/tipos';
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

/** Hash da Força com a loteria escolhida ("#/forca?loteria=quina"). */
export function hashDaForca(slug: Slug): string {
  return `${hashDaRota('forca')}?loteria=${slug}`;
}
