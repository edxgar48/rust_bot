// Comparação com o acaso (7.3): valores à mão em casos pequenos e comparação, com tolerância
// de 1e-9, com a referência gerada de forma independente por testes/gerar-esperado-acaso.py
// (Python exato; a hipergeométrica é conferida lá contra o back-end Rust de verdade).
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import type { Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA, SLUGS } from '../dados/tipos';
import { validarHistorico } from '../dados/validacao';
import {
  CAUDA_PADRAO,
  chanceDeAtrasoAoMenos,
  combinacao,
  contarSomas,
  dezenasEsperadasComAtraso,
  distribuicaoSomaAoAcaso,
  sorteiosNaFaixaDaSoma,
  compararDistribuicao,
  dezenasForaDaFaixa,
  distribuicaoParesAoAcaso,
  distribuicaoRepetidasAoAcaso,
  faixaCentral,
  faixaDaFrequencia,
  hipergeometrica,
  linhasVisiveis,
  logFatorial,
  mediaEDesvio,
  pmfBinomial,
  posicaoNaFaixa,
} from './acaso';
import { JANELAS_ESTATISTICA, calcularEstatisticas } from './estatisticas';

const TOL = 1e-9;
const soma = (v: readonly number[]) => v.reduce((a, b) => a + b, 0);

interface JanelaRef {
  n: number;
  media: number;
  desvio: number;
  inferior: number;
  superior: number;
  cobertura_real: number;
  fora_esperado_por_dezena: number;
  x0: number;
  pmf: number[];
}
interface HiperRef {
  n: number;
  sorteadas: number;
  escolhidas: number;
  total: string;
  casos: string[];
  pmf: number[];
  media: number;
  variancia: number;
  conferido_com_rust: boolean;
}
interface ForaRef {
  n: number;
  inferior: number;
  superior: number;
  abaixo: number[];
  acima: number[];
}
interface SomaRef {
  total_dezenas: number;
  k: number;
  minimo: number;
  maximo: number;
  total_casos: string;
  casos: string[];
  media: number;
  variancia: number;
  inferior: number;
  superior: number;
  cobertura_real: number;
  fora_esperado_por_sorteio: number;
}
interface AtrasoRef {
  total_dezenas: number;
  k: number;
  pontos: { atraso: number; chance: number; dezenas_esperadas: number }[];
}
const ref = JSON.parse(lerFixtureTexto('acaso-esperado.json')) as {
  soma: Record<Slug, SomaRef>;
  atraso: Record<Slug, AtrasoRef>;
  hipergeometrica: Record<string, HiperRef>;
  binomial: Record<
    Slug,
    { total_dezenas: number; k: number; janelas: JanelaRef[]; fora: Record<string, ForaRef> }
  >;
};

describe('logFatorial', () => {
  it('valores pequenos exatos', () => {
    expect(logFatorial(0)).toBe(0);
    expect(logFatorial(1)).toBe(0);
    expect(logFatorial(5)).toBeCloseTo(Math.log(120), 14);
    expect(logFatorial(10)).toBeCloseTo(Math.log(3628800), 13);
  });
  it('emenda da tabela com Stirling (29 e 30) sem salto', () => {
    expect(logFatorial(30) - logFatorial(29)).toBeCloseTo(Math.log(30), 13);
    expect(logFatorial(31) - logFatorial(30)).toBeCloseTo(Math.log(31), 13);
  });
  it('valores grandes iguais a lgamma de referência (Python)', () => {
    expect(Math.abs(logFatorial(100) - 363.73937555556347)).toBeLessThan(1e-11);
    expect(Math.abs(logFatorial(30) - 74.65823634883017)).toBeLessThan(1e-12);
    expect(Math.abs(logFatorial(3803) - 27552.24429985087)).toBeLessThan(1e-9);
  });
  it('recusa n negativo ou fracionário', () => {
    expect(() => logFatorial(-1)).toThrow(RangeError);
    expect(() => logFatorial(2.5)).toThrow(RangeError);
  });
});

