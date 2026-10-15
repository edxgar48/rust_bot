// Dados do gráfico de frequência (7.2): geometria, escala, ticks e extremos, com valores
// à mão. Nada de DOM aqui; o desenho em si é testado no componente.
import { describe, expect, it } from 'vitest';
import {
  ALTURA_GRAFICO_PADRAO,
  LARGURA_MINIMA_GRAFICO,
  larguraDoGrafico,
  dezenasRotuladas,
  escalaVertical,
  extremosFrequencia,
  linhasTabelaFrequencia,
  montarGrafico,
  passoRotulos,
} from './grafico';

describe('rótulos do eixo das dezenas', () => {
  it('Lotofácil de 5 em 5; Mega-Sena e Quina de 10 em 10; sempre com a dezena 1', () => {
    expect(passoRotulos(25)).toBe(5);
    expect(passoRotulos(60)).toBe(10);
    expect(passoRotulos(80)).toBe(10);
    expect(dezenasRotuladas(25)).toEqual([1, 5, 10, 15, 20, 25]);
    expect(dezenasRotuladas(60)).toEqual([1, 10, 20, 30, 40, 50, 60]);
    expect(dezenasRotuladas(80)).toEqual([1, 10, 20, 30, 40, 50, 60, 70, 80]);
  });
});

describe('escalaVertical', () => {
  it('passos 1, 2 ou 5 vezes uma potência de 10, no máximo 4 intervalos, topo >= máximo', () => {
    expect(escalaVertical(7)).toEqual({ topo: 8, ticks: [0, 2, 4, 6, 8] });
    expect(escalaVertical(10)).toEqual({ topo: 10, ticks: [0, 5, 10] });
    expect(escalaVertical(9)).toEqual({ topo: 10, ticks: [0, 5, 10] });
    expect(escalaVertical(3)).toEqual({ topo: 3, ticks: [0, 1, 2, 3] });
    expect(escalaVertical(2282)).toEqual({ topo: 3000, ticks: [0, 1000, 2000, 3000] });
    expect(escalaVertical(450)).toEqual({ topo: 600, ticks: [0, 200, 400, 600] });
  });

  it('nunca tem tick fracionário (contagens são inteiras), nem com tudo zero', () => {
    expect(escalaVertical(0)).toEqual({ topo: 1, ticks: [0, 1] });
    expect(escalaVertical(0.4).ticks.every(Number.isInteger)).toBe(true);
    for (const m of [1, 2, 3, 5, 13, 99, 101, 1234, 7000]) {
      const e = escalaVertical(m);
      expect(e.topo).toBeGreaterThanOrEqual(m);
      expect(e.ticks.length - 1).toBeLessThanOrEqual(4);
      expect(e.ticks.every(Number.isInteger)).toBe(true);
      expect(e.ticks[0]).toBe(0);
      expect(e.ticks[e.ticks.length - 1]).toBe(e.topo);
    }
  });
});

describe('extremosFrequencia', () => {
  it('maior e menor, com as dezenas empatadas em ordem crescente', () => {
    const e = extremosFrequencia([3, 7, 1, 7, 1, 4]);
    expect(e.maximo).toBe(7);
    expect(e.minimo).toBe(1);
    expect(e.dezenasMaximo).toEqual([2, 4]);
    expect(e.dezenasMinimo).toEqual([3, 5]);
    expect(e.todasIguais).toBe(false);
  });

  it('todas iguais e lista vazia não têm extremo a destacar', () => {
    expect(extremosFrequencia([2, 2, 2]).todasIguais).toBe(true);
    expect(extremosFrequencia([]).todasIguais).toBe(true);
  });
});

