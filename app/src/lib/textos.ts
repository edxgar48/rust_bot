// Todos os textos de interface ficam neste módulo.
import type { DirecaoEvidencia, GrauEvidencia, Normalizacao } from '../dados/tipos';

export const textos = {
  appNome: 'Força das Dezenas',
  navegacao: 'Navegação principal',
  abas: {
    inicio: 'Início',
    forca: 'Força',
    conferir: 'Conferir',
    estatisticas: 'Estatísticas',
    entenda: 'Entenda',
  },
  placeholder: 'Esta tela ainda está em construção.',
} as const;

// Rótulos dos identificadores enumerados do contrato v1 ("Valores enumerados").
// O tipo Record exige uma entrada por valor conhecido: valor novo no contrato
// quebra o `npm run check` e o teste até receber um rótulo.
export const rotulosEvidenciaGrau: Readonly<Record<GrauEvidencia, string>> = {
  muito_fraca: 'Muito fraca',
  fraca: 'Fraca',
  moderada: 'Moderada',
  forte: 'Forte',
  muito_forte: 'Muito forte',
};

export const rotulosEvidenciaDirecao: Readonly<Record<DirecaoEvidencia, string>> = {
  vies: 'A favor de viés',
  acaso: 'A favor do acaso',
};

export const rotulosNormalizacao: Readonly<Record<Normalizacao, string>> = {
  zscore: 'Padronização (z-score)',
  minmax: 'Escala de 0 a 1 (mínimo e máximo)',
};

export const rotuloEvidenciaGrau = (grau: GrauEvidencia): string => rotulosEvidenciaGrau[grau];
export const rotuloEvidenciaDirecao = (direcao: DirecaoEvidencia): string =>
  rotulosEvidenciaDirecao[direcao];
export const rotuloNormalizacao = (normalizacao: Normalizacao): string =>
  rotulosNormalizacao[normalizacao];

export type IdAba = keyof typeof textos.abas;

export const idsAbas = Object.keys(textos.abas) as IdAba[];
