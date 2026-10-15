// Validação pura dos jogos salvos: quantidades por loteria, dezenas, nome, registro.
import { describe, expect, it } from 'vitest';
import type { Slug } from '../dados/tipos';
import {
  TAMANHO_MAXIMO_NOME,
  chaveDoJogo,
  criarJogo,
  ehDataIso,
  gerarId,
  limparNome,
  ordenarJogos,
  validarDezenas,
  validarJogoSalvo,
  validarNome,
  type JogoSalvo,
} from './jogo';

const seq = (n: number, de = 1): number[] => Array.from({ length: n }, (_, i) => de + i);
const AGORA = new Date('2026-10-15T12:00:00Z');

function jogo(parte: Partial<JogoSalvo> = {}): JogoSalvo {
  return {
    id: 'a1',
    loteria: 'quina',
    nome: null,
    dezenas: [1, 2, 3, 4, 5],
    criadoEm: AGORA.toISOString(),
    ...parte,
  };
}

describe('validarDezenas: quantidade e faixa por loteria', () => {
  const limites: [Slug, number, number, number][] = [
    ['lotofacil', 15, 20, 25],
    ['megasena', 6, 20, 60],
    ['quina', 5, 15, 80],
  ];

  it.each(limites)('%s aceita de %i a %i dezenas e recusa fora disso', (slug, min, max) => {
    expect(validarDezenas(slug, seq(min - 1)).ok).toBe(false);
    expect(validarDezenas(slug, seq(min))).toEqual({ ok: true, valor: seq(min) });
    expect(validarDezenas(slug, seq(max))).toEqual({ ok: true, valor: seq(max) });
    expect(validarDezenas(slug, seq(max + 1))).toMatchObject({ ok: false, motivo: 'quantidade' });
    expect(validarDezenas(slug, [])).toMatchObject({ ok: false, motivo: 'quantidade' });
  });

  it.each(limites)(
    '%s: dezena 0 e dezena acima do total são recusadas',
    (slug, min, _max, total) => {
      expect(validarDezenas(slug, [0, ...seq(min - 1, 1)])).toMatchObject({
        ok: false,
        motivo: 'dezenas',
      });
      expect(validarDezenas(slug, [...seq(min - 1), total + 1])).toMatchObject({
        ok: false,
        motivo: 'dezenas',
      });
      expect(validarDezenas(slug, [...seq(min - 1), total]).ok).toBe(true);
    },
  );

  it('recusa repetida, não inteira, texto, nulo e não-lista', () => {
    expect(validarDezenas('quina', [1, 2, 3, 4, 4])).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', [1, 2, 3, 4, 5.5])).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', [1, 2, 3, 4, '5'])).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', [1, 2, 3, 4, null])).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', [1, 2, 3, 4, Number.NaN])).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', '1,2,3,4,5')).toMatchObject({ motivo: 'dezenas' });
    expect(validarDezenas('quina', null)).toMatchObject({ motivo: 'dezenas' });
  });

  it('devolve em ordem crescente e não altera a entrada', () => {
    const entrada = [40, 5, 12, 33, 71];
    expect(validarDezenas('quina', entrada)).toEqual({ ok: true, valor: [5, 12, 33, 40, 71] });
    expect(entrada).toEqual([40, 5, 12, 33, 71]);
  });
});

describe('nome', () => {
  it('limparNome: apara, junta espaços, tira controle; vazio vira null', () => {
    expect(limparNome('  Jogo   da\tsemana \n')).toBe('Jogo da semana');
    expect(limparNome('   ')).toBeNull();
    expect(limparNome('')).toBeNull();
  });

  it('validarNome: null/ausente = sem nome; limite em caracteres, não em unidades UTF-16', () => {
    expect(validarNome(undefined)).toEqual({ ok: true, valor: null });
    expect(validarNome(null)).toEqual({ ok: true, valor: null });
    expect(validarNome('a'.repeat(TAMANHO_MAXIMO_NOME))).toMatchObject({ ok: true });
    expect(validarNome('a'.repeat(TAMANHO_MAXIMO_NOME + 1))).toEqual({ ok: false, motivo: 'nome' });
    // 40 emojis = 80 unidades UTF-16, mas 40 caracteres: cabe.
    expect(validarNome('😀'.repeat(TAMANHO_MAXIMO_NOME))).toMatchObject({ ok: true });
    expect(validarNome(42)).toEqual({ ok: false, motivo: 'nome' });
  });
});

