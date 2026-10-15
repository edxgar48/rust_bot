// Conferência automática (6.4): funções puras, com valores calculados à mão.
import { describe, expect, it } from 'vitest';
import type { ConcursoHistorico, Slug, UltimoV1 } from '../dados/tipos';
import { acertosPorConcurso } from '../telas/conferir';
import {
  FAIXA_PREMIO,
  LIMITE_RESULTADOS_LISTADOS,
  concursosPosteriores,
  conferirConcursosNovos,
  novidadesDaLoteria,
} from './conferencia-automatica';
import { criarJogo, type JogoSalvo } from './jogo';

const c = (n: number, dezenas: number[]): ConcursoHistorico => [n, '2026-10-01', dezenas];

function ultimoDe(slug: Slug, n: number, dezenas: number[]): UltimoV1 {
  return { schemaVersion: 1, loteria: slug, concurso: n, data: '2026-10-13', dezenas };
}

function jogo(
  id: string,
  loteria: Slug,
  dezenas: number[],
  conferidoAte: number | null,
): JogoSalvo {
  const r = criarJogo({ loteria, dezenas, conferidoAte }, id, new Date('2026-10-01T12:00:00Z'));
  if (!r.ok) throw new Error('fixture inválida');
  return r.valor;
}

describe('faixa de prêmio por loteria', () => {
  it('Lotofácil 11, Mega-Sena 4, Quina 2', () => {
    expect(FAIXA_PREMIO).toEqual({ lotofacil: 11, megasena: 4, quina: 2 });
  });
});

describe('conferirConcursosNovos (acertos à mão)', () => {
  it('Quina: 2, 1 e 0 acertos; melhor 2 no concurso 100; atinge a faixa (>= 2)', () => {
    const jogoQuina = [1, 2, 3, 4, 5];
    const r = conferirConcursosNovos('quina', jogoQuina, [
      c(100, [1, 2, 9, 10, 11]), // 1 e 2 -> 2 acertos
      c(101, [1, 20, 21, 22, 23]), // só o 1 -> 1 acerto
      c(102, [40, 41, 42, 43, 44]), // nenhum -> 0
    ]);
    // Do mais recente ao mais antigo.
    expect(r.resultados).toEqual([
      { concurso: 102, acertos: 0 },
      { concurso: 101, acertos: 1 },
      { concurso: 100, acertos: 2 },
    ]);
    expect(r.melhor).toEqual({ concurso: 100, acertos: 2 });
    expect(r.naFaixaPremio).toBe(true);
  });

  it('Quina: 1 acerto no melhor fica fora da faixa', () => {
    const r = conferirConcursosNovos('quina', [1, 2, 3, 4, 5], [c(100, [1, 20, 21, 22, 23])]);
    expect(r.melhor).toEqual({ concurso: 100, acertos: 1 });
    expect(r.naFaixaPremio).toBe(false);
  });

  it('empate no melhor: vale o concurso mais recente', () => {
    const r = conferirConcursosNovos(
      'quina',
      [1, 2, 3, 4, 5],
      [c(100, [1, 2, 9, 10, 11]), c(101, [3, 4, 12, 13, 14])],
    );
    expect(r.melhor).toEqual({ concurso: 101, acertos: 2 });
  });

  it('Lotofácil: 11 acertos entram na faixa, 10 não', () => {
    const jogo15 = Array.from({ length: 15 }, (_, i) => i + 1); // 1..15
    const onze = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 20, 21, 22, 23]; // 11 em comum
    const dez = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 21, 22, 23, 24, 25]; // 10 em comum
    expect(conferirConcursosNovos('lotofacil', jogo15, [c(1, onze)]).naFaixaPremio).toBe(true);
    expect(conferirConcursosNovos('lotofacil', jogo15, [c(1, dez)]).naFaixaPremio).toBe(false);
  });

  it('Mega-Sena: 4 acertos (quadra) entram na faixa, 3 não', () => {
    const jogo6 = [1, 2, 3, 4, 5, 6];
    const quatro = conferirConcursosNovos('megasena', jogo6, [c(1, [1, 2, 3, 4, 30, 31])]);
    const tres = conferirConcursosNovos('megasena', jogo6, [c(1, [1, 2, 3, 30, 31, 32])]);
    expect(quatro.melhor?.acertos).toBe(4);
    expect(quatro.naFaixaPremio).toBe(true);
    expect(tres.melhor?.acertos).toBe(3);
    expect(tres.naFaixaPremio).toBe(false);
  });

  it('jogo maior que a quantidade sorteada: conta a interseção (Mega 7 dezenas)', () => {
    const r = conferirConcursosNovos('megasena', [1, 2, 3, 4, 5, 6, 7], [c(1, [1, 2, 3, 4, 5, 6])]);
    expect(r.melhor?.acertos).toBe(6);
  });

  it('sem concursos: sem melhor e fora da faixa', () => {
    const r = conferirConcursosNovos('quina', [1, 2, 3, 4, 5], []);
    expect(r).toEqual({ resultados: [], totalConcursos: 0, melhor: null, naFaixaPremio: false });
  });

  it('guarda só os mais recentes, mas conta todos e acha o melhor (e a faixa) nos antigos', () => {
    const jogoQuina = [1, 2, 3, 4, 5];
    const concursos = [
      c(100, [1, 2, 9, 10, 11]), // 2 acertos: o melhor, fora dos listados
      ...[101, 102, 103, 104, 105, 106, 107].map((n) => c(n, [40, 41, 42, 43, 44])),
    ];
    const r = conferirConcursosNovos('quina', jogoQuina, concursos);
    expect(r.totalConcursos).toBe(8);
    expect(r.resultados.map((x) => x.concurso)).toEqual([107, 106, 105, 104, 103]);
    expect(r.resultados).toHaveLength(LIMITE_RESULTADOS_LISTADOS);
    expect(r.melhor).toEqual({ concurso: 100, acertos: 2 });
    expect(r.naFaixaPremio).toBe(true);
  });
});

