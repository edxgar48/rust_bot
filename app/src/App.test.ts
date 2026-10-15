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

  it('abrir direto em #/estatisticas ativa a aba Estatísticas e mostra a tela (sem rede: sem dados)', async () => {
    montarApp('#/estatisticas');
    await esperar(60);
    expect(abaAtiva()).toBe('Estatísticas');
    expect(document.querySelector('main h2')?.textContent).toBe('Estatísticas');
    expect(document.querySelector('main')?.textContent).toContain('Elas não preveem o sorteio');
    expect(document.querySelector('main')?.textContent).not.toContain('em construção');
  });

  it('abrir direto em #/conferir ativa a aba Conferir e mostra Conferir jogo (sem rede: sem dados)', async () => {
    montarApp('#/conferir');
    await esperar(60);
    expect(abaAtiva()).toBe('Conferir');
    expect(document.querySelector('main h2')?.textContent).toBe('Conferir jogo');
    expect(document.querySelector('main')?.textContent).toContain('Jogue com responsabilidade');
    const alertasJogo = Array.from(document.querySelectorAll('main [role="alert"]')).map(
      (a) => a.textContent,
    );
    expect(alertasJogo.some((t) => t?.includes('Ainda sem dados'))).toBe(true);
  });

  it('a seção Histórico da aba Conferir abre por ?secao=historico e a troca pelo seletor muda o hash', async () => {
    montarApp('#/conferir?secao=historico');
    await esperar(60);
    expect(abaAtiva()).toBe('Conferir');
    expect(document.querySelector('main h2')?.textContent).toBe('Histórico de concursos');
    const radios = Array.from(document.querySelectorAll<HTMLElement>('[role="radio"]'));
    radios.find((b) => b.textContent?.includes('Conferir jogo'))?.click();
    await esperar(60);
    expect(location.hash).toContain('#/conferir?loteria=');
    expect(location.hash).not.toContain('secao=historico');
    expect(document.querySelector('main h2')?.textContent).toBe('Conferir jogo');
    expect(abaAtiva()).toBe('Conferir');
  });

  it('o Histórico (seção) mostra o aviso e o estado sem dados', async () => {
    montarApp('#/conferir?secao=historico');
    await esperar(60);
    expect(document.querySelector('main')?.textContent).not.toContain('em construção');
    expect(document.querySelector('main')?.textContent).toContain('não prevê o sorteio');
    const alertas = Array.from(document.querySelectorAll('main [role="alert"]')).map(
      (a) => a.textContent,
    );
    expect(alertas.some((t) => t?.includes('Ainda sem dados'))).toBe(true);
  });

  it('abrir direto em #/entenda mostra a tela Entenda (sem rede: sem tabelas, sem quebrar)', async () => {
    montarApp('#/entenda');
    await esperar(60);
    expect(abaAtiva()).toBe('Entenda');
    expect(document.querySelectorAll('h1')).toHaveLength(1);
    expect(document.querySelector('main h2')?.textContent).toBe('Entenda');
    const t = document.querySelector('main')?.textContent ?? '';
    expect(t).toContain('Este app não prevê sorteios');
    expect(t).toContain('Jogue com responsabilidade');
    expect(t).toContain('tabela de força ainda não baixada');
    expect(t).not.toContain('em construção');
  });

  it('abrir direto em #/forca ativa a aba Força e mostra a Tabela de força (sem rede: sem dados)', async () => {
    montarApp('#/forca');
    await esperar(60);
    expect(abaAtiva()).toBe('Força');
    expect(document.querySelector('main h2')?.textContent).toBe('Tabela de força');
    expect(document.querySelector('main')?.textContent).not.toContain('em construção');
    expect(document.querySelector('main')?.textContent).toContain('Ele não prevê o sorteio.');
    const alertas = Array.from(document.querySelectorAll('main [role="alert"]')).map(
      (a) => a.textContent,
    );
    expect(alertas.some((t) => t?.includes('Ainda sem dados'))).toBe(true);
  });

  it('#/forca?loteria=quina abre na Quina', async () => {
    montarApp('#/forca?loteria=quina');
    await esperar(60);
    expect(abaAtiva()).toBe('Força');
    const marcada = document.querySelector('main [role="radio"][aria-checked="true"]');
    expect(marcada?.textContent).toContain('Quina');
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
