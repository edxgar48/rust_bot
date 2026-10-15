// @vitest-environment node
// Conferência automática (6.4) de ponta a ponta, sem rede: servidor falso com as
// fixtures reais mais concursos novos SINTÉTICOS (a Quina real termina no 7141),
// jogos salvos em IndexedDB falso (reabrir = recarregar a página) e cache de dados.
import { IDBFactory } from 'fake-indexeddb';
import { describe, expect, it } from 'vitest';
import { abrirComCache } from '../src/dados/abertura';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../src/dados/cliente';
import { OPCOES_DA_ABERTURA, garantirArquivos } from '../src/dados/sob-demanda';
import { criarJogo, type JogoSalvo } from '../src/jogos/jogo';
import { calcularNovidades } from '../src/jogos/novidades';
import { RepositorioEmMemoria } from '../src/jogos/repositorio';
import { RepositorioResiliente } from '../src/jogos/repositorio-idb';
import { marcarComoVisto, salvarJogo } from '../src/jogos/servico';
import { ServidorFalso, URL_BASE_TESTE } from './servidor-falso';

const T1 = new Date('2026-10-14T12:30:00Z');
const semRede = async (): Promise<Response> => {
  throw new TypeError('Failed to fetch');
};

function cliente(servidor: ServidorFalso, arm: ArmazenamentoEmMemoria, rede = true) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: rede ? servidor.fetch : semRede,
    armazenamento: arm,
    agora: () => T1,
  });
}

const abrir = (c: ReturnType<typeof cliente>) =>
  abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );

function jogoQuina(id: string, dezenas: number[], conferidoAte: number | null): JogoSalvo {
  const r = criarJogo({ loteria: 'quina', dezenas, conferidoAte }, id, T1);
  if (!r.ok) throw new Error('fixture inválida');
  return r.valor;
}

const A = [1, 18, 2, 3, 4]; // no 7142 sintético: 1 e 18 = 2 acertos
const B = [40, 41, 42, 43, 44]; // no 7142 sintético: 40, 41 e 42 = 3 acertos
const DEZENAS_7142 = [1, 18, 40, 41, 42];

