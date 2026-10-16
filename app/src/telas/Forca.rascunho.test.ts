// Forca (7.5): botão "Inserir" por dezena, jogo em montagem e abertura em Conferir jogo.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { RepositorioEmMemoria } from '../jogos/repositorio';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import ConferirJogo from './ConferirJogo.svelte';
import Forca from './Forca.svelte';
import { alternarNoRascunho, dezenasDoRascunho, limparRascunho } from './rascunho-jogo';

const T1 = new Date('2026-10-14T12:30:00Z');

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(): Fonte {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: new ServidorFalso().fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  return {
    lerCache: (o) => cliente.lerCache(o),
    garantir: (o) => garantirArquivos(cliente, o),
  };
}

let montado: Record<string, unknown> | null = null;
function montarTela(tela: unknown, props: Record<string, unknown>): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(tela as Component<Record<string, unknown>>, { target: alvo, props });
  flushSync();
  return alvo;
}
const montarForca = () => montarTela(Forca, { fonte: criarFonte() });
function desmontar(): void {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
}

beforeEach(() => {
  localStorage.clear();
  limparRascunho();
  location.hash = '#/forca';
});

afterEach(() => {
  desmontar();
  vi.restoreAllMocks();
  location.hash = '';
});

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const esperarLinhas = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(alvo.querySelectorAll('ul.lista > li')).toHaveLength(n));
const inserir = (alvo: HTMLElement, dezena: number) =>
  alvo.querySelector<HTMLButtonElement>(
    `button.inserir[data-inserir="${dezena}"]`,
  ) as HTMLButtonElement;
const todosInserir = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('button.inserir'));
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const atalho = (alvo: HTMLElement) => alvo.querySelector('.atalho');
const aviso = (alvo: HTMLElement) => texto(alvo.querySelector('.aviso-rascunho'));
const abrir = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('button')).find((b) =>
    b.textContent?.includes(textos.forca.rascunho.abrir),
  );

async function tocar(alvo: HTMLElement, dezena: number): Promise<void> {
  inserir(alvo, dezena).click();
  flushSync();
  await new Promise((ok) => setTimeout(ok, 5)); // o aviso é regravado depois do tick
  flushSync();
}

describe('Forca: botão Inserir', () => {
  it('uma por dezena, com rótulo acessível, fora do botão da linha, sem estar marcada', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    expect(todosInserir(alvo)).toHaveLength(25);
    const botao = inserir(alvo, 7);
    expect(botao.hasAttribute('aria-label')).toBe(false);
    // O nome acessível contém o texto visível.
    expect(texto(botao)).toBe('Inserir dezena 07 no jogo');
    expect(texto(botao.querySelector('span:not(.so-leitor)'))).toBe('Inserir');
    expect(botao.getAttribute('aria-pressed')).toBe('false');
    expect(botao.closest('button.linha')).toBeNull(); // botão não aninha botão
    // Fica entre a variação e o índice (a vaga reserva a coluna dentro da linha).
    const li = botao.closest('li') as HTMLElement;
    expect(li.classList.contains('com-inserir')).toBe(true);
    expect(atalho(alvo)).toBeNull();
  });

  it('tocar insere: botão marcado ("Inserida", aria-pressed), aviso e atalho com contador', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    await tocar(alvo, 7);
    expect(inserir(alvo, 7).getAttribute('aria-pressed')).toBe('true');
    expect(texto(inserir(alvo, 7))).toBe('✓ Inserida dezena 07 no jogo');
    expect(inserir(alvo, 8).getAttribute('aria-pressed')).toBe('false');
    expect(aviso(alvo)).toBe('Dezena 07 inserida no jogo.');
    expect(texto(atalho(alvo))).toBe('1 dezena no jogo · Abrir em Conferir');
    await tocar(alvo, 20);
    expect(texto(atalho(alvo))).toBe('2 dezenas no jogo · Abrir em Conferir');
    expect(dezenasDoRascunho('lotofacil')).toEqual([7, 20]);
  });

  it('tocar de novo remove e o atalho some com o rascunho vazio', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    await tocar(alvo, 7);
    await tocar(alvo, 7);
    expect(inserir(alvo, 7).getAttribute('aria-pressed')).toBe('false');
    expect(aviso(alvo)).toBe('Dezena 07 removida do jogo.');
    expect(atalho(alvo)).toBeNull();
    expect(dezenasDoRascunho('lotofacil')).toEqual([]);
  });

  it('tocar em Inserir não abre o detalhe da dezena', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    await tocar(alvo, 7);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it('limite do volante: no máximo da loteria não adiciona e explica; remover libera', async () => {
    location.hash = '#/forca?loteria=quina';
    const alvo = montarForca();
    await esperarLinhas(alvo, 80);
    for (let d = 1; d <= 15; d++) await tocar(alvo, d);
    expect(texto(atalho(alvo))).toBe('15 dezenas no jogo · Abrir em Conferir');
    await tocar(alvo, 16);
    expect(dezenasDoRascunho('quina')).toHaveLength(15);
    expect(inserir(alvo, 16).getAttribute('aria-pressed')).toBe('false');
    expect(aviso(alvo)).toBe(textos.forca.rascunho.cheio('16', 15));
    await tocar(alvo, 1);
    await tocar(alvo, 16);
    expect(inserir(alvo, 16).getAttribute('aria-pressed')).toBe('true');
  });

  it('outra loteria: o rascunho anterior não aparece e inserir começa um novo', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    await tocar(alvo, 7);
    radio(alvo, 'Quina')?.click();
    await esperarLinhas(alvo, 80);
    expect(atalho(alvo)).toBeNull();
    expect(todosInserir(alvo).every((b) => b.getAttribute('aria-pressed') === 'false')).toBe(true);
    await tocar(alvo, 9);
    expect(texto(atalho(alvo))).toBe('1 dezena no jogo · Abrir em Conferir');
    expect(dezenasDoRascunho('quina')).toEqual([9]);
    expect(dezenasDoRascunho('lotofacil')).toEqual([]);
  });

  it('ao voltar para a tela, o rascunho pendente da loteria reaparece marcado', async () => {
    alternarNoRascunho('lotofacil', 12);
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    expect(inserir(alvo, 12).getAttribute('aria-pressed')).toBe('true');
    expect(texto(atalho(alvo))).toBe('1 dezena no jogo · Abrir em Conferir');
  });
});

