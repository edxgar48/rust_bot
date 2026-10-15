// Sessão: o cálculo da conferência automática de uma chamada antiga, que termina depois
// da nova, não pode sobrescrever o resultado novo (corrida).
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { NovidadeLoteria } from './jogos/conferencia-automatica';

const pendentes: Array<(v: NovidadeLoteria[]) => void> = [];
const falhas: Array<(e: unknown) => void> = [];

vi.mock('./jogos/novidades', () => ({
  calcularNovidades: vi.fn(
    () =>
      new Promise<NovidadeLoteria[]>((ok, ruim) => {
        pendentes.push(ok);
        falhas.push(ruim);
      }),
  ),
}));

import { criarSessao } from './sessao.svelte';

const novidade = (ultimoConcurso: number): NovidadeLoteria => ({
  slug: 'quina',
  ultimoConcurso,
  jogos: [],
  conferidos: 0,
  pendentes: 0,
  melhorAcertos: null,
  naFaixaPremio: 0,
  concursosNovos: 0,
});

afterEach(() => {
  pendentes.length = 0;
  falhas.length = 0;
});

describe('sessão: recalcularNovidades', () => {
  it('chamada antiga que termina depois da nova é descartada', async () => {
    const sessao = criarSessao();
    const primeira = sessao.recalcularNovidades();
    const segunda = sessao.recalcularNovidades();
    expect(pendentes).toHaveLength(2);

    pendentes[1]!([novidade(2)]);
    await segunda;
    expect(sessao.novidades.map((n) => n.ultimoConcurso)).toEqual([2]);

    pendentes[0]!([novidade(1)]);
    await primeira;
    expect(sessao.novidades.map((n) => n.ultimoConcurso)).toEqual([2]);
  });

  it('em ordem normal, cada resultado vale; falha mantém o que já estava', async () => {
    const sessao = criarSessao();
    const a = sessao.recalcularNovidades();
    pendentes[0]!([novidade(5)]);
    await a;
    expect(sessao.novidades.map((n) => n.ultimoConcurso)).toEqual([5]);

    const b = sessao.recalcularNovidades();
    falhas[1]!(new Error('falhou'));
    await b;
    expect(sessao.novidades.map((n) => n.ultimoConcurso)).toEqual([5]);
  });
});
