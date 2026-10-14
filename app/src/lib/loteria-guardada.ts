// Última loteria escolhida na tela Força, guardada no aparelho (localStorage).
// Tolerante a falha: modo privado ou armazenamento bloqueado só significam que
// a escolha não é lembrada. A URL (#/forca?loteria=...) tem precedência.
import type { Slug } from '../dados/tipos';
import { ehSlug } from './rotas';
import { armazenamentoLocal, type EscritorArmazenamento, type LeitorArmazenamento } from './tema';

export const CHAVE_LOTERIA_FORCA = 'forca.loteria';

export function lerLoteriaGuardada(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): Slug | null {
  try {
    const valor = armazenamento?.getItem(CHAVE_LOTERIA_FORCA);
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
    armazenamento.setItem(CHAVE_LOTERIA_FORCA, slug);
    return true;
  } catch {
    return false;
  }
}
