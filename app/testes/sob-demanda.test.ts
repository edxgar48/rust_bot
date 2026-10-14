// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { abrirComCache } from '../src/dados/abertura';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente } from '../src/dados/cliente';
import { OPCOES_DA_ABERTURA, garantirArquivos, precarregar } from '../src/dados/sob-demanda';
import { ServidorFalso, URL_BASE_TESTE } from './servidor-falso';

const T1 = new Date('2026-10-14T12:30:00Z');

function cliente(servidor: ServidorFalso, arm = new ArmazenamentoEmMemoria(), rede = true) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: rede
      ? servidor.fetch
      : async () => {
          throw new TypeError('Failed to fetch');
        },
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

describe('abertura enxuta', () => {
  it('baixa só index.json e o ultimo.json das 3 loterias', async () => {
    const servidor = new ServidorFalso();
    const final = await abrir(cliente(servidor));
    expect(servidor.pedidos.sort()).toEqual([
      'index.json',
      'lotofacil/ultimo.json',
      'megasena/ultimo.json',
      'quina/ultimo.json',
    ]);
    expect(final.estado.tipo).toBe('atualizado');
    expect(final.cache?.loterias.quina?.dados.ultimo?.concurso).toBe(7141);
    expect(final.cache?.loterias.quina?.dados.tabela_geral).toBeUndefined();
  });

  it('retrocompatível: sem opções, abrirComCache continua baixando tudo', async () => {
    const servidor = new ServidorFalso();
    const final = await abrirComCache(cliente(servidor), () => undefined);
    expect(servidor.pedidosDeDados()).toHaveLength(9);
    expect(final.cache?.loterias.quina?.dados.historico).not.toBeNull();
  });

  it('segunda abertura (nada mudou): só o índice é baixado', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(servidor, arm));
    servidor.pedidos.length = 0;
    await abrir(cliente(servidor, arm));
    expect(servidor.pedidos).toEqual(['index.json']);
  });

  it('o estado e a data "dados de" não dependem do que ainda não foi baixado', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(servidor, arm));
    const offline = await abrir(cliente(servidor, arm, false));
    expect(offline.estado.tipo).toBe('sem_rede');
    expect(offline.estado.texto).toBe('Sem rede, mostrando dados de 14/10/2026 09:30.');
  });
});

describe('garantirArquivos (sob demanda)', () => {
  it('baixa só o pedido e devolve no cache', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(servidor, arm));
    servidor.pedidos.length = 0;
    const r = await garantirArquivos(cliente(servidor, arm), {
      loterias: ['quina'],
      arquivos: ['tabela_geral'],
    });
    expect(servidor.pedidos.sort()).toEqual(['index.json', 'quina/tabela_geral.json']);
    expect(r.falhou).toBe(false);
    expect(r.cache?.loterias.quina?.dados.tabela_geral?.dezenas.length).toBe(80);
  });

  it('já em cache e vigente: só o índice é baixado', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await garantirArquivos(cliente(servidor, arm), { arquivos: ['historico'] });
    servidor.pedidos.length = 0;
    await garantirArquivos(cliente(servidor, arm), { arquivos: ['historico'] });
    expect(servidor.pedidos).toEqual(['index.json']);
  });

  it('sem rede: o que já está em cache continua disponível, sem lançar', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await garantirArquivos(cliente(servidor, arm), { arquivos: ['tabela_geral'] });
    const r = await garantirArquivos(cliente(servidor, arm, false), { arquivos: ['tabela_geral'] });
    expect(r.falhou).toBe(true);
    expect(r.cache?.loterias.megasena?.dados.tabela_geral?.dezenas.length).toBe(60);
  });

  it('sem rede e sem cache: devolve cache nulo, sem lançar', async () => {
    const r = await garantirArquivos(cliente(new ServidorFalso(), undefined, false), {
      arquivos: ['historico'],
    });
    expect(r).toEqual({ cache: null, falhou: true, falha: 'sem_rede' });
  });

  it('arquivo adulterado: falhou, e o guardado não é trocado', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await garantirArquivos(cliente(servidor, arm), { arquivos: ['historico'] });
    const antes = await arm.ler('quina/historico.json');
    const trocado = servidor.arquivos.get('quina/historico.json')!.slice();
    trocado[trocado.length - 3] = (trocado[trocado.length - 3] ?? 0) ^ 1;
    // Índice novo com hash diferente; conteúdo servido não confere com ele.
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.historico.sha256 = 'b'.repeat(64);
    servidor.gravarIndice(indice);
    servidor.arquivos.set('quina/historico.json', trocado);
    const r = await garantirArquivos(cliente(servidor, arm), { arquivos: ['historico'] });
    expect(r.falhou).toBe(true);
    expect(r.falha).toBe('recusado');
    expect((await arm.ler('quina/historico.json'))?.sha256).toBe(antes?.sha256);
    expect(r.cache?.loterias.quina?.dados.historico).not.toBeNull();
  });
});

describe('precarregar (segundo plano)', () => {
  it('baixa tabela_geral e depois historico, cedendo a vez antes de cada um', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(servidor, arm));
    servidor.pedidos.length = 0;
    let cedeu = 0;
    await precarregar(cliente(servidor, arm), async () => {
      cedeu += 1;
      // Quando cede a vez pela 2ª vez, a tabela_geral já foi baixada e o historico não.
      if (cedeu === 2) {
        expect(servidor.pedidosDeDados().every((p) => p.endsWith('tabela_geral.json'))).toBe(true);
      }
    });
    expect(cedeu).toBe(2);
    expect(servidor.pedidosDeDados().sort()).toEqual([
      'lotofacil/historico.json',
      'lotofacil/tabela_geral.json',
      'megasena/historico.json',
      'megasena/tabela_geral.json',
      'quina/historico.json',
      'quina/tabela_geral.json',
    ]);
  });

  it('falha de rede no segundo plano não lança', async () => {
    await expect(
      precarregar(cliente(new ServidorFalso(), undefined, false), async () => undefined),
    ).resolves.toBeUndefined();
  });
});
