// Componente de instalação: botão do navegador, instrução do iOS, dispensa e "já instalado".
import { flushSync, mount, unmount } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  CHAVE_INSTALACAO_DISPENSADA,
  ControleInstalacao,
  type EventoInstalacao,
} from '../lib/instalacao';
import { textos } from '../lib/textos';
import InstalarApp from './InstalarApp.svelte';

const IPHONE_SAFARI =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1';

let montado: Record<string, unknown> | null = null;
let controle: ControleInstalacao;
let alvoEventos: EventTarget;

function montar(modo: 'cartao' | 'secao' = 'cartao'): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(InstalarApp, { target: alvo, props: { modo, controle } });
  flushSync();
  return alvo;
}

function desmontar(): void {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
}

function oferecer(resultado: 'accepted' | 'dismissed' = 'accepted'): EventoInstalacao {
  const e = new Event('beforeinstallprompt', { cancelable: true }) as EventoInstalacao;
  e.prompt = vi.fn(() => Promise.resolve());
  e.userChoice = Promise.resolve({ outcome: resultado });
  alvoEventos.dispatchEvent(e);
  flushSync();
  return e;
}

function simular(opcoes: { standalone?: boolean; ios?: boolean }): void {
  vi.stubGlobal('matchMedia', () => ({ matches: opcoes.standalone === true }));
  if (opcoes.ios) {
    vi.spyOn(navigator, 'userAgent', 'get').mockReturnValue(IPHONE_SAFARI);
    vi.spyOn(navigator, 'platform', 'get').mockReturnValue('iPhone');
  }
}

async function esperar(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
  flushSync();
}

beforeEach(() => {
  localStorage.clear();
  alvoEventos = new EventTarget();
  controle = new ControleInstalacao(alvoEventos as unknown as Window);
});

afterEach(() => {
  desmontar();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  controle.encerrar();
});

describe('InstalarApp', () => {
  it('sem evento e fora do iOS: o cartão não aparece', () => {
    simular({});
    expect(montar().textContent?.trim()).toBe('');
  });

  it('com o evento do navegador mostra o botão, instala e o evento vale uma vez', async () => {
    simular({});
    const alvo = montar();
    const e = oferecer();
    const botao = [...alvo.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === textos.instalar.botao,
    );
    expect(botao).toBeTruthy();
    botao?.click();
    await esperar();
    expect(e.prompt).toHaveBeenCalledTimes(1);
    expect(alvo.textContent).not.toContain(textos.instalar.botao);
  });

  it('instalação recusada explica como instalar depois', async () => {
    simular({});
    const alvo = montar();
    oferecer('dismissed');
    alvo.querySelector('button')?.click();
    await esperar();
    expect(alvo.textContent).toContain(textos.instalar.recusado);
  });

  it('dispensar esconde, lembra no localStorage e não volta na próxima abertura', () => {
    simular({});
    const alvo = montar();
    oferecer();
    const dispensar = alvo.querySelector<HTMLButtonElement>(
      `button[aria-label="${textos.instalar.dispensarRotulo}"]`,
    );
    expect(dispensar).toBeTruthy();
    dispensar?.click();
    flushSync();
    expect(localStorage.getItem(CHAVE_INSTALACAO_DISPENSADA)).toBe('1');
    expect(alvo.querySelector('button')).toBeNull();

    desmontar();
    const outro = montar();
    oferecer();
    expect(outro.querySelector('button')).toBeNull();
  });

  it('dispensar funciona com localStorage bloqueado (vale só nesta sessão)', () => {
    simular({});
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('bloqueado');
    });
    const alvo = montar();
    oferecer();
    alvo.querySelector<HTMLButtonElement>('button.secundario')?.click();
    flushSync();
    expect(alvo.querySelector('button')).toBeNull();
  });

  it('iOS Safari fora do modo app: mostra a instrução, sem botão de instalar', () => {
    simular({ ios: true });
    const alvo = montar();
    expect(alvo.textContent).toContain(textos.instalar.ios);
    expect(alvo.textContent).not.toContain(textos.instalar.botao);
  });

  it('já instalado (standalone): nada é mostrado no cartão', () => {
    simular({ standalone: true, ios: true });
    const alvo = montar();
    oferecer();
    expect(alvo.textContent?.trim()).toBe('');
  });

  it('seção da Entenda: ignora a dispensa e não tem "Agora não"', () => {
    localStorage.setItem(CHAVE_INSTALACAO_DISPENSADA, '1');
    simular({});
    const alvo = montar('secao');
    oferecer();
    expect(alvo.textContent).toContain(textos.instalar.botao);
    expect(alvo.textContent).not.toContain(textos.instalar.dispensar);
  });

  it('seção da Entenda: sem oferta explica o menu do navegador; instalado avisa', () => {
    simular({});
    expect(montar('secao').textContent).toContain(textos.instalar.semOferta);
    desmontar();
    simular({ standalone: true });
    expect(montar('secao').textContent).toContain(textos.instalar.instalado);
  });

  it('cartão rotulado pelo título (h3)', () => {
    simular({});
    const alvo = montar();
    oferecer();
    const secao = alvo.querySelector('section');
    const id = secao?.getAttribute('aria-labelledby');
    expect(id).toBeTruthy();
    expect(alvo.querySelector(`h3[id="${id}"]`)?.textContent).toBe(textos.instalar.titulo);
  });

  it('região de status existe desde a montagem e só o texto muda', () => {
    simular({});
    const alvo = montar();
    const regiao = alvo.querySelector('[role="status"]');
    expect(regiao).not.toBeNull();
    expect(regiao?.textContent).toBe('');
    alvoEventos.dispatchEvent(new Event('appinstalled'));
    flushSync();
    expect(alvo.querySelector('[role="status"]')).toBe(regiao);
    expect(regiao?.textContent).toBe(textos.instalar.instaladoAgora);
  });

  it('remontar depois do evento appinstalled não repete a mensagem', () => {
    simular({});
    montar();
    alvoEventos.dispatchEvent(new Event('appinstalled'));
    flushSync();
    desmontar();
    const outro = montar('secao');
    expect(outro.querySelector('[role="status"]')?.textContent).toBe('');
    expect(outro.textContent).not.toContain(textos.instalar.instaladoAgora);
  });
});
