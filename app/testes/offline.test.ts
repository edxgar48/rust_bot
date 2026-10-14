// @vitest-environment node
import { IDBFactory } from 'fake-indexeddb';
import { describe, expect, it } from 'vitest';
import { abrirComCache, type SituacaoDados } from '../src/dados/abertura';
import {
  CAMINHO_INDICE,
  ArmazenamentoEmMemoria,
  type Armazenamento,
} from '../src/dados/armazenamento';
import { ArmazenamentoIndexedDB, ArmazenamentoResiliente } from '../src/dados/armazenamento-idb';
import { criarCliente } from '../src/dados/cliente';
import { ErroDados } from '../src/dados/erros';
import {
  estadoInicial,
  estadoSincronizacao,
  formatarDataHora,
  formatarDataSorteio,
} from '../src/dados/estado-sincronizacao';
import { sha256Hex } from '../src/dados/hash';
import { textos } from '../src/lib/textos';
import { ServidorFalso, URL_BASE_TESTE, lerFixture } from './servidor-falso';

const T1 = new Date('2026-10-14T12:30:00Z'); // 09:30 em Brasília
const T2 = new Date('2026-10-15T15:00:00Z'); // 12:00 em Brasília

function cliente(servidor: ServidorFalso, armazenamento: Armazenamento, agora = T1) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento,
    agora: () => agora,
  });
}

const semRede = async (): Promise<Response> => {
  throw new TypeError('Failed to fetch');
};

/** Troca o conteúdo servido sem o índice saber: arquivo adulterado em trânsito. */
function adulterar(servidor: ServidorFalso, caminho: string): void {
  const bytes = lerFixture(caminho).slice();
  bytes[bytes.length - 3] = (bytes[bytes.length - 3] ?? 0) ^ 1;
  servidor.arquivos.set(caminho, bytes);
}

describe('ArmazenamentoIndexedDB', () => {
  it('ida e volta: sha256, bytes e data de sincronização', async () => {
    const arm = new ArmazenamentoIndexedDB(new IDBFactory());
    expect(await arm.ler('quina/ultimo.json')).toBeNull();
    const bytes = lerFixture('quina/ultimo.json');
    await arm.guardar({
      caminho: 'quina/ultimo.json',
      sha256: await sha256Hex(bytes),
      bytes,
      sincronizadoEm: T1.toISOString(),
    });
    const lido = await arm.ler('quina/ultimo.json');
    expect(lido?.sha256).toBe(await sha256Hex(bytes));
    expect(Array.from(lido?.bytes ?? [])).toEqual(Array.from(bytes));
    expect(lido?.sincronizadoEm).toBe(T1.toISOString());
  });

  it('sobrescreve o mesmo caminho e mantém caminhos separados', async () => {
    const arm = new ArmazenamentoIndexedDB(new IDBFactory());
    await arm.guardar({ caminho: 'a', sha256: 'x', bytes: new Uint8Array([1]) });
    await arm.guardar({ caminho: 'b', sha256: 'y', bytes: new Uint8Array([1]) });
    await arm.guardar({ caminho: 'a', sha256: 'z', bytes: new Uint8Array([2]) });
    expect((await arm.ler('a'))?.sha256).toBe('z');
    expect((await arm.ler('b'))?.sha256).toBe('y');
    expect((await arm.ler('a'))?.sincronizadoEm).toBeUndefined();
  });

  it('nova instância sobre o mesmo banco devolve os dados (recarregar o app)', async () => {
    const fabrica = new IDBFactory();
    const servidor = new ServidorFalso();
    const primeira = new ArmazenamentoIndexedDB(fabrica);
    await cliente(servidor, primeira).sincronizar();
    await primeira.fechar();

    const c2 = cliente(servidor, new ArmazenamentoIndexedDB(fabrica), T2);
    const cache = await c2.lerCache();
    expect(cache?.indice.loterias.quina?.ultimoConcurso).toBe(7141);
    expect(cache?.loterias.quina?.dados.ultimo?.concurso).toBe(7141);
    expect(cache?.sincronizadoEm).toBe(T1.toISOString());

    servidor.pedidos.length = 0;
    const r = await c2.sincronizar();
    expect(servidor.pedidos).toEqual(['index.json']);
    expect(r.loterias.quina?.arquivos.ultimo?.estado).toBe('em_cache');
  });

  it('registro com forma estranha no banco é tratado como ausente', async () => {
    const fabrica = new IDBFactory();
    const arm = new ArmazenamentoIndexedDB(fabrica);
    await arm.guardar({ caminho: 'a', sha256: 'x', bytes: new Uint8Array([1]) });
    const banco = await new Promise<IDBDatabase>((ok, ruim) => {
      const p = fabrica.open('forca-loterias', 1);
      p.onsuccess = () => ok(p.result);
      p.onerror = () => ruim(p.error);
    });
    await new Promise<void>((ok) => {
      const t = banco.transaction('arquivos', 'readwrite');
      t.objectStore('arquivos').put({ caminho: 'a', sha256: 'x', bytes: 'texto' });
      t.oncomplete = () => ok();
    });
    banco.close();
    expect(await arm.ler('a')).toBeNull();
  });
});

