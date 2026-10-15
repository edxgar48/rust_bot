// Lógica pura da tela Conferir jogo: quantidades, conferência (valores à mão e
// comparada com um cálculo independente sobre as fixtures reais) e visão.
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import type { ResultadoCache } from '../dados/cliente';
import type {
  ConcursoHistorico,
  HistoricoV1,
  IndiceV1,
  LoteriaNoIndice,
  Slug,
} from '../dados/tipos';
import { PARAMETROS_LOTERIA, SLUGS } from '../dados/tipos';
import { validarHistorico } from '../dados/validacao';
import { textos } from '../lib/textos';
import {
  QUANTIDADE_JOGO,
  acertosNoUltimoConcurso,
  alternarDezenaDoJogo,
  avaliarQuantidade,
  concursosComAcertos,
  conferirJogo,
  faixasDoResultado,
  montarVisaoConferir,
} from './conferir';
import type { SituacaoTela } from './estado-tela';

// Quina (80 dezenas, 5 sorteadas), quatro concursos escritos à mão.
const QUINA_MAO: readonly ConcursoHistorico[] = [
  [1, '2020-01-01', [1, 2, 3, 4, 5]],
  [2, '2020-01-02', [1, 2, 3, 10, 11]],
  [3, '2020-01-03', [10, 11, 12, 13, 14]],
  [4, '2020-01-04', [1, 2, 3, 4, 20]],
];

describe('quantidades válidas por loteria', () => {
  it('Lotofácil 15-20, Mega-Sena 6-20, Quina 5-15', () => {
    expect(QUANTIDADE_JOGO).toEqual({
      lotofacil: { minimo: 15, maximo: 20 },
      megasena: { minimo: 6, maximo: 20 },
      quina: { minimo: 5, maximo: 15 },
    });
  });

  it('cada faixa cabe no volante e o mínimo é o que se sorteia', () => {
    for (const slug of SLUGS) {
      const { minimo, maximo } = QUANTIDADE_JOGO[slug];
      expect(minimo).toBe(PARAMETROS_LOTERIA[slug].dezenasSorteadas);
      expect(maximo).toBeLessThanOrEqual(PARAMETROS_LOTERIA[slug].totalDezenas);
      expect(maximo).toBeGreaterThanOrEqual(minimo);
    }
  });

  it('avaliarQuantidade: vazia, faltam, válida nas pontas, excede', () => {
    expect(avaliarQuantidade('quina', 0)).toEqual({ tipo: 'vazia' });
    expect(avaliarQuantidade('quina', 4)).toEqual({ tipo: 'faltam', faltam: 1 });
    expect(avaliarQuantidade('quina', 5)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('quina', 15)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('quina', 16)).toEqual({ tipo: 'excede', sobram: 1 });
    expect(avaliarQuantidade('lotofacil', 14)).toEqual({ tipo: 'faltam', faltam: 1 });
    expect(avaliarQuantidade('lotofacil', 15)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('lotofacil', 20)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('lotofacil', 21)).toEqual({ tipo: 'excede', sobram: 1 });
    expect(avaliarQuantidade('megasena', 5)).toEqual({ tipo: 'faltam', faltam: 1 });
    expect(avaliarQuantidade('megasena', 6)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('megasena', 20)).toEqual({ tipo: 'valida' });
    expect(avaliarQuantidade('megasena', 21)).toEqual({ tipo: 'excede', sobram: 1 });
  });
});

describe('alternarDezenaDoJogo', () => {
  it('adiciona em ordem crescente, remove ao repetir e não passa do máximo', () => {
    let jogo: number[] = [];
    for (const d of [30, 5, 12]) jogo = alternarDezenaDoJogo('quina', jogo, d);
    expect(jogo).toEqual([5, 12, 30]);
    expect(alternarDezenaDoJogo('quina', jogo, 12)).toEqual([5, 30]);
    const cheio = Array.from({ length: 15 }, (_, i) => i + 1);
    expect(alternarDezenaDoJogo('quina', cheio, 40)).toEqual(cheio);
    expect(alternarDezenaDoJogo('quina', cheio, 15)).toHaveLength(14);
  });

  it('não altera a lista recebida', () => {
    const jogo = [3, 9];
    alternarDezenaDoJogo('quina', jogo, 1);
    expect(jogo).toEqual([3, 9]);
  });
});

