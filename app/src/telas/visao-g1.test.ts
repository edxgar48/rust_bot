// @vitest-environment node
// G-1 na Força (7.8), lógica pura: junção por dezena, ordenação e detalhe. A junção com dados reais
// usa o par tabela_geral + ranking_posicoes do MESMO snapshot (fixtures/snapshot-g1). As variações
// usadas nos exemplos pequenos são sintéticas e ilustrativas.
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import type { RankingPosicoesV1 } from '../dados/tipos';
import { validarRankingPosicoes, validarTabelaGeral } from '../dados/validacao';
import { montarDetalheDaDezena } from './detalhe-dezena';
import { juntarG1, linhasDaTabela, ordenarLinhas } from './forca';

const tabelaReal = validarTabelaGeral(
  JSON.parse(lerFixtureTexto('snapshot-g1/lotofacil/tabela_geral.json')),
  'lotofacil',
);
const rankingReal = validarRankingPosicoes(
  JSON.parse(lerFixtureTexto('snapshot-g1/lotofacil/ranking_posicoes.json')),
  'lotofacil',
);

describe('juntarG1: junção por dezena (dados reais do mesmo snapshot)', () => {
  it('cada dezena recebe o g_1 e a variação da sua entrada no arquivo', () => {
    const juncao = juntarG1(tabelaReal, rankingReal);
    expect(juncao).not.toBeNull();
    expect(juncao?.size).toBe(25);
    for (const d of rankingReal.dezenas) {
      expect(juncao?.get(d.dezena)).toEqual({ g1: d.g1, variacao: d.variacaoG1 });
    }
  });

  it('a posição de força da linha é a da tabela, e coincide com posicao_indice do G-1', () => {
    const linhas = linhasDaTabela(tabelaReal, juntarG1(tabelaReal, rankingReal));
    for (const l of linhas) {
      const g = rankingReal.dezenas.find((d) => d.dezena === l.dezena);
      expect(g?.posicaoIndice).toBe(l.posicao);
      expect(l.g1).toBe(g?.g1);
    }
  });

  it('sem o arquivo: null, e as linhas não ganham campos de G-1', () => {
    expect(juntarG1(tabelaReal, null)).toBeNull();
    const [primeira] = linhasDaTabela(tabelaReal, null);
    expect(primeira).not.toHaveProperty('g1');
  });

  it('outro concurso (G-1 guardado de um snapshot antigo): ignora, não mistura', () => {
    const antigo: RankingPosicoesV1 = {
      ...rankingReal,
      concurso: rankingReal.concurso - 1,
      ultimoSorteado: rankingReal.ultimoSorteado - 1,
    };
    expect(juntarG1(tabelaReal, antigo)).toBeNull();
  });

  it('posição diferente da tabela (mesmo concurso): ignora', () => {
    const [a, b, ...resto] = rankingReal.dezenas;
    if (!a || !b) throw new Error('fixture sem dezenas');
    const trocado: RankingPosicoesV1 = {
      ...rankingReal,
      dezenas: [
        { ...a, posicaoIndice: b.posicaoIndice },
        { ...b, posicaoIndice: a.posicaoIndice },
        ...resto,
      ],
    };
    expect(juntarG1(tabelaReal, trocado)).toBeNull();
  });

  it('loteria diferente: ignora', () => {
    expect(juntarG1(tabelaReal, { ...rankingReal, loteria: 'quina' })).toBeNull();
  });
});

describe('ordenarLinhas', () => {
  const linhas = linhasDaTabela(tabelaReal, juntarG1(tabelaReal, rankingReal));

  it('por força: posição 1..N', () => {
    const o = ordenarLinhas([...linhas].reverse(), 'forca');
    expect(o.map((l) => l.posicao)).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
  });

  it('por G-1: g_1 1..N (é uma permutação) e é outra ordem de dezenas', () => {
    const o = ordenarLinhas(linhas, 'g1');
    expect(o.map((l) => l.g1)).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
    expect(o.map((l) => l.dezena)).not.toEqual(ordenarLinhas(linhas, 'forca').map((l) => l.dezena));
  });

  it('não altera a entrada, e dezena sem G-1 vai para o fim', () => {
    const antes = linhas.map((l) => l.dezena);
    const primeira = linhas[0];
    if (!primeira) throw new Error('sem linhas');
    const o = ordenarLinhas([{ ...primeira, g1: null }, ...linhas.slice(1)], 'g1');
    expect(o.at(-1)?.dezena).toBe(primeira.dezena);
    ordenarLinhas(linhas, 'g1');
    expect(linhas.map((l) => l.dezena)).toEqual(antes);
  });
});

describe('detalhe da dezena com G-1', () => {
  // A variação é sintética (ilustrativa); o que se testa é o formato do texto.
  const juncao = new Map([
    [25, { g1: 5, variacao: 2 }],
    [3, { g1: 9, variacao: null }],
  ]);

  it('mostra o lugar e a variação da dezena (subiu), com os textos da variação de posição', () => {
    const d = montarDetalheDaDezena(tabelaReal, 25, juncao);
    expect(d?.g1?.valor).toBe('5º lugar');
    expect(d?.g1?.variacao.sentido).toBe('subiu');
    expect(d?.g1?.variacao.texto).toBe('subiu 2 posições');
  });

  it('null = sem concurso anterior (nunca zero)', () => {
    const d = montarDetalheDaDezena(tabelaReal, 3, juncao);
    expect(d?.g1?.variacao.sentido).toBe('sem');
    expect(d?.g1?.variacao.texto).toBe('sem concurso anterior para comparar');
  });

  it('sem G-1 (ou dezena fora do mapa): o bloco some', () => {
    expect(montarDetalheDaDezena(tabelaReal, 25)?.g1).toBeNull();
    expect(montarDetalheDaDezena(tabelaReal, 7, juncao)?.g1).toBeNull();
  });
});
