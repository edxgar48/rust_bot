// Roteamento por hash ligado à barra de navegação: aba ativa por aria-current
// e botão voltar do navegador. Sem rede: fetch falha, o app mostra "sem dados".
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App from './App.svelte';

let montado: Record<string, unknown> | null = null;

const esperar = (ms = 30) => new Promise((ok) => setTimeout(ok, ms));
const abaAtiva = () =>
  document.querySelector('nav button[aria-current="page"]')?.textContent?.trim() ?? null;
const clicar = (rotulo: string) =>
  Array.from(document.querySelectorAll('nav button'))
    .find((b) => b.textContent?.includes(rotulo))
    ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));

function montarApp(hash: string): void {
  location.hash = hash;
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(App, { target: alvo });
  flushSync();
}

beforeEach(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => {
      throw new TypeError('Failed to fetch');
    }),
  );
});

afterEach(async () => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  vi.unstubAllGlobals();
  location.hash = '';
  await esperar();
});

describe('App: navegação por hash', () => {
  it('sem hash abre na Início e a aba ativa tem aria-current', async () => {
    montarApp('');
    await esperar();
    expect(abaAtiva()).toBe('Início');
    expect(document.querySelectorAll('nav [aria-current="page"]')).toHaveLength(1);
    expect(document.querySelector('main h2')?.textContent).toBe('Resultados');
  });

  it.each([
    ['#/forca', 'Força'],
    ['#/conferir', 'Conferir'],
    ['#/estatisticas', 'Estatísticas'],
    ['#/entenda', 'Entenda'],
  ])('abrir direto em %s ativa a aba %s e mostra o placeholder', async (hash, aba) => {
    montarApp(hash);
    await esperar();
    expect(abaAtiva()).toBe(aba);
    expect(document.querySelector('main h2')?.textContent).toBe(aba);
    expect(document.querySelector('main')?.textContent).toContain('em construção');
  });

  it('hash desconhecido cai na Início', async () => {
    montarApp('#/nada');
    await esperar();
    expect(abaAtiva()).toBe('Início');
  });

  it('tocar numa aba muda o hash e a tela; voltar do navegador volta a aba anterior', async () => {
    montarApp('#/inicio');
    await esperar();
    clicar('Força');
    await esperar();
    expect(location.hash).toBe('#/forca');
    expect(abaAtiva()).toBe('Força');
    clicar('Entenda');
    await esperar();
    expect(location.hash).toBe('#/entenda');
    expect(abaAtiva()).toBe('Entenda');

    history.back();
    await esperar(100);
    expect(location.hash).toBe('#/forca');
    expect(abaAtiva()).toBe('Força');
    history.back();
    await esperar(100);
    expect(abaAtiva()).toBe('Início');
  });
});
