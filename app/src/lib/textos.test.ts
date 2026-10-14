import { describe, expect, it } from 'vitest';
import { idsAbas, textos } from './textos';

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
