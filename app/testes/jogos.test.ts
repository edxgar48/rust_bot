// @vitest-environment node
// Jogos salvos (6.3): persistência em IndexedDB (reabrir o armazenamento), isolamento
// em relação ao cache de dados do contrato, exportar/importar ida e volta, arquivo
// adulterado e armazenamento persistente. Sem rede (servidor falso).
import { IDBFactory } from 'fake-indexeddb';
import { describe, expect, it, vi } from 'vitest';
import { ArmazenamentoIndexedDB, NOME_BANCO } from '../src/dados/armazenamento-idb';
import { criarCliente } from '../src/dados/cliente';
import { FORMATO_ARQUIVO } from '../src/jogos/arquivo';
import { criarJogo, type JogoSalvo } from '../src/jogos/jogo';
import { RepositorioEmMemoria, type RepositorioJogos } from '../src/jogos/repositorio';
import {
  NOME_BANCO_JOGOS,
  RepositorioIndexedDB,
  RepositorioResiliente,
} from '../src/jogos/repositorio-idb';
import {
  exportarJogos,
  importarJogos,
  pedirArmazenamentoPersistente,
  salvarJogo,
} from '../src/jogos/servico';
import { ServidorFalso, URL_BASE_TESTE } from './servidor-falso';

const T0 = new Date('2026-10-15T12:00:00Z');

function ids(prefixo = 'j'): () => string {
  let n = 0;
  return () => `${prefixo}${++n}`;
}

function jogoValido(id: string, dezenas: number[], extra: Partial<JogoSalvo> = {}): JogoSalvo {
  const r = criarJogo({ loteria: 'quina', dezenas }, id, new Date(T0.getTime() + dezenas[0]!));
  if (!r.ok) throw new Error('fixture inválida');
  return { ...r.valor, ...extra };
}

const Q1 = [1, 2, 3, 4, 5];
const Q2 = [10, 20, 30, 40, 50];
const M1 = [1, 2, 3, 4, 5, 6];

/** Executa a mesma bateria contra qualquer repositório. */
function comportamentoComum(nome: string, criar: () => RepositorioJogos): void {
  describe(`repositório: ${nome}`, () => {
    it('lista vazia, grava, lista por loteria e do mais recente ao mais antigo', async () => {
      const repo = criar();
      expect(await repo.listar()).toEqual([]);
      const a = jogoValido('a', Q1);
      const b = jogoValido('b', Q2);
      const m = criarJogo({ loteria: 'megasena', dezenas: M1 }, 'm', T0);
      if (!m.ok) throw new Error('fixture');
      await repo.gravar([a, b, m.valor]);
      expect((await repo.listar()).map((j) => j.id)).toEqual(['b', 'a', 'm']);
      expect((await repo.listar('quina')).map((j) => j.id)).toEqual(['b', 'a']);
      expect((await repo.listar('megasena')).map((j) => j.id)).toEqual(['m']);
      expect(await repo.listar('lotofacil')).toEqual([]);
    });

    it('renomear troca só o nome (e aceita limpar); id inexistente devolve null', async () => {
      const repo = criar();
      const a = jogoValido('a', Q1);
      await repo.gravar([a]);
      expect(await repo.renomear('a', 'Meu jogo')).toEqual({ ...a, nome: 'Meu jogo' });
      expect((await repo.listar())[0]).toEqual({ ...a, nome: 'Meu jogo' });
      expect((await repo.renomear('a', null))?.nome).toBeNull();
      expect(await repo.renomear('nao-existe', 'x')).toBeNull();
      expect(await repo.listar()).toHaveLength(1);
    });

    it('apagar remove só aquele jogo; apagar de novo não dá erro', async () => {
      const repo = criar();
      await repo.gravar([jogoValido('a', Q1), jogoValido('b', Q2)]);
      await repo.apagar('a');
      await repo.apagar('a');
      expect((await repo.listar()).map((j) => j.id)).toEqual(['b']);
    });

    it('gravar com o mesmo id substitui (usado pelo desfazer)', async () => {
      const repo = criar();
      const a = jogoValido('a', Q1);
      await repo.gravar([a]);
      await repo.apagar('a');
      await repo.gravar([a]);
      expect(await repo.listar()).toEqual([a]);
    });
  });
}

