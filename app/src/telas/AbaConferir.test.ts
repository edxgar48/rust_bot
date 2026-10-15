// Aba Conferir como um todo (6.3): as três seções, o caminho salvar -> Meus jogos ->
// Conferir no volante, e a sobrevivência dos jogos a recarregar a página.
import { IDBFactory } from 'fake-indexeddb';
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { garantirArquivos } from '../dados/sob-demanda';
import { RepositorioIndexedDB } from '../jogos/repositorio-idb';
import { textos } from '../lib/textos';
import Conferir from './Conferir.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

function criarFonte() {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: new ServidorFalso().fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  return {
    lerCache: (o: Parameters<typeof cliente.lerCache>[0]) => cliente.lerCache(o),
    garantir: (o: Parameters<typeof garantirArquivos>[1]) => garantirArquivos(cliente, o),
  };
}

let montado: Record<string, unknown> | null = null;
function montar(jogos: RepositorioIndexedDB): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Conferir as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte: criarFonte(), jogos },
  });
  flushSync();
  return alvo;
}
function desmontar(): void {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
}

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const pausa = (ms = 30) => new Promise((ok) => setTimeout(ok, ms));
const bola = (alvo: HTMLElement, d: number) =>
  alvo.querySelector<HTMLButtonElement>(`button[aria-label="Dezena ${d}"][aria-pressed]`)!;
const botao = (alvo: ParentNode, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.trim() === rotulo);
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  )!;
const secoes = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll('.secoes [role="radio"]')).map((b) =>
    texto(b).replace(/^✓ /, ''),
  );

async function ir(secao: string): Promise<void> {
  location.hash = `#/conferir?loteria=quina${secao}`;
  window.dispatchEvent(new HashChangeEvent('hashchange'));
  flushSync();
  await pausa();
}

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/conferir?loteria=quina';
});

afterEach(() => {
  desmontar();
  vi.restoreAllMocks();
  location.hash = '';
});

describe('aba Conferir: seções', () => {
  it('oferece Conferir jogo, Histórico e Meus jogos; o hash escolhe a seção', async () => {
    const alvo = montar(new RepositorioIndexedDB(new IDBFactory()));
    expect(secoes(alvo)).toEqual(['Conferir jogo', 'Histórico', 'Meus jogos']);
    expect(texto(alvo.querySelector('h2'))).toBe('Conferir jogo');
    await ir('&secao=jogos');
    expect(texto(alvo.querySelector('h2'))).toBe(textos.jogos.titulo);
    await ir('&secao=historico');
    expect(texto(alvo.querySelector('h2'))).toBe(textos.historico.titulo);
    await ir('');
    expect(texto(alvo.querySelector('h2'))).toBe('Conferir jogo');
  });
});

describe('salvar, ver em Meus jogos e conferir de novo', () => {
  async function salvarUm(alvo: HTMLElement, dezenas: number[], nome: string): Promise<void> {
    await vi.waitFor(() => expect(bola(alvo, 1)).not.toBeNull());
    for (const d of dezenas) bola(alvo, d).click();
    flushSync();
    const campo = alvo.querySelector<HTMLInputElement>('form.salvar input[type="text"]')!;
    campo.value = nome;
    campo.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    alvo.querySelector('form.salvar')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('form.salvar .mensagem'))).toBe(textos.conferir.salvar.salvo),
    );
  }

  it('"Ver Meus jogos" mostra o jogo; "Conferir" o carrega no volante com o resultado', async () => {
    const alvo = montar(new RepositorioIndexedDB(new IDBFactory()));
    await salvarUm(alvo, [5, 12, 33, 47, 71], 'Aniversário');

    botao(alvo, textos.conferir.salvar.verMeusJogos)!.click();
    await vi.waitFor(() => expect(texto(alvo.querySelector('h2'))).toBe(textos.jogos.titulo));
    expect(location.hash).toBe('#/conferir?loteria=quina&secao=jogos');
    await vi.waitFor(() => expect(alvo.querySelectorAll('.lista > li')).toHaveLength(1));
    expect(texto(alvo.querySelector('.lista .nome'))).toBe('Aniversário');

    botao(alvo.querySelector('.lista')!, 'Conferir')!.click();
    await vi.waitFor(() => expect(texto(alvo.querySelector('h2'))).toBe('Conferir jogo'));
    expect(location.hash).toBe('#/conferir?loteria=quina');
    await vi.waitFor(() => expect(bola(alvo, 5)).not.toBeNull());
    expect(bola(alvo, 5).getAttribute('aria-pressed')).toBe('true');
    expect(bola(alvo, 71).getAttribute('aria-pressed')).toBe('true');
    expect(bola(alvo, 6).getAttribute('aria-pressed')).toBe('false');
    expect(texto(alvo.querySelector('.contador'))).toContain('5 dezenas marcadas');
    await vi.waitFor(() => expect(texto(alvo)).toContain('Melhor resultado'));
    expect(texto(alvo)).toContain('Jogo "Aniversário" carregado no volante.');

    // Salvar de novo o mesmo jogo carregado: já está salvo.
    alvo.querySelector('form.salvar')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('form.salvar .mensagem'))).toContain('já está salvo'),
    );
  });

  it('um jogo de outra loteria carrega o volante daquela loteria', async () => {
    const fabrica = new IDBFactory();
    const alvo = montar(new RepositorioIndexedDB(fabrica));
    await vi.waitFor(() => expect(bola(alvo, 1)).not.toBeNull());
    radio(alvo, 'Mega-Sena').click();
    flushSync();
    await vi.waitFor(() => expect(alvo.querySelectorAll('button[aria-pressed]')).toHaveLength(60));
    await salvarUm(alvo, [1, 2, 3, 4, 5, 6], '');
    desmontar();

    location.hash = '#/conferir?loteria=quina&secao=jogos';
    const outra = montar(new RepositorioIndexedDB(fabrica));
    await vi.waitFor(() => expect(texto(outra.querySelector('.titulo-lista'))).toContain('Quina'));
    radio(outra, 'Mega-Sena').click();
    flushSync();
    await vi.waitFor(() => expect(outra.querySelectorAll('.lista > li')).toHaveLength(1));
    botao(outra.querySelector('.lista')!, 'Conferir')!.click();
    await vi.waitFor(() =>
      expect(outra.querySelectorAll('button[aria-pressed="true"]')).toHaveLength(6),
    );
    expect(outra.querySelectorAll('button[aria-pressed]')).toHaveLength(60);
    expect(location.hash).toBe('#/conferir?loteria=megasena');
  });

  it('sobrevive a recarregar a página: novo componente e nova conexão sobre o mesmo banco', async () => {
    const fabrica = new IDBFactory();
    const primeiro = new RepositorioIndexedDB(fabrica);
    const alvo = montar(primeiro);
    await salvarUm(alvo, [1, 2, 3, 4, 5], 'Sobrevivente');
    desmontar();
    await primeiro.fechar();

    location.hash = '#/conferir?loteria=quina&secao=jogos';
    const recarregado = montar(new RepositorioIndexedDB(fabrica));
    await vi.waitFor(() => expect(recarregado.querySelectorAll('.lista > li')).toHaveLength(1));
    expect(texto(recarregado.querySelector('.lista .nome'))).toBe('Sobrevivente');
    expect(Array.from(recarregado.querySelectorAll('.lista .dezena')).map((e) => texto(e))).toEqual(
      ['01', '02', '03', '04', '05'],
    );
  });
});
