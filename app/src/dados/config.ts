// Única configuração da camada de dados: de onde o app lê os JSON do contrato v1.
// Trocar de hospedagem (R2, Pages, S3...) é mudar só esta constante.
export const URL_BASE_PADRAO =
  'https://raw.githubusercontent.com/edxgar48/rust_spider_dados/main/v1/';

// Versão do esquema que este app entende (docs/contrato-v1.md).
export const SCHEMA_VERSION_SUPORTADA = 1;

// Garante a barra final, para que new URL(caminho, base) preserve a pasta v1/.
export function normalizarUrlBase(url: string): string {
  return url.endsWith('/') ? url : `${url}/`;
}