describe('validarJogoSalvo e criarJogo', () => {
  it('aceita um registro bom e descarta campos desconhecidos', () => {
    const r = validarJogoSalvo({ ...jogo({ nome: ' Meu  jogo ' }), extra: 1, outro: { a: 2 } });
    expect(r).toEqual({ ok: true, valor: jogo({ nome: 'Meu jogo' }) });
    expect(Object.keys((r as { valor: object }).valor).sort()).toEqual([
      'criadoEm',
      'dezenas',
      'id',
      'loteria',
      'nome',
    ]);
  });

  it('nome ausente vale null', () => {
    const semNome: Record<string, unknown> = { ...jogo() };
    delete semNome.nome;
    expect(validarJogoSalvo(semNome)).toEqual({ ok: true, valor: jogo() });
  });

  it.each([
    ['forma', null],
    ['forma', [1, 2]],
    ['forma', 'texto'],
    ['id', { ...jogo(), id: '' }],
    ['id', { ...jogo(), id: 'tem espaço' }],
    ['id', { ...jogo(), id: 12 }],
    ['loteria', { ...jogo(), loteria: 'timemania' }],
    ['loteria', { ...jogo(), loteria: undefined }],
    ['dezenas', { ...jogo(), dezenas: [1, 2, 3, 4, 99] }],
    ['quantidade', { ...jogo(), dezenas: [1, 2, 3] }],
    ['nome', { ...jogo(), nome: 'x'.repeat(41) }],
    ['nome', { ...jogo(), nome: 7 }],
    ['criado_em', { ...jogo(), criadoEm: 'ontem' }],
    ['criado_em', { ...jogo(), criadoEm: '2026-02-31T00:00:00Z' }],
    ['criado_em', { ...jogo(), criadoEm: 20261015 }],
  ])('recusa com motivo %s', (motivo, valor) => {
    expect(validarJogoSalvo(valor)).toEqual({ ok: false, motivo });
  });

  it('criarJogo valida na entrada, ordena e usa a data dada', () => {
    const r = criarJogo({ loteria: 'quina', dezenas: [5, 4, 3, 2, 1], nome: '  ' }, 'id1', AGORA);
    expect(r).toEqual({ ok: true, valor: jogo({ id: 'id1' }) });
    expect(criarJogo({ loteria: 'quina', dezenas: [1, 2] }, 'id1', AGORA)).toEqual({
      ok: false,
      motivo: 'quantidade',
    });
    expect(
      criarJogo({ loteria: 'quina', dezenas: [1, 2, 3, 4, 5], nome: 'x'.repeat(50) }),
    ).toMatchObject({ ok: false, motivo: 'nome' });
  });

  it('ehDataIso exige fuso e data real', () => {
    expect(ehDataIso('2026-10-15T12:00:00Z')).toBe(true);
    expect(ehDataIso('2026-10-15T12:00:00.123+03:00')).toBe(true);
    expect(ehDataIso('2026-10-15T12:00:00')).toBe(false);
    expect(ehDataIso('2026-10-15')).toBe(false);
  });

  it('gerarId produz ids válidos e diferentes', () => {
    const a = gerarId();
    expect(a).not.toBe(gerarId());
    expect(validarJogoSalvo(jogo({ id: a })).ok).toBe(true);
  });
});

describe('criadoEm com fusos diferentes', () => {
  it('é normalizado para UTC e a ordem segue o instante, não o texto', () => {
    // 10:00-03:00 = 13:00Z (mais tarde) ; 12:00Z é anterior, embora "12" > "10" como texto.
    const tarde = validarJogoSalvo(jogo({ id: 'tarde', criadoEm: '2026-10-14T10:00:00-03:00' }));
    const cedo = validarJogoSalvo(jogo({ id: 'cedo', criadoEm: '2026-10-14T12:00:00Z' }));
    const mesmo = validarJogoSalvo(
      jogo({ id: 'mesmo', criadoEm: '2026-10-14T13:00:00.000+00:00' }),
    );
    if (!tarde.ok || !cedo.ok || !mesmo.ok) throw new Error('fixture');
    expect(tarde.valor.criadoEm).toBe('2026-10-14T13:00:00.000Z');
    expect(cedo.valor.criadoEm).toBe('2026-10-14T12:00:00.000Z');
    expect(mesmo.valor.criadoEm).toBe(tarde.valor.criadoEm);
    expect(ordenarJogos([cedo.valor, tarde.valor]).map((j) => j.id)).toEqual(['tarde', 'cedo']);
  });

  it('deslocamento que empurra o ano para fora de 4 dígitos é recusado', () => {
    expect(validarJogoSalvo(jogo({ criadoEm: '9999-12-31T23:59:59-03:00' }))).toEqual({
      ok: false,
      motivo: 'criado_em',
    });
  });
});

describe('chave e ordem', () => {
  it('a chave ignora o nome, o id e a ordem das dezenas, mas não a loteria', () => {
    expect(chaveDoJogo(jogo({ nome: 'a', id: 'x' }))).toBe(
      chaveDoJogo(jogo({ nome: 'b', id: 'y', dezenas: [5, 4, 3, 2, 1] })),
    );
    expect(chaveDoJogo(jogo())).not.toBe(chaveDoJogo(jogo({ loteria: 'megasena' })));
    expect(chaveDoJogo(jogo())).not.toBe(chaveDoJogo(jogo({ dezenas: [1, 2, 3, 4, 6] })));
  });

  it('ordenarJogos: mais recentes primeiro, empate pelo id, sem mexer na entrada', () => {
    const a = jogo({ id: 'a', criadoEm: '2026-10-01T00:00:00Z' });
    const b = jogo({ id: 'b', criadoEm: '2026-10-03T00:00:00Z' });
    const c = jogo({ id: 'c', criadoEm: '2026-10-03T00:00:00Z' });
    const entrada = [a, c, b];
    expect(ordenarJogos(entrada).map((j) => j.id)).toEqual(['b', 'c', 'a']);
    expect(entrada.map((j) => j.id)).toEqual(['a', 'c', 'b']);
  });
});