describe('cache do cliente (índice e dados)', () => {
  it('guarda o index.json válido e o devolve sem rede', async () => {
    const arm = new ArmazenamentoEmMemoria();
    expect(await cliente(new ServidorFalso(), arm).lerCache()).toBeNull();
    await cliente(new ServidorFalso(), arm).sincronizar();
    const guardado = await arm.ler(CAMINHO_INDICE);
    expect(guardado?.sha256).toBe(await sha256Hex(guardado?.bytes ?? new Uint8Array()));
    const cache = await cliente(new ServidorFalso(), arm).lerCache({ loterias: ['megasena'] });
    expect(cache?.indice.loterias.megasena?.ultimoConcurso).toBe(3069);
    expect(cache?.indice.loterias.lotofacil?.data).toBe('2026-10-13');
    expect(Object.keys(cache?.loterias ?? {})).toEqual(['megasena']);
  });

  it('índice inválido (schema_version 2) não substitui o índice guardado', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await cliente(servidor, arm).sincronizar();
    const antes = await arm.ler(CAMINHO_INDICE);
    servidor.gravarIndice({ ...servidor.indice(), schema_version: 2 });
    await expect(cliente(servidor, arm, T2).sincronizar()).rejects.toBeInstanceOf(ErroDados);
    const depois = await arm.ler(CAMINHO_INDICE);
    expect(depois?.sha256).toBe(antes?.sha256);
    expect(depois?.sincronizadoEm).toBe(T1.toISOString());
    expect((await cliente(servidor, arm).lerCache())?.indice.schemaVersion).toBe(1);
  });

  it('índice guardado corrompido: lerCache devolve null, sem lançar', async () => {
    const arm = new ArmazenamentoEmMemoria();
    await arm.guardar({ caminho: CAMINHO_INDICE, sha256: 'errado', bytes: new Uint8Array([1]) });
    expect(await cliente(new ServidorFalso(), arm).lerCache()).toBeNull();
  });

  it('hash inválido no servidor não troca os dados guardados no IndexedDB', async () => {
    const arm = new ArmazenamentoIndexedDB(new IDBFactory());
    const servidor = new ServidorFalso();
    await cliente(servidor, arm).sincronizar();
    const original = await arm.ler('quina/tabela_geral.json');

    // O índice novo declara outro hash; o arquivo servido não confere com ele.
    await servidor.trocar('quina/tabela_geral.json', lerFixture('quina/historico.json'));
    adulterar(servidor, 'quina/tabela_geral.json');

    const r = await cliente(servidor, arm, T2).sincronizar();
    expect(r.loterias.quina?.arquivos.tabela_geral?.estado).toBe('falhou');
    const depois = await arm.ler('quina/tabela_geral.json');
    expect(depois?.sha256).toBe(original?.sha256);
    expect(depois?.sincronizadoEm).toBe(T1.toISOString());
    const cache = await cliente(servidor, arm).lerCache();
    expect(cache?.loterias.quina?.dados.tabela_geral).not.toBeNull();
  });

  it('arquivo vigente confirmado pelo índice atualiza a data de sincronização', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await cliente(servidor, arm).sincronizar();
    await cliente(servidor, arm, T2).sincronizar();
    expect((await arm.ler('quina/ultimo.json'))?.sincronizadoEm).toBe(T2.toISOString());
    expect((await cliente(servidor, arm).lerCache())?.sincronizadoEm).toBe(T2.toISOString());
  });

  it('falha ao gravar não descarta o arquivo conferido', async () => {
    const quebrado: Armazenamento = {
      ler: async () => null,
      guardar: async () => {
        throw new Error('cota excedida');
      },
    };
    const r = await cliente(new ServidorFalso(), quebrado).sincronizar({
      loterias: ['quina'],
      arquivos: ['ultimo'],
    });
    expect(r.loterias.quina?.arquivos.ultimo?.estado).toBe('baixado');
  });

  it('armazenamento que lança na leitura: lerCache devolve null', async () => {
    const quebrado: Armazenamento = {
      ler: async () => {
        throw new Error('bloqueado');
      },
      guardar: async () => undefined,
    };
    expect(await cliente(new ServidorFalso(), quebrado).lerCache()).toBeNull();
  });
});

