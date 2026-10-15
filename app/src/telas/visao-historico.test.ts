// Lógica pura do Histórico: busca digitada, consulta (busca + filtro), janela da
// lista virtual e visão. Valores à mão e as fixtures reais das 3 loterias,
// conferidas por um cálculo independente (leitura do texto do JSON, sem o validador).
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
import { SLUGS } from '../dados/tipos';
import { validarHistorico } from '../dados/validacao';
import type { SituacaoTela } from './estado-tela';
import {
  ALTURA_LINHA_REM,
  consultarHistorico,
  dezenasDoFiltroNoConcurso,
  interpretarBusca,
  janelaVirtual,
  lerDataDigitada,
  listarDezenas,
  mascararData,
  montarVisaoHistorico,
} from './historico';

const PEQUENO: readonly ConcursoHistorico[] = [
  [10, '2020-01-01', [1, 2, 3]],
  [11, '2020-01-04', [2, 3, 4]],
  [12, '2020-01-04', [3, 4, 5]],
  [13, '2020-01-08', [1, 3, 5]],
];

function historico(slug: Slug): HistoricoV1 {
  return validarHistorico(JSON.parse(lerFixtureTexto(`${slug}/historico.json`)), slug);
}

/** Leitura independente da fixture: uma linha por concurso, extraída por regex do texto cru. */
function linhasCruas(slug: Slug): { concurso: number; data: string; dezenas: number[] }[] {
  const saida = [];
  for (const m of lerFixtureTexto(`${slug}/historico.json`).matchAll(
    /\[(\d+), "(\d{4}-\d{2}-\d{2})", \[([\d, ]+)\]\]/g,
  )) {
    saida.push({
      concurso: Number(m[1]),
      data: m[2] as string,
      dezenas: (m[3] as string).split(',').map(Number),
    });
  }
  return saida;
}

describe('lerDataDigitada', () => {
  it('aceita dd/mm/aaaa, com ponto ou hífen, dia/mês sem zero e AAAA-MM-DD', () => {
    for (const t of ['13/10/2026', '13.10.2026', '13-10-2026', '13/10/2026 ', ' 2026-10-13']) {
      expect(lerDataDigitada(t)).toEqual({ tipo: 'ok', iso: '2026-10-13' });
    }
    expect(lerDataDigitada('1/2/2020')).toEqual({ tipo: 'ok', iso: '2020-02-01' });
    expect(lerDataDigitada('2020-2-1')).toEqual({ tipo: 'ok', iso: '2020-02-01' });
  });

  it('calendário: 29/02 só em ano bissexto (inclusive século), 31 só em meses de 31 dias', () => {
    expect(lerDataDigitada('29/02/2024').tipo).toBe('ok');
    expect(lerDataDigitada('29/02/2000').tipo).toBe('ok');
    expect(lerDataDigitada('29/02/2023')).toEqual({ tipo: 'inexistente' });
    expect(lerDataDigitada('29/02/1900')).toEqual({ tipo: 'inexistente' });
    expect(lerDataDigitada('31/04/2020')).toEqual({ tipo: 'inexistente' });
    expect(lerDataDigitada('31/12/2020').tipo).toBe('ok');
    expect(lerDataDigitada('00/10/2020')).toEqual({ tipo: 'inexistente' });
    expect(lerDataDigitada('10/13/2020')).toEqual({ tipo: 'inexistente' });
    expect(lerDataDigitada('10/00/2020')).toEqual({ tipo: 'inexistente' });
  });

  it('o que não parece data é formato inválido', () => {
    for (const t of ['', 'abc', '13/10', '13/10/26', '13102026', '13/10/20267', '2026/10/13']) {
      expect(lerDataDigitada(t)).toEqual({ tipo: 'formato' });
    }
  });
});

