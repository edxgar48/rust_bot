// Conferência da tela Força com o back-end: a ordem das dezenas, as posições,
// as variações e os totais que a visão pura entrega devem coincidir com o CSV
// que o `sync tabela <loteria> --perfil padrao_v1` gera a partir dos bancos
// (mesmo snapshot: o próximo concurso). Os CSV em fixtures/sync-tabela/ são a
// saída desse comando, gerada junto das fixtures de publico/v1/ (mesmo estado
// dos bancos); aqui nada de rede nem de banco.
import { describe, expect, it } from 'vitest';
import { formatarIndice } from '../src/componentes/ranking';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente } from '../src/dados/cliente';
import { SLUGS, PARAMETROS_LOTERIA, type Slug } from '../src/dados/tipos';
import { montarVisaoForca } from '../src/telas/forca';
import { ServidorFalso, URL_BASE_TESTE } from './servidor-falso';

const csvBrutos = import.meta.glob('./fixtures/sync-tabela/*.csv', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

interface LinhaCsv {
  concurso: number;
  dezena: number;
  posicao: number;
  total: number;
  variacao: number | null;
}

function lerCsv(slug: Slug): LinhaCsv[] {
  const bruto = csvBrutos[`./fixtures/sync-tabela/${slug}.csv`];
  if (bruto === undefined) throw new Error(`CSV ausente para ${slug}`);
  const [cabecalho, ...linhas] = bruto.replace(new RegExp('^\uFEFF'), '').trim().split(/\r?\n/);
  const colunas = (cabecalho ?? '').split(';');
  const col = (nome: string) => {
    const i = colunas.indexOf(nome);
    if (i < 0) throw new Error(`coluna ${nome} ausente`);
    return i;
  };
  return linhas.map((linha) => {
    const c = linha.split(';');
    const variacao = c[col('variacao_posicao')] ?? '';
    return {
      concurso: Number(c[col('concurso')]),
      dezena: Number(c[col('dezena')]),
      posicao: Number(c[col('posicao')]),
      total: Number((c[col('total')] ?? '').replace(',', '.')),
      variacao: variacao === '' ? null : Number(variacao),
    };
  });
}

async function visaoDaFixture(slug: Slug) {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: new ServidorFalso().fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
  });
  await cliente.sincronizar({ arquivos: ['tabela_geral'], loterias: [slug] });
  const cache = await cliente.lerCache({ arquivos: ['tabela_geral'], loterias: [slug] });
  return montarVisaoForca(slug, { cache, sincronizando: false, falha: 'nenhuma' });
}

describe.each(SLUGS)('Força x sync tabela (%s)', (slug) => {
  it('mesma ordem de dezenas, posições, variações e totais do CSV do back-end', async () => {
    const csv = lerCsv(slug);
    const visao = await visaoDaFixture(slug);
    if (visao.tipo !== 'conteudo') throw new Error('esperava conteúdo');

    expect(visao.linhas).toHaveLength(PARAMETROS_LOTERIA[slug].totalDezenas);
    expect(visao.linhas).toHaveLength(csv.length);
    expect(visao.linhas.map((l) => l.dezena)).toEqual(csv.map((l) => l.dezena));
    expect(visao.linhas.map((l) => l.posicao)).toEqual(csv.map((l) => l.posicao));
    expect(visao.linhas.map((l) => l.posicao)).toEqual(csv.map((_, i) => i + 1));
    expect(visao.linhas.map((l) => l.variacao)).toEqual(csv.map((l) => l.variacao));

    // O CSV tem 6 casas; o JSON, 9. O que a tela mostra (3 casas) tem de coincidir.
    visao.linhas.forEach((linha, i) => {
      const esperado = csv[i]?.total ?? Number.NaN;
      expect(Math.abs(linha.indice - esperado)).toBeLessThan(6e-7);
      expect(formatarIndice(linha.indice)).toBe(formatarIndice(esperado));
    });
  });

  it('o concurso do subtítulo é o do CSV (próximo concurso)', async () => {
    const csv = lerCsv(slug);
    const visao = await visaoDaFixture(slug);
    if (visao.tipo !== 'conteudo') throw new Error('esperava conteúdo');
    const proximo = csv[0]?.concurso;
    expect(new Set(csv.map((l) => l.concurso))).toEqual(new Set([proximo]));
    expect(visao.subtitulo).toBe(
      `Próximo concurso ${proximo}, com dados até o ${(proximo ?? 0) - 1}`,
    );
  });
});
