// Interface de armazenamento do último arquivo válido por caminho. Implementações:
// em memória (testes e fallback) aqui; IndexedDB em armazenamento-idb.ts.
//
// O index.json válido mais recente também é guardado, sob o caminho reservado
// CAMINHO_INDICE, para o app abrir offline mostrando o último concurso.

/** Caminho reservado do index.json válido mais recente. */
export const CAMINHO_INDICE = 'index.json';

export interface ArquivoGuardado {
  /** Caminho relativo a v1/, como no índice (ex.: quina/ultimo.json). */
  readonly caminho: string;
  /** SHA-256 (hex minúsculo) dos bytes, já conferido contra o índice. */
  readonly sha256: string;
  /** Bytes exatos recebidos (serão reconferidos ao ler). */
  readonly bytes: Uint8Array;
  /**
   * Quando este arquivo foi confirmado como o vigente pela última vez (ISO-8601
   * UTC). Opcional: gravações antigas não o têm (ausência = desconhecido).
   */
  readonly sincronizadoEm?: string;
}

export interface Armazenamento {
  ler(caminho: string): Promise<ArquivoGuardado | null>;
  /** Só é chamado com arquivo que passou em toda a conferência. */
  guardar(arquivo: ArquivoGuardado): Promise<void>;
}

export class ArmazenamentoEmMemoria implements Armazenamento {
  private readonly arquivos = new Map<string, ArquivoGuardado>();

  async ler(caminho: string): Promise<ArquivoGuardado | null> {
    return this.arquivos.get(caminho) ?? null;
  }

  async guardar(arquivo: ArquivoGuardado): Promise<void> {
    this.arquivos.set(arquivo.caminho, { ...arquivo, bytes: arquivo.bytes.slice() });
  }

  /** Auxiliar de teste: quantos arquivos há guardados. */
  get tamanho(): number {
    return this.arquivos.size;
  }
}