describe('concurso novo chega pelo sync', () => {
  it('abertura em dia: nada novo; jogo sem marca recebe o último concurso conhecido', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('antigo', A, null), jogoQuina('em-dia', B, 7141)]);

    await abrir(c);
    expect(await calcularNovidades(repo, c)).toEqual([]);

    const salvos = await repo.listar();
    expect(salvos.find((j) => j.id === 'antigo')?.conferidoAte).toBe(7141);
    expect(salvos.find((j) => j.id === 'em-dia')?.conferidoAte).toBe(7141);
    // Só o ultimo.json foi baixado: o histórico não é baixado só para conferir.
    expect(servidor.pedidosDeDados().every((p) => p.endsWith('/ultimo.json'))).toBe(true);
  });

  it('7142 chega: confere os jogos salvos só com o ultimo.json (sem baixar o histórico)', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141), jogoQuina('b', B, 7141)]);
    await abrir(c);
    expect(await calcularNovidades(repo, c)).toEqual([]);

    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    servidor.pedidos.length = 0;
    await abrir(c);
    const novidades = await calcularNovidades(repo, c);

    expect(servidor.pedidosDeDados()).toEqual(['quina/ultimo.json']);
    expect(novidades).toHaveLength(1);
    const q = novidades[0]!;
    expect(q.slug).toBe('quina');
    expect(q.ultimoConcurso).toBe(7142);
    expect(q.conferidos).toBe(2);
    expect(q.pendentes).toBe(0);
    expect(q.melhorAcertos).toBe(3);
    expect(q.naFaixaPremio).toBe(2); // Quina: 2 ou mais acertos
    expect(q.jogos.find((j) => j.jogoId === 'a')?.resultados).toEqual([
      { concurso: 7142, acertos: 2 },
    ]);
    expect(q.jogos.find((j) => j.jogoId === 'b')?.melhor).toEqual({ concurso: 7142, acertos: 3 });
  });

  it('não marca sozinho: calcular de novo continua mostrando; marcar como visto zera', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141), jogoQuina('b', B, 7141)]);
    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    await abrir(c);

    expect(await calcularNovidades(repo, c)).toHaveLength(1);
    expect(await calcularNovidades(repo, c)).toHaveLength(1);

    await marcarComoVisto(repo, 'a', 7142);
    const depois = await calcularNovidades(repo, c);
    expect(depois[0]?.jogos.map((j) => j.jogoId)).toEqual(['b']);
    await marcarComoVisto(repo, 'b', 7142);
    expect(await calcularNovidades(repo, c)).toEqual([]);
  });

  it('a marca sobrevive a recarregar (IndexedDB) e ao próximo sync', async () => {
    const fabrica = new IDBFactory();
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);

    // Fluxo da tela: o jogo é salvo com o último concurso do histórico mostrado.
    const antes = new RepositorioResiliente(fabrica);
    const salvo = await salvarJogo(antes, { loteria: 'quina', dezenas: A, conferidoAte: 7141 });
    expect(salvo.tipo === 'salvo' && salvo.jogo.conferidoAte).toBe(7141);

    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    await abrir(c);
    expect((await calcularNovidades(antes, c))[0]?.conferidos).toBe(1);
    const id = (await antes.listar())[0]!.id;
    await marcarComoVisto(antes, id, 7142);

    // "Recarregar a página": repositório novo sobre o mesmo banco.
    const depois = new RepositorioResiliente(fabrica);
    expect((await depois.listar())[0]?.conferidoAte).toBe(7142);
    expect(await calcularNovidades(depois, c)).toEqual([]);

    // Um concurso depois, só ele é novidade (o 7142 já foi visto).
    await servidor.publicarConcursoQuina(7143, [1, 2, 3, 4, 5]);
    await abrir(c);
    const n = await calcularNovidades(depois, c);
    expect(n[0]?.jogos[0]?.resultados).toEqual([{ concurso: 7143, acertos: 4 }]);
  });

  it('vários concursos de uma vez: sem histórico fica pendente; com histórico, confere todos', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141)]);
    await abrir(c);

    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    await servidor.publicarConcursoQuina(7143, [1, 2, 18, 60, 61], '2026-10-16');
    await abrir(c);

    const pendente = await calcularNovidades(repo, c);
    expect(pendente[0]?.pendentes).toBe(1);
    expect(pendente[0]?.conferidos).toBe(0);
    expect(pendente[0]?.melhorAcertos).toBeNull();

    // Meus jogos (ou o segundo plano) baixa o histórico; a conferência fica completa.
    await garantirArquivos(c, { arquivos: ['historico'], loterias: ['quina'] });
    const completo = await calcularNovidades(repo, c);
    expect(completo[0]?.pendentes).toBe(0);
    expect(completo[0]?.concursosNovos).toBe(2);
    expect(completo[0]?.jogos[0]?.resultados).toEqual([
      { concurso: 7143, acertos: 3 }, // 1, 18 e 2
      { concurso: 7142, acertos: 2 },
    ]);
    expect(completo[0]?.jogos[0]?.melhor).toEqual({ concurso: 7143, acertos: 3 });
  });
});

describe('só lê o histórico em cache quando precisa', () => {
  async function preparar(marca: number) {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    // Histórico já guardado (até 7141); depois sai o 7142 e só o ultimo.json é atualizado.
    await abrir(c);
    await garantirArquivos(c, { arquivos: ['historico'], loterias: ['quina'] });
    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, marca)]);
    const chamadas: string[][] = [];
    const fonte = {
      lerCache: (o: OpcoesSincronizacao) => {
        chamadas.push([...(o.arquivos ?? [])]);
        return c.lerCache(o);
      },
    };
    return { repo, fonte, chamadas };
  }

  it('um concurso novo: só o ultimo.json é lido, o histórico em cache nem é aberto', async () => {
    const { repo, fonte, chamadas } = await preparar(7141);
    const n = await calcularNovidades(repo, fonte);
    expect(chamadas).toEqual([['ultimo']]);
    expect(n[0]?.conferidos).toBe(1);
    expect(n[0]?.jogos[0]?.resultados).toEqual([{ concurso: 7142, acertos: 2 }]);
  });

  it('mais de um concurso atrás: o histórico é lido', async () => {
    const { repo, fonte, chamadas } = await preparar(7140);
    const n = await calcularNovidades(repo, fonte);
    expect(chamadas).toEqual([['ultimo'], ['historico']]);
    expect(n[0]?.jogos[0]?.totalConcursos).toBe(2);
  });
});

