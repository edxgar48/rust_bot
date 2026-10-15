// Resumo do acaso (7.3) sem Svelte: janela efetiva, textos e blocos de comparação.
import { describe, expect, it } from 'vitest';
import type { ConcursoHistorico } from '../dados/tipos';
import { calcularEstatisticas } from '../estatisticas/estatisticas';
import { textos } from '../lib/textos';
import { MAXIMO_FORA_CITADAS, montarBlocoComparacao, montarResumoAcaso } from './estatisticas';

// Quina (80 dezenas, k = 5), 4 concursos à mão.
const MAO: readonly ConcursoHistorico[] = [
  [1, '2020-01-01', [1, 2, 3, 4, 5]],
  [2, '2020-01-02', [2, 4, 10, 20, 80]],
  [3, '2020-01-03', [1, 2, 4, 7, 9]],
  [4, '2020-01-04', [10, 11, 12, 13, 14]],
];

describe('montarResumoAcaso', () => {
  it('janela maior que o histórico usa o tamanho EFETIVO (4), não 10', () => {
    const e = calcularEstatisticas('quina', MAO, 10);
    expect(e.tamanho).toBe(4);
    const r = montarResumoAcaso(e)!;
    expect(r.faixa.tamanho).toBe(4);
    expect(r.faixa.esperado).toBeCloseTo(0.25, 12);
    // binomial(4, 1/16): P(0) = (15/16)^4 = 77,2%; P(<=1) = 97,85% >= 97,5%.
    expect([r.faixa.inferior, r.faixa.superior]).toEqual([0, 1]);
    expect(r.aviso).toBe(textos.estatisticas.frequencia.faixaLarga(4));
  });

  it('janela vazia não tem resumo', () => {
    expect(montarResumoAcaso(calcularEstatisticas('quina', [], 'total'))).toBeNull();
  });

  it('à mão (Quina, 4 concursos): acima da faixa [0, 1] ficam 1 e 10 (2 vezes) e 2 e 4 (3 vezes)', () => {
    const r = montarResumoAcaso(calcularEstatisticas('quina', MAO, 'total'))!;
    expect(r.fora.acima).toEqual([1, 2, 4, 10]);
    expect(r.fora.abaixo).toEqual([]);
    expect(r.abaixo).toBeNull();
    expect(r.acima).toBe('Acima da faixa: 01, 02, 04, 10.');
    expect(r.texto).toContain('4 dezenas ficaram fora da faixa');
    expect(r.texto).toContain('o esperado é cerca de');
  });

  it('o resumo cita no máximo 10 dezenas por lista e conta o resto', () => {
    const e = calcularEstatisticas('quina', MAO, 'total');
    const r = montarResumoAcaso({
      ...e,
      frequencia: e.frequencia.map(() => 0),
      tamanho: 1000,
    })!;
    // com 1000 concursos, as 80 dezenas com 0 vezes ficam todas abaixo da faixa
    expect(r.fora.abaixo).toHaveLength(80);
    expect(r.abaixo).toContain(`e mais ${80 - MAXIMO_FORA_CITADAS}`);
    expect(r.abaixo!.split(',').length).toBe(MAXIMO_FORA_CITADAS);
  });

  it('frases do resumo: zero, uma e várias dezenas', () => {
    const f = textos.estatisticas.frequencia;
    expect(f.resumoAcaso(0, '1,2')).toMatch(
      /^Nenhuma dezena ficou fora da faixa\. Ao acaso, o esperado é cerca de 1,2 fora;/,
    );
    expect(f.resumoAcaso(1, '1,2')).toMatch(/^1 dezena ficou fora/);
    expect(f.resumoAcaso(3, '1,2')).toMatch(/^3 dezenas ficaram fora/);
  });

  it('não julga a contagem: nada de "normal", mesmo com muito mais fora que o esperado', () => {
    const f = textos.estatisticas.frequencia;
    for (const fora of [0, 1, 4, 40]) {
      const texto = f.resumoAcaso(fora, '0,5');
      expect(texto).not.toMatch(/normal|comum|esperad[oa]s? que/i);
      expect(texto).toContain('varia de janela para janela');
      expect(texto).toContain('muitas dezenas são comparadas de uma vez');
      expect(texto).toContain('não ajuda a prever o sorteio');
    }
  });
});

describe('montarBlocoComparacao', () => {
  it('sem sorteios para comparar, não há bloco (repetidas com 1 concurso)', () => {
    const e = calcularEstatisticas('quina', MAO.slice(0, 1), 'total');
    expect(montarBlocoComparacao('pares', e)!.comparacao.sorteios).toBe(1);
    expect(montarBlocoComparacao('repetidas', e)).toBeNull();
  });

  it('repetidas usa as comparações (tamanho − 1) como número de sorteios', () => {
    const e = calcularEstatisticas('quina', MAO, 'total');
    const b = montarBlocoComparacao('repetidas', e)!;
    expect(b.comparacao.sorteios).toBe(3);
    expect(b.nome).toBe('repetidas');
    expect(b.rotulo).toContain('3 sorteios');
  });
});
