// @vitest-environment node
// Mapeamento do estado de sincronização para o que a tela Início mostra (inicio.ts).
import { describe, expect, it } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixture } from '../../testes/servidor-falso';
import { abrirComCache, type SituacaoDados } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { estadoInicial } from '../dados/estado-sincronizacao';
import { OPCOES_DA_ABERTURA } from '../dados/sob-demanda';
import { montarVisaoInicio } from './inicio';

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

const abrir = (c: ReturnType<typeof cliente>): Promise<SituacaoDados> =>
  abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );

const dataBr = expect.stringMatching(/^\d\d\/\d\d\/\d{4}$/);

function adulterarUltimoDaMega(servidor: ServidorFalso): Promise<void> {
  // O índice passa a declarar outro conteúdo; o arquivo servido não confere.
  return servidor.trocar('megasena/ultimo.json', lerFixture('megasena/historico.json')).then(() => {
    const bytes = lerFixture('megasena/ultimo.json').slice();
    bytes[bytes.length - 3] = (bytes[bytes.length - 3] ?? 0) ^ 1;
    servidor.arquivos.set('megasena/ultimo.json', bytes);
  });
}

describe('montarVisaoInicio', () => {
  it('sem situação ainda: carregando', () => {
    expect(montarVisaoInicio(null)).toEqual({ tipo: 'carregando' });
  });

  it('cache vazio e sincronizando: carregando', () => {
    expect(montarVisaoInicio({ estado: estadoInicial(null), cache: null }).tipo).toBe('carregando');
  });

  it('sem cache e sem rede: sem dados, com o texto do estado', async () => {
    const final = await abrir(cliente(new ServidorFalso(), new ArmazenamentoEmMemoria(), false));
    expect(montarVisaoInicio(final)).toEqual({ tipo: 'sem_dados', texto: final.estado.texto });
  });

  it('atualizado: conteúdo com 3 cartões, sem erro', async () => {
    const final = await abrir(cliente(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    const v = montarVisaoInicio(final);
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    expect(v.estado).toBe('atualizado');
    expect(v.erro).toBe(false);
    expect(
      v.cartoes.map((c) => [c.slug, c.concurso, c.data, c.dezenas.length, c.disposicao]),
    ).toEqual([
      ['lotofacil', 3803, '13/10/2026', 15, 'grade'],
      ['megasena', 3069, dataBr, 6, 'linha'],
      ['quina', 7141, dataBr, 5, 'linha'],
    ]);
  });

  it('cache e sem rede: conteúdo com o estado sem_rede (não é erro)', async () => {
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(new ServidorFalso(), arm));
    const v = montarVisaoInicio(await abrir(cliente(new ServidorFalso(), arm, false)));
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    expect(v.estado).toBe('sem_rede');
    expect(v.erro).toBe(false);
    expect(v.texto).toContain('14/10/2026 09:30');
    expect(v.cartoes.every((c) => c.tipo === 'resultado')).toBe(true);
  });

  it('dado recusado: conteúdo com erro, mantendo os dados anteriores', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await abrir(cliente(servidor, arm));
    await adulterarUltimoDaMega(servidor);
    const v = montarVisaoInicio(await abrir(cliente(servidor, arm)));
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    expect(v.estado).toBe('recusado');
    expect(v.erro).toBe(true);
    const mega = v.cartoes.find((c) => c.slug === 'megasena');
    expect(mega?.concurso).toBe(3069);
    expect(mega?.dezenas).toHaveLength(6);
  });

  it('concurso e data vêm do ultimo.json guardado, não do índice mais novo', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await abrir(cliente(servidor, arm));
    // O índice novo anuncia o concurso 7142 da Quina, mas o arquivo não baixa.
    const indice = servidor.indice();
    indice.loterias.quina.ultimo_concurso = 7142;
    indice.loterias.quina.data = '2026-10-14';
    indice.loterias.quina.arquivos.ultimo.sha256 = 'a'.repeat(64);
    servidor.gravarIndice(indice);
    servidor.com500.add('quina/ultimo.json');
    const v = montarVisaoInicio(await abrir(cliente(servidor, arm)));
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    const quina = v.cartoes.find((c) => c.slug === 'quina');
    expect(quina?.concurso).toBe(7141);
    expect(quina?.data).not.toBe('14/10/2026');
    expect(quina?.dezenas).toHaveLength(5);
  });

  it('loteria null no índice: cartão sem_loteria, sem concurso nem dezenas', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, megasena: null } });
    const v = montarVisaoInicio(await abrir(cliente(servidor, new ArmazenamentoEmMemoria())));
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    expect(v.cartoes.find((c) => c.slug === 'megasena')).toMatchObject({
      tipo: 'sem_loteria',
      concurso: null,
      data: null,
      dezenas: [],
    });
    expect(v.cartoes.find((c) => c.slug === 'quina')?.tipo).toBe('resultado');
  });

  it('loteria no índice mas ultimo.json fora do cache: sem_dezenas, concurso do índice', async () => {
    const servidor = new ServidorFalso();
    servidor.com500.add('quina/ultimo.json');
    const v = montarVisaoInicio(await abrir(cliente(servidor, new ArmazenamentoEmMemoria())));
    if (v.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    expect(v.cartoes.find((c) => c.slug === 'quina')).toMatchObject({
      tipo: 'sem_dezenas',
      concurso: 7141,
      dezenas: [],
    });
  });
});