describe('gravações concorrentes não são desfeitas', () => {
  it('a marca inicial não desfaz uma renomeação nem um "marcar como visto" feitos durante o cálculo', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, null), jogoQuina('b', B, null)]);

    // Entre a listagem e a gravação da marca (a leitura do cache), a pessoa mexe nos jogos.
    const fonte = {
      lerCache: async (o: OpcoesSincronizacao) => {
        const r = await c.lerCache(o);
        await repo.renomear('a', 'Renomeado');
        await marcarComoVisto(repo, 'b', 7141);
        return r;
      },
    };
    await calcularNovidades(repo, fonte);

    const salvos = await repo.listar();
    expect(salvos.find((j) => j.id === 'a')).toMatchObject({
      nome: 'Renomeado',
      conferidoAte: 7141,
    });
    expect(salvos.find((j) => j.id === 'b')?.conferidoAte).toBe(7141);
  });

  it('marcarComoVisto e renomear ao mesmo tempo: os dois efeitos ficam (IndexedDB)', async () => {
    const fabrica = new IDBFactory();
    const repo = new RepositorioResiliente(fabrica);
    await repo.gravar([jogoQuina('a', A, 7141)]);
    await Promise.all([marcarComoVisto(repo, 'a', 7142), repo.renomear('a', 'Novo nome')]);
    expect((await repo.listar())[0]).toMatchObject({ nome: 'Novo nome', conferidoAte: 7142 });
  });
});

describe('offline e dado inválido', () => {
  it('sem rede, confere com o último cache válido', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await servidor.publicarConcursoQuina(7142, DEZENAS_7142);
    await abrir(cliente(servidor, arm));

    const offline = cliente(servidor, arm, false);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141)]);
    await abrir(offline);
    const n = await calcularNovidades(repo, offline);
    expect(n[0]?.ultimoConcurso).toBe(7142);
    expect(n[0]?.melhorAcertos).toBe(2);
  });

  it('ultimo.json adulterado nunca substitui o cache válido: nenhuma novidade falsa', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const c = cliente(servidor, arm);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141)]);
    await abrir(c);

    // O índice continua com o hash do original; o servidor entrega outro conteúdo.
    await servidor.trocar(
      'quina/ultimo.json',
      JSON.stringify({
        schema_version: 1,
        loteria: 'quina',
        concurso: 9999,
        data: '2026-10-15',
        dezenas: [1, 2, 3, 4, 5],
      }),
      false,
    );
    await abrir(c);
    expect(await calcularNovidades(repo, c)).toEqual([]);
  });

  it('sem cache nenhum: não inventa novidade nem marca', async () => {
    const servidor = new ServidorFalso();
    const c = cliente(servidor, new ArmazenamentoEmMemoria(), false);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, null)]);
    expect(await calcularNovidades(repo, c)).toEqual([]);
    expect((await repo.listar())[0]?.conferidoAte).toBeNull();
  });
});

describe('marcarComoVisto', () => {
  it('só avança a marca; jogo inexistente devolve null; nome e dezenas não mudam', async () => {
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, 7141)]);
    const atual = await marcarComoVisto(repo, 'a', 7143);
    expect(atual?.conferidoAte).toBe(7143);
    expect(atual?.dezenas).toEqual([1, 2, 3, 4, 18]);
    expect((await marcarComoVisto(repo, 'a', 7142))?.conferidoAte).toBe(7143);
    expect((await repo.listar())[0]?.conferidoAte).toBe(7143);
    expect(await marcarComoVisto(repo, 'nao-existe', 7143)).toBeNull();
  });

  it('jogo sem marca recebe a marca', async () => {
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', A, null)]);
    expect((await marcarComoVisto(repo, 'a', 7141))?.conferidoAte).toBe(7141);
  });
});