comportamentoComum('memória', () => new RepositorioEmMemoria());
comportamentoComum('IndexedDB', () => new RepositorioIndexedDB(new IDBFactory()));
comportamentoComum('resiliente sobre IndexedDB', () => new RepositorioResiliente(new IDBFactory()));
comportamentoComum('resiliente em memória (sem IndexedDB)', () => new RepositorioResiliente(null));

describe('persistência: reabrir o armazenamento (recarregar a página)', () => {
  it('os jogos, nomes e datas voltam de uma nova instância sobre o mesmo banco', async () => {
    const fabrica = new IDBFactory();
    const primeira = new RepositorioIndexedDB(fabrica);
    const a = jogoValido('a', Q1, { nome: 'Aniversário' });
    const b = jogoValido('b', Q2);
    await primeira.gravar([a, b]);
    await primeira.renomear('b', 'Segundo');
    await primeira.apagar('a');
    await primeira.fechar();

    const segunda = new RepositorioIndexedDB(fabrica);
    expect(await segunda.listar()).toEqual([{ ...b, nome: 'Segundo' }]);
    await segunda.fechar();
  });

  it('registro com forma estranha no banco é pulado e NÃO é apagado', async () => {
    const fabrica = new IDBFactory();
    const repo = new RepositorioIndexedDB(fabrica);
    await repo.gravar([jogoValido('bom', Q1)]);
    const banco = await new Promise<IDBDatabase>((ok, ruim) => {
      const p = fabrica.open(NOME_BANCO_JOGOS, 1);
      p.onsuccess = () => ok(p.result);
      p.onerror = () => ruim(p.error);
    });
    await new Promise<void>((ok) => {
      const t = banco.transaction('jogos', 'readwrite');
      t.objectStore('jogos').put({ id: 'ruim', loteria: 'quina', dezenas: 'texto' });
      t.oncomplete = () => ok();
    });
    expect((await repo.listar()).map((j) => j.id)).toEqual(['bom']);
    const restante = await new Promise<number>((ok) => {
      const r = banco.transaction('jogos').objectStore('jogos').count();
      r.onsuccess = () => ok(r.result);
    });
    expect(restante).toBe(2);
    banco.close();
  });
});

describe('isolamento do cache de dados do contrato', () => {
  it('usa outro banco, e o banco de dados não tem a loja de jogos', async () => {
    expect(NOME_BANCO_JOGOS).not.toBe(NOME_BANCO);
    const fabrica = new IDBFactory();
    await new RepositorioIndexedDB(fabrica).gravar([jogoValido('a', Q1)]);
    await new ArmazenamentoIndexedDB(fabrica).guardar({
      caminho: 'x',
      sha256: 'y',
      bytes: new Uint8Array([1]),
    });
    const bancos = (await fabrica.databases()).map((b) => b.name).sort();
    expect(bancos).toEqual([NOME_BANCO, NOME_BANCO_JOGOS].sort());
  });

  it('atualizar os dados (nova versão do contrato) e limpar o cache de dados não tocam nos jogos', async () => {
    const fabrica = new IDBFactory();
    const jogos = new RepositorioIndexedDB(fabrica);
    await jogos.gravar([jogoValido('a', Q1, { nome: 'Fica' }), jogoValido('b', Q2)]);
    const antes = await jogos.listar();

    const servidor = new ServidorFalso();
    const dados = new ArmazenamentoIndexedDB(fabrica);
    const cliente = criarCliente({
      urlBase: URL_BASE_TESTE,
      fetch: servidor.fetch,
      armazenamento: dados,
      agora: () => T0,
    });
    // 1) sincroniza (grava o cache de dados) e sincroniza de novo com índice novo
    await cliente.sincronizar();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, gerado_em: '2026-10-16T00:00:00Z' });
    await cliente.sincronizar();
    expect(await jogos.listar()).toEqual(antes);

    // 2) limpa o cache de dados por inteiro (apaga o banco de dados do contrato)
    await dados.fechar();
    await new Promise<void>((ok, ruim) => {
      const p = fabrica.deleteDatabase(NOME_BANCO);
      p.onsuccess = () => ok();
      p.onerror = () => ruim(p.error);
    });
    expect(await jogos.listar()).toEqual(antes);

    // 3) e uma nova instância, como após recarregar, ainda os enxerga
    await jogos.fechar();
    expect(await new RepositorioIndexedDB(fabrica).listar()).toEqual(antes);
  });
});

