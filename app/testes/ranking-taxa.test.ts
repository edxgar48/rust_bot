// @vitest-environment node
/* eslint-disable @typescript-eslint/no-explicit-any */
// ranking_taxa.json (opcional, 7.7): validação, chave opcional no index.json, download sob demanda
// com conferência de sha256/bytes e cache. Fixtures reais (cópias de `sync exportar`); sem rede.
import { describe, expect, it } from 'vitest';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente } from '../src/dados/cliente';
import { PARAMETROS_LOTERIA, SLUGS, type Slug } from '../src/dados/tipos';
import { validarIndice, validarRankingTaxa } from '../src/dados/validacao';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from './servidor-falso';

const cru = (slug: Slug) => JSON.parse(lerFixtureTexto(`${slug}/ranking_taxa.json`));
const clone = (v: unknown) => JSON.parse(JSON.stringify(v));

function novoCliente(servidor: ServidorFalso, armazenamento = new ArmazenamentoEmMemoria()) {
  return {
    armazenamento,
    cliente: criarCliente({ urlBase: URL_BASE_TESTE, fetch: servidor.fetch, armazenamento }),
  };
}

describe('validarRankingTaxa: fixtures reais', () => {
  it.each(SLUGS)('%s: dados idênticos ao arquivo, campo a campo', (slug) => {
    const bruto = cru(slug);
    const r = validarRankingTaxa(bruto, slug);
    expect(r.loteria).toBe(slug);
    expect(r.dezenas).toHaveLength(PARAMETROS_LOTERIA[slug].totalDezenas);
    expect(r.taxaAcaso).toBe(bruto.taxa_acaso);
    expect(r.perfil).toBe(bruto.perfil);
    expect(r.concurso).toBe(bruto.ultimo_sorteado + 1);
    r.dezenas.forEach((d, i) => {
      const b = bruto.dezenas[i];
      expect(d).toEqual({
        dezena: b.dezena,
        posicaoIndice: b.posicao_indice,
        notaA: b.nota_a,
        diferencaAcaso: b.diferenca_acaso,
        notaB: b.nota_b,
        posicaoA: b.posicao_a,
        posicaoB: b.posicao_b,
      });
    });
  });

  it('a taxa do acaso é K/N de cada loteria', () => {
    expect(validarRankingTaxa(cru('lotofacil')).taxaAcaso).toBeCloseTo(15 / 25, 9);
    expect(validarRankingTaxa(cru('megasena')).taxaAcaso).toBeCloseTo(6 / 60, 9);
    expect(validarRankingTaxa(cru('quina')).taxaAcaso).toBeCloseTo(5 / 80, 9);
  });
});

describe('validarRankingTaxa: arquivos adulterados são recusados', () => {
  const recusa = (alterar: (j: any) => void, trecho: RegExp, slug: Slug = 'lotofacil') => {
    const j = clone(cru(slug));
    alterar(j);
    expect(() => validarRankingTaxa(j, slug)).toThrow(trecho);
  };

  it('schema_version diferente de 1', () => {
    recusa((j) => (j.schema_version = 2), /schema/i);
  });
  it('loteria diferente da esperada', () => {
    expect(() => validarRankingTaxa(cru('quina'), 'lotofacil')).toThrow(/esperada lotofacil/);
  });
  it('número de dezenas diferente do domínio', () => {
    recusa((j) => j.dezenas.pop(), /25 dezenas/);
  });
  it('dezena repetida ou fora de ordem', () => {
    recusa((j) => (j.dezenas[3].dezena = 3), /esperada a dezena 4/);
  });
  it('dezena fora do domínio', () => {
    recusa((j) => (j.dezenas[24].dezena = 26), /esperada a dezena 25/);
  });
  it('posicao_a repetida', () => {
    recusa((j) => (j.dezenas[0].posicao_a = j.dezenas[1].posicao_a), /posicao_a/);
  });
  it('posicao_b fora de 1..N', () => {
    recusa((j) => (j.dezenas[0].posicao_b = 26), /posicao_b/);
  });
  it('posicao_indice repetida', () => {
    recusa((j) => (j.dezenas[0].posicao_indice = j.dezenas[1].posicao_indice), /posicao_indice/);
  });
  it('nota como texto', () => {
    recusa((j) => (j.dezenas[0].nota_a = '0.5'), /nota_a/);
  });
  it('nota com mais de 9 casas', () => {
    recusa((j) => (j.dezenas[0].nota_b = 0.1234567891), /9 casas/);
  });
  it('diferença que não é nota_a menos taxa_acaso', () => {
    recusa((j) => (j.dezenas[0].diferenca_acaso = 0.3), /nota_a menos taxa_acaso/);
  });
  it('campo ausente', () => {
    recusa((j) => delete j.dezenas[0].nota_a, /nota_a/);
  });
  it('concurso diferente de ultimo_sorteado + 1', () => {
    recusa((j) => (j.concurso += 1), /ultimo_sorteado \+ 1/);
  });
  it('campos desconhecidos são ignorados', () => {
    const j = clone(cru('quina'));
    j.futuro = 1;
    j.dezenas[0].novo = 'x';
    const r = validarRankingTaxa(j, 'quina');
    expect(r.dezenas[0]).not.toHaveProperty('novo');
  });
});

