export type TamanhoBola = 'p' | 'm' | 'g' | 'celula';
export type EstadoBola = 'neutra' | 'sorteada' | 'destaque' | 'selecionada';
export type VarianteCartao = 'elevado' | 'preenchido' | 'contorno';
export type TipoAviso = 'informativo' | 'erro';
export type NomeIcone =
  | 'inicio'
  | 'forca'
  | 'conferir'
  | 'estatisticas'
  | 'entenda'
  | 'info'
  | 'erro'
  | 'atualizar'
  | 'sem-rede'
  | 'tema-automatico'
  | 'tema-claro'
  | 'tema-escuro';

export interface OpcaoSegmentada<T extends string = string> {
  readonly valor: T;
  readonly rotulo: string;
}

export interface AbaNavegacao<T extends string = string> {
  readonly id: T;
  readonly rotulo: string;
  readonly icone: NomeIcone;
}