describe('Forca -> Conferir jogo', () => {
  it('"Abrir em Conferir" vai para o hash do jogo e o volante abre com as dezenas marcadas', async () => {
    const alvo = montarForca();
    await esperarLinhas(alvo, 25);
    for (const d of [3, 11, 25]) await tocar(alvo, d);
    abrir(alvo)?.click();
    expect(location.hash).toBe('#/conferir?loteria=lotofacil');

    desmontar();
    const conferir = montarTela(ConferirJogo, {
      fonte: criarFonte(),
      jogos: new RepositorioEmMemoria(),
    });
    await vi.waitFor(() =>
      expect(conferir.querySelectorAll('button[aria-pressed]').length).toBeGreaterThan(0),
    );
    const marcadas = Array.from(
      conferir.querySelectorAll<HTMLButtonElement>('button[aria-pressed="true"]'),
    ).map((b) => b.getAttribute('aria-label'));
    expect(marcadas).toEqual(['Dezena 3', 'Dezena 11', 'Dezena 25']);
    expect(texto(conferir.querySelector('.contador'))).toContain('3 dezenas marcadas');
    expect(texto(conferir.querySelector('.carregado'))).toBe(textos.conferir.rascunhoCarregado);
    // Consumido: não volta a aparecer em outra abertura.
    expect(dezenasDoRascunho('lotofacil')).toEqual([]);
  });

  it('Quina inserida na Força abre a Quina em Conferir, mesmo com outra loteria guardada', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'lotofacil');
    location.hash = '#/forca?loteria=quina';
    const alvo = montarForca();
    await esperarLinhas(alvo, 80);
    await tocar(alvo, 5);
    abrir(alvo)?.click();
    desmontar();
    const conferir = montarTela(ConferirJogo, {
      fonte: criarFonte(),
      jogos: new RepositorioEmMemoria(),
    });
    await vi.waitFor(() =>
      expect(conferir.querySelectorAll('button[aria-pressed]')).toHaveLength(80),
    );
    expect(
      conferir.querySelector('button[aria-label="Dezena 5"]')?.getAttribute('aria-pressed'),
    ).toBe('true');
  });

  it('abrir Conferir sem o atalho não usa nem apaga o rascunho', async () => {
    alternarNoRascunho('lotofacil', 12);
    const conferir = montarTela(ConferirJogo, {
      fonte: criarFonte(),
      jogos: new RepositorioEmMemoria(),
    });
    await vi.waitFor(() =>
      expect(conferir.querySelectorAll('button[aria-pressed]').length).toBeGreaterThan(0),
    );
    expect(conferir.querySelectorAll('button[aria-pressed="true"]')).toHaveLength(0);
    expect(conferir.querySelector('.carregado')).toBeNull();
    expect(dezenasDoRascunho('lotofacil')).toEqual([12]);
  });
});