describe('volume: backup grande com marca antiga', () => {
  // Gerador determinístico (LCG) de concursos e jogos da Quina.
  let semente = 12345;
  const proximo = (limite: number): number => {
    semente = (semente * 1103515245 + 12345) % 2147483648;
    return semente % limite;
  };
  const dezenasAleatorias = (qtd: number): number[] => {
    const s = new Set<number>();
    while (s.size < qtd) s.add(1 + proximo(80));
    return [...s].sort((a, b) => a - b);
  };

  it('1.000 jogos x 3.500 concursos: rápido, sem lista gigante e igual ao cálculo ingênuo', () => {
    const historico: ConcursoHistorico[] = Array.from({ length: 3500 }, (_, i) => [
      i + 1,
      '2026-01-01',
      dezenasAleatorias(5),
    ]);
    const ultimo = ultimoDe('quina', 3500, [...historico[3499]![2]]);
    const jogos: JogoSalvo[] = Array.from({ length: 1000 }, (_, i) =>
      jogo(
        `j${i}`,
        'quina',
        dezenasAleatorias(5 + (i % 11)),
        i % 7 === 0 ? 1 : 1 + ((i * 3) % 3400),
      ),
    );

    const inicio = performance.now();
    const r = novidadesDaLoteria('quina', jogos, ultimo, historico);
    const ms = performance.now() - inicio;

    expect(r).not.toBeNull();
    expect(ms).toBeLessThan(3000);
    expect(r?.jogos).toHaveLength(1000);
    for (const n of r?.jogos ?? []) {
      expect(n.resultados.length).toBeLessThanOrEqual(LIMITE_RESULTADOS_LISTADOS);
    }

    // Amostra contra o cálculo ingênuo (todos os acertos, depois ordenar e reduzir).
    for (let i = 0; i < 1000; i += 53) {
      const j = jogos[i]!;
      const todos = acertosPorConcurso(
        'quina',
        j.dezenas,
        historico.filter((x) => x[0] > (j.conferidoAte as number)),
      ).reverse();
      let melhor = todos[0]!;
      for (const x of todos) if (x.acertos > melhor.acertos) melhor = x;
      const n = (r?.jogos ?? []).find((x) => x.jogoId === j.id)!;
      expect(n.totalConcursos).toBe(todos.length);
      expect(n.resultados).toEqual(todos.slice(0, LIMITE_RESULTADOS_LISTADOS));
      expect(n.melhor).toEqual(melhor);
      expect(n.naFaixaPremio).toBe(melhor.acertos >= FAIXA_PREMIO.quina);
    }
    const maior = Math.max(...(r?.jogos ?? []).map((n) => n.totalConcursos));
    expect(r?.concursosNovos).toBe(maior);
  });
});