describe('binomial: valores à mão', () => {
  it('n = 4, p = 1/2: 1, 4, 6, 4, 1 sobre 16', () => {
    const pmf = pmfBinomial(4, 0.5);
    [1, 4, 6, 4, 1].forEach((c, x) => expect(pmf[x]).toBeCloseTo(c / 16, 14));
  });
  it('p = 0 e p = 1 são degeneradas', () => {
    expect(pmfBinomial(3, 0)).toEqual([1, 0, 0, 0]);
    expect(pmfBinomial(3, 1)).toEqual([0, 0, 0, 1]);
  });
  it('n = 0 tem um único valor', () => {
    expect(pmfBinomial(0, 0.3)).toEqual([1]);
  });
  it('n = 10, p = 1/2: faixa de 2 a 8 com cobertura 1002/1024', () => {
    // cdf(1) = 11/1024 = 1,07% < 2,5% <= cdf(2) = 56/1024; cdf(7) = 968/1024 = 94,5% < 97,5%
    // <= cdf(8) = 1013/1024. Dentro: (1013 − 11)/1024.
    const f = faixaCentral(pmfBinomial(10, 0.5));
    expect(f.inferior).toBe(2);
    expect(f.superior).toBe(8);
    expect(f.coberturaReal).toBeCloseTo(1002 / 1024, 12);
    expect(f.foraPorDezena).toBeCloseTo(22 / 1024, 12);
  });
  it('média e desvio de uma pmf: binomial(10, 1/2) tem média 5 e desvio √2,5', () => {
    const { media, desvio } = mediaEDesvio(pmfBinomial(10, 0.5));
    expect(media).toBeCloseTo(5, 12);
    expect(desvio).toBeCloseTo(Math.sqrt(2.5), 12);
  });
  it('recusa parâmetros inválidos', () => {
    expect(() => pmfBinomial(-1, 0.5)).toThrow(RangeError);
    expect(() => pmfBinomial(3, 1.2)).toThrow(RangeError);
    expect(() => faixaCentral([])).toThrow(RangeError);
  });
});

describe('faixaDaFrequencia', () => {
  it('janela vazia não tem faixa', () => {
    expect(faixaDaFrequencia('quina', 0)).toBeNull();
  });
  it('Lotofácil em 10 concursos: p = 0,6, esperado 6, desvio √2,4, faixa 3 a 9', () => {
    const f = faixaDaFrequencia('lotofacil', 10)!;
    expect(f.tamanho).toBe(10);
    expect(f.p).toBeCloseTo(0.6, 15);
    expect(f.esperado).toBeCloseTo(6, 12);
    expect(f.desvio).toBeCloseTo(Math.sqrt(2.4), 12);
    expect([f.inferior, f.superior]).toEqual([3, 9]);
    expect(f.coberturaReal).toBeGreaterThanOrEqual(0.95);
  });
  it('a cobertura real nunca é menor que a nominal, em nenhuma janela das fixtures', () => {
    for (const slug of SLUGS) {
      for (const j of ref.binomial[slug].janelas) {
        const f = faixaDaFrequencia(slug, j.n)!;
        expect(f.coberturaReal).toBeGreaterThanOrEqual(1 - 2 * CAUDA_PADRAO - 1e-12);
      }
    }
  });
  it('janelas maiores têm faixa relativamente mais estreita (desvio/esperado cai)', () => {
    const a = faixaDaFrequencia('megasena', 50)!;
    const b = faixaDaFrequencia('megasena', 3000)!;
    expect(b.desvio / b.esperado).toBeLessThan(a.desvio / a.esperado);
  });
});

