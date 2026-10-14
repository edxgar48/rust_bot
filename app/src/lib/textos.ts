// Todos os textos de interface ficam neste módulo.
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

export type IdAba = keyof typeof textos.abas;

export const idsAbas = Object.keys(textos.abas) as IdAba[];
