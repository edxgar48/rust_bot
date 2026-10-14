import { describe, expect, it } from 'vitest';
import { DIRECOES_EVIDENCIA, GRAUS_EVIDENCIA, NORMALIZACOES } from '../dados/tipos';
import {
  idsAbas,
  rotuloEvidenciaDirecao,
  rotuloEvidenciaGrau,
  rotuloNormalizacao,
  rotulosEvidenciaDirecao,
  rotulosEvidenciaGrau,
  rotulosNormalizacao,
  textos,
} from './textos';

describe('textos', () => {
  it('define as 5 abas na ordem do plano', () => {
    expect(idsAbas.map((id) => textos.abas[id])).toEqual([
      'Início',
      'Força',
      'Conferir',
      'Estatísticas',
      'Entenda',
    ]);
  });
});

// Falha se o contrato ganhar um valor sem rótulo, ou se sobrar rótulo de valor inexistente.
describe('rótulos dos identificadores do contrato', () => {
  const casos = [
    ['evidencia_grau', GRAUS_EVIDENCIA, rotulosEvidenciaGrau, rotuloEvidenciaGrau],
    ['evidencia_direcao', DIRECOES_EVIDENCIA, rotulosEvidenciaDirecao, rotuloEvidenciaDirecao],
    ['normalizacao', NORMALIZACOES, rotulosNormalizacao, rotuloNormalizacao],
  ] as const;

  it.each(casos)('%s: todos os valores têm rótulo, sem sobras', (_nome, valores, tabela) => {
    expect(Object.keys(tabela).sort()).toEqual([...valores].sort());
  });

  it.each(casos)('%s: rótulos não vazios e distintos entre si', (_nome, valores, tabela) => {
    const rotulos = valores.map((v) => (tabela as Record<string, string>)[v]);
    for (const r of rotulos) expect(r?.trim().length).toBeGreaterThan(0);
    expect(new Set(rotulos).size).toBe(valores.length);
  });

  it('fixa a tabela de evidencia_grau, do mais fraco ao mais forte', () => {
    expect(GRAUS_EVIDENCIA.map(rotuloEvidenciaGrau)).toEqual([
      'Muito fraca',
      'Fraca',
      'Moderada',
      'Forte',
      'Muito forte',
    ]);
    expect(rotuloEvidenciaDirecao('vies')).toBe('viés');
    expect(rotuloEvidenciaDirecao('acaso')).toBe('acaso');
    expect(rotuloNormalizacao('zscore')).toContain('z-score');
    expect(rotuloNormalizacao('minmax')).toContain('0 a 1');
  });
});
