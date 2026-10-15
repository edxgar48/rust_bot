// Arquivo de backup dos jogos: formato, leitura defensiva e plano de importação.
import { describe, expect, it } from 'vitest';
import {
  FORMATO_ARQUIVO,
  MAXIMO_ITENS_ARQUIVO,
  TAMANHO_MAXIMO_ARQUIVO,
  VERSAO_ARQUIVO,
  lerArquivoJogos,
  nomeDoArquivo,
  planejarImportacao,
  serializarJogos,
} from './arquivo';
import type { JogoSalvo } from './jogo';

const AGORA = new Date('2026-10-15T12:00:00Z');

function jogo(parte: Partial<JogoSalvo> = {}): JogoSalvo {
  return {
    id: 'a1',
    loteria: 'quina',
    nome: null,
    dezenas: [1, 2, 3, 4, 5],
    criadoEm: '2026-10-14T10:00:00.000Z',
    ...parte,
  };
}

function ids(prefixo = 'novo'): () => string {
  let n = 0;
  return () => `${prefixo}${++n}`;
}

describe('serializar e ler', () => {
  it('ida e volta preserva todos os jogos; é JSON versionado com LF no fim', () => {
    const jogos = [jogo({ nome: 'Meu jogo' }), jogo({ id: 'b2', dezenas: [6, 7, 8, 9, 10] })];
    const texto = serializarJogos(jogos, AGORA);
    expect(texto.endsWith('}\n')).toBe(true);
    expect(texto).not.toContain('\r');
    const bruto = JSON.parse(texto) as Record<string, unknown>;
    expect(bruto.formato).toBe(FORMATO_ARQUIVO);
    expect(bruto.versao).toBe(VERSAO_ARQUIVO);
    expect(bruto.exportadoEm).toBe(AGORA.toISOString());
    const lido = lerArquivoJogos(texto);
    expect(lido).toEqual({ ok: true, itens: JSON.parse(JSON.stringify(jogos)) });
  });

  it('é determinístico: mesma entrada, mesmos bytes', () => {
    expect(serializarJogos([jogo()], AGORA)).toBe(serializarJogos([jogo()], AGORA));
  });

  it('nome do arquivo leva a data', () => {
    expect(nomeDoArquivo(AGORA)).toBe('jogos-forca-loterias-2026-10-15.json');
  });

  it('ignora campos desconhecidos no envelope e aceita BOM', () => {
    const texto = `${String.fromCharCode(0xfeff)}${JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 1, jogos: [], futuro: { x: 1 } })}`;
    expect(lerArquivoJogos(texto)).toEqual({ ok: true, itens: [] });
  });

  it.each([
    ['malformado', '{"formato": '],
    ['malformado', ''],
    ['malformado', 'nao é json'],
    ['formato', '[]'],
    ['formato', 'null'],
    ['formato', '42'],
    ['formato', JSON.stringify({ versao: 1, jogos: [] })],
    ['formato', JSON.stringify({ formato: 'outro', versao: 1, jogos: [] })],
    ['formato', JSON.stringify({ formato: FORMATO_ARQUIVO, jogos: [] })],
    ['formato', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: '1', jogos: [] })],
    ['formato', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 0, jogos: [] })],
    ['formato', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 1 })],
    ['formato', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 1, jogos: {} })],
    ['versao', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 2, jogos: [] })],
  ])('recusa (%s): %s', (motivo, texto) => {
    expect(lerArquivoJogos(texto)).toEqual({ ok: false, motivo });
  });

  it('recusa arquivo grande demais', () => {
    expect(lerArquivoJogos(' '.repeat(TAMANHO_MAXIMO_ARQUIVO + 1))).toEqual({
      ok: false,
      motivo: 'grande',
    });
  });
});

