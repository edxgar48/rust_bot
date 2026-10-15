// Jogos salvos em IndexedDB, num banco PRÓPRIO (`forca-loterias-jogos`), distinto
// do cache de dados do contrato (`forca-loterias`). Se o IndexedDB não existir ou não
// abrir, os jogos passam a valer só na memória da sessão e a tela avisa. Se ele abrir
// e uma operação falhar depois, a operação é tentada de novo e, persistindo a falha,
// o erro chega à tela: nunca se finge que a lista está vazia nem se troca o banco
// real por uma memória vazia.
import type { Slug } from '../dados/tipos';
import { ordenarJogos, validarJogoSalvo, validarNome, type JogoSalvo } from './jogo';
import { RepositorioEmMemoria, proximaMarca, type RepositorioJogos } from './repositorio';

export const NOME_BANCO_JOGOS = 'forca-loterias-jogos';
const VERSAO_BANCO = 1;
const LOJA = 'jogos';
const INDICE_LOTERIA = 'loteria';

/** O banco não abriu (ou não existe): é o único caso em que se cai para a memória. */
export class ErroAbertura extends Error {
  constructor(causa: unknown) {
    super(causa instanceof Error ? causa.message : String(causa));
    this.name = 'ErroAbertura';
  }
}

function esperar<T>(requisicao: IDBRequest<T>): Promise<T> {
  return new Promise((resolver, rejeitar) => {
    requisicao.onsuccess = () => resolver(requisicao.result);
    requisicao.onerror = () => rejeitar(requisicao.error ?? new Error('Falha no IndexedDB.'));
  });
}

function concluir(transacao: IDBTransaction): Promise<void> {
  return new Promise((resolver, rejeitar) => {
    transacao.oncomplete = () => resolver();
    transacao.onerror = () => rejeitar(transacao.error ?? new Error('Falha ao gravar.'));
    transacao.onabort = () => rejeitar(transacao.error ?? new Error('Gravação abortada.'));
  });
}

function abrir(fabrica: IDBFactory, nome: string): Promise<IDBDatabase> {
  return new Promise((resolver, rejeitar) => {
    let pedido: IDBOpenDBRequest;
    try {
      pedido = fabrica.open(nome, VERSAO_BANCO);
    } catch (e) {
      rejeitar(e);
      return;
    }
    pedido.onupgradeneeded = () => {
      const banco = pedido.result;
      if (!banco.objectStoreNames.contains(LOJA)) {
        const loja = banco.createObjectStore(LOJA, { keyPath: 'id' });
        loja.createIndex(INDICE_LOTERIA, 'loteria');
      }
    };
    pedido.onsuccess = () => resolver(pedido.result);
    pedido.onerror = () => rejeitar(pedido.error ?? new Error('Não foi possível abrir o banco.'));
    pedido.onblocked = () => rejeitar(new Error('Abertura do banco bloqueada.'));
  });
}

/** Registros do banco são tratados como não confiáveis; os inválidos são pulados, nunca apagados. */
function paraJogos(valores: readonly unknown[]): JogoSalvo[] {
  const saida: JogoSalvo[] = [];
  for (const v of valores) {
    const r = validarJogoSalvo(v);
    if (r.ok) saida.push(r.valor);
  }
  return ordenarJogos(saida);
}

export class RepositorioIndexedDB implements RepositorioJogos {
  private banco: Promise<IDBDatabase> | null = null;

  constructor(
    private readonly fabrica: IDBFactory,
    private readonly nome: string = NOME_BANCO_JOGOS,
  ) {}

  private obterBanco(): Promise<IDBDatabase> {
    if (!this.banco) {
      this.banco = abrir(this.fabrica, this.nome).catch((e) => {
        this.banco = null; // permite nova tentativa
        throw new ErroAbertura(e);
      });
    }
    return this.banco;
  }

  async listar(loteria?: Slug): Promise<JogoSalvo[]> {
    const banco = await this.obterBanco();
    const loja = banco.transaction(LOJA, 'readonly').objectStore(LOJA);
    const valores =
      loteria === undefined
        ? await esperar(loja.getAll())
        : await esperar(loja.index(INDICE_LOTERIA).getAll(loteria));
    return paraJogos(valores as unknown[]);
  }

  async listarIds(): Promise<string[]> {
    const banco = await this.obterBanco();
    const chaves = await esperar(
      banco.transaction(LOJA, 'readonly').objectStore(LOJA).getAllKeys(),
    );
    return chaves.filter((c): c is string => typeof c === 'string');
  }

  async gravar(jogos: readonly JogoSalvo[]): Promise<void> {
    if (jogos.length === 0) return;
    const banco = await this.obterBanco();
    const transacao = banco.transaction(LOJA, 'readwrite');
    const concluida = concluir(transacao);
    const loja = transacao.objectStore(LOJA);
    for (const j of jogos) loja.put({ ...j, dezenas: [...j.dezenas] });
    await concluida;
  }

  async renomear(id: string, nome: string | null): Promise<JogoSalvo | null> {
    const limpo = validarNome(nome);
    if (!limpo.ok) return null;
    const banco = await this.obterBanco();
    const transacao = banco.transaction(LOJA, 'readwrite');
    const concluida = concluir(transacao);
    const loja = transacao.objectStore(LOJA);
    const atual = validarJogoSalvo(await esperar(loja.get(id)));
    if (!atual.ok) {
      transacao.abort();
      await concluida.catch(() => undefined);
      return null;
    }
    const novo: JogoSalvo = { ...atual.valor, nome: limpo.valor };
    loja.put({ ...novo, dezenas: [...novo.dezenas] });
    await concluida;
    return novo;
  }

