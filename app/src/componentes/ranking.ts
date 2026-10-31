// Formatação de itens do ranking (puro).
import type { GrauEvidencia } from '../dados/tipos';
import { GRAUS_EVIDENCIA } from '../dados/tipos';
import { textos } from '../lib/textos';

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

/** Símbolo de cada sentido (a cor só reforça; o texto falado e o sinal dizem o mesmo). */
export const SETAS_VARIACAO: Readonly<Record<SentidoVariacao, string>> = {
  subiu: '▲',
  caiu: '▼',
  igual: '■',
  sem: '',
};

/** "subiu 2 posições", "manteve a posição"... para leitor de tela. */
export function textoFaladoDaVariacao(variacao: number | null): string {
  const sentido = sentidoVariacao(variacao);
  if (sentido === 'subiu') return textos.ranking.subiu(variacao ?? 0);
  if (sentido === 'caiu') return textos.ranking.caiu(Math.abs(variacao ?? 0));
  if (sentido === 'igual') return textos.ranking.igual;
  return textos.ranking.semVariacao;
}

/** Variação da dezena no G-1, para leitor de tela: "subiu 3 posições no G-1". */
export function textoFaladoDaVariacaoG1(variacao: number | null): string {
  const sentido = sentidoVariacao(variacao);
  if (sentido === 'subiu') return textos.forca.g1.subiu(variacao ?? 0);
  if (sentido === 'caiu') return textos.forca.g1.caiu(Math.abs(variacao ?? 0));
  if (sentido === 'igual') return textos.forca.g1.igual;
  return textos.forca.g1.semVariacao;
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
