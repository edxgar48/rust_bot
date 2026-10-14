// Visão pura do detalhe da dezena: 9 componentes, null = "sem dados na janela", evidência 5x2.
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import {
  CHAVES_COMPONENTES,
  DIRECOES_EVIDENCIA,
  GRAUS_EVIDENCIA,
  type Slug,
  type TabelaGeralV1,
} from '../dados/tipos';
import { validarTabelaGeral } from '../dados/validacao';
import { textos, textosComponentes } from '../lib/textos';
import { descreverEvidencia, descreverVariacao, montarDetalheDaDezena } from './detalhe-dezena';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Json = any;

function bruto(slug: Slug): Json {
  return JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`));
}
function tabela(slug: Slug, alterar?: (json: Json) => void): TabelaGeralV1 {
  const json = bruto(slug);
  alterar?.(json);
  return validarTabelaGeral(json, slug);
}
function detalhe(t: TabelaGeralV1, dezena: number) {
  const d = montarDetalheDaDezena(t, dezena);
  if (d === null) throw new Error(`dezena ${dezena} ausente`);
  return d;
}

// Nomes do JSON, na mesma ordem de CHAVES_COMPONENTES (cálculo independente do mapeamento).
const CAMPOS_BRUTOS = [
  'frequencia',
  'maior_espaco',
  'maior_espaco_ocorrencias',
  'menor_espaco',
  'menor_espaco_ocorrencias',
  'soma_variacoes',
  'espaco_final',
  'repeticoes_total',
  'repeticao_atual',
] as const;

describe('detalhe da dezena: Lotofácil (1º lugar)', () => {
  const d = detalhe(tabela('lotofacil'), 25);

  it('título, índice com 3 casas e variação com seta, sinal e texto', () => {
    expect(d.titulo).toBe('Dezena 25 · 1º lugar');
    expect(d.posicao).toBe(1);
    expect(d.indice).toBe('4,318');
    expect(d.variacao).toEqual({ sentido: 'subiu', sinal: '▲ +1', texto: 'subiu 1 posição' });
  });

  it('evidência: grau e direção por extenso, frase e segmentos', () => {
    expect(d.evidencia?.rotulo).toBe('Fraca · viés');
    expect(d.evidencia?.frase).toContain('Evidência fraca de que esta dezena foge do acaso');
    expect(d.evidencia?.segmentos).toBe(2);
  });

  it('janela por extenso', () => {
    expect(d.janela).toBe('Janela dos componentes: últimos 50 concursos');
  });

  it('os 9 componentes na ordem do contrato, com unidade', () => {
    expect(d.componentes.map((c) => c.rotulo)).toEqual([
      'Frequência na janela',
      'Maior espaço sem sair',
      'Vezes do maior espaço',
      'Menor espaço sem sair',
      'Vezes do menor espaço',
      'Oscilação dos espaços',
      'Concursos desde a última vez',
      'Repetições em concursos seguidos',
      'Sequência atual',
    ]);
    expect(d.componentes.map((c) => c.texto)).toEqual([
      '30 vezes',
      '5 concursos',
      '1 vez',
      '0 concursos',
      '21 vezes',
      '25',
      '4 concursos',
      '21 vezes',
      '0 concursos',
    ]);
    expect(d.componentes.every((c) => !c.semDados && c.motivo === null)).toBe(true);
    for (const c of d.componentes) expect(c.explicacao.length).toBeGreaterThan(20);
  });
});

describe('detalhe da dezena: valores conferidos contra o arquivo, nas 3 loterias', () => {
  it.each(['lotofacil', 'megasena', 'quina'] as const)('%s: todas as dezenas', (slug) => {
    const json = bruto(slug);
    const t = validarTabelaGeral(json, slug);
    for (const bruta of json.dezenas) {
      const d = detalhe(t, bruta.dezena);
      expect(d.posicao).toBe(bruta.posicao);
      expect(d.componentes).toHaveLength(9);
      d.componentes.forEach((c, i) => {
        const original = bruta[CAMPOS_BRUTOS[i] as string];
        expect(c.valor).toBe(original);
        // null só vira "sem dados"; valor real nunca vira esse texto.
        expect(c.semDados).toBe(original === null);
        if (original !== null) expect(c.texto).toMatch(/^[\d.]+( \S+)?$/);
      });
    }
  });
});

describe('null de componente: "sem dados na janela", nunca zero', () => {
  it('Quina, dezena 55 (não saiu na janela): os 5 componentes de espaço sem dados', () => {
    const d = detalhe(tabela('quina'), 55);
    const por = Object.fromEntries(d.componentes.map((c) => [c.chave, c]));
    expect(por.frequencia?.texto).toBe('0 vezes'); // zero real, não null
    expect(por.frequencia?.semDados).toBe(false);
    for (const chave of [
      'maiorEspaco',
      'maiorEspacoOcorrencias',
      'menorEspaco',
      'menorEspacoOcorrencias',
      'somaVariacoes',
    ]) {
      expect(por[chave]?.texto).toBe('sem dados na janela');
      expect(por[chave]?.valor).toBeNull();
      expect(por[chave]?.semDados).toBe(true);
      expect(por[chave]?.texto).not.toMatch(/\d/);
      expect(por[chave]?.motivo?.length).toBeGreaterThan(0);
    }
    expect(por.espacoFinal?.texto).toBe('52 concursos'); // maior que a janela, e é válido
    expect(por.repeticoesTotal?.texto).toBe('0 vezes');
    expect(por.repeticaoAtual?.texto).toBe('0 concursos');
  });

  it('Quina, dezena 6 (2 aparições): só a soma das variações fica sem dados', () => {
    const d = detalhe(tabela('quina'), 6);
    expect(d.componentes.filter((c) => c.semDados).map((c) => c.chave)).toEqual(['somaVariacoes']);
  });

  const anulaveis = CHAVES_COMPONENTES.filter((c) => textosComponentes[c].motivoSemDados);

  it.each(anulaveis.filter((c) => c !== 'espacoFinal'))(
    'cada componente anulável (%s) mostra "sem dados na janela" quando null',
    (chave) => {
      const campo = CAMPOS_BRUTOS[CHAVES_COMPONENTES.indexOf(chave)] as string;
      const t = tabela('lotofacil', (j) => {
        j.dezenas[0][campo] = null;
      });
      const c = detalhe(t, 25).componentes.find((x) => x.chave === chave);
      expect(c?.semDados).toBe(true);
      expect(c?.valor).toBeNull();
      expect(c?.texto).toBe(textos.detalhe.semDados);
    },
  );

  it('só 6 componentes aceitam null; os outros 3 nunca têm motivo de ausência', () => {
    expect(anulaveis).toEqual([
      'maiorEspaco',
      'maiorEspacoOcorrencias',
      'menorEspaco',
      'menorEspacoOcorrencias',
      'somaVariacoes',
      'espacoFinal',
    ]);
  });

  it('espaço final nulo = nunca saiu (não é limitado pela janela, então não é "da janela")', () => {
    const t = tabela('lotofacil', (j) => {
      j.dezenas[0].espaco_final = null;
    });
    const c = detalhe(t, 25).componentes.find((x) => x.chave === 'espacoFinal');
    expect(c?.texto).toBe('-');
    expect(c?.semDados).toBe(true);
  });
});

describe('valores extremos', () => {
  it('números grandes ganham separador de milhar; 1 usa o singular', () => {
    const t = tabela('lotofacil', (j) => {
      j.dezenas[0].espaco_final = 12345;
      j.dezenas[0].frequencia = 1;
      j.dezenas[0].maior_espaco = 1;
    });
    const por = Object.fromEntries(detalhe(t, 25).componentes.map((c) => [c.chave, c.texto]));
    expect(por.espacoFinal).toBe('12.345 concursos');
    expect(por.frequencia).toBe('1 vez');
    expect(por.maiorEspaco).toBe('1 concurso');
  });

  it('índice negativo usa o menos tipográfico e zero fica 0,000', () => {
    const json = bruto('lotofacil');
    const [penultima, ultima] = json.dezenas.slice(-2);
    // Totais decrescentes (a validação exige): do positivo ao negativo, passando por zero.
    const t = tabela('lotofacil', (j) => {
      j.dezenas.forEach((d: Json, i: number) => (d.total = 24 - i));
      j.dezenas[23].total = 0;
      j.dezenas[24].total = -1.5;
    });
    expect(detalhe(t, ultima.dezena).indice).toBe('−1,500');
    expect(detalhe(t, penultima.dezena).indice).toBe('0,000');
  });

  it('janela 0 = histórico inteiro; janela 10 e 100 por extenso', () => {
    const j = (n: number) => tabela('lotofacil', (x) => void (x.janela = n));
    expect(detalhe(j(0), 25).janela).toBe('Janela dos componentes: histórico inteiro');
    expect(detalhe(j(10), 25).janela).toContain('últimos 10 concursos');
    expect(detalhe(j(100), 25).janela).toContain('últimos 100 concursos');
  });

  it('dezena que a tabela não tem = null', () => {
    expect(montarDetalheDaDezena(tabela('lotofacil'), 26)).toBeNull();
    expect(montarDetalheDaDezena(tabela('lotofacil'), 0)).toBeNull();
  });
});

describe('variação de posição', () => {
  it('subiu, caiu, ficou e sem concurso anterior', () => {
    expect(descreverVariacao(3)).toEqual({
      sentido: 'subiu',
      sinal: '▲ +3',
      texto: 'subiu 3 posições',
    });
    expect(descreverVariacao(-1)).toEqual({
      sentido: 'caiu',
      sinal: '▼ −1',
      texto: 'caiu 1 posição',
    });
    expect(descreverVariacao(0)).toEqual({
      sentido: 'igual',
      sinal: '■ 0',
      texto: 'manteve a posição',
    });
    expect(descreverVariacao(null)).toEqual({
      sentido: 'sem',
      sinal: '—',
      texto: 'sem concurso anterior para comparar',
    });
  });
});

describe('evidência: 5 graus x 2 direções', () => {
  const PROIBIDAS = /prev[êe]|vai sair|vão sair|palpite|aposte|garant|certeza|mais chance de sair/i;

  it('as 10 combinações têm rótulo, frase própria e segmentos de 1 a 5', () => {
    const frases = new Set<string>();
    for (const [i, grau] of GRAUS_EVIDENCIA.entries()) {
      for (const direcao of DIRECOES_EVIDENCIA) {
        const e = descreverEvidencia(grau, direcao);
        expect(e).not.toBeNull();
        expect(e?.segmentos).toBe(i + 1);
        expect(e?.rotulo).toContain(direcao === 'vies' ? 'viés' : 'acaso');
        expect(e?.frase.trim().length).toBeGreaterThan(30);
        expect(e?.frase).toMatch(/\.$/);
        // O texto explica; não é só "viés" ou "acaso".
        expect(e?.frase).toMatch(/acaso/);
        frases.add(e?.frase ?? '');
      }
    }
    expect(frases.size).toBe(10);
  });

  it('nenhuma frase sugere previsão do sorteio', () => {
    for (const grau of GRAUS_EVIDENCIA) {
      for (const direcao of DIRECOES_EVIDENCIA) {
        expect(descreverEvidencia(grau, direcao)?.frase).not.toMatch(PROIBIDAS);
      }
    }
    expect(textos.detalhe.evidenciaNota).not.toMatch(/prevê o resultado|vai sair/i);
  });

  it('viés e acaso dizem coisas diferentes', () => {
    expect(descreverEvidencia('muito_fraca', 'acaso')?.frase).toContain('Nada indica');
    expect(descreverEvidencia('fraca', 'vies')?.frase).toContain('foge do acaso');
    expect(descreverEvidencia('forte', 'vies')?.frase).toContain('minúsculo');
  });

  it('sem grau ou sem direção = sem evidência', () => {
    expect(descreverEvidencia(null, null)).toBeNull();
    expect(descreverEvidencia('fraca', null)).toBeNull();
    expect(descreverEvidencia(null, 'vies')).toBeNull();
    const t = tabela('lotofacil', (j) => {
      j.dezenas[0].evidencia_grau = null;
      j.dezenas[0].evidencia_direcao = null;
    });
    expect(detalhe(t, 25).evidencia).toBeNull();
  });

  it('o aviso permanente diz que o índice descreve o passado e não prevê', () => {
    expect(textos.detalhe.aviso).toContain('descreve o passado');
    expect(textos.detalhe.aviso).toContain('não prevê o sorteio');
  });
});
