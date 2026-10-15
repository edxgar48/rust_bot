import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import html from '../../index.html?raw';
import {
  CHAVE_TEMA,
  COR_BARRA,
  aplicarPreferencia,
  ehPreferenciaTema,
  gravarPreferencia,
  lerPreferencia,
  proximaPreferencia,
  resolverTema,
} from './tema';

function memoria(inicial: Record<string, string> = {}) {
  const dados = new Map(Object.entries(inicial));
  return {
    dados,
    getItem: (k: string) => dados.get(k) ?? null,
    setItem: (k: string, v: string) => void dados.set(k, v),
    removeItem: (k: string) => void dados.delete(k),
  };
}

const quebrado = {
  getItem: () => {
    throw new Error('bloqueado');
  },
  setItem: () => {
    throw new Error('bloqueado');
  },
  removeItem: () => {
    throw new Error('bloqueado');
  },
};

describe('resolverTema', () => {
  it('automático segue o sistema', () => {
    expect(resolverTema('automatico', true)).toBe('escuro');
    expect(resolverTema('automatico', false)).toBe('claro');
  });
  it('manual ignora o sistema', () => {
    expect(resolverTema('claro', true)).toBe('claro');
    expect(resolverTema('escuro', false)).toBe('escuro');
  });
});

describe('preferência guardada', () => {
  it('lê o valor gravado', () => {
    expect(lerPreferencia(memoria({ [CHAVE_TEMA]: 'escuro' }))).toBe('escuro');
    expect(lerPreferencia(memoria({ [CHAVE_TEMA]: 'claro' }))).toBe('claro');
  });
  it('vazio, estranho, nulo ou bloqueado viram automático', () => {
    expect(lerPreferencia(memoria())).toBe('automatico');
    expect(lerPreferencia(memoria({ [CHAVE_TEMA]: 'roxo' }))).toBe('automatico');
    expect(lerPreferencia(null)).toBe('automatico');
    expect(lerPreferencia(quebrado)).toBe('automatico');
  });
  it('grava claro/escuro e remove ao voltar para automático', () => {
    const m = memoria();
    expect(gravarPreferencia('escuro', m)).toBe(true);
    expect(m.dados.get(CHAVE_TEMA)).toBe('escuro');
    expect(gravarPreferencia('automatico', m)).toBe(true);
    expect(m.dados.has(CHAVE_TEMA)).toBe(false);
  });
  it('gravação sem armazenamento ou com erro devolve false, sem lançar', () => {
    expect(gravarPreferencia('claro', null)).toBe(false);
    expect(gravarPreferencia('claro', quebrado)).toBe(false);
  });
  it('valida e cicla as preferências', () => {
    expect(ehPreferenciaTema('claro')).toBe(true);
    expect(ehPreferenciaTema('x')).toBe(false);
    expect(proximaPreferencia('automatico')).toBe('claro');
    expect(proximaPreferencia('claro')).toBe('escuro');
    expect(proximaPreferencia('escuro')).toBe('automatico');
  });
});

describe('aplicarPreferencia', () => {
  it('põe data-tema só para claro/escuro', () => {
    const raiz = document.createElement('html');
    aplicarPreferencia('escuro', raiz);
    expect(raiz.getAttribute('data-tema')).toBe('escuro');
    aplicarPreferencia('claro', raiz);
    expect(raiz.getAttribute('data-tema')).toBe('claro');
    aplicarPreferencia('automatico', raiz);
    expect(raiz.hasAttribute('data-tema')).toBe(false);
  });
});

describe('aplicarPreferencia e theme-color', () => {
  function metas(): HTMLMetaElement[] {
    return [...document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')];
  }

  beforeEach(() => {
    document.head.innerHTML =
      '<meta name="theme-color" content="#E8EFEC" media="(prefers-color-scheme: light)">' +
      '<meta name="theme-color" content="#1A211F" media="(prefers-color-scheme: dark)">';
  });

  afterEach(() => {
    document.head.innerHTML = '';
  });

  it('tema explícito reescreve as duas metas; automático restaura cada media', () => {
    const raiz = document.documentElement;
    aplicarPreferencia('escuro', raiz);
    expect(metas().map((m) => m.content)).toEqual([COR_BARRA.escuro, COR_BARRA.escuro]);
    aplicarPreferencia('claro', raiz);
    expect(metas().map((m) => m.content)).toEqual([COR_BARRA.claro, COR_BARRA.claro]);
    aplicarPreferencia('automatico', raiz);
    expect(metas().map((m) => m.content)).toEqual([COR_BARRA.claro, COR_BARRA.escuro]);
    raiz.removeAttribute('data-tema');
  });

  it('as cores são as do index.html', () => {
    expect(html).toContain(`content="${COR_BARRA.claro}" media="(prefers-color-scheme: light)"`);
    expect(html).toContain(`content="${COR_BARRA.escuro}" media="(prefers-color-scheme: dark)"`);
  });
});

describe('script anti-flash de index.html', () => {
  it('usa a mesma chave e o mesmo atributo do módulo', () => {
    expect(html).toContain(`localStorage.getItem('${CHAVE_TEMA}')`);
    expect(html).toContain("setAttribute('data-tema'");
  });
});