describe('conferirJogo: valores à mão', () => {
  it('Quina: contagem por acertos, melhor resultado e empate com o mais recente primeiro', () => {
    // Jogo 1,2,3,4,6: c1 -> 1,2,3,4 = 4; c2 -> 1,2,3 = 3; c3 -> 0; c4 -> 1,2,3,4 = 4.
    const r = conferirJogo('quina', [1, 2, 3, 4, 6], QUINA_MAO);
    expect(r.totalConcursos).toBe(4);
    expect(r.porAcertos).toEqual([1, 0, 0, 1, 2, 0]);
    expect(r.melhor).toEqual({ acertos: 4, concursos: [4, 1] });
  });

  it('jogo grande (Quina com 8 dezenas) pode acertar as 5', () => {
    // 1,2,3,4,5,10,11,12: c1 = 5; c2 = 1,2,3,10,11 = 5; c3 = 10,11,12 = 3; c4 = 1,2,3,4 = 4.
    const r = conferirJogo('quina', [1, 2, 3, 4, 5, 10, 11, 12], QUINA_MAO);
    expect(r.porAcertos).toEqual([0, 0, 0, 1, 1, 2]);
    expect(r.melhor).toEqual({ acertos: 5, concursos: [2, 1] });
  });

  it('o melhor resultado é único quando não há empate', () => {
    const r = conferirJogo('quina', [10, 11, 12, 13, 14], QUINA_MAO);
    expect(r.melhor).toEqual({ acertos: 5, concursos: [3] });
    expect(r.porAcertos).toEqual([2, 0, 1, 0, 0, 1]);
  });

  it('dezenas repetidas ou fora do volante não contam', () => {
    const r = conferirJogo('quina', [1, 1, 2, 3, 4, 99, 0, -3, 2.5], QUINA_MAO);
    // Distintas válidas: 1,2,3,4 -> faixas de 0 a 4.
    expect(r.porAcertos).toEqual([1, 0, 0, 1, 2]);
    expect(r.melhor).toEqual({ acertos: 4, concursos: [4, 1] });
  });

  it('Mega-Sena com 20 dezenas: seis acertos em dois concursos', () => {
    const jogo = Array.from({ length: 20 }, (_, i) => i + 1);
    const concursos: ConcursoHistorico[] = [
      [1, '2020-01-01', [1, 2, 3, 4, 5, 6]],
      [2, '2020-01-04', [15, 16, 17, 50, 51, 52]],
      [3, '2020-01-08', [14, 15, 16, 17, 18, 19]],
      [4, '2020-01-11', [21, 22, 23, 24, 25, 26]],
    ];
    const r = conferirJogo('megasena', jogo, concursos);
    expect(r.porAcertos).toEqual([1, 0, 0, 1, 0, 0, 2]);
    expect(r.melhor).toEqual({ acertos: 6, concursos: [3, 1] });
  });

  it('Lotofácil: jogo de 15 contra um concurso idêntico = 15 acertos', () => {
    const quinze = Array.from({ length: 15 }, (_, i) => i + 1);
    const r = conferirJogo('lotofacil', quinze, [[1, '2020-01-01', quinze]]);
    expect(r.porAcertos).toHaveLength(16);
    expect(r.porAcertos[15]).toBe(1);
    expect(r.melhor).toEqual({ acertos: 15, concursos: [1] });
  });

  it('sem concursos: tudo zero e sem melhor resultado', () => {
    const r = conferirJogo('quina', [1, 2, 3, 4, 5], []);
    expect(r).toEqual({
      totalConcursos: 0,
      porAcertos: [0, 0, 0, 0, 0, 0],
      minimoAcertos: 0,
      melhor: null,
    });
  });

  it('zero acerto também é um resultado (nenhuma dezena em comum)', () => {
    const r = conferirJogo('quina', [60, 61, 62, 63, 64], QUINA_MAO);
    expect(r.melhor).toEqual({ acertos: 0, concursos: [4, 3, 2, 1] });
  });

  it('não altera o histórico recebido', () => {
    const copia = JSON.stringify(QUINA_MAO);
    conferirJogo('quina', [1, 2, 3, 4, 5], QUINA_MAO);
    expect(JSON.stringify(QUINA_MAO)).toBe(copia);
  });
});

