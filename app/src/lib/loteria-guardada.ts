// Última loteria escolhida nas telas Força e Histórico (compartilhada), guardada
// no aparelho (localStorage). A chave mantém o nome antigo 'forca.loteria'.
// Tolerante a falha: modo privado ou armazenamento bloqueado só significam que
// a escolha não é lembrada. A URL (#/forca?loteria=... ou #/conferir?loteria=...) tem precedência.
import type { Slug } from '../dados/tipos';
import { ehExibicao, ehOrdem, ehSlug, type ExibicaoForca, type OrdemForca } from './rotas';
import { armazenamentoLocal, type EscritorArmazenamento, type LeitorArmazenamento } from './tema';

export const CHAVE_LOTERIA_GUARDADA = 'forca.loteria';

export function lerLoteriaGuardada(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): Slug | null {
  try {
    const valor = armazenamento?.getItem(CHAVE_LOTERIA_GUARDADA);
    return ehSlug(valor) ? valor : null;
  } catch {
    return null;
  }
}

/** Devolve `false` se não foi possível guardar (o app segue funcionando). */
export function gravarLoteriaGuardada(
  slug: Slug,
  armazenamento: Pick<EscritorArmazenamento, 'setItem'> | null = armazenamentoLocal(),
): boolean {
  try {
    if (!armazenamento) return false;
    armazenamento.setItem(CHAVE_LOTERIA_GUARDADA, slug);
    return true;
  } catch {
    return false;
  }
}

export const CHAVE_EXIBICAO_FORCA = 'forca.visao';

/** Última forma de exibição (lista ou volante); ausente, inválida ou bloqueada = `null`. */
export function lerExibicaoGuardada(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): ExibicaoForca | null {
  try {
    const valor = armazenamento?.getItem(CHAVE_EXIBICAO_FORCA);
    return ehExibicao(valor) ? valor : null;
  } catch {
    return null;
  }
}

/** Devolve `false` se não foi possível guardar (o app segue funcionando). */
export function gravarExibicaoGuardada(
  exibicao: ExibicaoForca,
  armazenamento: Pick<EscritorArmazenamento, 'setItem'> | null = armazenamentoLocal(),
): boolean {
  try {
    if (!armazenamento) return false;
    armazenamento.setItem(CHAVE_EXIBICAO_FORCA, exibicao);
    return true;
  } catch {
    return false;
  }
}

export const CHAVE_ORDEM_FORCA = 'forca.ordem';

/** Última ordem da lista da Força (força ou G-1); ausente, inválida ou bloqueada = `null`. */
export function lerOrdemGuardada(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): OrdemForca | null {
  try {
    const valor = armazenamento?.getItem(CHAVE_ORDEM_FORCA);
    return ehOrdem(valor) ? valor : null;
  } catch {
    return null;
  }
}

/** Devolve `false` se não foi possível guardar (o app segue funcionando). */
export function gravarOrdemGuardada(
  ordem: OrdemForca,
  armazenamento: Pick<EscritorArmazenamento, 'setItem'> | null = armazenamentoLocal(),
): boolean {
  try {
    if (!armazenamento) return false;
    armazenamento.setItem(CHAVE_ORDEM_FORCA, ordem);
    return true;
  } catch {
    return false;
  }
}
