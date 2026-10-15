// Visão pura da seção "Por evidência" (sem Svelte): agrupar por grau x direção
// preservando a ordem de posição, conferida contra leitura independente das fixtures.
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import type { ResultadoCache } from '../dados/cliente';
import type { IndiceV1, LoteriaNoIndice, Slug, TabelaGeralV1 } from '../dados/tipos';
import { validarTabelaGeral } from '../dados/validacao';
import { textos } from '../lib/textos';
import type { SituacaoTela } from './estado-tela';
import {
  CHAVE_SEM_EVIDENCIA,
  agruparPorEvidencia,
  contarGrausFracos,
  montarVisaoEvidencia,
  rotuloDoGrupo,
} from './evidencia';
import { linhasDaTabela, type LinhaForca } from './forca';

const AGORA = new Date('2026-10-14T12:30:00Z');
const SINCRONIZADO = '2026-10-14T12:30:00.000Z';

// Ordem esperada, escrita à mão: grau do mais forte ao mais fraco; viés antes de acaso.
const ORDEM = [
  'muito_forte|vies',
  'muito_forte|acaso',
  'forte|vies',
  'forte|acaso',
  'moderada|vies',
  'moderada|acaso',
  'fraca|vies',
  'fraca|acaso',
  'muito_fraca|vies',
  'muito_fraca|acaso',
];

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function json(slug: Slug, alterar?: (j: any) => void): any {
  const j = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`));
  alterar?.(j);
  return j;
}

function tabela(slug: Slug, alterar?: (j: unknown) => void): TabelaGeralV1 {
  return validarTabelaGeral(json(slug, alterar), slug);
}

function cacheCom(slug: Slug, t: TabelaGeralV1 | null): ResultadoCache {
  return {
    indice: {} as IndiceV1,
    indiceSincronizadoEm: null,
    sincronizadoEm: SINCRONIZADO,
    loterias: { [slug]: { indice: {} as LoteriaNoIndice, dados: { tabela_geral: t } } },
  };
}

const pronta = (
  cache: ResultadoCache | null,
  falha: SituacaoTela['falha'] = 'nenhuma',
): SituacaoTela => ({ cache, sincronizando: false, falha });

function linha(posicao: number, grau: LinhaForca['grau'], direcao: LinhaForca['direcao']) {
  return { posicao, dezena: 100 + posicao, variacao: 0, indice: 0, grau, direcao };
}

/** Grupos esperados, calculados direto do JSON cru (snake_case), sem usar o código testado. */
function esperadoIndependente(slug: Slug) {
  const dezenas: {
    dezena: number;
    posicao: number;
    evidencia_grau: string | null;
    evidencia_direcao: string | null;
  }[] = json(slug).dezenas;
  const mapa = new Map<string, number[]>();
  for (const d of [...dezenas].sort((a, b) => a.posicao - b.posicao)) {
    const chave =
      d.evidencia_grau === null || d.evidencia_direcao === null
        ? 'sem'
        : `${d.evidencia_grau}|${d.evidencia_direcao}`;
    mapa.set(chave, [...(mapa.get(chave) ?? []), d.dezena]);
  }
  return [...ORDEM, 'sem']
    .filter((k) => mapa.has(k))
    .map((k) => ({ chave: k, dezenas: mapa.get(k)! }));
}

describe('agruparPorEvidencia', () => {
  for (const slug of ['lotofacil', 'megasena', 'quina'] as const) {
    it(`${slug}: grupos, ordem e dezenas iguais à leitura independente do arquivo`, () => {
      const grupos = agruparPorEvidencia(linhasDaTabela(tabela(slug)));
      expect(
        grupos.map((g) => ({ chave: g.chave, dezenas: g.dezenas.map((l) => l.dezena) })),
      ).toEqual(esperadoIndependente(slug));
    });

    it(`${slug}: cada dezena em exatamente um grupo; posição crescente dentro do grupo`, () => {
      const t = tabela(slug);
      const grupos = agruparPorEvidencia(linhasDaTabela(t));
      const todas = grupos.flatMap((g) => g.dezenas.map((l) => l.dezena));
      expect(todas.toSorted((a, b) => a - b)).toEqual(
        t.dezenas.map((d) => d.dezena).toSorted((a, b) => a - b),
      );
      for (const g of grupos) {
        const posicoes = g.dezenas.map((l) => l.posicao);
        expect(posicoes).toEqual(posicoes.toSorted((a, b) => a - b));
        // Cada dezena traz os mesmos campos do motor (sem recalcular).
        for (const l of g.dezenas) {
          const d = t.dezenas.find((x) => x.dezena === l.dezena)!;
          expect(l).toMatchObject({
            posicao: d.posicao,
            variacao: d.variacaoPosicao,
            grau: d.evidenciaGrau,
            direcao: d.evidenciaDirecao,
          });
        }
      }
    });
  }

  it('Lotofácil real: seis grupos, rótulos como os da Força e contagens do arquivo', () => {
    const grupos = agruparPorEvidencia(linhasDaTabela(tabela('lotofacil')));
    expect(grupos.map((g) => [g.rotulo, g.dezenas.length])).toEqual([
      ['Forte · viés', 1],
      ['Moderada · viés', 1],
      ['Fraca · viés', 3],
      ['Fraca · acaso', 12],
      ['Muito fraca · viés', 1],
      ['Muito fraca · acaso', 7],
    ]);
  });

  it('grupos vazios são omitidos (Quina real: só três combinações)', () => {
    const grupos = agruparPorEvidencia(linhasDaTabela(tabela('quina')));
    expect(grupos.map((g) => g.chave)).toEqual([
      'fraca|vies',
      'muito_fraca|vies',
      'muito_fraca|acaso',
    ]);
    expect(agruparPorEvidencia([])).toEqual([]);
  });

  it('ordem dos graus e das direções com as dez combinações presentes, fora de ordem', () => {
    const embaralhadas = ORDEM.toReversed().map((k, i) => {
      const [grau, direcao] = k.split('|') as [LinhaForca['grau'], LinhaForca['direcao']];
      return linha(i + 1, grau, direcao);
    });
    expect(agruparPorEvidencia(embaralhadas).map((g) => g.chave)).toEqual(ORDEM);
  });

  it('não depende da ordem de entrada: reordena pela posição dentro do grupo', () => {
    const a = linha(7, 'fraca', 'acaso');
    const b = linha(2, 'fraca', 'acaso');
    const c = linha(5, 'fraca', 'acaso');
    const [g] = agruparPorEvidencia([a, b, c]);
    expect(g?.dezenas.map((l) => l.posicao)).toEqual([2, 5, 7]);
  });

  it('grau ou direção nulos: grupo "Sem evidência calculada", sempre por último, sem inventar', () => {
    const grupos = agruparPorEvidencia([
      linha(1, null, null),
      linha(2, 'forte', null),
      linha(3, null, 'acaso'),
      linha(4, 'muito_fraca', 'acaso'),
    ]);
    expect(grupos.map((g) => g.chave)).toEqual(['muito_fraca|acaso', CHAVE_SEM_EVIDENCIA]);
    const sem = grupos[1]!;
    expect(sem.rotulo).toBe('Sem evidência calculada');
    expect(sem.grau).toBeNull();
    expect(sem.direcao).toBeNull();
    expect(sem.dezenas.map((l) => l.posicao)).toEqual([1, 2, 3]);
  });

  it('nulos vindos do arquivo (contrato: ausência = null) caem no grupo sem evidência', () => {
    const t = tabela('megasena', (j) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const dezenas = (j as any).dezenas;
      dezenas[0].evidencia_grau = null;
      dezenas[0].evidencia_direcao = null;
    });
    const grupos = agruparPorEvidencia(linhasDaTabela(t));
    const ultimo = grupos[grupos.length - 1]!;
    expect(ultimo.chave).toBe(CHAVE_SEM_EVIDENCIA);
    expect(ultimo.dezenas).toHaveLength(1);
    expect(ultimo.dezenas[0]!.grau).toBeNull();
  });

  it('rótulo = o mesmo texto da linha da Força (grau · direção)', () => {
    expect(rotuloDoGrupo('fraca', 'vies')).toBe('Fraca · viés');
    expect(rotuloDoGrupo('muito_forte', 'acaso')).toBe('Muito forte · acaso');
    expect(rotuloDoGrupo(null, 'acaso')).toBe(textos.evidencia.grupo.semEvidencia);
  });

  it('contarGrausFracos soma muito fraca e fraca, em qualquer direção', () => {
    const grupos = agruparPorEvidencia(linhasDaTabela(tabela('lotofacil')));
    expect(contarGrausFracos(grupos)).toBe(3 + 12 + 1 + 7);
    expect(contarGrausFracos([])).toBe(0);
  });
});

describe('montarVisaoEvidencia', () => {
  it('antes de ler o cache: carregando', () => {
    expect(montarVisaoEvidencia('quina', null, AGORA)).toEqual({ tipo: 'carregando' });
  });

  it('conteúdo: subtítulo da Força, perfil/janela do motor, total e fracos', () => {
    const v = montarVisaoEvidencia('quina', pronta(cacheCom('quina', tabela('quina'))), AGORA);
    if (v.tipo !== 'conteudo') throw new Error(v.tipo);
    expect(v.subtitulo).toBe('Próximo concurso 7142, com dados até o 7141');
    expect(v.perfilJanela).toContain('Perfil padrao_v1 · janela de 50 concursos');
    expect(v.perfilJanela).toContain('não muda com a janela de Frequência e atraso');
    expect(v.total).toBe(80);
    expect(v.fracos).toBe(80);
    expect(v.grupos).toHaveLength(3);
    expect(v.estado).toBe('atualizado');
    expect(v.erro).toBe(false);
  });

  it('janela 0 do arquivo: histórico total', () => {
    const t = tabela('quina', (j) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (j as any).janela = 0;
    });
    const v = montarVisaoEvidencia('quina', pronta(cacheCom('quina', t)), AGORA);
    if (v.tipo !== 'conteudo') throw new Error(v.tipo);
    expect(v.perfilJanela).toContain('Perfil padrao_v1 · histórico total');
  });

  it('sem rede com cache e recusado com cache: mantém os grupos', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'));
    const semRede = montarVisaoEvidencia('lotofacil', pronta(cache, 'sem_rede'), AGORA);
    const recusado = montarVisaoEvidencia('lotofacil', pronta(cache, 'recusado'), AGORA);
    if (semRede.tipo !== 'conteudo' || recusado.tipo !== 'conteudo') throw new Error('conteúdo');
    expect(semRede.estado).toBe('sem_rede');
    expect(semRede.erro).toBe(false);
    expect(semRede.grupos).toHaveLength(6);
    expect(recusado.estado).toBe('recusado');
    expect(recusado.erro).toBe(true);
    expect(recusado.grupos).toHaveLength(6);
  });

  it('sincronizando com cache mostra os grupos; sem cache e sincronizando é carregando', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'));
    const com = montarVisaoEvidencia(
      'lotofacil',
      { cache, sincronizando: true, falha: 'nenhuma' },
      AGORA,
    );
    expect(com.tipo).toBe('conteudo');
    const sem = montarVisaoEvidencia(
      'lotofacil',
      { cache: null, sincronizando: true, falha: 'nenhuma' },
      AGORA,
    );
    expect(sem).toEqual({ tipo: 'carregando' });
  });

  it('sem cache: sem rede é erro; recusado diz que não há versão anterior', () => {
    expect(montarVisaoEvidencia('quina', pronta(null, 'sem_rede'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.sincronizacao.semDados,
      erro: true,
    });
    expect(montarVisaoEvidencia('quina', pronta(null, 'recusado'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.evidencia.recusadoSemAnterior,
      erro: true,
    });
  });

  it('loteria nula no índice: informativo, sem erro', () => {
    const cache: ResultadoCache = { ...cacheCom('quina', null), loterias: { quina: null } };
    expect(montarVisaoEvidencia('quina', pronta(cache), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.evidencia.semLoteria,
      erro: false,
    });
  });

  it('cache de outra loteria não serve', () => {
    const cache = cacheCom('quina', tabela('quina'));
    expect(montarVisaoEvidencia('megasena', pronta(cache, 'sem_rede'), AGORA).tipo).toBe(
      'sem_dados',
    );
  });
});