  async apagar(id: string): Promise<void> {
    const banco = await this.obterBanco();
    const transacao = banco.transaction(LOJA, 'readwrite');
    const concluida = concluir(transacao);
    transacao.objectStore(LOJA).delete(id);
    await concluida;
  }

  async avancarMarcas(
    marcas: ReadonlyMap<string, number>,
    soSemMarca = false,
  ): Promise<JogoSalvo[]> {
    if (marcas.size === 0) return [];
    const banco = await this.obterBanco();
    const transacao = banco.transaction(LOJA, 'readwrite');
    const concluida = concluir(transacao);
    const loja = transacao.objectStore(LOJA);
    const saida: JogoSalvo[] = [];
    try {
      for (const [id, nova] of marcas) {
        const atual = validarJogoSalvo(await esperar(loja.get(id)));
        if (!atual.ok) continue;
        const marca = proximaMarca(atual.valor.conferidoAte, nova, soSemMarca);
        const novo: JogoSalvo = { ...atual.valor, conferidoAte: marca };
        if (marca !== atual.valor.conferidoAte) loja.put({ ...novo, dezenas: [...novo.dezenas] });
        saida.push(novo);
      }
    } catch (e) {
      transacao.abort();
      await concluida.catch(() => undefined);
      throw e;
    }
    await concluida;
    return saida;
  }

  /** Libera a conexão (testes e troca de banco). */
  async fechar(): Promise<void> {
    if (!this.banco) return;
    const banco = await this.banco.catch(() => null);
    banco?.close();
    this.banco = null;
  }
}

/**
 * Persistente quando possível. Só se o banco NÃO ABRE passa a usar memória pelo resto
 * da sessão e registra o motivo em `aviso`. Se ele abre e uma operação falha depois,
 * a operação é repetida uma vez com a conexão refeita; se a falha continua, o erro
 * é lançado para a tela mostrar (leitura: erro de leitura, nunca lista vazia).
 */
export class RepositorioResiliente implements RepositorioJogos {
  private readonly memoria = new RepositorioEmMemoria();
  private persistente: RepositorioIndexedDB | null;
  private motivo: string | null = null;

  constructor(
    fabrica: IDBFactory | null | undefined = globalThis.indexedDB,
    nome: string = NOME_BANCO_JOGOS,
  ) {
    this.persistente = fabrica ? new RepositorioIndexedDB(fabrica, nome) : null;
    if (!fabrica) this.motivo = 'IndexedDB indisponível neste navegador.';
  }

  /** `true` enquanto os jogos estão sendo gravados de forma persistente. */
  get ePersistente(): boolean {
    return this.persistente !== null;
  }

  /** Motivo técnico da queda para memória, ou `null` se tudo persiste. */
  get aviso(): string | null {
    return this.motivo;
  }

  private cair(e: unknown): void {
    const detalhe = e instanceof Error ? e.message : String(e);
    this.motivo = `Armazenamento persistente indisponível (${detalhe}).`;
    const antigo = this.persistente;
    this.persistente = null;
    void antigo?.fechar().catch(() => undefined);
  }

  private async executar<T>(
    noBanco: (repo: RepositorioIndexedDB) => Promise<T>,
    naMemoria: () => Promise<T>,
  ): Promise<T> {
    const repo = this.persistente;
    if (!repo) return naMemoria();
    try {
      return await noBanco(repo);
    } catch (primeira) {
      if (primeira instanceof ErroAbertura) {
        this.cair(primeira);
        return naMemoria();
      }
      // Abriu mas a operação falhou: refaz a conexão e tenta de novo (as operações são
      // idempotentes). Falhando de novo, o erro segue para a tela.
      await repo.fechar().catch(() => undefined);
      try {
        return await noBanco(repo);
      } catch (segunda) {
        if (segunda instanceof ErroAbertura) {
          this.cair(segunda);
          return naMemoria();
        }
        throw segunda;
      }
    }
  }

  listar(loteria?: Slug): Promise<JogoSalvo[]> {
    return this.executar(
      (r) => r.listar(loteria),
      () => this.memoria.listar(loteria),
    );
  }

  listarIds(): Promise<string[]> {
    return this.executar(
      (r) => r.listarIds(),
      () => this.memoria.listarIds(),
    );
  }

  gravar(jogos: readonly JogoSalvo[]): Promise<void> {
    return this.executar(
      (r) => r.gravar(jogos),
      () => this.memoria.gravar(jogos),
    );
  }

  renomear(id: string, nome: string | null): Promise<JogoSalvo | null> {
    return this.executar(
      (r) => r.renomear(id, nome),
      () => this.memoria.renomear(id, nome),
    );
  }

  avancarMarcas(marcas: ReadonlyMap<string, number>, soSemMarca = false): Promise<JogoSalvo[]> {
    return this.executar(
      (r) => r.avancarMarcas(marcas, soSemMarca),
      () => this.memoria.avancarMarcas(marcas, soSemMarca),
    );
  }

  apagar(id: string): Promise<void> {
    return this.executar(
      (r) => r.apagar(id),
      () => this.memoria.apagar(id),
    );
  }
}
