import { describe, expect, it } from 'vitest';
import { destinoDoTab } from './folha';

describe('destinoDoTab (foco preso na folha)', () => {
  const f = ['fechar', 'a', 'b'];

  it('Tab no último volta ao primeiro; no meio deixa o navegador seguir', () => {
    expect(destinoDoTab(f, 'b', false)).toBe('fechar');
    expect(destinoDoTab(f, 'a', false)).toBeNull();
    expect(destinoDoTab(f, 'fechar', false)).toBeNull();
  });

  it('Shift+Tab no primeiro vai ao último; no meio segue', () => {
    expect(destinoDoTab(f, 'fechar', true)).toBe('b');
    expect(destinoDoTab(f, 'a', true)).toBeNull();
  });

  it('foco fora da folha (ou nenhum) entra pela ponta certa', () => {
    expect(destinoDoTab(f, 'fora', false)).toBe('fechar');
    expect(destinoDoTab(f, null, false)).toBe('fechar');
    expect(destinoDoTab(f, 'fora', true)).toBe('b');
  });

  it('sem focáveis não faz nada; com um só, o foco fica nele', () => {
    expect(destinoDoTab([], null, false)).toBeNull();
    expect(destinoDoTab(['x'], 'x', false)).toBe('x');
    expect(destinoDoTab(['x'], 'x', true)).toBe('x');
  });
});
