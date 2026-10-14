import { describe, expect, it } from 'vitest';
import { PARAMETROS_LOTERIA, SLUGS } from '../dados/tipos';
import { formatarIndice, formatarVariacao, segmentosDoGrau, sentidoVariacao } from './ranking';
import { proximoIndice } from './segmentado';
import { dezenasDoVolante, faixaPorPosicao, layoutVolante, limitesDaFaixa } from './volante';

describe('layoutVolante', () => {
  it('Lotofácil 5x5, Mega-Sena 6x10, Quina 8x10', () => {
    expect(layoutVolante('lotofacil')).toEqual({ linhas: 5, colunas: 5, total: 25 });
    expect(layoutVolante('megasena')).toEqual({ linhas: 6, colunas: 10, total: 60 });
    expect(layoutVolante('quina')).toEqual({ linhas: 8, colunas: 10, total: 80 });
  });
  it('linhas x colunas cobre todas as dezenas, de 1 a N', () => {
    for (const slug of SLUGS) {
      const { linhas, colunas } = layoutVolante(slug);
      const dezenas = dezenasDoVolante(slug);
      expect(linhas * colunas).toBe(PARAMETROS_LOTERIA[slug].totalDezenas);
      expect(dezenas[0]).toBe(1);
      expect(dezenas.at(-1)).toBe(linhas * colunas);
    }
  });
});

describe('faixaPorPosicao', () => {
  it('Lotofácil (25): 5 posições por faixa', () => {
    expect([1, 5, 6, 10, 11, 15, 16, 20, 21, 25].map((p) => faixaPorPosicao(p, 25))).toEqual([
      1, 1, 2, 2, 3, 3, 4, 4, 5, 5,
    ]);
  });
  it('Mega-Sena (60): 12 por faixa; Quina (80): 16 por faixa', () => {
    expect([1, 12, 13, 36, 37, 60].map((p) => faixaPorPosicao(p, 60))).toEqual([1, 1, 2, 3, 4, 5]);
    expect([1, 16, 17, 48, 49, 80].map((p) => faixaPorPosicao(p, 80))).toEqual([1, 1, 2, 3, 4, 5]);
  });
  it('cada faixa tem o mesmo tamanho quando N é múltiplo de 5', () => {
    for (const total of [25, 60, 80]) {
      const contagem = [0, 0, 0, 0, 0, 0];
      for (let p = 1; p <= total; p++) contagem[faixaPorPosicao(p, total)]!++;
      expect(contagem.slice(1)).toEqual(Array(5).fill(total / 5));
    }
  });
  it('limita posições fora do intervalo às pontas', () => {
    expect(faixaPorPosicao(0, 25)).toBe(1);
    expect(faixaPorPosicao(-3, 25)).toBe(1);
    expect(faixaPorPosicao(99, 25)).toBe(5);
  });
  it('limites da faixa batem com faixaPorPosicao', () => {
    expect(limitesDaFaixa(1, 60)).toEqual({ de: 1, ate: 12 });
    expect(limitesDaFaixa(5, 60)).toEqual({ de: 49, ate: 60 });
    for (const total of [25, 60, 80, 17]) {
      for (let f = 1; f <= 5; f++) {
        const { de, ate } = limitesDaFaixa(f, total);
        if (de <= ate) {
          expect(faixaPorPosicao(de, total)).toBe(f);
          expect(faixaPorPosicao(ate, total)).toBe(f);
        }
      }
    }
  });
});

describe('formatação do ranking', () => {
  it('variação: +3, −2 (menos tipográfico), 0, e travessão para null', () => {
    expect(formatarVariacao(3)).toBe('+3');
    expect(formatarVariacao(-2)).toBe('−2');
    expect(formatarVariacao(0)).toBe('0');
    expect(formatarVariacao(null)).toBe('—');
  });
  it('sentido da variação', () => {
    expect(sentidoVariacao(1)).toBe('subiu');
    expect(sentidoVariacao(-1)).toBe('caiu');
    expect(sentidoVariacao(0)).toBe('igual');
    expect(sentidoVariacao(null)).toBe('sem');
  });
  it('índice com 2 casas, vírgula e menos tipográfico', () => {
    expect(formatarIndice(4.063717154)).toBe('4,064');
    expect(formatarIndice(-0.4)).toBe('−0,400');
    expect(formatarIndice(0)).toBe('0,000');
  });
  it('segmentos do grau de evidência', () => {
    expect(segmentosDoGrau('muito_fraca')).toBe(1);
    expect(segmentosDoGrau('muito_forte')).toBe(5);
    expect(segmentosDoGrau(null)).toBe(0);
  });
});

describe('proximoIndice (teclado do Segmentado)', () => {
  it('setas dão a volta; Home/End vão às pontas; outras teclas não navegam', () => {
    expect(proximoIndice(0, 'ArrowRight', 3)).toBe(1);
    expect(proximoIndice(2, 'ArrowRight', 3)).toBe(0);
    expect(proximoIndice(0, 'ArrowLeft', 3)).toBe(2);
    expect(proximoIndice(1, 'ArrowDown', 3)).toBe(2);
    expect(proximoIndice(1, 'ArrowUp', 3)).toBe(0);
    expect(proximoIndice(1, 'Home', 3)).toBe(0);
    expect(proximoIndice(1, 'End', 3)).toBe(2);
    expect(proximoIndice(1, 'a', 3)).toBeNull();
    expect(proximoIndice(0, 'ArrowRight', 0)).toBeNull();
  });
});
