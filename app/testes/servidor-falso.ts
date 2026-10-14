/* eslint-disable @typescript-eslint/no-explicit-any */
// Servidor falso para os testes: serve as fixtures reais (cópias de publico/v1/)
// por um `fetch` injetado, com possibilidade de adulterar arquivos. Sem rede.
import type { FetchInjetavel } from '../src/dados/cliente';
import { sha256Hex } from '../src/dados/hash';

export const URL_BASE_TESTE = 'https://dados.exemplo.test/v1/';

// Fixtures como texto (são ASCII puro, então texto <-> bytes é exato).
const brutas = import.meta.glob('./fixtures/**/*.json', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

export function lerFixtureTexto(caminho: string): string {
  const t = brutas[`./fixtures/${caminho}`];
  if (t === undefined) throw new Error(`Fixture inexistente: ${caminho}`);
  return t;
}

export function lerFixture(caminho: string): Uint8Array {
  return new TextEncoder().encode(lerFixtureTexto(caminho));
}

export const CAMINHOS = ['lotofacil', 'megasena', 'quina'].flatMap((s) =>
  ['ultimo', 'tabela_geral', 'historico'].map((a) => `${s}/${a}.json`),
);

const codificar = (t: string) => new TextEncoder().encode(t);

export class ServidorFalso {
  readonly arquivos = new Map<string, Uint8Array>();
  /** Caminhos pedidos, na ordem. */
  readonly pedidos: string[] = [];
  /** Caminhos que respondem com erro HTTP 500. */
  readonly com500 = new Set<string>();

  constructor() {
    this.arquivos.set('index.json', lerFixture('index.json'));
    for (const c of CAMINHOS) this.arquivos.set(c, lerFixture(c));
  }

  readonly fetch: FetchInjetavel = async (url) => {
    expect_(url.startsWith(URL_BASE_TESTE), `URL fora da base: ${url}`);
    const caminho = url.slice(URL_BASE_TESTE.length);
    this.pedidos.push(caminho);
    if (this.com500.has(caminho)) return new Response('erro', { status: 500 });
    const bytes = this.arquivos.get(caminho);
    if (!bytes) return new Response('nao achei', { status: 404 });
    return new Response(bytes as BufferSource, { status: 200 });
  };

  indice(): Record<string, any> {
    return JSON.parse(new TextDecoder().decode(this.arquivos.get('index.json')));
  }

  gravarIndice(indice: unknown): void {
    this.arquivos.set('index.json', codificar(JSON.stringify(indice, null, 2) + '\n'));
  }

  /**
   * Troca o conteúdo de um arquivo de dados. Com `atualizarIndice` (padrão), o
   * índice passa a declarar o hash e o tamanho novos (arquivo consistente com
   * o índice, mas possivelmente inválido na forma). Sem ele, o índice continua
   * apontando para o conteúdo original (arquivo adulterado).
   */
  async trocar(
    caminho: string,
    conteudo: string | Uint8Array,
    atualizarIndice = true,
  ): Promise<void> {
    const bytes = typeof conteudo === 'string' ? codificar(conteudo) : conteudo;
    this.arquivos.set(caminho, bytes);
    if (!atualizarIndice) return;
    const indice = this.indice();
    for (const loteria of Object.values<any>(indice.loterias)) {
      if (!loteria) continue;
      for (const d of Object.values<any>(loteria.arquivos)) {
        if (d.caminho === caminho) {
          d.sha256 = await sha256Hex(bytes);
          d.bytes = bytes.byteLength;
        }
      }
    }
    this.gravarIndice(indice);
  }

  /** Modifica o JSON de um arquivo (parse, função, stringify) e atualiza o índice. */
  async alterarJson(caminho: string, fn: (json: any) => void): Promise<void> {
    const json = JSON.parse(new TextDecoder().decode(this.arquivos.get(caminho)));
    fn(json);
    await this.trocar(caminho, JSON.stringify(json, null, 2) + '\n');
  }

  pedidosDeDados(): string[] {
    return this.pedidos.filter((p) => p !== 'index.json');
  }
}

function expect_(condicao: boolean, mensagem: string): void {
  if (!condicao) throw new Error(mensagem);
}