describe('resiliência', () => {
  it('sem IndexedDB: funciona na memória e avisa', async () => {
    const repo = new RepositorioResiliente(null);
    expect(repo.ePersistente).toBe(false);
    expect(repo.aviso).toContain('indisponível');
    await repo.gravar([jogoValido('a', Q1)]);
    expect(await repo.listar()).toHaveLength(1);
  });

  it('IndexedDB que falha ao abrir: cai para memória, avisa e não lança', async () => {
    const quebrada = {
      open: () => {
        throw new Error('bloqueado');
      },
    } as unknown as IDBFactory;
    const repo = new RepositorioResiliente(quebrada);
    expect(repo.ePersistente).toBe(true);
    expect(repo.aviso).toBeNull();
    await repo.gravar([jogoValido('a', Q1)]);
    expect(repo.ePersistente).toBe(false);
    expect(repo.aviso).toContain('bloqueado');
    expect((await repo.listar()).map((j) => j.id)).toEqual(['a']);
  });
});

/** IndexedDB real (falso) cujas transações passam a falhar depois que o banco abriu. */
function fabricaQueFalhaDepoisDeAbrir() {
  const real = new IDBFactory();
  const estado = { falhasRestantes: 0, aberturas: 0 };
  const fabrica = {
    open(nome: string, versao?: number) {
      const pedido = real.open(nome, versao);
      estado.aberturas++;
      pedido.addEventListener('success', () => {
        const banco = pedido.result;
        const original = banco.transaction.bind(banco);
        banco.transaction = ((...args: Parameters<IDBDatabase['transaction']>) => {
          if (estado.falhasRestantes > 0) {
            estado.falhasRestantes--;
            throw new DOMException('conexão perdida', 'InvalidStateError');
          }
          return original(...args);
        }) as IDBDatabase['transaction'];
      });
      return pedido;
    },
  } as unknown as IDBFactory;
  return { fabrica, estado };
}

describe('resiliência: abre e falha depois', () => {
  it('leitura que falha uma vez é repetida e funciona, sem cair para memória', async () => {
    const { fabrica, estado } = fabricaQueFalhaDepoisDeAbrir();
    const repo = new RepositorioResiliente(fabrica);
    await repo.gravar([jogoValido('a', Q1)]);
    estado.falhasRestantes = 1;
    expect((await repo.listar()).map((j) => j.id)).toEqual(['a']);
    expect(repo.ePersistente).toBe(true);
    expect(repo.aviso).toBeNull();
    expect(estado.aberturas).toBe(2); // a conexão foi refeita
  });

  it('leitura que continua falhando lança, não finge lista vazia e não vira memória', async () => {
    const { fabrica, estado } = fabricaQueFalhaDepoisDeAbrir();
    const repo = new RepositorioResiliente(fabrica);
    await repo.gravar([jogoValido('a', Q1)]);
    estado.falhasRestantes = 99;
    await expect(repo.listar()).rejects.toThrow('conexão perdida');
    await expect(repo.listarIds()).rejects.toThrow();
    await expect(exportarJogos(repo, T0)).rejects.toThrow();
    expect(repo.ePersistente).toBe(true);
    expect(repo.aviso).toBeNull();
    // Quando o banco volta, os jogos reais continuam lá.
    estado.falhasRestantes = 0;
    expect((await repo.listar()).map((j) => j.id)).toEqual(['a']);
  });

  it('gravação que continua falhando lança e não grava só na memória', async () => {
    const { fabrica, estado } = fabricaQueFalhaDepoisDeAbrir();
    const repo = new RepositorioResiliente(fabrica);
    await repo.gravar([jogoValido('a', Q1)]);
    estado.falhasRestantes = 99;
    await expect(repo.gravar([jogoValido('b', Q2)])).rejects.toThrow('conexão perdida');
    await expect(repo.apagar('a')).rejects.toThrow();
    await expect(repo.renomear('a', 'x')).rejects.toThrow();
    expect(repo.ePersistente).toBe(true);
    expect(repo.aviso).toBeNull();
    estado.falhasRestantes = 0;
    expect((await repo.listar()).map((j) => j.id)).toEqual(['a']);
  });

  it('gravação que falha uma vez é repetida e persiste', async () => {
    const { fabrica, estado } = fabricaQueFalhaDepoisDeAbrir();
    const repo = new RepositorioResiliente(fabrica);
    await repo.listar();
    estado.falhasRestantes = 1;
    await repo.gravar([jogoValido('b', Q2)]);
    expect((await new RepositorioIndexedDB(fabrica).listar()).map((j) => j.id)).toEqual(['b']);
  });
});