describe('interpretarBusca', () => {
  it('vazio (ou só espaços) = sem busca', () => {
    expect(interpretarBusca('concurso', '')).toEqual({ tipo: 'vazia' });
    expect(interpretarBusca('data', '   ')).toEqual({ tipo: 'vazia' });
  });

  it('concurso: só algarismos, maior que zero', () => {
    expect(interpretarBusca('concurso', ' 3000 ')).toEqual({
      tipo: 'ok',
      busca: { modo: 'concurso', numero: 3000 },
    });
    expect(interpretarBusca('concurso', '007')).toEqual({
      tipo: 'ok',
      busca: { modo: 'concurso', numero: 7 },
    });
    for (const t of ['0', 'abc', '12a', '-5', '1.5', '1234567890']) {
      expect(interpretarBusca('concurso', t)).toEqual({
        tipo: 'invalida',
        motivo: 'formato_concurso',
      });
    }
  });

  it('data: converte para AAAA-MM-DD e separa formato de data inexistente', () => {
    expect(interpretarBusca('data', '13/10/2026')).toEqual({
      tipo: 'ok',
      busca: { modo: 'data', data: '2026-10-13' },
    });
    expect(interpretarBusca('data', '31/02/2020')).toEqual({
      tipo: 'invalida',
      motivo: 'data_inexistente',
    });
    expect(interpretarBusca('data', '3000')).toEqual({ tipo: 'invalida', motivo: 'formato_data' });
  });
});

describe('mascararData', () => {
  it('insere as barras enquanto digita e ignora o que não é algarismo', () => {
    expect(mascararData('1')).toBe('1');
    expect(mascararData('13')).toBe('13');
    expect(mascararData('131')).toBe('13/1');
    expect(mascararData('1310')).toBe('13/10');
    expect(mascararData('13102')).toBe('13/10/2');
    expect(mascararData('13102026')).toBe('13/10/2026');
    expect(mascararData('131020261234')).toBe('13/10/2026');
    expect(mascararData('13/10/2026')).toBe('13/10/2026');
    expect(mascararData('a1b3')).toBe('13');
  });

  it('apagar uma barra não trava o campo; texto colado em AAAA-MM-DD fica como veio', () => {
    expect(mascararData('13/')).toBe('13');
    expect(mascararData('13/10/')).toBe('13/10');
    expect(mascararData('2026-10-13')).toBe('2026-10-13');
    expect(mascararData('2026-10-13999')).toBe('2026-10-13');
  });
});

