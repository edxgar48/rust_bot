// Visão pura da tela Força (sem Svelte): ordem, subtítulo, perfil/janela, estados.
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import { formatarIndice, formatarVariacao } from '../componentes/ranking';
import type { ResultadoCache } from '../dados/cliente';
import type { IndiceV1, LoteriaNoIndice, Slug, TabelaGeralV1 } from '../dados/tipos';
import { validarTabelaGeral } from '../dados/validacao';
import { textos } from '../lib/textos';
import { linhasDaTabela, montarVisaoForca, type SituacaoForca } from './forca';

const AGORA = new Date('2026-10-14T12:30:00Z');
const SINCRONIZADO = '2026-10-14T12:30:00.000Z'; // 09:30 em Brasília

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function tabela(slug: Slug, alterar?: (json: any) => void): TabelaGeralV1 {
  const json = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`));
  alterar?.(json);
  return validarTabelaGeral(json, slug);
}

function cacheCom(
  slug: Slug,
  t: TabelaGeralV1 | null,
  sincronizadoEm: string | null = SINCRONIZADO,
): ResultadoCache {
  return {
    indice: {} as IndiceV1,
    indiceSincronizadoEm: null,
    sincronizadoEm,
    loterias: { [slug]: { indice: {} as LoteriaNoIndice, dados: { tabela_geral: t } } },
  };
}

const pronta = (
  cache: ResultadoCache | null,
  falha: SituacaoForca['falha'] = 'nenhuma',
): SituacaoForca => ({ cache, sincronizando: false, falha });

function conteudo(slug: Slug, situacao: SituacaoForca | null) {
  const v = montarVisaoForca(slug, situacao, AGORA);
  if (v.tipo !== 'conteudo') throw new Error(`esperava conteúdo, veio ${v.tipo}`);
  return v;
}

describe('linhas da tabela', () => {
  it('uma linha por dezena, em ordem de posição de 1 a N (25, 60 e 80)', () => {
    for (const [slug, n] of [
      ['lotofacil', 25],
      ['megasena', 60],
      ['quina', 80],
    ] as const) {
      const linhas = linhasDaTabela(tabela(slug));
      expect(linhas).toHaveLength(n);
      expect(linhas.map((l) => l.posicao)).toEqual(Array.from({ length: n }, (_, i) => i + 1));
      expect(new Set(linhas.map((l) => l.dezena)).size).toBe(n);
    }
  });

  it('não depende da ordem da lista: reordena pela posição (a validação já a exige; defesa)', () => {
    const original = tabela('quina');
    const invertida = { ...original, dezenas: [...original.dezenas].reverse() };
    const linhas = linhasDaTabela(invertida);
    expect(linhas.map((l) => l.posicao)).toEqual(Array.from({ length: 80 }, (_, i) => i + 1));
    expect(linhas.map((l) => l.dezena)).toEqual(
      linhasDaTabela(tabela('quina')).map((l) => l.dezena),
    );
  });

  it('copia posição, dezena, variação, total e evidência sem recalcular (Lotofácil, fixture real)', () => {
    const [primeira, segunda, terceira] = linhasDaTabela(tabela('lotofacil'));
    expect(primeira).toEqual({
      posicao: 1,
      dezena: 25,
      variacao: 1,
      indice: 4.317951874,
      grau: 'fraca',
      direcao: 'vies',
    });
    expect(segunda).toMatchObject({ posicao: 2, dezena: 3, variacao: -1, direcao: 'acaso' });
    expect(terceira).toMatchObject({ posicao: 3, dezena: 19, variacao: 1, grau: 'fraca' });
  });

  it('total com 3 casas e variação com sinal vêm dos formatadores do ranking', () => {
    const [primeira, segunda] = linhasDaTabela(tabela('lotofacil'));
    expect(formatarIndice(primeira?.indice ?? 0)).toBe('4,318');
    expect(formatarIndice(segunda?.indice ?? 0)).toBe('4,178');
    expect(formatarVariacao(primeira?.variacao ?? null)).toBe('+1');
    expect(formatarVariacao(segunda?.variacao ?? null)).toBe('−1');
  });

  it('variação null (sem concurso anterior) e evidência null seguem null, nunca zero', () => {
    const t = tabela('megasena', (j) => {
      j.dezenas[0].variacao_posicao = null;
      j.dezenas[1].evidencia_grau = null;
      j.dezenas[1].evidencia_direcao = null;
    });
    const [a, b] = linhasDaTabela(t);
    expect(a?.variacao).toBeNull();
    expect(b?.grau).toBeNull();
    expect(b?.direcao).toBeNull();
  });

  it('variação 0 continua 0 (manteve a posição)', () => {
    const primeira = linhasDaTabela(tabela('megasena'))[0];
    expect(primeira?.variacao).toBe(0);
  });
});

describe('montarVisaoForca', () => {
  it('sem leitura do cache ainda: carregando', () => {
    expect(montarVisaoForca('quina', null, AGORA)).toEqual({ tipo: 'carregando' });
  });

  it('subtítulo "Próximo concurso N, com dados até o M" e perfil/janela do arquivo', () => {
    const v = conteudo('quina', pronta(cacheCom('quina', tabela('quina'))));
    expect(v.subtitulo).toBe('Próximo concurso 7142, com dados até o 7141');
    expect(v.perfilJanela).toBe('Perfil padrao_v1 · janela de 50 concursos');
    expect(v.nomeLoteria).toBe('Quina');
    expect(v.linhas).toHaveLength(80);
  });

  it('janela 0 vira "histórico total"; janela 1 fica no singular', () => {
    const t = tabela('quina', (j) => (j.janela = 0));
    const total = conteudo('quina', pronta(cacheCom('quina', t)));
    expect(total.perfilJanela).toBe('Perfil padrao_v1 · histórico total');
    expect(textos.forca.perfilJanela('x', 1)).toBe('Perfil x · janela de 1 concurso');
  });

  it('estado atualizado: texto com a data do arquivo (Brasília)', () => {
    const v = conteudo('lotofacil', pronta(cacheCom('lotofacil', tabela('lotofacil'))));
    expect(v.estado).toBe('atualizado');
    expect(v.erro).toBe(false);
    expect(v.texto).toBe('Atualizado em 14/10/2026 09:30.');
  });

  it('sincronizando com cache: mostra a tabela já, dizendo a data dos dados', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'));
    const v = conteudo('lotofacil', { cache, sincronizando: true, falha: 'nenhuma' });
    expect(v.estado).toBe('sincronizando');
    expect(v.texto).toBe('Verificando atualizações… Mostrando dados de 14/10/2026 09:30.');
    expect(v.linhas).toHaveLength(25);
  });

  it('sem rede com cache: mostra a tabela com a data do arquivo, sem erro', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'));
    const v = conteudo('lotofacil', pronta(cache, 'sem_rede'));
    expect(v.estado).toBe('sem_rede');
    expect(v.erro).toBe(false);
    expect(v.texto).toBe('Sem rede, mostrando dados de 14/10/2026 09:30.');
  });

  it('recusado com cache anterior: erro, e a tabela anterior continua', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'));
    const v = conteudo('lotofacil', pronta(cache, 'recusado'));
    expect(v.estado).toBe('recusado');
    expect(v.erro).toBe(true);
    expect(v.texto).toContain('Dado recusado');
    expect(v.texto).toContain('14/10/2026 09:30');
    expect(v.linhas).toHaveLength(25);
  });

  it('data desconhecida: o texto não inventa data', () => {
    const cache = cacheCom('lotofacil', tabela('lotofacil'), null);
    const v = conteudo('lotofacil', pronta(cache, 'sem_rede'));
    expect(v.texto).toBe('Sem rede, mostrando os últimos dados salvos.');
  });

  it('sem cache e sincronizando: carregando', () => {
    const situacao: SituacaoForca = { cache: null, sincronizando: true, falha: 'nenhuma' };
    expect(montarVisaoForca('quina', situacao, AGORA)).toEqual({ tipo: 'carregando' });
  });

  it('sem cache e sem rede: sem dados, como erro, com o texto de conectar', () => {
    expect(montarVisaoForca('quina', pronta(null, 'sem_rede'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.sincronizacao.semDados,
      erro: true,
    });
  });

  it('sem cache e recusado: diz que foi recusado e que não há versão anterior', () => {
    expect(montarVisaoForca('quina', pronta(null, 'recusado'), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.forca.recusadoSemAnterior,
      erro: true,
    });
  });

  it('cache de outra loteria não serve: a tabela da Quina não aparece na Mega-Sena', () => {
    const cache = cacheCom('quina', tabela('quina'));
    const v = montarVisaoForca('megasena', pronta(cache, 'sem_rede'), AGORA);
    expect(v.tipo).toBe('sem_dados');
  });

  it('loteria null no índice (sem falha): "Ainda sem dados desta loteria", sem erro', () => {
    const cache: ResultadoCache = { ...cacheCom('quina', null), loterias: { quina: null } };
    expect(montarVisaoForca('quina', pronta(cache), AGORA)).toEqual({
      tipo: 'sem_dados',
      texto: textos.forca.semLoteria,
      erro: false,
    });
  });

  it('arquivo ausente do cache com a loteria no índice: sem dados', () => {
    const v = montarVisaoForca('quina', pronta(cacheCom('quina', null), 'sem_rede'), AGORA);
    expect(v.tipo).toBe('sem_dados');
  });
});