describe('sem rede (modo avião)', () => {
  it('abre com os dados da última sincronização e diz de quando são', async () => {
    const fabrica = new IDBFactory();
    await cliente(new ServidorFalso(), new ArmazenamentoIndexedDB(fabrica)).sincronizar();

    // Novo carregamento do app, sem rede.
    const offline = criarCliente({
      urlBase: URL_BASE_TESTE,
      fetch: semRede,
      armazenamento: new ArmazenamentoIndexedDB(fabrica),
      agora: () => T2,
    });
    const passos: SituacaoDados[] = [];
    const final = await abrirComCache(
      offline,
      (s) => passos.push(s),
      () => T2,
    );

    expect(passos).toHaveLength(2);
    expect(passos[0]?.estado.tipo).toBe('sincronizando');
    expect(passos[0]?.cache?.indice.loterias.lotofacil?.ultimoConcurso).toBe(3803);
    expect(passos[0]?.estado.texto).toContain('14/10/2026 09:30');
    expect(final.estado.tipo).toBe('sem_rede');
    expect(final.estado.texto).toBe('Sem rede, mostrando dados de 14/10/2026 09:30.');
    expect(final.cache?.loterias.lotofacil?.dados.historico).not.toBeNull();
  });

  it('primeira abertura sem rede: ainda sem dados', async () => {
    const offline = criarCliente({
      urlBase: URL_BASE_TESTE,
      fetch: semRede,
      armazenamento: new ArmazenamentoEmMemoria(),
    });
    const final = await abrirComCache(offline, () => undefined);
    expect(final.estado.tipo).toBe('sem_dados');
    expect(final.cache).toBeNull();
  });

  it('com rede e tudo certo: atualizado', async () => {
    const c = cliente(new ServidorFalso(), new ArmazenamentoEmMemoria());
    const final = await abrirComCache(
      c,
      () => undefined,
      () => T1,
    );
    expect(final.estado.tipo).toBe('atualizado');
    expect(final.estado.texto).toBe('Atualizado em 14/10/2026 09:30.');
  });

  it('arquivo adulterado no servidor: dado recusado, mantendo os anteriores', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await cliente(servidor, arm).sincronizar();
    await servidor.trocar('megasena/ultimo.json', lerFixture('megasena/historico.json'));
    adulterar(servidor, 'megasena/ultimo.json');
    const final = await abrirComCache(
      cliente(servidor, arm, T2),
      () => undefined,
      () => T2,
    );
    expect(final.estado.tipo).toBe('recusado');
    expect(final.cache?.loterias.megasena?.dados.ultimo?.concurso).toBe(3069);
  });
});

describe('IndexedDB indisponível', () => {
  it('sem IndexedDB: usa memória, avisa e funciona', async () => {
    const arm = new ArmazenamentoResiliente(null);
    expect(arm.ePersistente).toBe(false);
    expect(arm.aviso).toMatch(/indisponível/);
    await arm.guardar({ caminho: 'a', sha256: 'x', bytes: new Uint8Array([1]) });
    expect((await arm.ler('a'))?.sha256).toBe('x');
  });

  it('banco que não abre (modo privado/bloqueado): cai na memória com aviso', async () => {
    const bloqueada = {
      open() {
        throw new DOMException('negado', 'SecurityError');
      },
    } as unknown as IDBFactory;
    const arm = new ArmazenamentoResiliente(bloqueada);
    expect(arm.aviso).toBeNull();
    const c = cliente(new ServidorFalso(), arm);
    await c.sincronizar({ loterias: ['quina'] });
    expect(arm.ePersistente).toBe(false);
    expect(arm.aviso).toMatch(/negado/);
    const cache = await c.lerCache({ loterias: ['quina'] });
    expect(cache?.indice.loterias.quina?.ultimoConcurso).toBe(7141);
  });

  it('falha de abertura assíncrona (onerror) também cai na memória', async () => {
    const quebrada = {
      open() {
        const req = {} as { onerror?: (e: Event) => void };
        queueMicrotask(() => {
          Object.defineProperty(req, 'error', { value: new Error('boom') });
          req.onerror?.(new Event('error'));
        });
        return req;
      },
    } as unknown as IDBFactory;
    const arm = new ArmazenamentoResiliente(quebrada);
    expect(await arm.ler('a')).toBeNull();
    await arm.guardar({ caminho: 'a', sha256: 'x', bytes: new Uint8Array([1]) });
    expect((await arm.ler('a'))?.sha256).toBe('x');
    expect(arm.aviso).toMatch(/boom/);
  });

  it('IndexedDB saudável: persistente e sem aviso', async () => {
    const arm = new ArmazenamentoResiliente(new IDBFactory());
    await arm.guardar({ caminho: 'a', sha256: 'x', bytes: new Uint8Array([1]) });
    expect(arm.ePersistente).toBe(true);
    expect(arm.aviso).toBeNull();
  });
});

