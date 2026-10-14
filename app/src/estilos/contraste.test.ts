// Verificação automática de contraste (WCAG 2.x) dos pares de tokens de tokens.css.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// readFileSync: o Vitest esvazia CSS importado (inclusive com ?raw).
const css = readFileSync(resolve(__dirname, 'tokens.css'), 'utf-8');

type Tema = 'claro' | 'escuro';

/** Tokens de cor (--cor-*) declarados no corpo de um bloco `seletor { ... }`. */
function tokensDoBloco(seletor: string): Map<string, string> {
  const inicio = css.indexOf(seletor);
  if (inicio < 0) throw new Error(`Bloco não encontrado: ${seletor}`);
  const corpo = css.slice(css.indexOf('{', inicio) + 1, css.indexOf('}', inicio));
  const mapa = new Map<string, string>();
  for (const m of corpo.matchAll(/--(cor-[\w-]+):\s*(#[0-9a-f]{6})\s*;/gi)) mapa.set(m[1]!, m[2]!);
  return mapa;
}

const tokens: Record<Tema, Map<string, string>> = {
  claro: tokensDoBloco(":root,\n[data-tema='claro']"),
  escuro: tokensDoBloco("\n[data-tema='escuro'] {"),
};
const escuroNaMedia = tokensDoBloco(":root:not([data-tema='claro'])");

function luminancia(hex: string): number {
  const canais = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * canais[0]! + 0.7152 * canais[1]! + 0.0722 * canais[2]!;
}

function razaoContraste(a: string, b: string): number {
  const [maior, menor] = [luminancia(a), luminancia(b)].sort((x, y) => y - x);
  return (maior! + 0.05) / (menor! + 0.05);
}

// [texto, fundo]
const paresTexto: [string, string][] = [
  ['cor-sobre-primaria', 'cor-primaria'],
  ['cor-sobre-primaria-cont', 'cor-primaria-cont'],
  ['cor-sobre-secundaria-cont', 'cor-secundaria-cont'],
  ['cor-sobre-erro', 'cor-erro'],
  ['cor-sobre-erro-cont', 'cor-erro-cont'],
  ['cor-texto', 'cor-superficie'],
  ['cor-texto', 'cor-cont-baixo'],
  ['cor-texto', 'cor-cont'],
  ['cor-texto', 'cor-cont-altissimo'],
  ['cor-texto-var', 'cor-superficie'],
  ['cor-texto-var', 'cor-cont'],
  ['cor-texto-var', 'cor-cont-altissimo'],
  ['cor-primaria', 'cor-superficie'],
  ['cor-primaria', 'cor-cont'],
  ['cor-erro', 'cor-superficie'],
  ...[1, 2, 3, 4, 5].map((f): [string, string] => [`cor-sobre-faixa-${f}`, `cor-faixa-${f}`]),
];

// [elemento gráfico, fundo]: mínimo 3:1 (WCAG 1.4.11)
const paresGraficos: [string, string][] = [
  ['cor-contorno', 'cor-superficie'],
  ['cor-contorno', 'cor-cont-altissimo'],
  ['cor-primaria', 'cor-superficie'],
];

describe('contraste dos tokens', () => {
  it('leu os tokens nos dois temas', () => {
    expect(tokens.claro.size).toBeGreaterThan(20);
    expect(tokens.claro.size).toBe(tokens.escuro.size);
  });

  it('o escuro automático (media query) é idêntico ao escuro forçado', () => {
    expect([...escuroNaMedia]).toEqual([...tokens.escuro]);
  });

  for (const tema of ['claro', 'escuro'] as const) {
    it.each(paresTexto)(`${tema}: %s sobre %s >= 4,5:1`, (texto, fundo) => {
      const t = tokens[tema].get(texto);
      const f = tokens[tema].get(fundo);
      expect(t, texto).toBeDefined();
      expect(f, fundo).toBeDefined();
      expect(razaoContraste(t!, f!)).toBeGreaterThanOrEqual(4.5);
    });

    it.each(paresGraficos)(`${tema}: %s sobre %s >= 3:1 (não texto)`, (frente, fundo) => {
      expect(
        razaoContraste(tokens[tema].get(frente)!, tokens[tema].get(fundo)!),
      ).toBeGreaterThanOrEqual(3);
    });
  }

  it('calcula a razão conhecida preto/branco = 21', () => {
    expect(razaoContraste('#000000', '#ffffff')).toBeCloseTo(21, 5);
  });
});
