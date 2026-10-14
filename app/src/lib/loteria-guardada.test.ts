import { describe, expect, it } from 'vitest';
import { CHAVE_LOTERIA_FORCA, gravarLoteriaGuardada, lerLoteriaGuardada } from './loteria-guardada';

function armazenamento(inicial: Record<string, string> = {}) {
  const dados = new Map(Object.entries(inicial));
  return {
    dados,
    getItem: (k: string) => dados.get(k) ?? null,
    setItem: (k: string, v: string) => void dados.set(k, v),
  };
}

describe('loteria guardada da Força', () => {
  it('grava e lê as três loterias', () => {
    for (const slug of ['lotofacil', 'megasena', 'quina'] as const) {
      const arm = armazenamento();
      expect(gravarLoteriaGuardada(slug, arm)).toBe(true);
      expect(arm.dados.get(CHAVE_LOTERIA_FORCA)).toBe(slug);
      expect(lerLoteriaGuardada(arm)).toBe(slug);
    }
  });

  it('ausente ou valor estranho vira null', () => {
    expect(lerLoteriaGuardada(armazenamento())).toBeNull();
    expect(lerLoteriaGuardada(armazenamento({ [CHAVE_LOTERIA_FORCA]: 'loto' }))).toBeNull();
    expect(lerLoteriaGuardada(null)).toBeNull();
  });

  it('armazenamento que lança: ler dá null e gravar dá false, sem lançar', () => {
    const quebrado = {
      getItem: () => {
        throw new Error('bloqueado');
      },
      setItem: () => {
        throw new Error('cheio');
      },
    };
    expect(lerLoteriaGuardada(quebrado)).toBeNull();
    expect(gravarLoteriaGuardada('quina', quebrado)).toBe(false);
    expect(gravarLoteriaGuardada('quina', null)).toBe(false);
  });
});