describe('binomial: contra a referência independente (Python exato), 1e-9', () => {
  for (const slug of SLUGS) {
    for (const j of ref.binomial[slug].janelas) {
      it(`${slug}, N = ${j.n}`, () => {
        const { totalDezenas, dezenasSorteadas } = PARAMETROS_LOTERIA[slug];
        const f = faixaDaFrequencia(slug, j.n)!;
        expect(Math.abs(f.esperado - j.media)).toBeLessThan(TOL);
        expect(Math.abs(f.desvio - j.desvio)).toBeLessThan(TOL);
        expect(f.inferior).toBe(j.inferior);
        expect(f.superior).toBe(j.superior);
        expect(Math.abs(f.coberturaReal - j.cobertura_real)).toBeLessThan(TOL);
        expect(Math.abs(f.foraPorDezena - j.fora_esperado_por_dezena)).toBeLessThan(TOL);

        const pmf = pmfBinomial(j.n, dezenasSorteadas / totalDezenas);
        expect(pmf).toHaveLength(j.n + 1);
        j.pmf.forEach((esperado, i) => {
          expect(Math.abs(pmf[j.x0 + i]! - esperado)).toBeLessThan(TOL);
        });
        // O que a fixture cortou (< 1e-15 por ponto) soma no máximo N · 1e-15 no app.
        const dentroDoCorte = soma(pmf.slice(j.x0, j.x0 + j.pmf.length));
        expect(Math.abs(soma(pmf) - dentroDoCorte)).toBeLessThan(TOL);
        expect(Math.abs(soma(pmf) - 1)).toBeLessThan(TOL);
        // média e desvio da própria pmf calculada pelo app
        const m = mediaEDesvio(pmf);
        expect(Math.abs(m.media - j.media)).toBeLessThan(TOL * 10);
        expect(Math.abs(m.desvio - j.desvio)).toBeLessThan(TOL * 10);
      });
    }
  }

  it('o custo é pequeno: janela de 8000 concursos em bem menos de um segundo', () => {
    const t0 = performance.now();
    const pmf = pmfBinomial(8000, 0.6);
    faixaCentral(pmf);
    expect(performance.now() - t0).toBeLessThan(1000);
    expect(Math.abs(soma(pmf) - 1)).toBeLessThan(TOL);
  });
});

describe('dezenas fora da faixa', () => {
  const f = { inferior: 2, superior: 4, coberturaReal: 0.95, foraPorDezena: 0.05 };
  it('posicaoNaFaixa: os limites estão dentro', () => {
    expect(posicaoNaFaixa(1, f)).toBe('abaixo');
    expect(posicaoNaFaixa(2, f)).toBe('dentro');
    expect(posicaoNaFaixa(4, f)).toBe('dentro');
    expect(posicaoNaFaixa(5, f)).toBe('acima');
  });
  it('à mão: 5 dezenas, 2 fora; esperado = total · foraPorDezena', () => {
    const r = dezenasForaDaFaixa([1, 3, 5, 4, 2], f);
    expect(r.posicao).toEqual(['abaixo', 'dentro', 'acima', 'dentro', 'dentro']);
    expect(r.abaixo).toEqual([1]);
    expect(r.acima).toEqual([3]);
    expect(r.fora).toBe(2);
    expect(r.foraEsperado).toBeCloseTo(0.25, 12);
  });
  it('Lotofácil: 5% de 25 dezenas é cerca de 1,25 (a cobertura real é maior, então um pouco menos)', () => {
    const faixa = faixaDaFrequencia('lotofacil', 3803)!;
    const r = dezenasForaDaFaixa(new Array<number>(25).fill(2282), faixa);
    expect(r.foraEsperado).toBeCloseTo(25 * (1 - faixa.coberturaReal), 12);
    expect(r.foraEsperado).toBeGreaterThan(1);
    expect(r.foraEsperado).toBeLessThanOrEqual(1.25);
  });

  for (const slug of SLUGS) {
    const concursos = validarHistorico(
      JSON.parse(lerFixtureTexto(`${slug}/historico.json`)),
      slug,
    ).concursos;
    for (const janela of JANELAS_ESTATISTICA) {
      it(`${slug}, janela ${janela}: mesmas dezenas fora da referência independente`, () => {
        const e = calcularEstatisticas(slug, concursos, janela);
        const faixa = faixaDaFrequencia(slug, e.tamanho)!;
        const r = dezenasForaDaFaixa(e.frequencia, faixa);
        const esp = ref.binomial[slug].fora[String(janela)]!;
        expect(e.tamanho).toBe(esp.n);
        expect([faixa.inferior, faixa.superior]).toEqual([esp.inferior, esp.superior]);
        expect(r.abaixo).toEqual(esp.abaixo);
        expect(r.acima).toEqual(esp.acima);
      });
    }
  }
});

describe('combinação exata', () => {
  it('casos conhecidos (iguais aos do back-end)', () => {
    expect(combinacao(60, 6)).toBe(50_063_860n);
    expect(combinacao(80, 40)).toBe(107507208733336176461620n);
    expect(combinacao(5, 6)).toBe(0n);
    expect(combinacao(7, 0)).toBe(1n);
    expect(combinacao(7, 7)).toBe(1n);
  });
});