describe('concursosPosteriores', () => {
  const hist = [c(99, [1, 2, 3, 4, 5]), c(100, [6, 7, 8, 9, 10]), c(101, [11, 12, 13, 14, 15])];

  it('em dia: nada novo', () => {
    const r = concursosPosteriores(101, ultimoDe('quina', 101, [11, 12, 13, 14, 15]), hist);
    expect(r).toEqual({ concursos: [], completo: true, ultimoConcurso: 101 });
  });

  it('marca à frente do que se conhece (cache antigo): nada novo, sem erro', () => {
    expect(
      concursosPosteriores(150, ultimoDe('quina', 101, [1, 2, 3, 4, 5]), hist).concursos,
    ).toEqual([]);
  });

  it('o ultimo.json cobre um único concurso novo sem precisar do histórico', () => {
    const u = ultimoDe('quina', 102, [20, 21, 22, 23, 24]);
    const r = concursosPosteriores(101, u, null);
    expect(r.completo).toBe(true);
    expect(r.concursos).toEqual([[102, '2026-10-13', [20, 21, 22, 23, 24]]]);
  });

  it('histórico atrasado e salto de um concurso: junta histórico e ultimo.json', () => {
    const u = ultimoDe('quina', 102, [20, 21, 22, 23, 24]);
    const r = concursosPosteriores(100, u, hist);
    expect(r.completo).toBe(true);
    expect(r.concursos.map((x) => x[0])).toEqual([101, 102]);
  });

  it('histórico atrasado e salto maior: incompleto (falta o 102)', () => {
    const u = ultimoDe('quina', 103, [20, 21, 22, 23, 24]);
    const r = concursosPosteriores(100, u, hist);
    expect(r.completo).toBe(false);
    expect(r.concursos.map((x) => x[0])).toEqual([101, 103]);
    expect(r.ultimoConcurso).toBe(103);
  });

  it('sem histórico e salto maior: incompleto e sem concursos', () => {
    const r = concursosPosteriores(100, ultimoDe('quina', 105, [1, 2, 3, 4, 5]), null);
    expect(r.completo).toBe(false);
    expect(r.ultimoConcurso).toBe(105);
  });

  it('histórico que já tem o último: sem duplicar o do ultimo.json', () => {
    const r = concursosPosteriores(99, ultimoDe('quina', 101, [11, 12, 13, 14, 15]), hist);
    expect(r.completo).toBe(true);
    expect(r.concursos.map((x) => x[0])).toEqual([100, 101]);
  });

  it('sem nenhum dado: nada a conferir', () => {
    expect(concursosPosteriores(100, null, null)).toEqual({
      concursos: [],
      completo: true,
      ultimoConcurso: null,
    });
  });
});

describe('novidadesDaLoteria', () => {
  const hist = [c(100, [6, 7, 8, 9, 10]), c(101, [1, 2, 9, 10, 11]), c(102, [1, 2, 3, 40, 41])];
  const u = ultimoDe('quina', 102, [1, 2, 3, 40, 41]);

  it('nada se não há jogos, se o jogo não tem marca ou se está em dia', () => {
    expect(novidadesDaLoteria('quina', [], u, hist)).toBeNull();
    expect(
      novidadesDaLoteria('quina', [jogo('a', 'quina', [1, 2, 3, 4, 5], null)], u, hist),
    ).toBeNull();
    expect(
      novidadesDaLoteria('quina', [jogo('a', 'quina', [1, 2, 3, 4, 5], 102)], u, hist),
    ).toBeNull();
  });

  it('ignora jogos de outra loteria', () => {
    const mega = jogo('m', 'megasena', [1, 2, 3, 4, 5, 6], 1);
    expect(novidadesDaLoteria('quina', [mega], u, hist)).toBeNull();
  });

  it('resume: jogos conferidos, melhor, faixa e concursos novos (valores à mão)', () => {
    const a = jogo('a', 'quina', [1, 2, 3, 4, 5], 100); // 101: 1,2 = 2; 102: 1,2,3 = 3
    const b = jogo('b', 'quina', [60, 61, 62, 63, 64], 101); // 102: 0
    const r = novidadesDaLoteria('quina', [a, b], u, hist);
    expect(r).not.toBeNull();
    expect(r?.ultimoConcurso).toBe(102);
    expect(r?.conferidos).toBe(2);
    expect(r?.pendentes).toBe(0);
    expect(r?.melhorAcertos).toBe(3);
    expect(r?.naFaixaPremio).toBe(1);
    expect(r?.concursosNovos).toBe(2);
    const ja = r?.jogos.find((j) => j.jogoId === 'a');
    expect(ja?.resultados).toEqual([
      { concurso: 102, acertos: 3 },
      { concurso: 101, acertos: 2 },
    ]);
    expect(ja?.melhor).toEqual({ concurso: 102, acertos: 3 });
    const jb = r?.jogos.find((j) => j.jogoId === 'b');
    expect(jb?.melhor).toEqual({ concurso: 102, acertos: 0 });
    expect(jb?.naFaixaPremio).toBe(false);
  });

  it('histórico em falta com salto grande: jogo fica pendente, nunca "conferido com zero"', () => {
    const a = jogo('a', 'quina', [1, 2, 3, 4, 5], 90);
    const r = novidadesDaLoteria('quina', [a], u, null);
    expect(r?.pendentes).toBe(1);
    expect(r?.conferidos).toBe(0);
    expect(r?.melhorAcertos).toBeNull();
    expect(r?.jogos[0]?.estado).toBe('pendente');
    expect(r?.jogos[0]?.resultados).toEqual([]);
  });

  it('só o ultimo.json basta quando falta um concurso: jogo conferido', () => {
    const a = jogo('a', 'quina', [1, 2, 3, 4, 5], 101);
    const r = novidadesDaLoteria('quina', [a], u, null);
    expect(r?.conferidos).toBe(1);
    expect(r?.melhorAcertos).toBe(3);
  });
});
