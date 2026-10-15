// Tela Força: volante colorido e detalhe da dezena (5.3). Fixtures reais, servidor falso, sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { garantirArquivos } from '../dados/sob-demanda';
import { CHAVE_EXIBICAO_FORCA, CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import Forca from './Forca.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

function criarFonte() {
  const servidor = new ServidorFalso();
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: async (url, init) => servidor.fetch(url, init),
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  return {
    lerCache: (o: Parameters<typeof cliente.lerCache>[0]) => cliente.lerCache(o),
    garantir: (o: Parameters<typeof garantirArquivos>[1]) => garantirArquivos(cliente, o),
  };
}

let montado: Record<string, unknown> | null = null;
function montar(): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Forca as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte: criarFonte() },
  });
  flushSync();
  return alvo;
}

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/forca';
});

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  document.body.style.overflow = '';
  location.hash = '';
});

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const linhas = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('ul.lista > li'));
const celulas = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('.grade button.celula'));
const dialogo = () => document.querySelector<HTMLElement>('[role="dialog"]');
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const esperarLinhas = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(linhas(alvo)).toHaveLength(n));
const esperarCelulas = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(celulas(alvo)).toHaveLength(n));
const esperarHash = (hash: string) => vi.waitFor(() => expect(location.hash).toBe(hash));
const teclar = (alvo: Element, key: string, init: KeyboardEventInit = {}) =>
  alvo.dispatchEvent(
    new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }),
  );
const botaoFechar = () =>
  Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
    (b) => b.textContent?.trim() === textos.detalhe.fechar,
  );

