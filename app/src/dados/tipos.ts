// Tipos dos 4 arquivos do contrato v1 (docs/contrato-v1.md).
// Campos opcionais futuros e campos anuláveis chegam aqui como `null`;
// campos desconhecidos do JSON não são copiados.

export const SLUGS = ['lotofacil', 'megasena', 'quina'] as const;
export type Slug = (typeof SLUGS)[number];

// Os três arquivos obrigatórios (o padrão de sincronização e de leitura do cache) e o
// opcionais `ranking_taxa` e `ranking_posicoes`, que só são baixados quando a tela que os usa pede.
export const CHAVES_ARQUIVO = ['ultimo', 'tabela_geral', 'historico'] as const;
export type ChaveObrigatoria = (typeof CHAVES_ARQUIVO)[number];
export type ChaveArquivo = ChaveObrigatoria | 'ranking_taxa' | 'ranking_posicoes';

export interface ParametrosLoteria {
  /** Quantidade de dezenas do volante (25, 60 ou 80). */
  readonly totalDezenas: number;
  /** Quantidade de dezenas sorteadas por concurso (15, 6 ou 5). */
  readonly dezenasSorteadas: number;
}

export const PARAMETROS_LOTERIA: Readonly<Record<Slug, ParametrosLoteria>> = {
  lotofacil: { totalDezenas: 25, dezenasSorteadas: 15 },
  megasena: { totalDezenas: 60, dezenasSorteadas: 6 },
  quina: { totalDezenas: 80, dezenasSorteadas: 5 },
};

// Identificadores enumerados do contrato ("Valores enumerados").
export const GRAUS_EVIDENCIA = [
  'muito_fraca',
  'fraca',
  'moderada',
  'forte',
  'muito_forte',
] as const;
export type GrauEvidencia = (typeof GRAUS_EVIDENCIA)[number];

export const DIRECOES_EVIDENCIA = ['vies', 'acaso'] as const;
export type DirecaoEvidencia = (typeof DIRECOES_EVIDENCIA)[number];

export const NORMALIZACOES = ['zscore', 'minmax'] as const;
export type Normalizacao = (typeof NORMALIZACOES)[number];

// index.json
export interface DescritorArquivo {
  readonly caminho: string;
  readonly sha256: string;
  readonly bytes: number;
}

export interface LoteriaNoIndice {
  readonly ultimoConcurso: number;
  readonly data: string;
  readonly arquivos: Readonly<Record<ChaveObrigatoria, DescritorArquivo>> & {
    /** Opcional no contrato: a chave some do índice quando o ranking não foi publicado. */
    readonly ranking_taxa?: DescritorArquivo;
    /** Opcional no contrato: some do índice quando o G-1 não foi publicado (ou está atrasado). */
    readonly ranking_posicoes?: DescritorArquivo;
  };
}

export interface IndiceV1 {
  readonly schemaVersion: 1;
  readonly geradoEm: string;
  /** `null` = loteria ainda sem arquivos na saída. */
  readonly loterias: Readonly<Record<Slug, LoteriaNoIndice | null>>;
}

// ultimo.json
export interface UltimoV1 {
  readonly schemaVersion: 1;
  readonly loteria: Slug;
  readonly concurso: number;
  readonly data: string;
  readonly dezenas: readonly number[];
}

// historico.json: [concurso, data, dezenas]
export type ConcursoHistorico = readonly [
  concurso: number,
  data: string,
  dezenas: readonly number[],
];

export interface HistoricoV1 {
  readonly schemaVersion: 1;
  readonly loteria: Slug;
  readonly concursos: readonly ConcursoHistorico[];
}

// tabela_geral.json
export interface DezenaDaTabela {
  readonly dezena: number;
  readonly posicao: number;
  readonly total: number;
  readonly variacaoPosicao: number | null;
  readonly frequencia: number;
  readonly maiorEspaco: number | null;
  readonly maiorEspacoOcorrencias: number | null;
  readonly menorEspaco: number | null;
  readonly menorEspacoOcorrencias: number | null;
  readonly somaVariacoes: number | null;
  readonly espacoFinal: number | null;
  readonly repeticoesTotal: number;
  readonly repeticaoAtual: number;
  readonly evidenciaGrau: GrauEvidencia | null;
  readonly evidenciaDirecao: DirecaoEvidencia | null;
}

