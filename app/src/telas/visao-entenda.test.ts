// Visão pura da Entenda: cabeçalho da tabela_geral (fixtures reais) -> o que a tela mostra.
import { describe, expect, it } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { abrirComCache, type SituacaoDados } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { OPCOES_DA_ABERTURA } from '../dados/sob-demanda';
import { SLUGS, type Slug, type TabelaGeralV1 } from '../dados/tipos';
import { validarTabelaGeral } from '../dados/validacao';
import { textos } from '../lib/textos';
import {
  EXEMPLO_LINHA,
  TAMANHO_HASH_ABREVIADO,
  abreviarHash,
  montarVisaoEntenda,
  textoDoExemplo,
  textoJanela,
} from './entenda';

const T1 = new Date('2026-10-14T12:30:00Z');

function tabelaDaFixture(slug: Slug): TabelaGeralV1 {
  return validarTabelaGeral(JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`)), slug);
}
const tabelas = Object.fromEntries(SLUGS.map((s) => [s, tabelaDaFixture(s)])) as Record<
  Slug,
  TabelaGeralV1
>;

async function situacaoAtualizada(): Promise<SituacaoDados> {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: new ServidorFalso().fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  return abrirComCache(
    cliente,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );
}

describe('montarVisaoEntenda', () => {
  it('com tudo em cache: data, formato, versão do app e o cabeçalho das 3 tabelas', async () => {
    const visao = montarVisaoEntenda({
      situacao: await situacaoAtualizada(),
      tabelas,
      versaoApp: '0.1.0',
      atualizacaoDoApp: false,
    });
    expect(visao.atualizadoEm).toBe('14/10/2026 09:30');
    expect(visao.formatoDados).toBe(1);
    expect(visao.versaoApp).toBe('0.1.0');
    expect(visao.atualizacaoDoApp).toBe(false);
    expect(visao.loterias.map((l) => l.slug)).toEqual([...SLUGS]);
    for (const l of visao.loterias) {
      const t = tabelas[l.slug];
      expect(l.nome).toBe(textos.loterias[l.slug]);
      expect(l.perfil).not.toBeNull();
      expect(l.perfil?.perfil).toBe(t.perfil);
      expect(l.perfil?.featureVersion).toBe(t.featureVersion);
      expect(l.perfil?.hashCompleto).toBe(t.hashToml);
      expect(l.perfil?.hashAbreviado).toBe(t.hashToml.slice(0, 8));
      // A tabela é do próximo concurso; o último sorteado (ultimo.json) é o dela.
      expect(l.ultimoConcurso).toContain(String(t.ultimoSorteado));
      expect(l.ultimoConcurso).toMatch(/\(\d{2}\/\d{2}\/\d{4}\)$/);
    }
  });

  it('janela 50 mostra os últimos 50 concursos; janela 0 é o histórico inteiro', () => {
    expect(textoJanela(50)).toBe('últimos 50 concursos');
    expect(textoJanela(1)).toBe('últimos 1 concurso');
    expect(textoJanela(0)).toBe('histórico inteiro');
    const zero = { ...tabelas.quina, janela: 0, normalizacao: 'minmax' } as TabelaGeralV1;
    const visao = montarVisaoEntenda({
      situacao: null,
      tabelas: { quina: zero },
      versaoApp: '1',
      atualizacaoDoApp: false,
    });
    const quina = visao.loterias.find((l) => l.slug === 'quina');
    expect(quina?.perfil?.janela).toBe('histórico inteiro');
    expect(quina?.perfil?.normalizacao).toBe('Escala de 0 a 1 (mínimo e máximo)');
  });

  it('a normalização usa o rótulo existente (z-score nas fixtures)', () => {
    const visao = montarVisaoEntenda({
      situacao: null,
      tabelas,
      versaoApp: '1',
      atualizacaoDoApp: false,
    });
    for (const l of visao.loterias) {
      expect(l.perfil?.normalizacao).toBe('Padronização (z-score)');
    }
  });

  it('sem tabela em cache: perfil nulo, mas o último concurso ainda vem do ultimo.json', async () => {
    const visao = montarVisaoEntenda({
      situacao: await situacaoAtualizada(),
      tabelas: {},
      versaoApp: '0.1.0',
      atualizacaoDoApp: true,
    });
    expect(visao.atualizacaoDoApp).toBe(true);
    for (const l of visao.loterias) {
      expect(l.perfil).toBeNull();
      expect(l.ultimoConcurso).not.toBeNull();
    }
  });

  it('sem nada (nem situação nem tabelas): sem data, sem perfil, sem concurso, sem quebrar', () => {
    const visao = montarVisaoEntenda({
      situacao: null,
      tabelas: {},
      versaoApp: '0.1.0',
      atualizacaoDoApp: false,
    });
    expect(visao.atualizadoEm).toBeNull();
    expect(visao.formatoDados).toBe(1);
    expect(visao.loterias.every((l) => l.perfil === null && l.ultimoConcurso === null)).toBe(true);
  });

  it('só com a tabela, o último concurso vem dela (sem data)', () => {
    const visao = montarVisaoEntenda({
      situacao: null,
      tabelas: { megasena: tabelas.megasena },
      versaoApp: '1',
      atualizacaoDoApp: false,
    });
    const mega = visao.loterias.find((l) => l.slug === 'megasena');
    expect(mega?.ultimoConcurso).toBe(String(tabelas.megasena.ultimoSorteado));
  });
});

describe('abreviarHash', () => {
  it('mantém os 8 primeiros caracteres', () => {
    expect(TAMANHO_HASH_ABREVIADO).toBe(8);
    expect(abreviarHash('5bea2957a4aa375ffaf102aab3ca21d39252213f1a133b4f3538589df6d2c873')).toBe(
      '5bea2957',
    );
  });
  it('hash curto volta inteiro e vazio fica vazio', () => {
    expect(abreviarHash('abc')).toBe('abc');
    expect(abreviarHash('')).toBe('');
  });
});

describe('exemplo ilustrativo', () => {
  it('descreve posição, variação, índice e evidência com os formatadores da tabela', () => {
    expect(EXEMPLO_LINHA.dezena).toBe(7);
    expect(textoDoExemplo()).toBe(
      '3º lugar · subiu 2 posições · Índice 0,850 · Evidência: Moderada',
    );
  });
});
