// Estatísticas (7.1): valores à mão em casos pequenos e comparação com o esperado
// gerado de forma independente por SQL sobre os bancos reais
// (testes/gerar-esperado-estatisticas.py -> fixtures/estatisticas-esperado.json).
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { PARAMETROS_LOTERIA, SLUGS } from '../dados/tipos';
import { validarHistorico } from '../dados/validacao';
import {
  JANELAS_ESTATISTICA,
  calcularEstatisticas,
  calcularTodasAsJanelas,
  tamanhoEfetivo,
} from './estatisticas';

// Quina (80 dezenas, k = 5), 4 concursos à mão.
const MAO: readonly ConcursoHistorico[] = [
  [1, '2020-01-01', [1, 2, 3, 4, 5]],
  [2, '2020-01-02', [2, 4, 10, 20, 80]],
  [3, '2020-01-03', [1, 2, 4, 7, 9]],
  [4, '2020-01-04', [10, 11, 12, 13, 14]],
];

describe('tamanhoEfetivo', () => {
  it('usa todos quando há menos que a janela', () => {
    expect(tamanhoEfetivo(10, 4)).toBe(4);
    expect(tamanhoEfetivo(10, 30)).toBe(10);
    expect(tamanhoEfetivo('total', 30)).toBe(30);
    expect(tamanhoEfetivo(10, 0)).toBe(0);
  });
});

// paresEsperado, somaEsperada e repetidas.esperada são conferidos à mão aqui; a
// conferência contra o back-end é da tarefa 7.3.
describe('calcularEstatisticas com valores à mão (Quina)', () => {
  const e = calcularEstatisticas('quina', MAO, 'total');

  it('tamanho e limites', () => {
    expect(e.tamanho).toBe(4);
    expect(e.primeiroConcurso).toBe(1);
    expect(e.ultimoConcurso).toBe(4);
  });

  it('frequência e esperado', () => {
    expect(e.frequencia[0]).toBe(2); // dezena 1
    expect(e.frequencia[1]).toBe(3); // dezena 2
    expect(e.frequencia[3]).toBe(3); // dezena 4
    expect(e.frequencia[9]).toBe(2); // dezena 10
    expect(e.frequencia[79]).toBe(1); // dezena 80
    expect(e.frequencia[49]).toBe(0);
    expect(e.frequencia.reduce((a, b) => a + b, 0)).toBe(20);
    expect(e.frequenciaEsperada).toBe((4 * 5) / 80);
  });

  it('atraso, ranking e dezenas sem aparição', () => {
    expect(e.atraso[13]).toBe(0); // 14 saiu no último
    expect(e.atraso[0]).toBe(1); // 1 saiu no 3 (último = 4)
    expect(e.atraso[2]).toBe(3); // 3 saiu só no 1
    expect(e.atraso[79]).toBe(2); // 80 saiu no 2
    expect(e.atraso[49]).toBeNull();
    expect(e.semAparicao).toHaveLength(80 - 14);
    expect(e.semAparicao[0]).toBe(6);
    expect(e.rankingAtraso).toHaveLength(14);
    // atraso 3: dezenas 3 e 5; atraso 2: 20 e 80 (empate: dezena menor primeiro)
    expect(e.rankingAtraso.slice(0, 4)).toEqual([
      { dezena: 3, atraso: 3 },
      { dezena: 5, atraso: 3 },
      { dezena: 20, atraso: 2 },
      { dezena: 80, atraso: 2 },
    ]);
  });

  it('pares/ímpares', () => {
    // pares por sorteio: 2, 5, 2, 3
    expect(e.distribuicaoPares).toEqual([0, 0, 2, 1, 0, 1]);
    expect(e.mediaPares).toBe(12 / 4);
    expect(e.paresEsperado).toBe((5 * 40) / 80);
  });

  it('soma', () => {
    // 15, 116, 23, 60
    expect(e.soma).toEqual({ media: 214 / 4, minimo: 15, maximo: 116 });
    expect(e.somaEsperada).toBe((5 * 81) / 2);
  });

  it('repetidas do anterior', () => {
    // 2 após 1: {2,4} = 2; 3 após 2: {2,4} = 2; 4 após 3: 0
    expect(e.repetidas.comparacoes).toBe(3);
    expect(e.repetidas.distribuicao).toEqual([1, 0, 2, 0, 0, 0]);
    expect(e.repetidas.media).toBe(4 / 3);
    expect(e.repetidas.esperada).toBe(25 / 80);
  });
});