describe('index.json: ranking_taxa é opcional', () => {
  it('índice sem a chave continua válido (as fixtures de base não a têm)', () => {
    const indice = validarIndice(JSON.parse(lerFixtureTexto('index.json')));
    for (const s of SLUGS) expect(indice.loterias[s]?.arquivos.ranking_taxa).toBeUndefined();
  });

  it('índice com a chave traz o descritor; null é tolerado como ausente', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa(['quina']);
    const bruto = servidor.indice();
    bruto.loterias.megasena.arquivos.ranking_taxa = null;
    const indice = validarIndice(bruto);
    expect(indice.loterias.quina?.arquivos.ranking_taxa?.caminho).toBe('quina/ranking_taxa.json');
    expect(indice.loterias.megasena?.arquivos.ranking_taxa).toBeUndefined();
  });

  it('descritor inválido da chave opcional é recusado (hash malformado)', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa(['quina']);
    const bruto = servidor.indice();
    bruto.loterias.quina.arquivos.ranking_taxa.sha256 = 'xyz';
    expect(() => validarIndice(bruto)).toThrow(/ranking_taxa\.sha256/);
  });
});

describe('cliente: ranking_taxa sob demanda', () => {
  it('a sincronização padrão (3 arquivos) não baixa o ranking', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar({ loterias: ['quina'] });
    expect(servidor.pedidos).not.toContain('quina/ranking_taxa.json');
  });

  it('pedido explícito baixa, confere e guarda; o seguinte vem do cache', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const { cliente } = novoCliente(servidor);
    const r1 = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    const a1 = r1.loterias.quina?.arquivos.ranking_taxa;
    expect(a1?.estado).toBe('baixado');
    const r2 = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    expect(r2.loterias.quina?.arquivos.ranking_taxa?.estado).toBe('em_cache');
    expect(servidor.pedidos.filter((p) => p === 'quina/ranking_taxa.json')).toHaveLength(1);
    const cache = await cliente.lerCache({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    expect(cache?.loterias.quina?.dados.ranking_taxa?.dezenas).toHaveLength(80);
  });

  it('sem a chave no índice: nada é pedido e o cache traz null', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    expect(r.loterias.quina?.arquivos.ranking_taxa).toBeUndefined();
    expect(servidor.pedidos).toEqual(['index.json']);
    const cache = await cliente.lerCache({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    expect(cache?.loterias.quina?.dados.ranking_taxa).toBeNull();
  });

  it('arquivo adulterado (hash do índice não bate) é recusado e não vai ao cache', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const adulterado = lerFixtureTexto('quina/ranking_taxa.json').replace('0.0625', '0.0626');
    await servidor.trocar('quina/ranking_taxa.json', adulterado, false);
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    const a = r.loterias.quina?.arquivos.ranking_taxa;
    expect(a?.estado).toBe('falhou');
    if (a?.estado === 'falhou') {
      expect(a.erro.codigo).toMatch(/hash|bytes/);
      expect(a.dados).toBeNull();
    }
    const cache = await cliente.lerCache({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    expect(cache?.loterias.quina?.dados.ranking_taxa).toBeNull();
  });

  it('arquivo com hash correto mas forma inválida é recusado; o cache válido anterior permanece', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    await servidor.alterarJson('quina/ranking_taxa.json', (j) => j.dezenas.pop());
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ranking_taxa'] });
    const a = r.loterias.quina?.arquivos.ranking_taxa;
    expect(a?.estado).toBe('falhou');
    if (a?.estado === 'falhou') expect(a.dados?.dezenas).toHaveLength(80);
  });
});
