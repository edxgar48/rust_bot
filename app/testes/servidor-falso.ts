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

  /**
   * Publica o ranking_taxa.json (fixture real) das loterias dadas: o arquivo passa a ser servido
   * e o índice ganha a chave opcional `ranking_taxa`. Sem esta chamada, o índice não a tem.
   *
   * ATENÇÃO, divergência conhecida: as fixtures ranking_taxa.json são de OUTRO snapshot que
   * tabela_geral/ultimo das fixtures base (concursos 3815/3076/7153 contra 3804/3070/7142 em
   * Lotofácil/Mega-Sena/Quina). O app não cruza os arquivos, então isso não afeta os testes
   * atuais; um teste que comparasse "No índice" com a Força exigiria fixtures do mesmo snapshot.
   */
  async publicarRankingTaxa(
    slugs: readonly string[] = ['lotofacil', 'megasena', 'quina'],
  ): Promise<void> {
    const indice = this.indice();
    for (const slug of slugs) {
      const caminho = `${slug}/ranking_taxa.json`;
      const bytes = lerFixture(caminho);
      this.arquivos.set(caminho, bytes);
      indice.loterias[slug].arquivos.ranking_taxa = {
        caminho,
        sha256: await sha256Hex(bytes),
        bytes: bytes.byteLength,
      };
    }
    this.gravarIndice(indice);
  }

  /**
   * Publica o ranking_posicoes.json das loterias dadas e a chave opcional `ranking_posicoes` do índice.
   *
   * Por padrão (`alinhado`) o arquivo é reescrito para o MESMO snapshot da tabela_geral base (as
   * fixtures reais de G-1 são de outro concurso): concurso/ultimo_sorteado e `posicao_indice` vêm da
   * tabela base, e cada dezena herda o `g_1` real da posição que passa a ocupar (é assim que o
   * back-end monta o arquivo). Os `variacao_g_1` aqui são ILUSTRATIVOS (a definição é por dezena e
   * os valores reais das fixtures são de outra definição): `(dezena % 5) - 2`, e `null` na dezena 1.
   * Com `alinhado: false` serve a fixture real como veio (outro snapshot da tabela base).
   */
  async publicarRankingPosicoes(
    slugs: readonly string[] = ['lotofacil', 'megasena', 'quina'],
    { alinhado = true }: { alinhado?: boolean } = {},
  ): Promise<void> {
    const indice = this.indice();
    for (const slug of slugs) {
      const caminho = `${slug}/ranking_posicoes.json`;
      let bytes = lerFixture(caminho);
      if (alinhado) {
        const real = JSON.parse(lerFixtureTexto(caminho));
        const tabela = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`));
        const g1DaPosicao = new Map<number, number>(
          real.dezenas.map((d: any) => [d.posicao_indice, d.g_1]),
        );
        real.concurso = tabela.concurso;
        real.ultimo_sorteado = tabela.ultimo_sorteado;
        real.dezenas = [...tabela.dezenas]
          .sort((a: any, b: any) => a.dezena - b.dezena)
          .map((d: any) => ({
            dezena: d.dezena,
            posicao_indice: d.posicao,
            g_1: g1DaPosicao.get(d.posicao),
            variacao_g_1: d.dezena === 1 ? null : (d.dezena % 5) - 2,
          }));
        bytes = codificar(JSON.stringify(real, null, 2) + '\n');
      }
      this.arquivos.set(caminho, bytes);
      indice.loterias[slug].arquivos.ranking_posicoes = {
        caminho,
        sha256: await sha256Hex(bytes),
        bytes: bytes.byteLength,
      };
    }
    this.gravarIndice(indice);
  }

  /** Modifica o JSON de um arquivo (parse, função, stringify) e atualiza o índice. */
  async alterarJson(caminho: string, fn: (json: any) => void): Promise<void> {
    const json = JSON.parse(new TextDecoder().decode(this.arquivos.get(caminho)));
    fn(json);
    await this.trocar(caminho, JSON.stringify(json, null, 2) + '\n');
  }

  /**
   * Simula a publicação de um concurso novo da Quina (a real termina no 7141): atualiza
   * ultimo.json, historico.json e o índice, sempre consistentes entre si. Dezenas em
   * ordem crescente (o contrato exige).
   */
  async publicarConcursoQuina(
    concurso: number,
    dezenas: number[],
    data = '2026-10-15',
  ): Promise<void> {
    await this.alterarJson('quina/ultimo.json', (j) => {
      j.concurso = concurso;
      j.data = data;
      j.dezenas = dezenas;
    });
    await this.alterarJson('quina/historico.json', (j) => {
      j.concursos.push([concurso, data, dezenas]);
    });
    const indice = this.indice();
    indice.loterias.quina.ultimo_concurso = concurso;
    indice.loterias.quina.data = data;
    this.gravarIndice(indice);
  }

  pedidosDeDados(): string[] {
    return this.pedidos.filter((p) => p !== 'index.json');
  }
}

function expect_(condicao: boolean, mensagem: string): void {
  if (!condicao) throw new Error(mensagem);
}
