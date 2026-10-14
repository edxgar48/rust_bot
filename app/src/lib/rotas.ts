// Roteamento mínimo por hash: #/inicio, #/forca, #/conferir, #/estatisticas,
// #/entenda. Funções puras (sem window), para testar sem navegador.
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
