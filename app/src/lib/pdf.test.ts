// Gerador de PDF: estrutura válida (xref aponta para os objetos), texto escapado, determinismo.
import { describe, expect, it } from 'vitest';
import { larguraDoTexto, montarPdf, PaginaPdf } from './pdf';

const meta = { titulo: 'Título (teste)', criadoEm: new Date('2026-11-05T12:00:00Z') };
const comoTexto = (b: Uint8Array) => Array.from(b, (x) => String.fromCharCode(x)).join('');

describe('montarPdf', () => {
  it('gera cabeçalho, uma página por PaginaPdf e fim de arquivo', () => {
    const a = new PaginaPdf();
    a.texto(10, 20, 12, 'Olá');
    const texto = comoTexto(montarPdf([a, new PaginaPdf()], meta));
    expect(texto.startsWith('%PDF-1.4\n')).toBe(true);
    expect(texto.endsWith('%%EOF\n')).toBe(true);
    expect(texto).toContain('/Count 2');
    expect(texto.match(/\/Type \/Page /g)).toHaveLength(2);
  });

  it('a tabela xref aponta para o início de cada objeto', () => {
    const p = new PaginaPdf();
    p.texto(1, 2, 10, 'x');
    const texto = comoTexto(montarPdf([p], meta));
    const inicio = Number(/startxref\n(\d+)\n/.exec(texto)?.[1]);
    expect(texto.slice(inicio, inicio + 4)).toBe('xref');
    const entradas = [...texto.slice(inicio).matchAll(/^(\d{10}) 00000 n $/gm)];
    expect(entradas).toHaveLength(7);
    entradas.forEach((e, i) => {
      const pos = Number(e[1]);
      expect(texto.slice(pos, pos + `${i + 1} 0 obj`.length)).toBe(`${i + 1} 0 obj`);
    });
  });

  it('escapa parênteses e barra, troca o sinal de menos e codifica acentos em um byte', () => {
    const p = new PaginaPdf();
    p.texto(0, 0, 10, String.raw`a(b)\c −1 Força º`);
    const bytes = montarPdf([p], meta);
    const texto = comoTexto(bytes);
    expect(texto).toContain(String.raw`(a\(b\)\\c -1 ` + 'Força º)');
    // Todos os caracteres cabem em um byte.
    expect(bytes.length).toBe(texto.length);
  });

  it('o /Length do conteúdo confere com o fluxo', () => {
    const p = new PaginaPdf();
    p.linha(0, 0, 10, 10);
    p.retanguloCinza(0, 0, 5, 5, 0.9);
    const texto = comoTexto(montarPdf([p], meta));
    const m = /\/Length (\d+) >>\nstream\n([\s\S]*?)\nendstream/.exec(texto);
    expect(Number(m?.[1])).toBe(m?.[2]?.length);
  });

  it('é determinístico', () => {
    const p = new PaginaPdf();
    p.textoDireita(100, 50, 10, '12');
    expect(montarPdf([p], meta)).toEqual(montarPdf([p], meta));
  });
});

describe('larguraDoTexto', () => {
  it('cresce com o tamanho e com o número de dígitos', () => {
    expect(larguraDoTexto('12', 10)).toBeCloseTo(11.12, 2);
    expect(larguraDoTexto('123', 10)).toBeGreaterThan(larguraDoTexto('12', 10));
  });
});