describe('teto de itens', () => {
  const arquivoCom = (n: number): string =>
    JSON.stringify({
      formato: FORMATO_ARQUIVO,
      versao: 1,
      jogos: Array.from({ length: n }, () => 0),
    });

  it('aceita até o teto e recusa o arquivo inteiro acima dele', () => {
    expect(MAXIMO_ITENS_ARQUIVO).toBe(1000);
    const ok = lerArquivoJogos(arquivoCom(MAXIMO_ITENS_ARQUIVO));
    expect(ok.ok && ok.itens.length).toBe(MAXIMO_ITENS_ARQUIVO);
    expect(lerArquivoJogos(arquivoCom(MAXIMO_ITENS_ARQUIVO + 1))).toEqual({
      ok: false,
      motivo: 'grande',
    });
  });
});

describe('planejarImportacao', () => {
  it('id ocupado no armazenamento (registro inválido que a lista não mostra) ganha id novo', () => {
    const plano = planejarImportacao([], [jogo({ id: 'ruim' })], ids('g'), ['ruim']);
    expect(plano.novos.map((j) => j.id)).toEqual(['g1']);
  });

  it('importa jogos válidos e conta os inválidos', () => {
    const itens = [
      jogo({ id: 'x1' }),
      { ...jogo({ id: 'x2' }), dezenas: [1, 2] },
      'lixo',
      null,
      { ...jogo({ id: 'x3' }), loteria: 'timemania' },
      jogo({ id: 'x4', dezenas: [9, 8, 7, 6, 5] }),
    ];
    const plano = planejarImportacao([], itens, ids());
    expect(plano.novos.map((j) => j.id)).toEqual(['x1', 'x4']);
    expect(plano.novos[1]!.dezenas).toEqual([5, 6, 7, 8, 9]);
    expect(plano.invalidos).toBe(4);
    expect(plano.ignorados).toBe(0);
  });

  it('não duplica jogo idêntico: já salvo, ou repetido no próprio arquivo', () => {
    const existente = jogo({ id: 'e1', nome: 'Antigo' });
    const itens = [
      jogo({ id: 'n1', nome: 'Outro nome', dezenas: [5, 4, 3, 2, 1] }), // igual ao existente
      jogo({ id: 'n2', dezenas: [10, 11, 12, 13, 14] }),
      jogo({ id: 'n3', dezenas: [10, 11, 12, 13, 14], nome: 'repetido' }),
      jogo({ id: 'n4', loteria: 'megasena', dezenas: [1, 2, 3, 4, 5, 6] }),
    ];
    const plano = planejarImportacao([existente], itens, ids());
    expect(plano.novos.map((j) => j.id)).toEqual(['n2', 'n4']);
    expect(plano.ignorados).toBe(2);
    expect(plano.invalidos).toBe(0);
  });

  it('id que já existe com outro conteúdo ganha id novo e nunca sobrescreve', () => {
    const existente = jogo({ id: 'igual', dezenas: [1, 2, 3, 4, 5] });
    const plano = planejarImportacao(
      [existente],
      [jogo({ id: 'igual', dezenas: [6, 7, 8, 9, 10] })],
      ids('gerado'),
    );
    expect(plano.novos).toHaveLength(1);
    expect(plano.novos[0]!.id).toBe('gerado1');
    expect(plano.novos[0]!.dezenas).toEqual([6, 7, 8, 9, 10]);
  });

  it('dois itens do arquivo com o mesmo id e conteúdo diferente: ambos entram, com ids distintos', () => {
    const plano = planejarImportacao(
      [],
      [jogo({ id: 'z' }), jogo({ id: 'z', dezenas: [6, 7, 8, 9, 10] })],
      ids(),
    );
    expect(new Set(plano.novos.map((j) => j.id)).size).toBe(2);
  });

  it('arquivo vazio: nada entra', () => {
    expect(planejarImportacao([jogo()], [], ids())).toEqual({
      novos: [],
      ignorados: 0,
      invalidos: 0,
    });
  });
});
