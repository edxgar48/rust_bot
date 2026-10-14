// Armazenamento persistente em IndexedDB (API nativa, sem biblioteca: são duas
// operações sobre um object store). Cada registro é o último arquivo válido por
// caminho, com sha256, bytes e data da última sincronização bem-sucedida.
//
// ArmazenamentoIndexedDB lança erro se o IndexedDB falhar; quem quiser que o app
// nunca caia usa ArmazenamentoResiliente, que cai para memória com aviso.
import { ArmazenamentoEmMemoria, type ArquivoGuardado, type Armazenamento } from './armazenamento';

export const NOME_BANCO = 'forca-loterias';
const VERSAO_BANCO = 1;
const LOJA = 'arquivos';

function esperar<T>(requisicao: IDBRequest<T>): Promise<T> {
  return new Promise((resolver, rejeitar) => {
    requisicao.onsuccess = () => resolver(requisicao.result);
    requisicao.onerror = () => rejeitar(requisicao.error ?? new Error('Falha no IndexedDB.'));
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
        banco.createObjectStore(LOJA, { keyPath: 'caminho' });
      }
    };
    pedido.onsuccess = () => resolver(pedido.result);
    pedido.onerror = () => rejeitar(pedido.error ?? new Error('Não foi possível abrir o banco.'));
    pedido.onblocked = () => rejeitar(new Error('Abertura do banco bloqueada.'));
  });
}

/** Registro lido do banco, tratado como não confiável: forma conferida. */
function paraArquivo(valor: unknown): ArquivoGuardado | null {
  if (typeof valor !== 'object' || valor === null) return null;
  const v = valor as Record<string, unknown>;
  if (typeof v.caminho !== 'string' || typeof v.sha256 !== 'string') return null;
  if (!ArrayBuffer.isView(v.bytes)) return null;
  const { buffer, byteOffset, byteLength } = v.bytes;
  return {
    caminho: v.caminho,
    sha256: v.sha256,
    bytes: new Uint8Array(buffer, byteOffset, byteLength),
    ...(typeof v.sincronizadoEm === 'string' ? { sincronizadoEm: v.sincronizadoEm } : {}),
  };
}

export class ArmazenamentoIndexedDB implements Armazenamento {
  private banco: Promise<IDBDatabase> | null = null;

  constructor(
    private readonly fabrica: IDBFactory,
    private readonly nome: string = NOME_BANCO,
  ) {}

  private obterBanco(): Promise<IDBDatabase> {
    if (!this.banco) {
      this.banco = abrir(this.fabrica, this.nome).catch((e) => {
        this.banco = null; // permite nova tentativa
        throw e;
      });
    }
    return this.banco;
  }

  async ler(caminho: string): Promise<ArquivoGuardado | null> {
    const banco = await this.obterBanco();
    const valor = await esperar(banco.transaction(LOJA, 'readonly').objectStore(LOJA).get(caminho));
    return paraArquivo(valor);
  }

  async guardar(arquivo: ArquivoGuardado): Promise<void> {
    const banco = await this.obterBanco();
    const transacao = banco.transaction(LOJA, 'readwrite');
    const concluida = new Promise<void>((resolver, rejeitar) => {
      transacao.oncomplete = () => resolver();
      transacao.onerror = () => rejeitar(transacao.error ?? new Error('Falha ao gravar.'));
      transacao.onabort = () => rejeitar(transacao.error ?? new Error('Gravação abortada.'));
    });
    transacao.objectStore(LOJA).put({ ...arquivo, bytes: arquivo.bytes.slice() });
    await concluida;
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
 * Persistente quando possível; se o IndexedDB não existir, estiver bloqueado
 * (modo privado, política do navegador) ou falhar em qualquer operação, passa
 * a usar memória pelo resto da sessão e registra um aviso. Nunca lança.
 */
export class ArmazenamentoResiliente implements Armazenamento {
  private readonly memoria = new ArmazenamentoEmMemoria();
  private persistente: ArmazenamentoIndexedDB | null;
  private motivo: string | null = null;

  constructor(
    fabrica: IDBFactory | null | undefined = globalThis.indexedDB,
    nome: string = NOME_BANCO,
  ) {
    this.persistente = fabrica ? new ArmazenamentoIndexedDB(fabrica, nome) : null;
    if (!fabrica) this.motivo = 'IndexedDB indisponível neste navegador.';
  }

  /** `true` enquanto os dados estão sendo gravados de forma persistente. */
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

  async ler(caminho: string): Promise<ArquivoGuardado | null> {
    if (this.persistente) {
      try {
        return await this.persistente.ler(caminho);
      } catch (e) {
        this.cair(e);
      }
    }
    return this.memoria.ler(caminho);
  }

  async guardar(arquivo: ArquivoGuardado): Promise<void> {
    if (this.persistente) {
      try {
        await this.persistente.guardar(arquivo);
        return;
      } catch (e) {
        this.cair(e);
      }
    }
    await this.memoria.guardar(arquivo);
  }
}
