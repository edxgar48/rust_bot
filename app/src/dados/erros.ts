import { SCHEMA_VERSION_SUPORTADA } from './config';

export type CodigoErro =
  | 'rede' // fetch falhou (sem conexão etc.)
  | 'http' // resposta com status de erro
  | 'bytes' // tamanho diferente do declarado no índice
  | 'hash' // SHA-256 diferente do declarado no índice
  | 'json_invalido' // bytes não são JSON (UTF-8) válido
  | 'schema_version' // versão do esquema que o app não entende
  | 'forma'; // JSON válido, mas fora da forma do contrato

export class ErroDados extends Error {
  readonly codigo: CodigoErro;

  constructor(codigo: CodigoErro, mensagem: string) {
    super(mensagem);
    this.name = 'ErroDados';
    this.codigo = codigo;
  }
}

/** Códigos que significam "não consegui falar com o servidor" (os demais são dado recusado). */
const CODIGOS_DE_REDE: ReadonlySet<CodigoErro> = new Set<CodigoErro>(['rede', 'http']);

export function ehErroDeRede(erro: ErroDados): boolean {
  return CODIGOS_DE_REDE.has(erro.codigo);
}

export class ErroSchemaNaoSuportado extends ErroDados {
  readonly versaoEncontrada: unknown;

  constructor(onde: string, versaoEncontrada: unknown) {
    super(
      'schema_version',
      `${onde}: schema_version ${JSON.stringify(versaoEncontrada)} não é suportado por este app (entende só a versão ${SCHEMA_VERSION_SUPORTADA}). É preciso atualizar o app.`,
    );
    this.name = 'ErroSchemaNaoSuportado';
    this.versaoEncontrada = versaoEncontrada;
  }
}