/** Os 9 componentes brutos de cada dezena, na ordem do contrato. */
export const CHAVES_COMPONENTES = [
  'frequencia',
  'maiorEspaco',
  'maiorEspacoOcorrencias',
  'menorEspaco',
  'menorEspacoOcorrencias',
  'somaVariacoes',
  'espacoFinal',
  'repeticoesTotal',
  'repeticaoAtual',
] as const satisfies readonly (keyof DezenaDaTabela)[];
export type ChaveComponente = (typeof CHAVES_COMPONENTES)[number];

export interface TabelaGeralV1 {
  readonly schemaVersion: 1;
  readonly loteria: Slug;
  readonly perfil: string;
  readonly hashToml: string;
  /** 0 = histórico total. */
  readonly janela: number;
  readonly normalizacao: Normalizacao;
  readonly featureVersion: number;
  /** Próximo concurso (ainda não sorteado). */
  readonly concurso: number;
  readonly ultimoSorteado: number;
  readonly dezenas: readonly DezenaDaTabela[];
}

// ranking_taxa.json (opcional): uma linha por dezena, em ordem crescente de dezena.
export interface DezenaDoRanking {
  readonly dezena: number;
  readonly posicaoIndice: number;
  /** Nota A (principal): taxa histórica da posição do índice. */
  readonly notaA: number;
  /** `notaA - taxaAcaso`, como veio no arquivo. */
  readonly diferencaAcaso: number;
  /** Nota B (experimental). */
  readonly notaB: number;
  readonly posicaoA: number;
  readonly posicaoB: number;
}

export interface RankingTaxaV1 {
  readonly schemaVersion: 1;
  readonly loteria: Slug;
  readonly perfil: string;
  readonly hashToml: string;
  readonly janela: number;
  readonly normalizacao: Normalizacao;
  readonly featureVersion: number;
  /** Próximo concurso (ainda não sorteado). */
  readonly concurso: number;
  readonly ultimoSorteado: number;
  /** K/N: a chance de qualquer dezena sair se tudo fosse acaso. */
  readonly taxaAcaso: number;
  /** Em ordem crescente de dezena, como no arquivo. */
  readonly dezenas: readonly DezenaDoRanking[];
}

// ranking_posicoes.json (opcional): o G-1 por dezena, em ordem crescente de dezena.
export interface DezenaDoG1 {
  readonly dezena: number;
  /** Igual à `posicao` da dezena em tabela_geral.json. */
  readonly posicaoIndice: number;
  /** G-1: lugar (1 = o melhor) da posição da dezena no índice das posições. Não é probabilidade. */
  readonly g1: number;
  /** Da DEZENA: `g_1` dela no concurso anterior menos o atual (positivo = subiu); `null` = sem concurso anterior. */
  readonly variacaoG1: number | null;
}

export interface RankingPosicoesV1 {
  readonly schemaVersion: 1;
  readonly loteria: Slug;
  readonly perfil: string;
  readonly hashToml: string;
  readonly janela: number;
  readonly normalizacao: Normalizacao;
  readonly featureVersion: number;
  /** Próximo concurso (ainda não sorteado). */
  readonly concurso: number;
  readonly ultimoSorteado: number;
  readonly perfilPosicoes: string;
  readonly hashTomlPosicoes: string;
  readonly janelaPosicoes: number;
  readonly normalizacaoPosicoes: Normalizacao;
  /** Em ordem crescente de dezena, como no arquivo. */
  readonly dezenas: readonly DezenaDoG1[];
}

export interface DadosPorChave {
  readonly ultimo: UltimoV1;
  readonly tabela_geral: TabelaGeralV1;
  readonly historico: HistoricoV1;
  readonly ranking_taxa: RankingTaxaV1;
  readonly ranking_posicoes: RankingPosicoesV1;
}
