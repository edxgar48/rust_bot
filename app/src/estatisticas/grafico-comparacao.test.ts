// Mini-barras emparelhadas (7.3): posições e textos, sem DOM.
import { describe, expect, it } from 'vitest';
import type { LinhaComparacao } from './acaso';
import { ALTURA_LINHA, RESERVA_TEXTO, X_BARRAS, montarMiniBarras } from './grafico-comparacao';
import { formatarPercentual } from '../lib/numeros';

const linha = (valor: number, obs: number, esp: number): LinhaComparacao => ({
  valor,
  observado: obs * 100,
  fracaoObservada: obs,
  fracaoEsperada: esp,
  esperado: esp * 100,
});

describe('montarMiniBarras', () => {
  const linhas = [linha(2, 0.1, 0.2), linha(3, 0.4, 0.3), linha(4, 0.5, 0.5)];
  const d = montarMiniBarras(linhas, 320);

  it('uma linha por valor, com altura proporcional e rótulos de texto', () => {
    expect(d.linhas.map((l) => l.rotulo)).toEqual(['2', '3', '4']);
    expect(d.altura).toBeGreaterThanOrEqual(3 * ALTURA_LINHA);
    expect(d.linhas[1]!.y - d.linhas[0]!.y).toBe(ALTURA_LINHA);
  });

  it('as barras usam a MESMA escala; a maior ocupa a largura útil inteira', () => {
    expect(d.escalaMaxima).toBe(0.5);
    expect(d.larguraUtil).toBe(320 - X_BARRAS - RESERVA_TEXTO);
    expect(d.linhas[2]!.larguraObservada).toBeCloseTo(d.larguraUtil, 9);
    expect(d.linhas[2]!.larguraEsperada).toBeCloseTo(d.larguraUtil, 9);
    expect(d.linhas[0]!.larguraObservada / d.linhas[1]!.larguraObservada).toBeCloseTo(0.1 / 0.4, 9);
    expect(d.linhas[0]!.larguraEsperada / d.linhas[1]!.larguraEsperada).toBeCloseTo(0.2 / 0.3, 9);
    // observado e esperado do mesmo valor são comparáveis na mesma escala
    expect(d.linhas[1]!.larguraObservada / d.linhas[1]!.larguraEsperada).toBeCloseTo(0.4 / 0.3, 9);
  });

  it('o texto do percentual fica logo após o fim de cada barra e dentro do viewBox', () => {
    for (const l of d.linhas) {
      expect(l.xTextoObservado).toBeCloseTo(X_BARRAS + l.larguraObservada + 5, 9);
      expect(l.xTextoEsperado).toBeCloseTo(X_BARRAS + l.larguraEsperada + 5, 9);
      expect(l.xTextoObservado).toBeLessThan(d.largura);
    }
    expect(d.linhas.map((l) => l.textoObservado)).toEqual(['10%', '40%', '50%']);
    expect(d.linhas.map((l) => l.textoEsperado)).toEqual(['20%', '30%', '50%']);
  });

  it('observado zero tem largura zero (e texto "0%"), esperado positivo mínimo tem 1 px', () => {
    const z = montarMiniBarras([linha(0, 0, 0.5), linha(1, 0.5, 0.0000001)], 320);
    expect(z.linhas[0]!.larguraObservada).toBe(0);
    expect(z.linhas[0]!.textoObservado).toBe('0%');
    expect(z.linhas[1]!.larguraEsperada).toBeGreaterThanOrEqual(1);
    expect(z.linhas[1]!.textoEsperado).toBe('<0,1%');
  });

  it('largura pequena demais vira o mínimo; sem linhas não quebra', () => {
    expect(montarMiniBarras(linhas, 50).largura).toBe(200);
    const vazio = montarMiniBarras([], 320);
    expect(vazio.linhas).toEqual([]);
    expect(vazio.escalaMaxima).toBe(1);
  });
});

describe('formatarPercentual', () => {
  it('uma casa abaixo de 10%, nenhuma a partir daí', () => {
    expect(formatarPercentual(0.234)).toBe('23%');
    expect(formatarPercentual(0.045)).toBe('4,5%');
    expect(formatarPercentual(0.1)).toBe('10%');
    expect(formatarPercentual(1)).toBe('100%');
    expect(formatarPercentual(0)).toBe('0%');
    expect(formatarPercentual(0.004)).toBe('0,4%');
    expect(formatarPercentual(0.000001)).toBe('<0,1%');
  });
});