describe('concursosComAcertos e faixasDoResultado', () => {
  it('lista os concursos da faixa, do mais recente ao mais antigo, com limite', () => {
    const jogo = [1, 2, 3, 4, 6];
    expect(concursosComAcertos('quina', jogo, QUINA_MAO, 4)).toEqual([4, 1]);
    expect(concursosComAcertos('quina', jogo, QUINA_MAO, 4, 1)).toEqual([4]);
    expect(concursosComAcertos('quina', jogo, QUINA_MAO, 3)).toEqual([2]);
    expect(concursosComAcertos('quina', jogo, QUINA_MAO, 5)).toEqual([]);
    expect(concursosComAcertos('quina', jogo, QUINA_MAO, 0)).toEqual([3]);
  });

  it('as faixas vão do maior número de acertos ao menor, com a proporção da faixa mais cheia', () => {
    const r = conferirJogo('quina', [1, 2, 3, 4, 6], QUINA_MAO);
    expect(faixasDoResultado(r)).toEqual([
      { acertos: 5, concursos: 0, proporcao: 0 },
      { acertos: 4, concursos: 2, proporcao: 1 },
      { acertos: 3, concursos: 1, proporcao: 0.5 },
      { acertos: 2, concursos: 0, proporcao: 0 },
      { acertos: 1, concursos: 0, proporcao: 0 },
      { acertos: 0, concursos: 1, proporcao: 0.5 },
    ]);
  });

  it('faixas impossíveis ficam de fora: Lotofácil com 15 dezenas acerta ao menos 5', () => {
    const quinze = Array.from({ length: 15 }, (_, i) => i + 1);
    const concursos: ConcursoHistorico[] = [
      [1, '2020-01-01', quinze],
      [2, '2020-01-02', [11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25]],
    ];
    const r = conferirJogo('lotofacil', quinze, concursos);
    expect(r.minimoAcertos).toBe(5);
    const faixas = faixasDoResultado(r);
    expect(faixas.map((f) => f.acertos)).toEqual([15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5]);
    expect(faixas.at(-1)).toEqual({ acertos: 5, concursos: 1, proporcao: 1 });
    // 20 dezenas: 20 + 15 - 25 = 10; Mega-Sena com 20 dezenas e Quina não têm piso.
    const vinte = Array.from({ length: 20 }, (_, i) => i + 1);
    expect(conferirJogo('lotofacil', vinte, concursos).minimoAcertos).toBe(10);
    expect(conferirJogo('megasena', vinte, []).minimoAcertos).toBe(0);
    expect(conferirJogo('quina', [1, 2, 3, 4, 5], []).minimoAcertos).toBe(0);
  });

  it('acertos no último concurso (o mais recente do arquivo)', () => {
    expect(acertosNoUltimoConcurso('quina', [1, 2, 3, 4, 6], QUINA_MAO)).toEqual({
      concurso: 4,
      acertos: 4,
    });
    expect(acertosNoUltimoConcurso('quina', [60, 61, 62, 63, 64], QUINA_MAO)).toEqual({
      concurso: 4,
      acertos: 0,
    });
    expect(acertosNoUltimoConcurso('quina', [1, 2, 3, 4, 5], [])).toBeNull();
  });

  it('sem concursos, a proporção é 0 (sem dividir por zero)', () => {
    const faixas = faixasDoResultado(conferirJogo('quina', [1, 2, 3, 4, 5], []));
    expect(faixas.every((f) => f.proporcao === 0 && f.concursos === 0)).toBe(true);
  });
});

function historico(slug: Slug): HistoricoV1 {
  return validarHistorico(JSON.parse(lerFixtureTexto(`${slug}/historico.json`)), slug);
}

/** Leitura independente da fixture: regex sobre o texto cru, sem o validador. */
function cru(slug: Slug): { concurso: number; dezenas: number[] }[] {
  return Array.from(
    lerFixtureTexto(`${slug}/historico.json`).matchAll(
      /\[(\d+), "\d{4}-\d{2}-\d{2}", \[([\d, ]+)\]\]/g,
    ),
    (m) => ({ concurso: Number(m[1]), dezenas: (m[2] as string).split(',').map(Number) }),
  );
}

/** Cálculo ingênuo e independente: Set e interseção por concurso. */
function ingenuo(jogo: number[], dados: { concurso: number; dezenas: number[] }[]) {
  const conj = new Set(jogo);
  const contagem = new Map<number, number>();
  let melhor = -1;
  let melhores: number[] = [];
  for (const { concurso, dezenas } of dados) {
    const a = dezenas.filter((d) => conj.has(d)).length;
    contagem.set(a, (contagem.get(a) ?? 0) + 1);
    if (a > melhor) {
      melhor = a;
      melhores = [concurso];
    } else if (a === melhor) {
      melhores.unshift(concurso);
    }
  }
  return { contagem, melhor, melhores };
}

