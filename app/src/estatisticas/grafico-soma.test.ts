// Régua da soma (7.3): posições coerentes, dentro da largura e na ordem certa.
import { describe, expect, it } from 'vitest';
import { SLUGS } from '../dados/tipos';
import { distribuicaoSomaAoAcaso } from './acaso';
import { dominioDaSoma, montarReguaSoma } from './grafico-soma';

describe('montarReguaSoma', () => {
  for (const slug of SLUGS) {
    it(`${slug}: faixa e marcas ficam dentro da largura, em ordem, em 360 px e 1280 px`, () => {
      const d = distribuicaoSomaAoAcaso(slug);
      const obs = {
        media: d.media + 3,
        minimo: d.faixa.inferior - 10,
        maximo: d.faixa.superior + 10,
      };
      for (const largura of [300, 1100]) {
        const r = montarReguaSoma(d, obs, largura);
        expect(r.largura).toBe(largura);
        for (const x of [
          r.faixaX,
          r.faixaX + r.faixaLargura,
          r.esperadoX,
          r.mediaX,
          r.minimoX,
          r.maximoX,
        ]) {
          expect(x).toBeGreaterThanOrEqual(0);
          expect(x).toBeLessThanOrEqual(largura);
        }
        expect(r.minimoX).toBeLessThan(r.faixaX);
        expect(r.maximoX).toBeGreaterThan(r.faixaX + r.faixaLargura);
        expect(r.esperadoX).toBeGreaterThan(r.faixaX);
        expect(r.esperadoX).toBeLessThan(r.faixaX + r.faixaLargura);
        expect(r.ticks.map((t) => t.valor)).toEqual([d.faixa.inferior, d.media, d.faixa.superior]);
      }
    });
  }

  it('o domínio sempre inclui a faixa e o observado', () => {
    const d = distribuicaoSomaAoAcaso('quina');
    const [baixo, alto] = dominioDaSoma(d, { media: 200, minimo: 5, maximo: 399 });
    expect(baixo).toBeLessThanOrEqual(5);
    expect(alto).toBeGreaterThanOrEqual(399);
  });

  it('largura muito pequena vira um mínimo utilizável', () => {
    const d = distribuicaoSomaAoAcaso('lotofacil');
    expect(montarReguaSoma(d, { media: 195, minimo: 180, maximo: 210 }, 0).largura).toBe(240);
  });
});