describe('hipergeométrica: valores à mão (mesmos do back-end)', () => {
  it('n = 10, 3 sorteadas, 4 escolhidas: 35, 105, 63, 7 sobre 210 (teste do Rust)', () => {
    const h = hipergeometrica(10, 3, 4);
    expect(h.total).toBe(210n);
    expect(h.casos.slice(0, 4)).toEqual([35n, 105n, 63n, 7n]);
    expect(h.casos[4]).toBe(0n);
    expect([h.minimo, h.maximo]).toEqual([0, 3]);
    expect(h.media).toBe(1.2);
    expect(h.pmf[1]).toBeCloseTo(0.5, 15);
  });
  it('n = 25, 15 sorteadas, 20 escolhidas: mínimo 10 (teste do Rust)', () => {
    const h = hipergeometrica(25, 15, 20);
    expect(h.total).toBe(53_130n);
    expect([h.minimo, h.maximo]).toEqual([10, 15]);
    expect(h.casos.slice(10, 16)).toEqual([3003n, 13650n, 20475n, 12600n, 3150n, 252n]);
    expect(h.casos.slice(0, 10).every((c) => c === 0n)).toBe(true);
    expect(h.media).toBe(12);
  });
  it('tabela da Caixa: Lotofácil 15 pontos 1 em 3.268.760, Mega sena 1 em 50.063.860, Quina 1 em 24.040.016', () => {
    const lf = distribuicaoRepetidasAoAcaso('lotofacil');
    expect(lf.total).toBe(3_268_760n);
    expect(lf.casos[15]).toBe(1n);
    expect(lf.casos[11]).toBe(286_650n);
    expect(lf.total / lf.casos[11]!).toBe(11n);
    const ms = distribuicaoRepetidasAoAcaso('megasena');
    expect(ms.total).toBe(50_063_860n);
    expect(ms.total / ms.casos[5]!).toBe(154_518n);
    const qn = distribuicaoRepetidasAoAcaso('quina');
    expect(qn.total).toBe(24_040_016n);
    expect(qn.total / qn.casos[3]!).toBe(866n);
    expect(qn.media).toBe(0.3125);
  });
  it('Vandermonde: os casos somam o total, em todas as distribuições', () => {
    for (const slug of SLUGS) {
      for (const h of [distribuicaoParesAoAcaso(slug), distribuicaoRepetidasAoAcaso(slug)]) {
        expect(h.casos.reduce((a, b) => a + b, 0n)).toBe(h.total);
        expect(Math.abs(soma(h.pmf) - 1)).toBeLessThan(1e-12);
      }
    }
  });
  it('as medias são as fórmulas simples do 7.1: pares K·(total/2)/total e repetidas K²/total', () => {
    for (const slug of SLUGS) {
      const { totalDezenas: t, dezenasSorteadas: k } = PARAMETROS_LOTERIA[slug];
      expect(distribuicaoParesAoAcaso(slug).media).toBeCloseTo((k * Math.floor(t / 2)) / t, 12);
      expect(distribuicaoRepetidasAoAcaso(slug).media).toBeCloseTo((k * k) / t, 12);
      // e a média calculada da própria pmf bate com a fórmula
      expect(mediaEDesvio(distribuicaoParesAoAcaso(slug).pmf).media).toBeCloseTo(
        distribuicaoParesAoAcaso(slug).media,
        12,
      );
    }
  });
  it('recusa parâmetros inválidos', () => {
    expect(() => hipergeometrica(4, 5, 2)).toThrow(RangeError);
    expect(() => hipergeometrica(4, 2, 5)).toThrow(RangeError);
    expect(() => hipergeometrica(4, -1, 2)).toThrow(RangeError);
  });
});

