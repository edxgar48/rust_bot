// @vitest-environment node
/* eslint-disable @typescript-eslint/no-explicit-any */
// ranking_posicoes.json (opcional, 7.8): validação, chave opcional no index.json, download junto da
// Força com conferência de sha256/bytes e cache. Fixtures reais (cópias de `sync exportar`); sem rede.
// Os valores de `variacao_g_1` das fixtures reais são de uma definição anterior (da posição); estes
// testes só conferem a FORMA (inteiro ou null), nunca o valor.
import { describe, expect, it } from 'vitest';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente } from '../src/dados/cliente';
import { PARAMETROS_LOTERIA, SLUGS, type Slug } from '../src/dados/tipos';
import { validarIndice, validarRankingPosicoes } from '../src/dados/validacao';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from './servidor-falso';

const cru = (slug: Slug) => JSON.parse(lerFixtureTexto(`${slug}/ranking_posicoes.json`));
const clone = (v: unknown) => JSON.parse(JSON.stringify(v));

function novoCliente(servidor: ServidorFalso) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
  });
}
const OPC = { loterias: ['quina'], arquivos: ['ranking_posicoes'] } as const;

describe('validarRankingPosicoes: fixtures reais', () => {
  it.each(SLUGS)('%s: campos do arquivo copiados um a um', (slug) => {
    const bruto = cru(slug);
    const r = validarRankingPosicoes(bruto, slug);
    expect(r.dezenas).toHaveLength(PARAMETROS_LOTERIA[slug].totalDezenas);
    expect(r.concurso).toBe(bruto.ultimo_sorteado + 1);
    expect(r.perfilPosicoes).toBe('posicoes_v1');
    expect(r.hashTomlPosicoes).toBe(bruto.hash_toml_posicoes);
    r.dezenas.forEach((d, i) => {
      expect(d.dezena).toBe(bruto.dezenas[i].dezena);
      expect(d.posicaoIndice).toBe(bruto.dezenas[i].posicao_indice);
      expect(d.g1).toBe(bruto.dezenas[i].g_1);
      expect(d.variacaoG1 === null || Number.isInteger(d.variacaoG1)).toBe(true);
    });
  });
});

describe('validarRankingPosicoes: adulterados e tolerância', () => {
  const recusa = (alterar: (j: any) => void, trecho: RegExp, slug: Slug = 'lotofacil') => {
    const j = clone(cru(slug));
    alterar(j);
    expect(() => validarRankingPosicoes(j, slug)).toThrow(trecho);
  };

  it('schema_version diferente de 1', () => recusa((j) => (j.schema_version = 2), /schema/i));
  it('loteria diferente da esperada', () => {
    expect(() => validarRankingPosicoes(cru('quina'), 'lotofacil')).toThrow(/esperada lotofacil/);
  });
  it('número de dezenas errado', () => recusa((j) => j.dezenas.pop(), /25 dezenas/));
  it('dezena fora de ordem', () => recusa((j) => (j.dezenas[3].dezena = 3), /esperada a dezena 4/));
  it('g_1 repetido', () =>
    recusa((j) => (j.dezenas[0].g_1 = j.dezenas[1].g_1), /g_1.*sem repetição/));
  it('g_1 fora de 1..N', () => recusa((j) => (j.dezenas[0].g_1 = 26), /g_1/));
  it('g_1 como texto', () => recusa((j) => (j.dezenas[0].g_1 = '3'), /g_1/));
  it('posicao_indice repetida', () =>
    recusa((j) => (j.dezenas[0].posicao_indice = j.dezenas[1].posicao_indice), /posicao_indice/));
  it('concurso diferente de ultimo_sorteado + 1', () => recusa((j) => (j.concurso += 1), /\+ 1/));
  it('hash do perfil das posições malformado', () =>
    recusa((j) => (j.hash_toml_posicoes = 'abc'), /hash_toml_posicoes/));
  it('normalização desconhecida', () =>
    recusa((j) => (j.normalizacao_posicoes = 'log'), /normalizacao_posicoes/));

  it('variacao_g_1 null ou ausente vira null (nunca zero)', () => {
    const j = clone(cru('lotofacil'));
    j.dezenas[0].variacao_g_1 = null;
    delete j.dezenas[1].variacao_g_1;
    const r = validarRankingPosicoes(j, 'lotofacil');
    expect(r.dezenas[0]?.variacaoG1).toBeNull();
    expect(r.dezenas[1]?.variacaoG1).toBeNull();
  });
  it('variacao_g_1 fracionária é recusada', () =>
    recusa((j) => (j.dezenas[0].variacao_g_1 = 1.5), /variacao_g_1/));

  it('campo desconhecido é ignorado', () => {
    const j = clone(cru('quina'));
    j.futuro = 1;
    j.dezenas[0].novo = 'x';
    const r = validarRankingPosicoes(j, 'quina');
    expect(r.dezenas[0]).not.toHaveProperty('novo');
  });
});