describe('renomear grava o nome limpo (memória e IndexedDB)', () => {
  it.each([
    ['memória', () => new RepositorioEmMemoria()],
    ['IndexedDB', () => new RepositorioIndexedDB(new IDBFactory())],
  ])('%s', async (_n, criar) => {
    const repo = criar();
    await repo.gravar([jogoValido('a', Q1)]);
    expect((await repo.renomear('a', '  Meu \n  jogo  '))?.nome).toBe('Meu jogo');
    expect((await repo.listar())[0]!.nome).toBe('Meu jogo');
    expect((await repo.renomear('a', '   '))?.nome).toBeNull();
    // nome longo demais: recusado, nada muda
    await repo.renomear('a', 'Fica');
    expect(await repo.renomear('a', 'x'.repeat(41))).toBeNull();
    expect((await repo.listar())[0]!.nome).toBe('Fica');
  });
});

describe('importação não sobrescreve registro inválido com o mesmo id', () => {
  async function banco(fabrica: IDBFactory): Promise<IDBDatabase> {
    return new Promise((ok, ruim) => {
      const p = fabrica.open(NOME_BANCO_JOGOS, 1);
      p.onsuccess = () => ok(p.result);
      p.onerror = () => ruim(p.error);
    });
  }

  it('o item do arquivo com id de um registro inválido recebe id novo', async () => {
    const fabrica = new IDBFactory();
    const repo = new RepositorioIndexedDB(fabrica);
    await repo.gravar([jogoValido('bom', Q1)]);
    const db = await banco(fabrica);
    await new Promise<void>((ok) => {
      const t = db.transaction('jogos', 'readwrite');
      t.objectStore('jogos').put({ id: 'ruim', loteria: 'quina', dezenas: 'texto', extra: 42 });
      t.oncomplete = () => ok();
    });
    expect((await repo.listarIds()).sort()).toEqual(['bom', 'ruim']);

    const texto = JSON.stringify({
      formato: FORMATO_ARQUIVO,
      versao: 1,
      jogos: [
        { id: 'ruim', loteria: 'quina', nome: null, dezenas: Q2, criadoEm: T0.toISOString() },
      ],
    });
    expect(await importarJogos(repo, texto, ids('novo'))).toMatchObject({ entraram: 1 });
    const salvos = await repo.listarIds();
    expect(salvos.sort()).toEqual(['bom', 'novo1', 'ruim']);
    const bruto = await new Promise<{ extra?: number }>((ok) => {
      const r = db.transaction('jogos').objectStore('jogos').get('ruim');
      r.onsuccess = () => ok(r.result);
    });
    expect(bruto.extra).toBe(42); // o registro inválido continua intacto
    db.close();
  });
});