describe('estado de sincronização', () => {
  const comCache = async () => {
    const c = cliente(new ServidorFalso(), new ArmazenamentoEmMemoria());
    const resultado = await c.sincronizar();
    return { resultado, cache: await c.lerCache() };
  };

  it('atualizado: data de agora, no horário de Brasília', async () => {
    const { resultado, cache } = await comCache();
    expect(estadoSincronizacao({ resultado, cache, agora: T1 })).toEqual({
      tipo: 'atualizado',
      dataDosDados: T1.toISOString(),
      texto: 'Atualizado em 14/10/2026 09:30.',
    });
  });

  it('sem rede com cache: mostra a data do cache', async () => {
    const { cache } = await comCache();
    const e = estadoSincronizacao({ erro: new ErroDados('rede', 'x'), cache, agora: T2 });
    expect(e.tipo).toBe('sem_rede');
    expect(e.texto).toBe('Sem rede, mostrando dados de 14/10/2026 09:30.');
  });

  it('HTTP 500 no índice e erro desconhecido contam como sem rede', async () => {
    const { cache } = await comCache();
    const http = estadoSincronizacao({ erro: new ErroDados('http', 'x'), cache, agora: T2 });
    const outro = estadoSincronizacao({ erro: new TypeError('?'), cache, agora: T2 });
    expect(http.tipo).toBe('sem_rede');
    expect(outro.tipo).toBe('sem_rede');
  });

  it('sem rede e sem cache: ainda sem dados', () => {
    const e = estadoSincronizacao({ erro: new ErroDados('rede', 'x'), cache: null, agora: T2 });
    expect(e).toEqual({
      tipo: 'sem_dados',
      dataDosDados: null,
      texto: textos.sincronizacao.semDados,
    });
  });

  it.each(['hash', 'bytes', 'json_invalido', 'forma', 'schema_version'] as const)(
    'índice recusado (%s) com cache: dado recusado, mantendo os anteriores',
    async (codigo) => {
      const { cache } = await comCache();
      const e = estadoSincronizacao({ erro: new ErroDados(codigo, 'x'), cache, agora: T2 });
      expect(e.tipo).toBe('recusado');
      expect(e.texto).toBe(
        'Dado recusado (falha de conferência), mantendo os anteriores, de 14/10/2026 09:30.',
      );
    },
  );

  it('recusa sem cache nenhum vira ainda sem dados', () => {
    const e = estadoSincronizacao({ erro: new ErroDados('hash', 'x'), cache: null, agora: T2 });
    expect(e.tipo).toBe('sem_dados');
  });

  it('falha de arquivo no resultado: rede -> sem_rede; hash -> recusado', async () => {
    const { resultado, cache } = await comCache();
    const falha = (codigo: 'rede' | 'hash') => ({
      ...resultado,
      loterias: {
        quina: {
          ...resultado.loterias.quina!,
          arquivos: {
            ultimo: { estado: 'falhou' as const, erro: new ErroDados(codigo, 'x'), dados: null },
          },
        },
      },
    });
    expect(estadoSincronizacao({ resultado: falha('rede'), cache, agora: T2 }).tipo).toBe(
      'sem_rede',
    );
    expect(estadoSincronizacao({ resultado: falha('hash'), cache, agora: T2 }).tipo).toBe(
      'recusado',
    );
  });

  it('data desconhecida no cache: texto sem data', async () => {
    const { cache } = await comCache();
    const sem = cache ? { ...cache, sincronizadoEm: null, indiceSincronizadoEm: null } : null;
    const e = estadoSincronizacao({ erro: new ErroDados('rede', 'x'), cache: sem, agora: T2 });
    expect(e.texto).toBe('Sem rede, mostrando os últimos dados salvos.');
    expect(e.dataDosDados).toBeNull();
  });

  it('estado inicial (sincronizando) com e sem cache', async () => {
    const { cache } = await comCache();
    expect(estadoInicial(null).texto).toBe('Verificando atualizações…');
    expect(estadoInicial(cache).texto).toBe(
      'Verificando atualizações… Mostrando dados de 14/10/2026 09:30.',
    );
  });

  it('formatação de datas', () => {
    expect(formatarDataHora('2026-01-15T02:05:00Z')).toBe('14/01/2026 23:05');
    expect(formatarDataHora('lixo')).toBeNull();
    expect(formatarDataSorteio('2026-10-07')).toBe('07/10/2026');
  });
});