describe('consultarHistorico (valores à mão)', () => {
  const sem = { busca: null, dezenas: [] as number[] };

  it('sem busca nem filtro: todos, do mais recente ao mais antigo', () => {
    const r = consultarHistorico(PEQUENO, sem);
    expect(r.indices).toEqual([3, 2, 1, 0]);
    expect(r.aviso).toBeNull();
  });

  it('histórico vazio: nada, sem quebrar', () => {
    expect(consultarHistorico([], sem).indices).toEqual([]);
    expect(
      consultarHistorico([], { busca: { modo: 'concurso', numero: 1 }, dezenas: [] }).aviso,
    ).toMatchObject({ tipo: 'concurso_inexistente' });
    expect(
      consultarHistorico([], { busca: { modo: 'data', data: '2020-01-01' }, dezenas: [] }).aviso,
    ).toEqual({ tipo: 'data_sem_sorteio', data: '2020-01-01', anterior: null, seguinte: null });
  });

  it('concurso existente: só ele; inexistente: aviso com a faixa que existe', () => {
    expect(
      consultarHistorico(PEQUENO, { busca: { modo: 'concurso', numero: 12 }, dezenas: [] }),
    ).toEqual({ indices: [2], aviso: null });
    for (const numero of [9, 14, 1, 99999]) {
      expect(
        consultarHistorico(PEQUENO, { busca: { modo: 'concurso', numero }, dezenas: [] }),
      ).toEqual({
        indices: [],
        aviso: { tipo: 'concurso_inexistente', numero, primeiro: 10, ultimo: 13 },
      });
    }
  });

  it('data: todos os concursos do dia (recente primeiro); data sem sorteio traz os vizinhos', () => {
    const dia = (data: string, dezenas: number[] = []) =>
      consultarHistorico(PEQUENO, { busca: { modo: 'data', data }, dezenas });
    expect(dia('2020-01-04').indices).toEqual([2, 1]);
    expect(dia('2020-01-01').indices).toEqual([0]);
    expect(dia('2020-01-08').indices).toEqual([3]);
    expect(dia('2020-01-05').aviso).toEqual({
      tipo: 'data_sem_sorteio',
      data: '2020-01-05',
      anterior: { concurso: 12, data: '2020-01-04' },
      seguinte: { concurso: 13, data: '2020-01-08' },
    });
    expect(dia('2019-12-31').aviso).toEqual({
      tipo: 'data_sem_sorteio',
      data: '2019-12-31',
      anterior: null,
      seguinte: { concurso: 10, data: '2020-01-01' },
    });
    expect(dia('2021-01-01').aviso).toEqual({
      tipo: 'data_sem_sorteio',
      data: '2021-01-01',
      anterior: { concurso: 13, data: '2020-01-08' },
      seguinte: null,
    });
    // O filtro vale dentro do dia.
    expect(dia('2020-01-04', [4, 5]).indices).toEqual([2]);
  });

  it('filtro: o concurso precisa ter TODAS as dezenas', () => {
    const f = (dezenas: number[]) => consultarHistorico(PEQUENO, { busca: null, dezenas });
    expect(f([3]).indices).toEqual([3, 2, 1, 0]);
    expect(f([3, 5]).indices).toEqual([3, 2]);
    expect(f([1, 5]).indices).toEqual([3]);
    expect(f([1, 4])).toEqual({ indices: [], aviso: { tipo: 'sem_resultado' } });
    expect(f([9]).aviso).toEqual({ tipo: 'sem_resultado' });
  });

  it('busca por concurso combinada com filtro: se o concurso não tem a dezena, sem resultado', () => {
    const c = (dezenas: number[]) =>
      consultarHistorico(PEQUENO, { busca: { modo: 'concurso', numero: 10 }, dezenas });
    expect(c([1, 2]).indices).toEqual([0]);
    expect(c([4])).toEqual({ indices: [], aviso: { tipo: 'sem_resultado' } });
  });
});