describe('Volante da Força', () => {
  it('Volante liga sem "em breve": 25 células-botão com rótulo, legenda e URL com visao', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    expect(texto(alvo)).not.toContain('em breve');
    radio(alvo, 'Volante')?.click();
    await esperarCelulas(alvo, 25);
    expect(linhas(alvo)).toHaveLength(0);
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=volante');
    expect(localStorage.getItem(CHAVE_EXIBICAO_FORCA)).toBe('volante');
    expect(celulas(alvo)[0]?.getAttribute('aria-label')).toMatch(/^Dezena 1, \d+º lugar$/);
    expect(alvo.querySelectorAll('.legenda li')).toHaveLength(5);
    expect(texto(alvo)).toContain('Faixa 1: posições 1 a 5');
    expect(texto(alvo)).toContain('O índice resume o passado'); // aviso de honestidade continua
    radio(alvo, 'Lista')?.click();
    await esperarLinhas(alvo, 25);
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=lista');
  });

  it.each([
    ['lotofacil', 25],
    ['megasena', 60],
    ['quina', 80],
  ] as const)(
    '%s: %i células, em ordem de dezena, com a posição do arquivo',
    async (slug, total) => {
      location.hash = `#/forca?loteria=${slug}&visao=volante`;
      const alvo = montar();
      await esperarCelulas(alvo, total);
      const json = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`));
      const posicao = new Map<number, number>(
        json.dezenas.map((d: { dezena: number; posicao: number }) => [d.dezena, d.posicao]),
      );
      expect(celulas(alvo).map((c) => c.getAttribute('aria-label'))).toEqual(
        Array.from({ length: total }, (_, i) => `Dezena ${i + 1}, ${posicao.get(i + 1)}º lugar`),
      );
    },
  );

  it('a lista e o volante mostram a mesma dezena na mesma posição', async () => {
    location.hash = '#/forca?loteria=quina&visao=lista';
    const alvo = montar();
    await esperarLinhas(alvo, 80);
    const daLista = linhas(alvo).map((l) => {
      const dezena = l.querySelector('[aria-label^="Dezena"]')?.getAttribute('aria-label');
      const posicao = l.querySelector('.posicao .so-leitor')?.textContent;
      return `${dezena}, ${posicao}`;
    });
    radio(alvo, 'Volante')?.click();
    await esperarCelulas(alvo, 80);
    const doVolante = celulas(alvo).map((c) => c.getAttribute('aria-label'));
    expect([...doVolante].sort()).toEqual([...daLista].sort());
    // A 1ª da lista é a célula correspondente do volante.
    expect(doVolante).toContain(daLista[0]);
    expect(doVolante).toContain(daLista[79]);
  });

  it('as faixas do volante seguem a posição (Quina: 16 dezenas por faixa)', async () => {
    location.hash = '#/forca?loteria=quina&visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 80);
    const contagem = [0, 0, 0, 0, 0, 0];
    for (const c of celulas(alvo)) {
      const faixa = Number(c.querySelector('[data-faixa]')?.getAttribute('data-faixa'));
      const posicao = Number(/(\d+)º lugar/.exec(c.getAttribute('aria-label') ?? '')?.[1]);
      expect(faixa).toBe(Math.floor((posicao - 1) / 16) + 1);
      contagem[faixa] = (contagem[faixa] ?? 0) + 1;
    }
    expect(contagem.slice(1)).toEqual([16, 16, 16, 16, 16]);
  });
});

describe('forma de exibição: URL, aparelho, padrão', () => {
  it('a URL (visao=) vale mais que o aparelho', async () => {
    localStorage.setItem(CHAVE_EXIBICAO_FORCA, 'lista');
    location.hash = '#/forca?visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 25);
  });

  it('sem parâmetro usa a última escolha guardada', async () => {
    localStorage.setItem(CHAVE_EXIBICAO_FORCA, 'volante');
    const alvo = montar();
    await esperarCelulas(alvo, 25);
  });

  it('valores inválidos caem na lista', async () => {
    localStorage.setItem(CHAVE_EXIBICAO_FORCA, 'grade');
    location.hash = '#/forca?visao=mosaico';
    const alvo = montar();
    await esperarLinhas(alvo, 25);
  });

  it('trocar de loteria mantém a forma de exibição na URL', async () => {
    location.hash = '#/forca?loteria=lotofacil&visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 25);
    radio(alvo, 'Quina')?.click();
    await esperarCelulas(alvo, 80);
    expect(location.hash).toBe('#/forca?loteria=quina&visao=volante');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('quina');
  });

  it('localStorage bloqueado não quebra: volante e detalhe seguem funcionando', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('bloqueado');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('bloqueado');
    });
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    radio(alvo, 'Volante')?.click();
    await esperarCelulas(alvo, 25);
    celulas(alvo)[0]?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(location.hash).toContain('visao=volante');
  });

  it('o botão voltar entre formas de exibição por hash troca a visão', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    location.hash = '#/forca?loteria=lotofacil&visao=volante';
    await esperarCelulas(alvo, 25);
    location.hash = '#/forca?loteria=lotofacil&visao=lista';
    await esperarLinhas(alvo, 25);
  });
});

describe('detalhe da dezena a partir da lista', () => {
  it('toque numa linha abre a folha com título, índice, variação, evidência e 9 componentes', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    const d = dialogo() as HTMLElement;
    expect(d.getAttribute('aria-modal')).toBe('true');
    expect(texto(document.getElementById(d.getAttribute('aria-labelledby') ?? ''))).toBe(
      'Dezena 25 · 1º lugar',
    );
    const t = texto(d);
    expect(t).toContain('4,318');
    expect(t).toContain('▲ +1');
    expect(t).toContain('subiu 1 posição');
    expect(t).toContain('Fraca · viés');
    expect(t).toContain('Evidência fraca de que esta dezena foge do acaso');
    expect(t).toContain('Janela dos componentes: últimos 50 concursos');
    expect(t).toContain('O índice descreve o passado desta dezena e não prevê o sorteio');
    expect(d.querySelectorAll('dl .item')).toHaveLength(9);
    expect(texto(d.querySelector('[data-componente="frequencia"]'))).toContain('30 vezes');
    expect(texto(d.querySelector('[data-componente="espacoFinal"]'))).toContain('4 concursos');
  });

  it('abre com a URL da dezena e uma entrada no histórico; o foco vai ao botão Fechar', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    const antes = history.length;
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=lista&dezena=25');
    expect(history.length).toBe(antes + 1);
    expect(document.activeElement).toBe(botaoFechar());
    expect(document.body.style.overflow).toBe('hidden');
  });

  it('Fechar volta a URL e o foco à linha de origem', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    const linha = alvo.querySelector<HTMLButtonElement>('ul.lista li:nth-child(4) button');
    linha?.focus();
    linha?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    botaoFechar()?.click();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    await esperarHash('#/forca');
    await vi.waitFor(() => expect(document.activeElement).toBe(linha));
    expect(document.body.style.overflow).toBe('');
  });

  it('Esc fecha e o foco volta à linha', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    const linha = alvo.querySelector<HTMLButtonElement>('ul.lista li:nth-child(2) button');
    linha?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    teclar(dialogo() as HTMLElement, 'Escape');
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(linha));
  });

  it('toque no fundo fecha; toque dentro da folha não', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    dialogo()?.click();
    flushSync();
    expect(dialogo()).not.toBeNull();
    (dialogo()?.parentElement as HTMLElement).click();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
  });

  it('o botão voltar do navegador fecha o detalhe', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    history.back();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    expect(location.hash).toBe('#/forca');
    await esperarLinhas(alvo, 25);
  });

  it('avançar do navegador reabre o detalhe', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    history.back();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    history.forward();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
  });

  it('Tab e Shift+Tab ficam presos dentro da folha', async () => {
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    alvo.querySelector<HTMLButtonElement>('ul.lista button')?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    const d = dialogo() as HTMLElement;
    const fechar = botaoFechar() as HTMLButtonElement;
    // Só há um focável (Fechar): Tab e Shift+Tab ficam nele.
    const livre = teclar(fechar, 'Tab');
    expect(livre).toBe(false); // preventDefault chamado: o foco foi tratado pela folha
    expect(d.contains(document.activeElement)).toBe(true);
    teclar(fechar, 'Tab', { shiftKey: true });
    expect(document.activeElement).toBe(fechar);
  });
});

describe('detalhe da dezena a partir do volante', () => {
  it('toque numa célula abre o detalhe da mesma dezena/posição que a lista mostra', async () => {
    location.hash = '#/forca?loteria=quina&visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 80);
    const celula = celulas(alvo).find((c) =>
      c.getAttribute('aria-label')?.startsWith('Dezena 55,'),
    );
    expect(celula?.getAttribute('aria-label')).toBe('Dezena 55, 6º lugar');
    celula?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(texto(dialogo()?.querySelector('h2'))).toBe('Dezena 55 · 6º lugar');
    expect(location.hash).toBe('#/forca?loteria=quina&visao=volante&dezena=55');
  });

  it('Quina, dezena com componentes null: "sem dados na janela" e nenhum zero no lugar', async () => {
    location.hash = '#/forca?loteria=quina&visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 80);
    celulas(alvo)
      .find((c) => c.getAttribute('aria-label')?.startsWith('Dezena 55,'))
      ?.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    const d = dialogo() as HTMLElement;
    const sem = Array.from(d.querySelectorAll('.item[data-sem-dados="true"]'));
    expect(sem.map((i) => i.getAttribute('data-componente'))).toEqual([
      'maiorEspaco',
      'maiorEspacoOcorrencias',
      'menorEspaco',
      'menorEspacoOcorrencias',
      'somaVariacoes',
    ]);
    for (const i of sem) {
      expect(texto(i.querySelector('.valor'))).toBe('sem dados na janela');
    }
    expect(texto(d.querySelector('[data-componente="frequencia"] .valor'))).toBe('0 vezes');
    expect(texto(d.querySelector('[data-componente="espacoFinal"] .valor'))).toBe('52 concursos');
    expect(texto(d)).toContain('Muito fraca · acaso');
    expect(texto(d)).toContain('Nada indica que esta dezena fuja do acaso');
  });

  it('Esc fecha e o foco volta à célula de origem (teclado: Tab, Enter, Esc)', async () => {
    location.hash = '#/forca?loteria=lotofacil&visao=volante';
    const alvo = montar();
    await esperarCelulas(alvo, 25);
    const celula = celulas(alvo)[6] as HTMLButtonElement;
    celula.focus();
    expect(document.activeElement).toBe(celula);
    celula.click(); // Enter em um <button> dispara click
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    teclar(dialogo() as HTMLElement, 'Escape');
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(celula));
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=volante');
  });
});

describe('detalhe aberto direto pela URL', () => {
  it('?dezena=20 abre assim que a tabela chega; Fechar só tira o parâmetro', async () => {
    location.hash = '#/forca?loteria=lotofacil&visao=volante&dezena=20';
    const alvo = montar();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(texto(dialogo()?.querySelector('h2'))).toMatch(/^Dezena 20 · \d+º lugar$/);
    const antes = history.length;
    botaoFechar()?.click();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=volante');
    expect(history.length).toBe(antes);
    await esperarCelulas(alvo, 25);
    await vi.waitFor(() => expect(document.activeElement?.getAttribute('data-dezena')).toBe('20'));
  });

  it('dezena que a loteria não tem (?dezena=99) é ignorada e sai da URL', async () => {
    location.hash = '#/forca?loteria=lotofacil&visao=lista&dezena=99';
    const alvo = montar();
    await esperarLinhas(alvo, 25);
    await vi.waitFor(() => expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=lista'));
    expect(dialogo()).toBeNull();
  });

  it('trocar de loteria com a folha fechada não carrega dezena', async () => {
    location.hash = '#/forca?loteria=quina&visao=lista&dezena=80';
    const alvo = montar();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    botaoFechar()?.click();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    radio(alvo, 'Lotofácil')?.click();
    await esperarLinhas(alvo, 25);
    expect(location.hash).toBe('#/forca?loteria=lotofacil&visao=lista');
  });
});
