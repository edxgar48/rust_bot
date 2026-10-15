// Lógica pura da tela Estatísticas: visão por estado e textos montados a partir da 7.1.
import { describe, expect, it } from 'vitest';
import type { ResultadoCache } from '../dados/cliente';
import type {
  ConcursoHistorico,
  HistoricoV1,
  IndiceV1,
  LoteriaNoIndice,
  Slug,
} from '../dados/tipos';
import { calcularEstatisticas } from '../estatisticas/estatisticas';
import { extremosFrequencia } from '../estatisticas/grafico';
import { janelaDoHash, hashDasEstatisticas } from '../lib/rotas';
import type { SituacaoTela } from './estado-tela';
import {
  MAXIMO_ATRASO_EXIBIDO,
  anuncioDasEstatisticas,
  linhasDoResumoFrequencia,
  montarCartaoAtraso,
  montarVisaoEstatisticas,
  rotuloDoGrafico,
  subtituloDasEstatisticas,
} from './estatisticas';

// Quina (80 dezenas), 12 concursos à mão. Dezena 7 saiu só no 1º; 2 saiu em todos.
const QUINA: readonly ConcursoHistorico[] = [
  [1, '2020-01-01', [2, 7, 10, 20, 30]],
  [2, '2020-01-02', [2, 11, 21, 31, 41]],
  [3, '2020-01-03', [2, 12, 22, 32, 42]],
  [4, '2020-01-04', [2, 13, 23, 33, 43]],
  [5, '2020-01-05', [2, 14, 24, 34, 44]],
  [6, '2020-01-06', [2, 15, 25, 35, 45]],
  [7, '2020-01-07', [2, 16, 26, 36, 46]],
  [8, '2020-01-08', [2, 17, 27, 37, 47]],
  [9, '2020-01-09', [2, 18, 28, 38, 48]],
  [10, '2020-01-10', [2, 19, 29, 39, 49]],
  [11, '2020-01-11', [2, 50, 51, 52, 53]],
  [12, '2020-01-12', [2, 54, 55, 56, 57]],
];

describe('subtítulo e anúncio', () => {
  it('"Quina · últimos 10 concursos", singular e "todos os N"', () => {
    const dez = calcularEstatisticas('quina', QUINA, 10);
    expect(subtituloDasEstatisticas(dez)).toBe('Quina · últimos 10 concursos');
    expect(anuncioDasEstatisticas(dez)).toBe('Mostrando Quina, últimos 10 concursos.');
    const todos = calcularEstatisticas('quina', QUINA, 'total');
    expect(subtituloDasEstatisticas(todos)).toBe('Quina · todos os 12 concursos');
    expect(anuncioDasEstatisticas(todos)).toBe('Mostrando Quina, todos os 12 concursos.');
    // Janela maior que o histórico: o texto diz o tamanho efetivo, não o pedido.
    expect(subtituloDasEstatisticas(calcularEstatisticas('quina', QUINA, 100))).toBe(
      'Quina · últimos 12 concursos',
    );
    const um = calcularEstatisticas('megasena', [[1, '2020-01-01', [1, 2, 3, 4, 5, 6]]], 10);
    expect(subtituloDasEstatisticas(um)).toBe('Mega-Sena · último concurso');
    expect(anuncioDasEstatisticas(um)).toBe('Mostrando Mega-Sena, último concurso.');
  });

  it('milhar com ponto (Quina total tem mais de 7 mil concursos)', () => {
    const muitos: ConcursoHistorico[] = Array.from({ length: 1234 }, (_, i) => [
      i + 1,
      '2020-01-01',
      [1, 2, 3, 4, 5],
    ]);
    expect(subtituloDasEstatisticas(calcularEstatisticas('quina', muitos, 'total'))).toBe(
      'Quina · todos os 1.234 concursos',
    );
  });
});

