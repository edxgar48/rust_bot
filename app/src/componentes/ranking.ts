// Formatação de itens do ranking (puro).
import type { GrauEvidencia } from '../dados/tipos';
import { GRAUS_EVIDENCIA } from '../dados/tipos';

export type SentidoVariacao = 'subiu' | 'caiu' | 'igual' | 'sem';

const MENOS = '−'; // sinal de menos tipográfico

export function sentidoVariacao(variacao: number | null): SentidoVariacao {
  if (variacao === null) return 'sem';
  if (variacao > 0) return 'subiu';
  if (variacao < 0) return 'caiu';
  return 'igual';
}

/** +3, −2, 0; `null` (sem concurso anterior) vira travessão. */
export function formatarVariacao(variacao: number | null): string {
  if (variacao === null) return '—';
  if (variacao > 0) return `+${variacao}`;
  if (variacao < 0) return `${MENOS}${Math.abs(variacao)}`;
  return '0';
}

/** Índice de força com 3 casas, vírgula decimal e menos tipográfico. */
export function formatarIndice(indice: number): string {
  const texto = new Intl.NumberFormat('pt-BR', {
    minimumFractionDigits: 3,
    maximumFractionDigits: 3,
  }).format(indice);
  return texto.replace('-', MENOS);
}

/** Quantos dos 5 segmentos acendem para o grau (muito_fraca = 1 ... muito_forte = 5). */
export function segmentosDoGrau(grau: GrauEvidencia | null): number {
  return grau === null ? 0 : GRAUS_EVIDENCIA.indexOf(grau) + 1;
}