describe('montarGrafico', () => {
  const freq = [4, 0, 8, 2, 2, 6];
  const g = montarGrafico({ frequencia: freq, esperado: 3.5, largura: 300 });

  it('tamanho do viewBox: largura pedida (mínimo 200) e altura padrão', () => {
    expect(g.largura).toBe(300);
    expect(g.altura).toBe(ALTURA_GRAFICO_PADRAO);
    expect(montarGrafico({ frequencia: freq, esperado: 3, largura: 50 }).largura).toBe(
      LARGURA_MINIMA_GRAFICO,
    );
    expect(montarGrafico({ frequencia: freq, esperado: 3, largura: 301.6 }).largura).toBe(302);
  });

  it('o eixo vertical começa em zero: a altura da barra é proporcional ao valor', () => {
    const util = g.base - g.topo;
    for (const b of g.barras) {
      expect(b.altura).toBeCloseTo((b.vezes / g.escalaMaxima) * util, 6);
      expect(b.y + b.altura).toBeCloseTo(g.base, 6);
    }
    // Zero é zero: sem barra "mínima" escondendo o vazio.
    expect(g.barras[1]!.altura).toBe(0);
    // A maior barra (8) com topo da escala em 8 ocupa a área inteira.
    expect(g.escalaMaxima).toBe(8);
    expect(g.barras[2]!.altura).toBeCloseTo(util, 6);
    expect(g.barras[2]!.y).toBeCloseTo(g.topo, 6);
  });

  it('barras dentro da área, sem sobreposição e na ordem das dezenas', () => {
    expect(g.barras.map((b) => b.dezena)).toEqual([1, 2, 3, 4, 5, 6]);
    for (let i = 0; i < g.barras.length; i++) {
      const b = g.barras[i]!;
      expect(b.x).toBeGreaterThanOrEqual(g.esquerda);
      expect(b.x + b.largura).toBeLessThanOrEqual(g.direita + 1e-9);
      expect(b.largura).toBeGreaterThan(0);
      const prox = g.barras[i + 1];
      if (prox) expect(b.x + b.largura).toBeLessThan(prox.x);
    }
  });

  it('linha do esperado fica na altura certa da escala', () => {
    const util = g.base - g.topo;
    expect(g.esperadoY).toBeCloseTo(g.base - (3.5 / 8) * util, 6);
  });

  it('esperado maior que o máximo das barras entra na escala (a linha nunca sai do gráfico)', () => {
    const alto = montarGrafico({ frequencia: [1, 1, 2], esperado: 9, largura: 300 });
    expect(alto.escalaMaxima).toBeGreaterThanOrEqual(9);
    expect(alto.esperadoY).toBeGreaterThanOrEqual(alto.topo);
    expect(alto.esperadoY).toBeLessThanOrEqual(alto.base);
  });

  it('ticks do eixo vertical: rótulo e posição; ticks do eixo das dezenas no centro da barra', () => {
    expect(g.ticksY.map((t) => t.valor)).toEqual([0, 2, 4, 6, 8]);
    expect(g.ticksY.map((t) => t.rotulo)).toEqual(['0', '2', '4', '6', '8']);
    expect(g.ticksY[0]!.y).toBeCloseTo(g.base, 6);
    expect(g.ticksY[4]!.y).toBeCloseTo(g.topo, 6);
    expect(g.ticksX.map((t) => t.rotulo)).toEqual(['01', '05']);
    const barra1 = g.barras[0]!;
    expect(g.ticksX[0]!.x).toBeCloseTo(barra1.x + barra1.largura / 2, 6);
  });

  it('marca o maior com "maior" e o menor com "menor" quando são poucos', () => {
    expect(g.barras.map((b) => b.marca)).toEqual([null, 'menor', 'maior', null, null, null]);
  });

  it('empate no extremo: até 3 barras marcadas; acima disso, nenhuma (marcar tudo não informa)', () => {
    const tres = montarGrafico({ frequencia: [5, 5, 5, 1, 2, 3], esperado: 3, largura: 300 });
    expect(tres.barras.filter((b) => b.marca === 'maior')).toHaveLength(3);
    const quatro = montarGrafico({ frequencia: [5, 5, 5, 5, 1, 2], esperado: 3, largura: 300 });
    expect(quatro.barras.filter((b) => b.marca === 'maior')).toHaveLength(0);
    expect(quatro.barras.filter((b) => b.marca === 'menor')).toHaveLength(1);
  });

  it('todas iguais: nenhuma marca', () => {
    const iguais = montarGrafico({ frequencia: [2, 2, 2, 2], esperado: 2, largura: 300 });
    expect(iguais.barras.every((b) => b.marca === null)).toBe(true);
  });

  it('valor positivo pequeno não some (pelo menos 1 px); zero continua sendo zero', () => {
    const grande = Array.from({ length: 80 }, () => 0);
    grande[0] = 5000;
    grande[1] = 1;
    const m = montarGrafico({ frequencia: grande, esperado: 100, largura: 300 });
    expect(m.barras[1]!.altura).toBeGreaterThanOrEqual(1);
    expect(m.barras[2]!.altura).toBe(0);
  });

  it('Quina com 80 barras em 296 px: cada barra tem largura útil e os rótulos 01, 10 ... 80 não se tocam', () => {
    const q = montarGrafico({
      frequencia: Array.from({ length: 80 }, (_, i) => (i % 7) + 1),
      esperado: 4,
      largura: 296,
    });
    expect(q.barras).toHaveLength(80);
    expect(q.barras[0]!.largura).toBeGreaterThan(2);
    expect(q.ticksX.map((t) => t.rotulo)).toEqual([
      '01',
      '10',
      '20',
      '30',
      '40',
      '50',
      '60',
      '70',
      '80',
    ]);
    // Rótulos de 2 dígitos têm ~13 px: a distância mínima entre centros é maior que isso.
    for (let i = 1; i < q.ticksX.length; i++) {
      expect(q.ticksX[i]!.x - q.ticksX[i - 1]!.x).toBeGreaterThan(18);
    }
    // O último rótulo não passa da borda.
    expect(q.ticksX[q.ticksX.length - 1]!.x + 7).toBeLessThanOrEqual(q.largura);
  });

  it('margem esquerda cresce com o tamanho dos rótulos do eixo vertical', () => {
    const pequeno = montarGrafico({ frequencia: [3, 4], esperado: 3, largura: 300 });
    const grande = montarGrafico({ frequencia: [2282, 2100], esperado: 2282, largura: 300 });
    expect(grande.esquerda).toBeGreaterThan(pequeno.esquerda);
    expect(grande.ticksY.some((t) => t.rotulo === '1.000')).toBe(true);
  });

  it('largura do viewBox: 320 só antes da medição; medida abaixo de 200 vira o mínimo', () => {
    expect(larguraDoGrafico(0)).toBe(320);
    expect(larguraDoGrafico(150)).toBe(LARGURA_MINIMA_GRAFICO);
    expect(larguraDoGrafico(199)).toBe(LARGURA_MINIMA_GRAFICO);
    expect(larguraDoGrafico(200)).toBe(200);
    expect(larguraDoGrafico(296)).toBe(296);
  });

  it('marcas de extremo encolhem com barras estreitas e não passam de 4,5', () => {
    const quina = montarGrafico({
      frequencia: Array.from({ length: 80 }, (_, i) => i % 7),
      esperado: 3,
      largura: 296,
    });
    const loto = montarGrafico({
      frequencia: Array.from({ length: 25 }, (_, i) => i % 7),
      esperado: 3,
      largura: 296,
    });
    expect(quina.raioMarca).toBeLessThan(loto.raioMarca);
    expect(quina.raioMarca).toBeGreaterThanOrEqual(2.5);
    expect(loto.raioMarca).toBe(4.5);
  });

  it('lista vazia não quebra', () => {
    const vazio = montarGrafico({ frequencia: [], esperado: 0, largura: 300 });
    expect(vazio.barras).toEqual([]);
    expect(vazio.ticksX).toEqual([]);
  });
});

