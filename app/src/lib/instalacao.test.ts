import { describe, expect, it, vi } from 'vitest';
import {
  CHAVE_INSTALACAO_DISPENSADA,
  ControleInstalacao,
  decidirOferta,
  ehIosSafari,
  estaInstalado,
  gravarDispensa,
  lerDispensa,
  type EventoInstalacao,
} from './instalacao';

const IPHONE_SAFARI =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1';
const IPHONE_CHROME =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/126.0.6478.153 Mobile/15E148 Safari/604.1';
const IPHONE_FIREFOX =
  'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/127.0 Mobile/15E148 Safari/605.1.15';
// iPadOS 13+ se apresenta como Mac.
const IPAD_OU_MAC_SAFARI =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15';
const ANDROID_CHROME =
  'Mozilla/5.0 (Linux; Android 14; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

describe('ehIosSafari', () => {
  it('reconhece iPhone com Safari', () => {
    expect(ehIosSafari({ userAgent: IPHONE_SAFARI, platform: 'iPhone', maxTouchPoints: 5 })).toBe(
      true,
    );
  });
  it('reconhece iPadOS que se apresenta como Mac (tem toque)', () => {
    expect(
      ehIosSafari({ userAgent: IPAD_OU_MAC_SAFARI, platform: 'MacIntel', maxTouchPoints: 5 }),
    ).toBe(true);
  });
  it('Mac de verdade (sem toque) não é iOS', () => {
    expect(
      ehIosSafari({ userAgent: IPAD_OU_MAC_SAFARI, platform: 'MacIntel', maxTouchPoints: 0 }),
    ).toBe(false);
  });
  it('navegadores de terceiros no iOS ficam de fora', () => {
    for (const userAgent of [IPHONE_CHROME, IPHONE_FIREFOX]) {
      expect(ehIosSafari({ userAgent, platform: 'iPhone', maxTouchPoints: 5 })).toBe(false);
    }
  });
  it('Android não é iOS', () => {
    expect(
      ehIosSafari({ userAgent: ANDROID_CHROME, platform: 'Linux armv8l', maxTouchPoints: 5 }),
    ).toBe(false);
  });
});

describe('estaInstalado', () => {
  const mm = (matches: boolean) => () => ({ matches });
  it('display-mode standalone = instalado', () => {
    expect(estaInstalado({ matchMedia: mm(true) })).toBe(true);
  });
  it('navigator.standalone do iOS = instalado', () => {
    expect(estaInstalado({ matchMedia: mm(false), navigator: { standalone: true } })).toBe(true);
  });
  it('aba normal do navegador = não instalado', () => {
    expect(estaInstalado({ matchMedia: mm(false), navigator: { standalone: false } })).toBe(false);
    expect(estaInstalado({ navigator: {} })).toBe(false);
    expect(estaInstalado({})).toBe(false);
  });
  it('matchMedia que lança = não instalado', () => {
    expect(
      estaInstalado({
        matchMedia: () => {
          throw new Error('x');
        },
      }),
    ).toBe(false);
  });
});

describe('dispensa', () => {
  it('grava e lê', () => {
    const dados = new Map<string, string>();
    const arm = {
      getItem: (k: string) => dados.get(k) ?? null,
      setItem: (k: string, v: string) => void dados.set(k, v),
    };
    expect(lerDispensa(arm)).toBe(false);
    expect(gravarDispensa(arm)).toBe(true);
    expect(dados.get(CHAVE_INSTALACAO_DISPENSADA)).toBe('1');
    expect(lerDispensa(arm)).toBe(true);
  });
  it('valor estranho não conta como dispensa', () => {
    expect(lerDispensa({ getItem: () => 'talvez' })).toBe(false);
  });
  it('armazenamento bloqueado: não lança, não lembra', () => {
    const quebrado = {
      getItem: () => {
        throw new Error('bloqueado');
      },
      setItem: () => {
        throw new Error('bloqueado');
      },
    };
    expect(lerDispensa(quebrado)).toBe(false);
    expect(gravarDispensa(quebrado)).toBe(false);
    expect(lerDispensa(null)).toBe(false);
    expect(gravarDispensa(null)).toBe(false);
  });
});