describe('hipergeométrica: contra o back-end Rust (via fixture), inteiro a inteiro', () => {
  for (const [rotulo, r] of Object.entries(ref.hipergeometrica)) {
    it(rotulo, () => {
      expect(r.conferido_com_rust).toBe(true);
      const [slug, tipo] = rotulo.split('_') as [Slug, 'pares' | 'repetidas'];
      const h =
        tipo === 'pares' ? distribuicaoParesAoAcaso(slug) : distribuicaoRepetidasAoAcaso(slug);
      expect([h.n, h.sorteadas, h.escolhidas]).toEqual([r.n, r.sorteadas, r.escolhidas]);
      expect(h.total.toString()).toBe(r.total);
      expect(h.casos.map(String)).toEqual(r.casos);
      r.pmf.forEach((p, i) => expect(Math.abs(h.pmf[i]! - p)).toBeLessThan(TOL));
      expect(Math.abs(h.media - r.media)).toBeLessThan(TOL);
      expect(Math.abs(h.variancia - r.variancia)).toBeLessThan(TOL);
    });
  }
  it('a fixture cobre as 3 loterias × pares e repetidas', () => {
    expect(Object.keys(ref.hipergeometrica).sort()).toEqual(
      SLUGS.flatMap((s) => [`${s}_pares`, `${s}_repetidas`]).sort(),
    );
  });
});