describe('consultarHistorico (fixtures reais, conferido por cálculo independente)', () => {
  for (const slug of SLUGS) {
    const h = historico(slug);
    const cru = linhasCruas(slug);

    it(`${slug}: o validador e a leitura independente concordam (${cru.length} concursos)`, () => {
      expect(cru.length).toBeGreaterThan(1000);
      expect(h.concursos).toHaveLength(cru.length);
      expect(h.concursos.map((c) => c[0])).toEqual(cru.map((c) => c.concurso));
    });

    it(`${slug}: sem filtro, todos em ordem decrescente de concurso`, () => {
      const r = consultarHistorico(h.concursos, { busca: null, dezenas: [] });
      expect(r.indices).toHaveLength(cru.length);
      const numeros = r.indices.map((i) => (h.concursos[i] as ConcursoHistorico)[0]);
      expect(numeros).toEqual(cru.map((c) => c.concurso).reverse());
    });

    it(`${slug}: filtro por 1, 2 e 3 dezenas bate com a contagem independente`, () => {
      const ultima = cru[cru.length - 1];
      expect(ultima).toBeDefined();
      const [a, b, c] = ultima!.dezenas as [number, number, number];
      for (const dezenas of [[a], [a, b], [a, b, c]]) {
        const esperado = cru
          .filter((l) => dezenas.every((d) => l.dezenas.includes(d)))
          .map((l) => l.concurso)
          .reverse();
        const r = consultarHistorico(h.concursos, { busca: null, dezenas });
        expect(r.indices.map((i) => (h.concursos[i] as ConcursoHistorico)[0])).toEqual(esperado);
        // O último concurso tem todas as dezenas escolhidas: aparece primeiro.
        expect(esperado[0]).toBe(ultima!.concurso);
      }
    });

    it(`${slug}: busca por cada concurso conhecido (primeiro, meio, último) e por inexistentes`, () => {
      const primeiro = cru[0]!.concurso;
      const ultimo = cru[cru.length - 1]!.concurso;
      for (const numero of [primeiro, Math.floor((primeiro + ultimo) / 2), ultimo]) {
        const r = consultarHistorico(h.concursos, {
          busca: { modo: 'concurso', numero },
          dezenas: [],
        });
        expect(r.indices).toHaveLength(1);
        expect((h.concursos[r.indices[0] as number] as ConcursoHistorico)[0]).toBe(numero);
      }
      for (const numero of [primeiro - 1, ultimo + 1]) {
        expect(
          consultarHistorico(h.concursos, { busca: { modo: 'concurso', numero }, dezenas: [] })
            .aviso,
        ).toEqual({ tipo: 'concurso_inexistente', numero, primeiro, ultimo });
      }
    });

    it(`${slug}: busca por data devolve exatamente os concursos daquele dia`, () => {
      // Dia com sorteio: o do concurso do meio; e o dia seguinte sem sorteio (se existir).
      const meio = cru[Math.floor(cru.length / 2)]!;
      const doDia = cru
        .filter((l) => l.data === meio.data)
        .map((l) => l.concurso)
        .reverse();
      const r = consultarHistorico(h.concursos, {
        busca: { modo: 'data', data: meio.data },
        dezenas: [],
      });
      expect(r.indices.map((i) => (h.concursos[i] as ConcursoHistorico)[0])).toEqual(doDia);

      const datas = new Set(cru.map((l) => l.data));
      // Um dia sem sorteio qualquer entre dois concursos: procura dia 31/12 sem sorteio ou 01/01.
      const semSorteio = ['1994-01-01', '2999-12-31'].find((d) => !datas.has(d));
      expect(semSorteio).toBeDefined();
      const sem = consultarHistorico(h.concursos, {
        busca: { modo: 'data', data: semSorteio! },
        dezenas: [],
      });
      expect(sem.indices).toEqual([]);
      expect(sem.aviso).toMatchObject({ tipo: 'data_sem_sorteio', data: semSorteio });
    });
  }
});

describe('listarDezenas e dezenasDoFiltroNoConcurso', () => {
  it('dois dígitos, em ordem; o filtro marca só as dezenas escolhidas que saíram', () => {
    expect(listarDezenas([5, 12, 33])).toBe('05, 12, 33');
    expect(listarDezenas([])).toBe('');
    expect(dezenasDoFiltroNoConcurso([5, 12, 33, 41, 70], [33, 5, 99])).toEqual([5, 33]);
    expect(dezenasDoFiltroNoConcurso([5, 12], [])).toEqual([]);
  });
});

