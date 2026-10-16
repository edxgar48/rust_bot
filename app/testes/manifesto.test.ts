// @vitest-environment node
// Manifesto do PWA (8.1): campos exigidos, ícones realmente existentes em public/ com as
// dimensões declaradas, coerência com os tokens de cor e com o index.html.
import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { COR_FUNDO_CLARO, COR_TEMA_CLARO, COR_TEMA_ESCURO, manifesto } from '../manifesto';

const RAIZ = new URL('../', import.meta.url);
const ler = (caminho: string): string => readFileSync(new URL(caminho, RAIZ), 'utf-8');

/** Largura e altura do cabeçalho IHDR de um PNG (e confere a assinatura). */
function dimensoesPng(caminho: string): [number, number] {
  const b = readFileSync(new URL(caminho, RAIZ));
  expect(b.subarray(0, 8).toString('hex')).toBe('89504e470d0a1a0a');
  return [b.readUInt32BE(16), b.readUInt32BE(20)];
}

describe('manifesto', () => {
  it('traz os campos de instalação em português', () => {
    expect(manifesto.name).toBe('Fezinha');
    expect(manifesto.short_name).toBeTruthy();
    expect(manifesto.short_name?.length).toBeLessThanOrEqual(12);
    expect(manifesto.description).toMatch(/Lotofácil/);
    expect(manifesto.lang).toBe('pt-BR');
    expect(manifesto.display).toBe('standalone');
    expect(manifesto.orientation).toBe('any');
    expect(manifesto.categories?.length).toBeGreaterThan(0);
  });

  it('start_url e scope são relativos (sem URL final) e coerentes', () => {
    expect(manifesto.start_url).toBe('./');
    expect(manifesto.scope).toBe('./');
  });

  it('não inventa capturas de tela e a descrição diz que não prevê sorteios', () => {
    expect(manifesto).not.toHaveProperty('screenshots');
    expect(manifesto.description).toMatch(/Não prevê/);
  });

  it('cores do tema e do fundo batem com os tokens', () => {
    const tokens = ler('src/estilos/tokens.css').toLowerCase();
    expect(tokens).toContain(`--cor-superficie: ${COR_FUNDO_CLARO.toLowerCase()}`);
    expect(tokens).toContain(`--cor-cont: ${COR_TEMA_CLARO.toLowerCase()}`);
    expect(tokens).toContain(`--cor-cont: ${COR_TEMA_ESCURO.toLowerCase()}`);
    expect(manifesto.theme_color).toBe(COR_TEMA_CLARO);
    expect(manifesto.background_color).toBe(COR_FUNDO_CLARO);
  });

  it('ícones: 192 e 512, any e maskable, existem em public/ com o tamanho declarado', () => {
    const icones = manifesto.icons ?? [];
    for (const lado of [192, 512]) {
      for (const purpose of ['any', 'maskable']) {
        expect(
          icones.some(
            (i) => i.sizes === `${lado}x${lado}` && i.purpose === purpose && i.type === 'image/png',
          ),
        ).toBe(true);
      }
    }
    for (const icone of icones) {
      expect(icone.src).not.toMatch(/^[a-z]+:|^\//);
      expect(existsSync(new URL(`public/${icone.src}`, RAIZ))).toBe(true);
      if (icone.type === 'image/png') {
        const [l, a] = dimensoesPng(`public/${icone.src}`);
        expect(`${l}x${a}`).toBe(icone.sizes);
      }
    }
    expect(dimensoesPng('public/apple-touch-icon.png')).toEqual([180, 180]);
    expect(dimensoesPng('public/favicon-32.png')).toEqual([32, 32]);
    expect(existsSync(new URL('public/favicon.ico', RAIZ))).toBe(true);
  });
});

describe('index.html', () => {
  const html = ler('index.html');
  it('theme-color claro e escuro com media', () => {
    expect(html).toContain(
      `<meta name="theme-color" content="${COR_TEMA_CLARO}" media="(prefers-color-scheme: light)" />`,
    );
    expect(html).toContain(
      `<meta name="theme-color" content="${COR_TEMA_ESCURO}" media="(prefers-color-scheme: dark)" />`,
    );
  });
  it('metas do iOS e ícones apontam para arquivos que existem', () => {
    for (const m of [
      'apple-mobile-web-app-capable',
      'apple-mobile-web-app-status-bar-style',
      'apple-mobile-web-app-title',
    ]) {
      expect(html).toContain(`name="${m}"`);
    }
    const hrefs = [...html.matchAll(/<link rel="[^"]*icon[^"]*" href="([^"]+)"/g)].map(
      (m) => m[1] ?? '',
    );
    expect(hrefs).toEqual(
      expect.arrayContaining(['favicon-32.png', 'favicon.ico', 'apple-touch-icon.png']),
    );
    for (const h of hrefs) expect(existsSync(new URL(`public/${h}`, RAIZ))).toBe(true);
  });
});