describe('resumo da frequência (sem ler as dezenas todas)', () => {
  it('um maior e um menor: cita a dezena e as vezes', () => {
    const l = linhasDoResumoFrequencia(extremosFrequencia([3, 7, 1, 4]));
    expect(l).toEqual({
      iguais: null,
      maior: 'Mais vezes: dezena 02, com 7 vezes.',
      menor: 'Menos vezes: dezena 03, com 1 vez.',
    });
  });

  it('até 3 empatados são citados; mais que isso vira contagem', () => {
    const tres = linhasDoResumoFrequencia(extremosFrequencia([5, 5, 5, 1, 2]));
    expect(tres.maior).toBe('Mais vezes: dezenas 01, 02 e 03, com 5 vezes cada.');
    const dois = linhasDoResumoFrequencia(extremosFrequencia([5, 5, 1, 2]));
    expect(dois.maior).toBe('Mais vezes: dezenas 01 e 02, com 5 vezes cada.');
    const muitos = linhasDoResumoFrequencia(extremosFrequencia([0, 0, 0, 0, 0, 3, 4]));
    expect(muitos.menor).toBe('Menos vezes: 5 dezenas empatadas, com 0 vezes cada.');
  });

  it('todas iguais: frase própria', () => {
    expect(linhasDoResumoFrequencia(extremosFrequencia([2, 2, 2]))).toEqual({
      iguais: 'Todas as dezenas saíram 2 vezes.',
      maior: null,
      menor: null,
    });
  });

  it('o rótulo do gráfico só descreve e dá o esperado; os extremos ficam na legenda', () => {
    const e = calcularEstatisticas('quina', QUINA, 'total');
    const rotulo = rotuloDoGrafico(e);
    expect(rotulo).toContain('Gráfico de barras');
    expect(rotulo).toContain('80 dezenas da Quina');
    expect(rotulo).toContain('12 concursos');
    expect(rotulo).toContain('Esperado ao acaso: 0,8 vezes por dezena');
    expect(rotulo).not.toContain('Mais vezes');
    expect(rotulo).not.toContain('Menos vezes');
    expect(rotulo).toContain('tabela');
    expect(rotulo.length).toBeLessThan(300);
  });
});

describe('cartão de atraso', () => {
  const e = calcularEstatisticas('quina', QUINA, 'total');
  const c = montarCartaoAtraso(e);

  it('até 5 dezenas de maior atraso, do maior para o menor; empate pela dezena menor', () => {
    expect(c.itens.length).toBeLessThanOrEqual(MAXIMO_ATRASO_EXIBIDO);
    expect(c.itens).toHaveLength(5);
    // Dezena 7 saiu só no 1º de 12 concursos: atraso 11; 10, 20, 30 também (empate): 07, 10, 20, 30, depois 11.
    expect(c.itens.map((i) => i.dezena)).toEqual([7, 10, 20, 30, 11]);
    expect(c.itens[0]).toMatchObject({
      rotulo: '07',
      atraso: 11,
      texto: 'Dezena 07: 11 concursos sem sair',
    });
    expect(c.itens[4]!.atraso).toBe(10);
  });

  it('as dezenas sem aparição vêm SEMPRE junto, com o tamanho da janela', () => {
    const dez = calcularEstatisticas('quina', QUINA, 10);
    const cd = montarCartaoAtraso(dez);
    expect(cd.semAparicao).toEqual(dez.semAparicao);
    expect(cd.semAparicao).toContain(7);
    expect(cd.semAparicao.length).toBe(80 - dez.rankingAtraso.length);
    expect(cd.textoSemAparicao).toBe(
      `Não saíram nos últimos 10 concursos: ${cd.semAparicao.length} dezenas`,
    );
    // Com todas as dezenas aparecendo, o texto diz isso (em vez de sumir).
    const cheio: ConcursoHistorico[] = [
      [1, '2020-01-01', [1, 2, 3, 4, 5, 6]],
      [2, '2020-01-02', [7, 8, 9, 10, 11, 12]],
    ];
    const l = montarCartaoAtraso(calcularEstatisticas('megasena', cheio, 'total'));
    expect(l.semAparicao.length).toBe(48);
    const todas: ConcursoHistorico[] = Array.from({ length: 10 }, (_, i) => [
      i + 1,
      '2020-01-01',
      Array.from({ length: 6 }, (_, j) => ((i * 6 + j) % 60) + 1),
    ]);
    const nenhuma = montarCartaoAtraso(calcularEstatisticas('megasena', todas, 'total'));
    expect(nenhuma.semAparicao).toEqual([]);
    expect(nenhuma.textoSemAparicao).toBe(
      'Todas as dezenas saíram pelo menos uma vez nos últimos 10 concursos.',
    );
  });

  it('atraso 0 diz que saiu no último concurso; singular para 1', () => {
    const e2 = calcularEstatisticas('quina', QUINA, 'total');
    const item12 = montarCartaoAtraso({
      ...e2,
      rankingAtraso: [
        { dezena: 54, atraso: 0 },
        { dezena: 3, atraso: 1 },
      ],
    });
    expect(item12.itens.map((i) => i.texto)).toEqual([
      'Dezena 54: saiu no último concurso',
      'Dezena 03: 1 concurso sem sair',
    ]);
  });

  it('janela de 1 concurso: "no último concurso"', () => {
    const e1 = calcularEstatisticas('quina', QUINA.slice(-1), 10);
    expect(montarCartaoAtraso(e1).textoSemAparicao).toBe(
      'Não saíram no último concurso: 75 dezenas',
    );
  });
});