describe('observado × esperado', () => {
  it('à mão: 10 sorteios, hipergeométrica (10, 3, 4)', () => {
    const h = hipergeometrica(10, 3, 4);
    const c = compararDistribuicao([2, 4, 3, 1, 0], h);
    expect(c.sorteios).toBe(10);
    expect(c.linhas.map((l) => l.valor)).toEqual([0, 1, 2, 3]);
    expect(c.linhas[1]!.observado).toBe(4);
    expect(c.linhas[1]!.fracaoObservada).toBe(0.4);
    expect(c.linhas[1]!.fracaoEsperada).toBeCloseTo(0.5, 15);
    expect(c.linhas[1]!.esperado).toBeCloseTo(5, 12);
  });
  it('o suporte começa no mínimo possível (Lotofácil, repetidas: 5 a 15)', () => {
    const c = compararDistribuicao(
      new Array<number>(16).fill(0),
      distribuicaoRepetidasAoAcaso('lotofacil'),
    );
    expect(c.linhas[0]!.valor).toBe(5);
    expect(c.linhas.at(-1)!.valor).toBe(15);
    expect(c.sorteios).toBe(0);
    expect(c.linhas.every((l) => l.fracaoObservada === 0 && l.esperado === 0)).toBe(true);
  });
  it('linhasVisiveis corta as pontas raras sem ocorrência, mas guarda as que ocorreram', () => {
    const h = distribuicaoRepetidasAoAcaso('megasena'); // 0..6; P(5) = 6,5e-6, P(6) = 2e-8
    const sem = linhasVisiveis(compararDistribuicao([50, 40, 8, 2, 0, 0, 0], h));
    expect(sem.map((l) => l.valor)).toEqual([0, 1, 2, 3]);
    const com = linhasVisiveis(compararDistribuicao([50, 40, 8, 2, 0, 1, 0], h));
    expect(com.map((l) => l.valor)).toEqual([0, 1, 2, 3, 4, 5]);
  });
  it('Lotofácil, pares: valores raros nas duas pontas ficam só na tabela', () => {
    const c = compararDistribuicao(
      [0, 0, 0, 1, 3, 10, 12, 14, 8, 2, 0, 0, 0, 0, 0, 0],
      distribuicaoParesAoAcaso('lotofacil'),
    );
    expect(c.linhas.map((l) => l.valor)).toEqual([2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    expect(linhasVisiveis(c).map((l) => l.valor)).toEqual([3, 4, 5, 6, 7, 8, 9, 10, 11]);
  });
  it('contra a distribuição observada real: soma das frações é 1', () => {
    for (const slug of SLUGS) {
      const concursos = validarHistorico(
        JSON.parse(lerFixtureTexto(`${slug}/historico.json`)),
        slug,
      ).concursos;
      const e = calcularEstatisticas(slug, concursos, 'total');
      const cp = compararDistribuicao(e.distribuicaoPares, distribuicaoParesAoAcaso(slug));
      expect(cp.sorteios).toBe(e.tamanho);
      expect(soma(cp.linhas.map((l) => l.fracaoObservada))).toBeCloseTo(1, 12);
      expect(soma(cp.linhas.map((l) => l.esperado))).toBeCloseTo(e.tamanho, 6);
      const cr = compararDistribuicao(e.repetidas.distribuicao, distribuicaoRepetidasAoAcaso(slug));
      expect(cr.sorteios).toBe(e.repetidas.comparacoes);
    }
  });
});

describe('soma das dezenas ao acaso (distribuição exata)', () => {
  it('à mão: 2 dezenas de 1..4 somam 3,4,5,5,6,7', () => {
    const c = contarSomas(4, 2);
    expect(c.map(Number)).toEqual([0, 0, 0, 1, 1, 2, 1, 1, 0]);
    expect(c.reduce((a, b) => a + b, 0n)).toBe(6n);
  });

  it('k = 0 tem uma soma só (0); parâmetros inválidos lançam', () => {
    expect(contarSomas(5, 0).map(Number)).toEqual([1]);
    expect(() => contarSomas(3, 4)).toThrow(RangeError);
  });

  for (const slug of SLUGS) {
    it(`${slug}: casos inteiro a inteiro, faixa e momentos iguais à referência Python`, () => {
      const r = ref.soma[slug];
      const d = distribuicaoSomaAoAcaso(slug);
      expect(d.minimo).toBe(r.minimo);
      expect(d.maximo).toBe(r.maximo);
      expect(d.totalCasos.toString()).toBe(r.total_casos);
      // índice 0 da referência = soma `minimo`
      const doApp = d.casos.slice(d.minimo, d.maximo + 1).map(String);
      expect(doApp).toEqual(r.casos);
      expect(d.casos.slice(0, d.minimo).every((c) => c === 0n)).toBe(true);
      expect(d.casos.slice(d.maximo + 1).every((c) => c === 0n)).toBe(true);
      expect(d.casos.reduce((a, b) => a + b, 0n)).toBe(d.totalCasos);
      expect(d.faixa.inferior).toBe(r.inferior);
      expect(d.faixa.superior).toBe(r.superior);
      expect(Math.abs(d.faixa.coberturaReal - r.cobertura_real)).toBeLessThan(TOL);
      expect(Math.abs(d.faixa.foraPorDezena - r.fora_esperado_por_sorteio)).toBeLessThan(TOL);
      expect(d.faixa.coberturaReal).toBeGreaterThanOrEqual(0.95);
      expect(d.media).toBe(r.media);
      const { media, desvio } = mediaEDesvio(d.pmf);
      expect(Math.abs(media - r.media)).toBeLessThan(TOL);
      expect(Math.abs(desvio ** 2 - r.variancia)).toBeLessThan(1e-7);
    });
  }

  it('contagem de sorteios dentro da faixa soma os intervalos inclusive', () => {
    const obs = new Array<number>(30).fill(0);
    obs[5] = 2;
    obs[6] = 3;
    obs[7] = 4;
    obs[9] = 1;
    expect(sorteiosNaFaixaDaSoma(obs, { inferior: 6, superior: 9 })).toBe(8);
    expect(sorteiosNaFaixaDaSoma(obs, { inferior: 20, superior: 99 })).toBe(0);
  });
});

describe('atraso ao acaso: P(atraso >= a) = ((total − K)/total)^a', () => {
  it('à mão: Lotofácil, 2 concursos sem sair = (10/25)^2 = 16%', () => {
    expect(chanceDeAtrasoAoMenos('lotofacil', 0)).toBe(1);
    expect(chanceDeAtrasoAoMenos('lotofacil', 2)).toBeCloseTo(0.16, 12);
    expect(dezenasEsperadasComAtraso('lotofacil', 2)).toBeCloseTo(4, 12);
  });

  it('atraso inválido lança', () => {
    expect(() => chanceDeAtrasoAoMenos('quina', -1)).toThrow(RangeError);
    expect(() => chanceDeAtrasoAoMenos('quina', 1.5)).toThrow(RangeError);
  });

  for (const slug of SLUGS) {
    it(`${slug}: bate com a referência Fraction exata (erro relativo < 1e-12)`, () => {
      for (const p of ref.atraso[slug].pontos) {
        const c = chanceDeAtrasoAoMenos(slug, p.atraso);
        const e = dezenasEsperadasComAtraso(slug, p.atraso);
        expect(Math.abs(c - p.chance)).toBeLessThanOrEqual(1e-12 * p.chance + 1e-300);
        expect(Math.abs(e - p.dezenas_esperadas)).toBeLessThanOrEqual(
          1e-12 * p.dezenas_esperadas + 1e-300,
        );
      }
    });
  }
});