describe('janelaVirtual', () => {
  const base = { total: 7141, alturaLinha: 100, alturaVisivel: 600, extra: 3 };

  it('no começo da lista: desenha as visíveis e as extras de baixo', () => {
    expect(janelaVirtual({ ...base, rolagem: 0 })).toEqual({ inicio: 0, fim: 9 }); // 600/100 + 3
  });

  it('no meio: visíveis mais extras dos dois lados', () => {
    // linhas 50..55 visíveis (rolagem 5000..5600): 47..58
    expect(janelaVirtual({ ...base, rolagem: 5000 })).toEqual({ inicio: 47, fim: 59 });
    // meio de uma linha: ainda cobre a que está cortada
    expect(janelaVirtual({ ...base, rolagem: 5050 })).toEqual({ inicio: 47, fim: 60 });
  });

  it('no fim: limita a total', () => {
    const r = janelaVirtual({ ...base, rolagem: 7141 * 100 - 600 });
    expect(r.fim).toBe(7141);
    expect(r.inicio).toBe(7141 - 9);
  });

  it('lista ainda abaixo da tela: quase nada; abaixo de 1 tela de distância, já adianta as primeiras', () => {
    expect(janelaVirtual({ ...base, rolagem: -5000 })).toEqual({ inicio: 0, fim: 0 });
    // lista começa 700 px abaixo do topo (100 px depois da tela de 600): a folga de 300 já a alcança.
    expect(janelaVirtual({ ...base, rolagem: -700 })).toEqual({ inicio: 0, fim: 2 });
  });

  it('lista já passou toda da tela: nada além do limite, sem valores negativos', () => {
    const r = janelaVirtual({ ...base, rolagem: 10_000_000 });
    expect(r).toEqual({ inicio: 7141, fim: 7141 });
  });

  it('o número de linhas não depende do tamanho do histórico', () => {
    for (const total of [10, 100, 7141, 1_000_000]) {
      const r = janelaVirtual({ ...base, total, rolagem: 0 });
      expect(r.fim - r.inicio).toBeLessThanOrEqual(9);
      const m = janelaVirtual({ ...base, total, rolagem: Math.floor(total / 2) * 100 });
      expect(m.fim - m.inicio).toBeLessThanOrEqual(12);
    }
  });

  it('total zero ou altura inválida: janela vazia', () => {
    expect(janelaVirtual({ ...base, total: 0, rolagem: 0 })).toEqual({ inicio: 0, fim: 0 });
    expect(janelaVirtual({ ...base, alturaLinha: 0, rolagem: 0 })).toEqual({ inicio: 0, fim: 0 });
  });

  it('cada loteria tem altura de linha fixa em rem', () => {
    expect(ALTURA_LINHA_REM).toEqual({ lotofacil: 7.5, megasena: 6.25, quina: 6.25 });
  });
});

describe('montarVisaoHistorico', () => {
  const AGORA = new Date('2026-10-14T12:30:00Z');
  const SINCRONIZADO = '2026-10-14T12:30:00.000Z';

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

  it('antes de ler o cache: carregando', () => {
    expect(montarVisaoHistorico('quina', null, AGORA)).toEqual({ tipo: 'carregando' });
  });

  it('sem histórico guardado: carregando enquanto sincroniza; depois sem dados (erro) ou recusado', () => {
    const vazio = cacheCom('quina', null);
    expect(
      montarVisaoHistorico('quina', { cache: vazio, sincronizando: true, falha: 'nenhuma' }, AGORA),
    ).toEqual({ tipo: 'carregando' });
    expect(montarVisaoHistorico('quina', pronta(null, 'sem_rede'), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: true,
    });
    expect(montarVisaoHistorico('quina', pronta(vazio, 'sem_rede'), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: true,
    });
    expect(montarVisaoHistorico('quina', pronta(vazio, 'recusado'), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: true,
      texto: expect.stringContaining('recusado'),
    });
  });

  it('loteria nula no índice: aviso informativo, sem erro', () => {
    const cache: ResultadoCache = { ...cacheCom('quina', null), loterias: { quina: null } };
    expect(montarVisaoHistorico('quina', pronta(cache), AGORA)).toMatchObject({
      tipo: 'sem_dados',
      erro: false,
    });
  });

  it('com histórico (Quina real): subtítulo, primeiro e último, sem copiar os concursos', () => {
    const h = historico('quina');
    const v = montarVisaoHistorico('quina', pronta(cacheCom('quina', h)), AGORA);
    if (v.tipo !== 'conteudo') throw new Error(v.tipo);
    expect(v.concursos).toBe(h.concursos);
    expect(v.primeiro).toBe(1);
    expect(v.ultimo).toBe(h.concursos.length);
    expect(v.subtitulo).toBe(
      `${h.concursos.length.toLocaleString('pt-BR')} concursos, do 1 ao ${h.concursos.length}`,
    );
    expect(v.estado).toBe('atualizado');
    expect(v.erro).toBe(false);
  });

  it('dado recusado com histórico anterior: segue mostrando e marca erro', () => {
    const v = montarVisaoHistorico(
      'quina',
      pronta(cacheCom('quina', historico('quina')), 'recusado'),
      AGORA,
    );
    expect(v).toMatchObject({ tipo: 'conteudo', estado: 'recusado', erro: true });
  });
});