describe('janelas e bordas', () => {
  it('menos concursos que a janela usa todos; janela 10 sobre 12 usa os 10 últimos', () => {
    expect(calcularEstatisticas('quina', MAO, 10).tamanho).toBe(4);

    const lista: ConcursoHistorico[] = Array.from({ length: 12 }, (_, i) => [
      i + 1,
      '2020-01-01',
      [1, 2, 3, 4, 5 + (i % 2)],
    ]);
    const e = calcularEstatisticas('quina', lista, 10);
    expect(e.tamanho).toBe(10);
    expect(e.primeiroConcurso).toBe(3);
    expect(e.ultimoConcurso).toBe(12);
    expect(e.frequencia[0]).toBe(10);
    expect(e.atraso[4]).toBe(1); // dezena 5 saiu no concurso 11
    expect(e.atraso[5]).toBe(0); // dezena 6 saiu no 12
    expect(e.repetidas.comparacoes).toBe(9);
  });

  it('lista vazia e de um concurso não lançam e não inventam números', () => {
    const v = calcularEstatisticas('megasena', [], 'total');
    expect(v.tamanho).toBe(0);
    expect(v.mediaPares).toBeNull();
    expect(v.soma).toBeNull();
    expect(v.repetidas.media).toBeNull();
    expect(v.atraso.every((a) => a === null)).toBe(true);
    expect(v.rankingAtraso).toEqual([]);
    expect(v.primeiroConcurso).toBeNull();

    const um = calcularEstatisticas('megasena', [[1, '2020-01-01', [1, 2, 3, 4, 5, 6]]], 10);
    expect(um.tamanho).toBe(1);
    expect(um.repetidas.comparacoes).toBe(0);
    expect(um.repetidas.media).toBeNull();
    expect(um.mediaPares).toBe(3);
  });

  it('dezena fora do volante é ignorada sem lançar', () => {
    const e = calcularEstatisticas('quina', [[1, '2020-01-01', [1, 2, 3, 4, 999]]], 'total');
    expect(e.frequencia.reduce((a, b) => a + b, 0)).toBe(4);
  });

  it('calcularTodasAsJanelas devolve as quatro', () => {
    const t = calcularTodasAsJanelas('quina', MAO);
    expect(Object.keys(t)).toEqual(['10', '50', '100', 'total']);
  });
});

interface EsperadoJanela {
  tamanho: number;
  primeiro_concurso: number;
  ultimo_concurso: number;
  frequencia: number[];
  atraso: (number | null)[];
  distribuicao_pares: number[];
  soma_total: number;
  soma_min: number;
  soma_max: number;
  distribuicao_repetidas: number[];
}

const esperado = JSON.parse(lerFixtureTexto('estatisticas-esperado.json')) as {
  loterias: Record<Slug, Record<string, EsperadoJanela>>;
};

const somaPonderada = (dist: readonly number[]) => dist.reduce((s, n, i) => s + n * i, 0);

describe('contra o cálculo independente em SQL (bancos reais)', () => {
  for (const slug of SLUGS) {
    const { dezenasSorteadas: k, totalDezenas: total } = PARAMETROS_LOTERIA[slug];
    const concursos = validarHistorico(
      JSON.parse(lerFixtureTexto(`${slug}/historico.json`)),
      slug,
    ).concursos;

    for (const janela of JANELAS_ESTATISTICA) {
      it(`${slug}, janela ${janela}`, () => {
        const esp = esperado.loterias[slug][String(janela)]!;
        const e = calcularEstatisticas(slug, concursos, janela);
        expect(e.tamanho).toBe(esp.tamanho);
        expect(e.primeiroConcurso).toBe(esp.primeiro_concurso);
        expect(e.ultimoConcurso).toBe(esp.ultimo_concurso);
        expect(e.frequencia).toEqual(esp.frequencia);
        expect(e.atraso).toEqual(esp.atraso);
        expect(e.distribuicaoPares).toEqual(esp.distribuicao_pares);
        expect(e.mediaPares).toBe(somaPonderada(esp.distribuicao_pares) / esp.tamanho);
        expect(e.soma).toEqual({
          media: esp.soma_total / esp.tamanho,
          minimo: esp.soma_min,
          maximo: esp.soma_max,
        });
        expect(e.repetidas.distribuicao).toEqual(esp.distribuicao_repetidas);
        expect(e.repetidas.comparacoes).toBe(esp.tamanho - 1);
        expect(e.repetidas.media).toBe(
          somaPonderada(esp.distribuicao_repetidas) / (esp.tamanho - 1),
        );
        expect(e.frequenciaEsperada).toBe((esp.tamanho * k) / total);
        // coerência interna
        expect(e.frequencia.reduce((a, b) => a + b, 0)).toBe(esp.tamanho * k);
        expect(e.semAparicao.length + e.rankingAtraso.length).toBe(total);
      });
    }

    it(`${slug}: janela total cobre o histórico todo e é rápida`, () => {
      const t0 = performance.now();
      const e = calcularEstatisticas(slug, concursos, 'total');
      expect(e.tamanho).toBe(concursos.length);
      expect(performance.now() - t0).toBeLessThan(1000);
    });
  }
});

describe('atraso da janela total x espaco_final de tabela_geral.json', () => {
  for (const slug of SLUGS) {
    it(slug, () => {
      const concursos = validarHistorico(
        JSON.parse(lerFixtureTexto(`${slug}/historico.json`)),
        slug,
      ).concursos;
      const tabela = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`)) as {
        ultimo_sorteado: number;
        dezenas: { dezena: number; espaco_final: number | null }[];
      };
      expect(concursos[concursos.length - 1]![0]).toBe(tabela.ultimo_sorteado);
      const e = calcularEstatisticas(slug, concursos, 'total');
      expect(tabela.dezenas).toHaveLength(PARAMETROS_LOTERIA[slug].totalDezenas);
      for (const d of tabela.dezenas) {
        expect(e.atraso[d.dezena - 1], `dezena ${d.dezena}`).toBe(d.espaco_final);
      }
    });
  }
});
