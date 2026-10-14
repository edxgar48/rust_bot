// Foco preso numa folha modal (puro: recebe os elementos, não toca no documento).

export const SELETOR_FOCAVEIS =
  'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Para onde levar o foco quando Tab (ou Shift+Tab) sairia da folha: o primeiro
 * focável ao passar do último, o último ao recuar do primeiro. `null` = deixe o
 * navegador seguir a ordem normal (o foco continua dentro).
 */
export function destinoDoTab<T>(
  focaveis: readonly T[],
  ativo: T | null,
  recuar: boolean,
): T | null {
  const primeiro = focaveis[0];
  const ultimo = focaveis[focaveis.length - 1];
  if (primeiro === undefined || ultimo === undefined) return null;
  const dentro = ativo !== null && focaveis.includes(ativo);
  if (recuar) return !dentro || ativo === primeiro ? ultimo : null;
  return !dentro || ativo === ultimo ? primeiro : null;
}