describe('janela e loteria na URL', () => {
  it('janelaDoHash aceita 10, 50, 100 e total; o resto é ignorado', () => {
    expect(janelaDoHash('#/estatisticas?loteria=quina&janela=10')).toBe(10);
    expect(janelaDoHash('#/estatisticas?janela=100&loteria=quina')).toBe(100);
    expect(janelaDoHash('#/estatisticas?janela=total')).toBe('total');
    expect(janelaDoHash('#/estatisticas?janela=50#x')).toBe(50);
    for (const h of [
      '#/estatisticas',
      '#/estatisticas?janela=',
      '#/estatisticas?janela=7',
      '#/estatisticas?janela=todos',
      '#/estatisticas?janela=010',
      '#/estatisticas?janela=-50',
      '#/estatisticas?janela=50abc',
    ]) {
      expect(janelaDoHash(h)).toBeNull();
    }
  });

  it('hashDasEstatisticas faz ida e volta', () => {
    expect(hashDasEstatisticas('quina', 50)).toBe('#/estatisticas?loteria=quina&janela=50');
    expect(hashDasEstatisticas('megasena', 'total')).toBe(
      '#/estatisticas?loteria=megasena&janela=total',
    );
    expect(janelaDoHash(hashDasEstatisticas('lotofacil', 'total'))).toBe('total');
  });
});

describe('montarVisaoEstatisticas', () => {
  const AGORA = new Date('2026-10-14T12:30:00Z');
  const historicoMao = { concursos: QUINA } as unknown as HistoricoV1;
  function cacheCom(slug: Slug, h: HistoricoV1 | null): ResultadoCache {
    return {
      indice: {} as IndiceV1,
      indiceSincronizadoEm: null,
      sincronizadoEm: '2026-10-14T12:30:00.000Z',
      loterias: { [slug]: { indice: {} as LoteriaNoIndice, dados: { historico: h } } },
    };
  }
  const pronta = (
    cache: ResultadoCache | null,
    falha: SituacaoTela['falha'] = 'nenhuma',
  ): SituacaoTela => ({ cache, sincronizando: false, falha });

  it('antes do cache: carregando; sem histórico e sincronizando: carregando', () => {
    expect(montarVisaoEstatisticas('quina', null, AGORA)).toEqual({ tipo: 'carregando' });
    expect(
      montarVisaoEstatisticas(
        'quina',
        { cache: cacheCom('quina', null), sincronizando: true, falha: 'nenhuma' },
        AGORA,
      ),
    ).toEqual({ tipo: 'carregando' });
  });

  it('sem rede e sem cache: sem dados (erro); recusado: texto de recusa; loteria nula: sem erro', () => {
    expect(montarVisaoEstatisticas('quina', pronta(null, 'sem_rede'), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: true,
    });
    expect(
      montarVisaoEstatisticas('quina', pronta(cacheCom('quina', null), 'recusado'), AGORA),
    ).toMatchObject({ tipo: 'sem_dados', erro: true, texto: expect.stringContaining('recusado') });
    const nula: ResultadoCache = { ...cacheCom('quina', null), loterias: { quina: null } };
    expect(montarVisaoEstatisticas('quina', pronta(nula), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: false,
    });
  });

  it('com histórico: conteúdo com a mesma lista (sem copiar) e estado; recusado mantém e marca erro', () => {
    const v = montarVisaoEstatisticas('quina', pronta(cacheCom('quina', historicoMao)), AGORA);
    if (v.tipo !== 'conteudo') throw new Error(v.tipo);
    expect(v.concursos).toBe(QUINA);
    expect(v).toMatchObject({ estado: 'atualizado', erro: false, nomeLoteria: 'Quina' });
    const r = montarVisaoEstatisticas(
      'quina',
      pronta(cacheCom('quina', historicoMao), 'recusado'),
      AGORA,
    );
    expect(r).toMatchObject({ tipo: 'conteudo', estado: 'recusado', erro: true });
  });
});