describe('decidirOferta', () => {
  const base = { instalado: false, dispensado: false, eventoDisponivel: false, ios: false };
  it('nada a oferecer sem evento e fora do iOS', () => {
    expect(decidirOferta(base)).toBe('nenhuma');
  });
  it('evento do navegador = botão', () => {
    expect(decidirOferta({ ...base, eventoDisponivel: true })).toBe('botao');
  });
  it('iOS Safari = instrução', () => {
    expect(decidirOferta({ ...base, ios: true })).toBe('ios');
  });
  it('instalado ou dispensado esconde tudo', () => {
    for (const extra of [{ instalado: true }, { dispensado: true }]) {
      expect(decidirOferta({ ...base, eventoDisponivel: true, ios: true, ...extra })).toBe(
        'nenhuma',
      );
    }
  });
});

function eventoFalso(resultado: 'accepted' | 'dismissed'): EventoInstalacao {
  const e = new Event('beforeinstallprompt', { cancelable: true }) as EventoInstalacao;
  e.prompt = vi.fn(() => Promise.resolve());
  e.userChoice = Promise.resolve({ outcome: resultado });
  return e;
}

const novoAlvo = (): Window => new EventTarget() as unknown as Window;

describe('ControleInstalacao', () => {
  it('guarda o evento, impede o aviso do navegador e avisa quem assinou', () => {
    const alvo = novoAlvo();
    const controle = new ControleInstalacao(alvo);
    const ouvinte = vi.fn();
    controle.assinar(ouvinte);
    const e = eventoFalso('accepted');
    alvo.dispatchEvent(e);
    expect(e.defaultPrevented).toBe(true);
    expect(controle.eventoDisponivel).toBe(true);
    expect(ouvinte).toHaveBeenCalledTimes(1);
  });

  it('instalar usa o evento uma única vez e relata a escolha', async () => {
    const alvo = novoAlvo();
    const controle = new ControleInstalacao(alvo);
    const e = eventoFalso('dismissed');
    alvo.dispatchEvent(e);
    expect(await controle.instalar()).toBe('recusado');
    expect(e.prompt).toHaveBeenCalledTimes(1);
    expect(controle.eventoDisponivel).toBe(false);
    expect(await controle.instalar()).toBe('indisponivel');

    alvo.dispatchEvent(eventoFalso('accepted'));
    expect(await controle.instalar()).toBe('aceito');
  });

  it('prompt que falha vira indisponível, sem lançar', async () => {
    const alvo = novoAlvo();
    const controle = new ControleInstalacao(alvo);
    const e = eventoFalso('accepted');
    e.prompt = () => Promise.reject(new Error('já usado'));
    alvo.dispatchEvent(e);
    expect(await controle.instalar()).toBe('indisponivel');
  });

  it('appinstalled limpa o evento e marca instalado', () => {
    const alvo = novoAlvo();
    const controle = new ControleInstalacao(alvo);
    alvo.dispatchEvent(eventoFalso('accepted'));
    alvo.dispatchEvent(new Event('appinstalled'));
    expect(controle.eventoDisponivel).toBe(false);
    expect(controle.instaladoAgora).toBe(true);
  });

  it('cancelar a assinatura e encerrar param os avisos', () => {
    const alvo = novoAlvo();
    const controle = new ControleInstalacao(alvo);
    const ouvinte = vi.fn();
    controle.assinar(ouvinte)();
    alvo.dispatchEvent(eventoFalso('accepted'));
    expect(ouvinte).not.toHaveBeenCalled();

    const outro = vi.fn();
    controle.assinar(outro);
    controle.encerrar();
    alvo.dispatchEvent(new Event('appinstalled'));
    expect(outro).not.toHaveBeenCalled();
  });
});