describe('salvarJogo', () => {
  it('salva, ordena as dezenas, marca o primeiro jogo e depois não repete', async () => {
    const repo = new RepositorioEmMemoria();
    const r1 = await salvarJogo(
      repo,
      { loteria: 'quina', dezenas: [5, 4, 3, 2, 1], nome: ' A ' },
      T0,
      ids(),
    );
    expect(r1).toMatchObject({ tipo: 'salvo', primeiro: true });
    if (r1.tipo !== 'salvo') throw new Error();
    expect(r1.jogo).toMatchObject({ id: 'j1', nome: 'A', dezenas: Q1, criadoEm: T0.toISOString() });

    const r2 = await salvarJogo(repo, { loteria: 'quina', dezenas: Q2 }, T0, ids('k'));
    expect(r2).toMatchObject({ tipo: 'salvo', primeiro: false });
  });

  it('jogo idêntico não duplica: devolve o existente e não grava', async () => {
    const repo = new RepositorioEmMemoria();
    await salvarJogo(repo, { loteria: 'quina', dezenas: Q1, nome: 'Primeiro' }, T0, ids());
    const r = await salvarJogo(repo, { loteria: 'quina', dezenas: [3, 1, 2, 5, 4] }, T0, ids('k'));
    expect(r).toMatchObject({ tipo: 'duplicado', existente: { nome: 'Primeiro' } });
    expect(await repo.listar()).toHaveLength(1);
    // outra loteria com dezenas parecidas não é duplicado
    expect((await salvarJogo(repo, { loteria: 'megasena', dezenas: M1 }, T0, ids('m'))).tipo).toBe(
      'salvo',
    );
  });

  it('quantidade inválida e nome grande demais não gravam nada', async () => {
    const repo = new RepositorioEmMemoria();
    expect(await salvarJogo(repo, { loteria: 'quina', dezenas: [1, 2, 3, 4] })).toEqual({
      tipo: 'invalido',
      motivo: 'quantidade',
    });
    expect(await salvarJogo(repo, { loteria: 'quina', dezenas: Q1, nome: 'x'.repeat(41) })).toEqual(
      { tipo: 'invalido', motivo: 'nome' },
    );
    expect(await repo.listar()).toEqual([]);
  });
});