describe('index.json: ranking_posicoes é opcional', () => {
  it('índice sem a chave continua válido', () => {
    const indice = validarIndice(JSON.parse(lerFixtureTexto('index.json')));
    for (const s of SLUGS) expect(indice.loterias[s]?.arquivos.ranking_posicoes).toBeUndefined();
  });

  it('com a chave traz o descritor; null é tolerado como ausente', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['quina']);
    const bruto = servidor.indice();
    bruto.loterias.megasena.arquivos.ranking_posicoes = null;
    const indice = validarIndice(bruto);
    expect(indice.loterias.quina?.arquivos.ranking_posicoes?.caminho).toBe(
      'quina/ranking_posicoes.json',
    );
    expect(indice.loterias.megasena?.arquivos.ranking_posicoes).toBeUndefined();
  });

  it('descritor inválido da chave opcional é recusado', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['quina']);
    const bruto = servidor.indice();
    bruto.loterias.quina.arquivos.ranking_posicoes.sha256 = 'xyz';
    expect(() => validarIndice(bruto)).toThrow(/ranking_posicoes\.sha256/);
  });
});

describe('cliente: ranking_posicoes sob demanda', () => {
  it('a sincronização padrão (3 arquivos) não baixa o G-1', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes();
    await novoCliente(servidor).sincronizar({ loterias: ['quina'] });
    expect(servidor.pedidos).not.toContain('quina/ranking_posicoes.json');
  });

  it('pedido explícito baixa, confere e guarda; o seguinte vem do cache', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['quina'], { alinhado: false });
    const cliente = novoCliente(servidor);
    const r1 = await cliente.sincronizar(OPC);
    expect(r1.loterias.quina?.arquivos.ranking_posicoes?.estado).toBe('baixado');
    const r2 = await cliente.sincronizar(OPC);
    expect(r2.loterias.quina?.arquivos.ranking_posicoes?.estado).toBe('em_cache');
    expect(servidor.pedidos.filter((p) => p === 'quina/ranking_posicoes.json')).toHaveLength(1);
    const cache = await cliente.lerCache(OPC);
    expect(cache?.loterias.quina?.dados.ranking_posicoes?.dezenas).toHaveLength(80);
  });

  it('sem a chave no índice: nada é pedido, sem erro, e o cache traz null', async () => {
    const servidor = new ServidorFalso();
    const cliente = novoCliente(servidor);
    const r = await cliente.sincronizar(OPC);
    expect(r.loterias.quina?.arquivos.ranking_posicoes).toBeUndefined();
    expect(servidor.pedidos).toEqual(['index.json']);
    const cache = await cliente.lerCache(OPC);
    expect(cache?.loterias.quina?.dados.ranking_posicoes).toBeNull();
  });

  it('arquivo adulterado (hash do índice não bate) é recusado e não vai ao cache', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['quina'], { alinhado: false });
    const original = lerFixtureTexto('quina/ranking_posicoes.json');
    const adulterado = original.replace('"g_1": 73', '"g_1": 74');
    expect(adulterado).not.toBe(original);
    await servidor.trocar('quina/ranking_posicoes.json', adulterado, false);
    const cliente = novoCliente(servidor);
    const a = (await cliente.sincronizar(OPC)).loterias.quina?.arquivos.ranking_posicoes;
    expect(a?.estado).toBe('falhou');
    if (a?.estado === 'falhou') {
      expect(a.erro.codigo).toMatch(/hash|bytes/);
      expect(a.dados).toBeNull();
    }
    expect((await cliente.lerCache(OPC))?.loterias.quina?.dados.ranking_posicoes).toBeNull();
  });

  it('hash correto mas forma inválida: recusado, e o cache válido anterior permanece', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['quina']);
    const cliente = novoCliente(servidor);
    await cliente.sincronizar(OPC);
    await servidor.alterarJson('quina/ranking_posicoes.json', (j) => j.dezenas.pop());
    const a = (await cliente.sincronizar(OPC)).loterias.quina?.arquivos.ranking_posicoes;
    expect(a?.estado).toBe('falhou');
    if (a?.estado === 'falhou') expect(a.dados?.dezenas).toHaveLength(80);
  });
});