describe('linhasTabelaFrequencia', () => {
  it('uma linha por dezena com vezes e diferença para o esperado', () => {
    const linhas = linhasTabelaFrequencia([4, 0, 8], 3.5);
    expect(linhas).toEqual([
      { dezena: 1, vezes: 4, diferenca: 0.5, posicao: null },
      { dezena: 2, vezes: 0, diferenca: -3.5, posicao: null },
      { dezena: 3, vezes: 8, diferenca: 4.5, posicao: null },
    ]);
  });

  it('com faixa do acaso: dentro, abaixo ou acima (limites inclusivos)', () => {
    const linhas = linhasTabelaFrequencia([4, 0, 8, 2, 6], 3.5, { inferior: 2, superior: 6 });
    expect(linhas.map((l) => l.posicao)).toEqual(['dentro', 'abaixo', 'acima', 'dentro', 'dentro']);
  });
});

describe('montarGrafico com a faixa do acaso (7.3)', () => {
  const entrada = { frequencia: [4, 0, 8, 2, 6], esperado: 3.5, largura: 300 };

  it('sem faixa nada muda: faixa nula e nenhuma barra fora', () => {
    const d = montarGrafico(entrada);
    expect(d.faixa).toBeNull();
    expect(d.barras.every((b) => !b.foraDaFaixa)).toBe(true);
  });

  it('a faixa vai de yDe(inferior) a yDe(superior), contém o esperado e fica na escala', () => {
    const d = montarGrafico({ ...entrada, faixa: { inferior: 2, superior: 6 } });
    expect(d.faixa).not.toBeNull();
    expect(d.faixa!.topo).toBeLessThan(d.faixa!.base);
    expect(d.esperadoY).toBeGreaterThan(d.faixa!.topo);
    expect(d.esperadoY).toBeLessThan(d.faixa!.base);
    expect(d.faixa!.base).toBeLessThanOrEqual(d.base);
    expect(d.faixa!.topo).toBeGreaterThanOrEqual(d.topo);
    // inferior 2 e superior 6 estão nas alturas das barras de 2 e de 6
    const b2 = d.barras[3]!;
    const b6 = d.barras[4]!;
    expect(d.faixa!.base).toBeCloseTo(b2.y, 9);
    expect(d.faixa!.topo).toBeCloseTo(b6.y, 9);
  });

  it('barras fora da faixa: abaixo do inferior ou acima do superior', () => {
    const d = montarGrafico({ ...entrada, faixa: { inferior: 2, superior: 6 } });
    expect(d.barras.map((b) => b.foraDaFaixa)).toEqual([false, true, true, false, false]);
  });

  it('a escala inclui o limite superior mesmo acima de todas as barras', () => {
    const d = montarGrafico({
      frequencia: [1, 2, 1],
      esperado: 1.3,
      largura: 300,
      faixa: { inferior: 0, superior: 9 },
    });
    expect(d.escalaMaxima).toBeGreaterThanOrEqual(9);
    expect(d.faixa!.topo).toBeGreaterThanOrEqual(d.topo);
  });
});
