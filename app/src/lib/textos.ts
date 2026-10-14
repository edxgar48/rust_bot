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
  sincronizacao: {
    titulo: 'Situação dos dados',
    sincronizando: (data: string | null) =>
      data === null
        ? 'Verificando atualizações…'
        : `Verificando atualizações… Mostrando dados de ${data}.`,
    atualizado: (data: string) => `Atualizado em ${data}.`,
    semRede: (data: string | null) =>
      data === null
        ? 'Sem rede, mostrando os últimos dados salvos.'
        : `Sem rede, mostrando dados de ${data}.`,
    recusado: (data: string | null) =>
      data === null
        ? 'Dado recusado (falha de conferência), mantendo os anteriores.'
        : `Dado recusado (falha de conferência), mantendo os anteriores, de ${data}.`,
    semDados: 'Ainda sem dados. Conecte-se à internet para baixar a primeira vez.',
    armazenamentoVolatil:
      'Este navegador não permite guardar dados neste aparelho (modo privado ou bloqueado). Os dados valem só até fechar o app.',
    atualizacaoDisponivel: 'Nova versão do app pronta. Ela vale na próxima vez que você abrir.',
  },
  loterias: {
    lotofacil: 'Lotofácil',
    megasena: 'Mega-Sena',
    quina: 'Quina',
  },
  inicio: {
    titulo: 'Resultados',
    concursoData: (concurso: number, data: string) => `Concurso ${concurso} · ${data}`,
    dezenasSorteadas: (loteria: string) => `Dezenas sorteadas na ${loteria}`,
    semResultado: 'Sem dezenas guardadas para este concurso.',
    semLoteria: 'Ainda sem dados desta loteria',
    carregando: 'Carregando resultados…',
    atualizar: 'Atualizar',
    atualizando: 'Atualizando…',
    tentarDeNovo: 'Tentar de novo',
    independencia:
      'Cada sorteio é independente: nenhuma dezena tem mais chance de sair por causa do histórico. Este app não prevê resultados.',
  },
  tema: {
    // `atual` é o rótulo da preferência em uso.
    alternar: (atual: string) => `Tema: ${atual}. Toque para trocar.`,
    automatico: 'automático (segue o aparelho)',
    claro: 'claro',
    escuro: 'escuro',
  },
  aviso: {
    informativo: 'Aviso',
    erro: 'Erro',
  },
  bola: {
    dezena: (numero: number) => `Dezena ${numero}`,
    estados: {
      neutra: '',
      sorteada: 'sorteada',
      destaque: 'em destaque',
      selecionada: 'selecionada',
    },
  },
  volante: {
    // Rótulo do grupo de dezenas de uma loteria.
    grupo: (loteria: string) => `Volante da ${loteria}`,
    dezenaNaFaixa: (numero: number, posicao: number, faixa: number, faixas: number) =>
      `Dezena ${numero}, posição ${posicao}, faixa ${faixa} de ${faixas}`,
    dezenaSemPosicao: (numero: number) => `Dezena ${numero}, sem posição`,
    emDestaque: 'em destaque',
    legenda: 'Faixas de posição no ranking',
    faixa: (faixa: number, de: number, ate: number) => `Faixa ${faixa}: posições ${de} a ${ate}`,
    faixaCurta: (faixa: number) => `Faixa ${faixa}`,
  },
  ranking: {
    posicao: (posicao: number) => `${posicao}º lugar`,
    indice: 'Índice',
    evidencia: 'Evidência',
    semEvidencia: 'sem evidência calculada',
    subiu: (n: number) => `subiu ${n} ${n === 1 ? 'posição' : 'posições'}`,
    caiu: (n: number) => `caiu ${n} ${n === 1 ? 'posição' : 'posições'}`,
    igual: 'manteve a posição',
    semVariacao: 'sem concurso anterior para comparar',
  },
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
