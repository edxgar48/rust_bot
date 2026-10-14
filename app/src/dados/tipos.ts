// Tipos dos 4 arquivos do contrato v1 (docs/contrato-v1.md).
// Campos opcionais futuros e campos anuláveis chegam aqui como `null`;
// campos desconhecidos do JSON não são copiados.

export const SLUGS = ['lotofacil', 'megasena', 'quina'] as const;
export type Slug = (typeof SLUGS)[number];

export const CHAVES_ARQUIVO = ['ultimo', 'tabela_geral', 'historico'] as const;
export type ChaveArquivo = (typeof CHAVES_ARQUIVO)[number];

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
  readonly arquivos: Readonly<Record<ChaveArquivo, DescritorArquivo>>;
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

export interface DadosPorChave {
  readonly ultimo: UltimoV1;
  readonly tabela_geral: TabelaGeralV1;
  readonly historico: HistoricoV1;
}