describe('conferirJogo contra as fixtures reais', () => {
  const jogos: Record<Slug, number[][]> = {
    lotofacil: [
      Array.from({ length: 15 }, (_, i) => i + 1),
      Array.from({ length: 20 }, (_, i) => i + 6),
      [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 2, 4],
    ],
    megasena: [
      [4, 8, 15, 16, 23, 42],
      Array.from({ length: 20 }, (_, i) => i * 3 + 1),
      [1, 2, 3, 4, 5, 6],
    ],
    quina: [
      [5, 12, 33, 47, 71],
      Array.from({ length: 15 }, (_, i) => i * 5 + 2),
      [1, 2, 3, 4, 5, 6, 7],
    ],
  };

  for (const slug of SLUGS) {
    it(`${slug}: contagem, melhor e empates iguais ao cálculo independente`, () => {
      const dados = cru(slug);
      const concursos = historico(slug).concursos;
      expect(dados.length).toBe(concursos.length);
      for (const jogo of jogos[slug]) {
        const r = conferirJogo(slug, jogo, concursos);
        const esperado = ingenuo(jogo, dados);
        expect(r.totalConcursos).toBe(dados.length);
        expect(r.porAcertos.reduce((a, b) => a + b, 0)).toBe(dados.length);
        r.porAcertos.forEach((n, k) => expect(n).toBe(esperado.contagem.get(k) ?? 0));
        expect(r.melhor?.acertos).toBe(esperado.melhor);
        expect(r.melhor?.concursos).toEqual(esperado.melhores);
        expect(concursosComAcertos(slug, jogo, concursos, esperado.melhor)).toEqual(
          esperado.melhores,
        );
      }
    });
  }

  it('desempenho: 200 conferências do maior histórico em tempo folgado', () => {
    const concursos = historico('lotofacil').concursos;
    const jogo = jogos.lotofacil[1] as number[];
    const inicio = performance.now();
    for (let i = 0; i < 200; i++) conferirJogo('lotofacil', jogo, concursos);
    // Limite generoso (evita teste instável): cada conferência custa poucos ms.
    expect(performance.now() - inicio).toBeLessThan(4000);
  });
});

describe('montarVisaoConferir', () => {
  const AGORA = new Date('2026-10-14T12:30:00Z');
  const SINCRONIZADO = '2026-10-14T09:30:00Z';

  function cacheCom(slug: Slug, h: HistoricoV1 | null): ResultadoCache {
    return {
      indice: {} as IndiceV1,
      indiceSincronizadoEm: null,
      sincronizadoEm: SINCRONIZADO,
      loterias: { [slug]: { indice: {} as LoteriaNoIndice, dados: { historico: h } } },
    };
  }
  const pronta = (
    cache: ResultadoCache | null,
    falha: SituacaoTela['falha'] = 'nenhuma',
  ): SituacaoTela => ({ cache, sincronizando: false, falha });

  it('antes de ler o cache ou sem histórico e sincronizando: carregando', () => {
    expect(montarVisaoConferir('quina', null, AGORA)).toEqual({ tipo: 'carregando' });
    expect(
      montarVisaoConferir('quina', { cache: null, sincronizando: true, falha: 'nenhuma' }, AGORA),
    ).toEqual({ tipo: 'carregando' });
  });

  it('sem cache e sem rede, recusado sem anterior e loteria nula', () => {
    expect(montarVisaoConferir('quina', pronta(null, 'sem_rede'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.sincronizacao.semDados,
      erro: true,
    });
    expect(montarVisaoConferir('quina', pronta(null, 'recusado'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.conferir.recusadoSemAnterior,
      erro: true,
    });
    const semEstaLoteria = cacheCom('megasena', historico('megasena'));
    expect(montarVisaoConferir('quina', pronta(semEstaLoteria), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.conferir.semLoteria,
      erro: false,
    });
  });

  it('com histórico: nome, subtítulo com a faixa de dezenas e os concursos', () => {
    const h = historico('quina');
    const v = montarVisaoConferir('quina', pronta(cacheCom('quina', h)), AGORA);
    expect(v.tipo).toBe('conteudo');
    if (v.tipo !== 'conteudo') return;
    expect(v.nomeLoteria).toBe('Quina');
    expect(v.subtitulo).toBe('Quina · escolha de 5 a 15 dezenas');
    expect(v.estado).toBe('atualizado');
    expect(v.erro).toBe(false);
    expect(v.concursos).toBe(h.concursos);
  });

  it('dado recusado com histórico anterior: mostra o anterior e sinaliza o erro', () => {
    const v = montarVisaoConferir(
      'lotofacil',
      pronta(cacheCom('lotofacil', historico('lotofacil')), 'recusado'),
      AGORA,
    );
    expect(v.tipo === 'conteudo' && v.estado === 'recusado' && v.erro).toBe(true);
  });
});