describe('exportar e importar', () => {
  it('ida e volta entre dois aparelhos: mesmos jogos, ids, nomes e datas', async () => {
    const origem = new RepositorioEmMemoria();
    await salvarJogo(origem, { loteria: 'quina', dezenas: Q1, nome: 'Aniversário' }, T0, ids('o'));
    await salvarJogo(
      origem,
      { loteria: 'megasena', dezenas: M1 },
      new Date(T0.getTime() + 1000),
      ids('p'),
    );
    const arquivo = await exportarJogos(origem, T0);
    expect(arquivo.quantidade).toBe(2);
    expect(arquivo.nome).toBe('jogos-forca-loterias-2026-10-15.json');

    const destino = new RepositorioIndexedDB(new IDBFactory());
    const r = await importarJogos(destino, arquivo.conteudo, ids('d'));
    expect(r).toEqual({ tipo: 'importado', entraram: 2, ignorados: 0, invalidos: 0 });
    expect(await destino.listar()).toEqual(await origem.listar());
  });

  it('importar o mesmo arquivo duas vezes não duplica nada', async () => {
    const origem = new RepositorioEmMemoria();
    await salvarJogo(origem, { loteria: 'quina', dezenas: Q1 }, T0, ids());
    const { conteudo } = await exportarJogos(origem, T0);
    const destino = new RepositorioEmMemoria();
    await importarJogos(destino, conteudo);
    const r = await importarJogos(destino, conteudo);
    expect(r).toEqual({ tipo: 'importado', entraram: 0, ignorados: 1, invalidos: 0 });
    expect(await destino.listar()).toHaveLength(1);
  });

  it('exportar sem jogos gera arquivo válido com lista vazia', async () => {
    const arquivo = await exportarJogos(new RepositorioEmMemoria(), T0);
    expect(arquivo.quantidade).toBe(0);
    expect(JSON.parse(arquivo.conteudo)).toMatchObject({ formato: FORMATO_ARQUIVO, jogos: [] });
  });

  it('arquivo com itens bons, repetidos e inválidos: relata cada grupo', async () => {
    const repo = new RepositorioEmMemoria();
    await salvarJogo(repo, { loteria: 'quina', dezenas: Q1 }, T0, ids());
    const texto = JSON.stringify({
      formato: FORMATO_ARQUIVO,
      versao: 1,
      campoFuturo: true,
      jogos: [
        { id: 'n1', loteria: 'quina', nome: null, dezenas: Q2, criadoEm: T0.toISOString(), x: 1 },
        { id: 'n2', loteria: 'quina', nome: null, dezenas: Q1, criadoEm: T0.toISOString() },
        { id: 'n3', loteria: 'quina', nome: null, dezenas: [1, 2], criadoEm: T0.toISOString() },
        'lixo',
      ],
    });
    expect(await importarJogos(repo, texto, ids('z'))).toEqual({
      tipo: 'importado',
      entraram: 1,
      ignorados: 1,
      invalidos: 2,
    });
    expect(await repo.listar()).toHaveLength(2);
  });

  describe('arquivo adulterado ou malformado é recusado sem alterar nada', () => {
    async function prepara() {
      const repo = new RepositorioEmMemoria();
      await salvarJogo(repo, { loteria: 'quina', dezenas: Q1, nome: 'Intacto' }, T0, ids());
      return { repo, antes: await repo.listar() };
    }
    const bom = (): string =>
      JSON.stringify({
        formato: FORMATO_ARQUIVO,
        versao: 1,
        jogos: [{ id: 'n', loteria: 'quina', nome: null, dezenas: Q2, criadoEm: T0.toISOString() }],
      });

    it.each([
      ['truncado', () => bom().slice(0, -7), 'malformado'],
      ['vazio', () => '', 'malformado'],
      ['não é JSON', () => 'oi', 'malformado'],
      ['lista na raiz', () => '[]', 'formato'],
      ['outro formato', () => bom().replace(FORMATO_ARQUIVO, 'outra-coisa'), 'formato'],
      ['versão futura', () => bom().replace('"versao":1', '"versao":9'), 'versao'],
      ['jogos não é lista', () => bom().replace(/"jogos":\[.*\]/, '"jogos":"x"'), 'formato'],
    ])('%s', async (_nome, gerar, motivo) => {
      const { repo, antes } = await prepara();
      const gravar = vi.spyOn(repo, 'gravar');
      expect(await importarJogos(repo, gerar())).toEqual({ tipo: 'recusado', motivo });
      expect(gravar).not.toHaveBeenCalled();
      expect(await repo.listar()).toEqual(antes);
    });

    it('um byte alterado dentro de um jogo só invalida aquele jogo', async () => {
      const { repo } = await prepara();
      const adulterado = bom().replace('"quina"', '"quinaX"');
      expect(await importarJogos(repo, adulterado)).toEqual({
        tipo: 'importado',
        entraram: 0,
        ignorados: 0,
        invalidos: 1,
      });
      expect(await repo.listar()).toHaveLength(1);
    });
  });
});

describe('pedirArmazenamentoPersistente (best-effort)', () => {
  it('concedido, negado, já persistente, sem suporte e erro: nunca lança', async () => {
    expect(
      await pedirArmazenamentoPersistente({
        persist: async () => true,
        persisted: async () => false,
      }),
    ).toBe(true);
    expect(
      await pedirArmazenamentoPersistente({
        persist: async () => false,
        persisted: async () => false,
      }),
    ).toBe(false);
    const persist = vi.fn(async () => false);
    expect(await pedirArmazenamentoPersistente({ persist, persisted: async () => true })).toBe(
      true,
    );
    expect(persist).not.toHaveBeenCalled();
    expect(await pedirArmazenamentoPersistente(undefined)).toBeNull();
    expect(
      await pedirArmazenamentoPersistente({
        persist: async () => {
          throw new Error('x');
        },
        persisted: async () => false,
      }),
    ).toBeNull();
  });
});
