// @vitest-environment node
// PDF da tabela G-1: ordem das linhas, paginação e nome do arquivo. Fixture real (snapshot-g1).
import { describe, expect, it } from 'vitest';
import { lerFixtureTexto } from '../../testes/servidor-falso';
import { validarRankingPosicoes, validarTabelaGeral } from '../dados/validacao';
import { juntarG1 } from './forca';
import { gerarPdfG1, linhasDoPdf, nomeDoArquivoG1 } from './pdf-g1';

const tabela = validarTabelaGeral(
  JSON.parse(lerFixtureTexto('snapshot-g1/lotofacil/tabela_geral.json')),
  'lotofacil',
);
const ranking = validarRankingPosicoes(
  JSON.parse(lerFixtureTexto('snapshot-g1/lotofacil/ranking_posicoes.json')),
  'lotofacil',
);
const g1 = juntarG1(tabela, ranking);
if (g1 === null) throw new Error('fixture sem junção G-1');
const comoTexto = (b: Uint8Array) => Array.from(b, (x) => String.fromCharCode(x)).join('');

describe('pdf-g1', () => {
  it('nomeia o arquivo pela loteria e pelo concurso', () => {
    expect(nomeDoArquivoG1('quina', 6900)).toBe('tabela-g1-quina-concurso-6900.pdf');
  });

  it('as linhas vêm em ordem de G-1, 1 a N, cada dezena uma vez', () => {
    const linhas = linhasDoPdf(tabela, g1);
    expect(linhas.map((l) => l.g1)).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
    expect(new Set(linhas.map((l) => l.dezena)).size).toBe(25);
    for (const l of linhas) {
      expect(l.forca).toBe(tabela.dezenas.find((d) => d.dezena === l.dezena)?.posicao);
    }
  });

  it('gera um PDF de uma página para a Lotofácil, com título, concurso e aviso', () => {
    const texto = comoTexto(
      gerarPdfG1({
        nomeLoteria: 'Lotofácil',
        tabela,
        g1,
        geradoEm: new Date('2026-11-05T15:00:00Z'),
      }),
    );
    expect(texto.startsWith('%PDF-')).toBe(true);
    expect(texto).toContain('/Count 1');
    expect(texto).toContain(`Tabela G-1 \xb7 Lotof\xe1cil`);
    expect(texto).toContain(`Para o concurso ${tabela.concurso}`);
    expect(texto).toContain('n\xe3o prev\xea o sorteio');
    expect(texto).toContain('P\xe1gina 1 de 1');
  });

  const dezenasN = (n: number) =>
    Array.from({ length: n }, (_, i) => ({ ...tabela.dezenas[0]!, dezena: i + 1, posicao: i + 1 }));
  const pdfDeN = (n: number, sorteadas?: ReadonlySet<number>) => {
    const dezenas = dezenasN(n);
    const mapa = new Map(dezenas.map((d) => [d.dezena, { g1: d.dezena, variacao: null }]));
    return comoTexto(
      gerarPdfG1({
        nomeLoteria: 'x',
        tabela: { ...tabela, dezenas },
        g1: mapa,
        ...(sorteadas ? { posicoesSorteadas: sorteadas } : {}),
        geradoEm: new Date(),
      }),
    );
  };

  it('com 80 dezenas ocupa duas páginas e cada linha cabe acima do rodapé', () => {
    const texto = pdfDeN(80);
    expect(texto).toContain('/Count 2');
    expect(texto).toContain('Página 2 de 2');
    expect(pdfDeN(70)).toContain('/Count 1');
    expect(pdfDeN(71)).toContain('/Count 2');
    expect(pdfDeN(60)).toContain('/Count 1');
  });

  it('contorna só as posições de força sorteadas e traz a legenda', () => {
    const contornos = (t: string) => (t.match(/ re S/g) ?? []).length;
    expect(contornos(pdfDeN(25))).toBe(0);
    const t = pdfDeN(25, new Set([3, 7, 20]));
    // Um contorno por posição sorteada mais o da legenda.
    expect(contornos(t)).toBe(4);
    expect(t).toContain('Borda: posi');
  });
});
