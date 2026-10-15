// Teste de componente da tela Estatísticas: estados, troca de janela e de loteria, URL,
// gráfico em SVG, tabela alternativa e honestidade dos textos. Fixtures reais servidas por
// um servidor falso; nada de rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { PARAMETROS_LOTERIA, type Slug } from '../dados/tipos';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import Estatisticas from './Estatisticas.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(
  servidor: ServidorFalso,
  arm: ArmazenamentoEmMemoria,
  rede = () => true,
): Fonte {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: async (url, init) => {
      if (!rede()) throw new TypeError('Failed to fetch');
      return servidor.fetch(url, init);
    },
    armazenamento: arm,
    agora: () => T1,
  });
  return {
    lerCache: (o) => cliente.lerCache(o),
    garantir: (o) => garantirArquivos(cliente, o),
  };
}

let montado: Record<string, unknown> | null = null;
function montar(fonte: Fonte): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Estatisticas as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte },
  });
  flushSync();
  return alvo;
}

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/estatisticas';
});

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  location.hash = '';
});

/** Fixture crua, lida por regex (independente do validador). */
function cru(slug: Slug) {
  return Array.from(
    lerFixtureTexto(`${slug}/historico.json`).matchAll(
      /\[(\d+), "(\d{4}-\d{2}-\d{2})", \[([\d, ]+)\]\]/g,
    ),
    (m) => ({
      concurso: Number(m[1]),
      dezenas: (m[3] as string).split(',').map(Number),
    }),
  );
}

/** Frequência por dezena nos últimos `n` concursos (ou todos), contada à mão. */
function frequenciaIndependente(slug: Slug, n: number | 'total'): number[] {
  const dados = cru(slug);
  const recorte = n === 'total' ? dados : dados.slice(-n);
  const freq = new Array<number>(PARAMETROS_LOTERIA[slug].totalDezenas).fill(0);
  for (const c of recorte) for (const d of c.dezenas) freq[d - 1]!++;
  return freq;
}

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const pausa = (ms = 20) => new Promise((ok) => setTimeout(ok, ms));
const radio = (alvo: HTMLElement, grupo: string, rotulo: string) =>
  Array.from(
    alvo.querySelectorAll<HTMLButtonElement>(
      `[role="radiogroup"][aria-label="${grupo}"] [role="radio"]`,
    ),
  ).find((b) => b.textContent?.includes(rotulo));
const marcada = (alvo: HTMLElement, grupo: string) =>
  Array.from(
    alvo.querySelectorAll(
      `[role="radiogroup"][aria-label="${grupo}"] [role="radio"][aria-checked="true"]`,
    ),
  ).map((b) => texto(b));
const GRUPO_LOTERIA = textos.estatisticas.rotuloLoteria;
const GRUPO_JANELA = textos.estatisticas.rotuloJanela;
const botao = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(rotulo));
const esperarGrafico = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
const cartao = (alvo: HTMLElement, titulo: string) =>
  Array.from(alvo.querySelectorAll('section')).find(
    (s) => s.querySelector('h3')?.textContent === titulo,
  );
const medida = (alvo: HTMLElement, nome: string) =>
  texto(alvo.querySelector(`[data-medida="${nome}"]`));
const subtitulo = (alvo: HTMLElement) => texto(alvo.querySelector('.subtitulo'));
const anuncio = (alvo: HTMLElement) => texto(alvo.querySelector('[data-anuncio]'));
const linhasTabela = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll('details.tabela-alt tbody tr')).map((tr) =>
    Array.from(tr.querySelectorAll('th, td')).map((c) => texto(c)),
  );

async function montarEm(slug: Slug, janela?: string): Promise<HTMLElement> {
  location.hash = `#/estatisticas?loteria=${slug}${janela ? `&janela=${janela}` : ''}`;
  const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
  await esperarGrafico(alvo);
  return alvo;
}

describe('Estatísticas: carregando e estrutura', () => {
  it('antes do cache: título, seletores, aviso e esqueleto discreto; nenhum gráfico', () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(alvo.querySelector('h2')?.textContent).toBe('Estatísticas');
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(texto(alvo)).toContain(textos.estatisticas.carregando);
    expect(texto(alvo)).toContain('Elas não preveem o sorteio');
    expect(alvo.querySelector('svg[role="img"]')).toBeNull();
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 50']);
  });

  it('janelas 10, 50, 100 e Todos em um radiogroup; sem parâmetro a janela é 50', async () => {
    const alvo = await montarEm('quina');
    const opcoes = Array.from(
      alvo.querySelectorAll(`[role="radiogroup"][aria-label="${GRUPO_JANELA}"] [role="radio"]`),
    );
    expect(opcoes.map((o) => texto(o).replace('✓ ', ''))).toEqual(['10', '50', '100', 'Todos']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 50']);
  });

  it('título, subtítulo "<Loteria> · últimos N concursos" e região viva curta', async () => {
    const alvo = await montarEm('quina', '50');
    expect(subtitulo(alvo)).toBe('Quina · últimos 50 concursos');
    expect(anuncio(alvo)).toBe('Mostrando Quina, últimos 50 concursos.');
    expect(alvo.querySelector('[data-anuncio]')?.getAttribute('role')).toBe('status');
    expect(alvo.querySelector('[data-anuncio]')?.classList.contains('so-leitor')).toBe(true);
  });
});

describe('Estatísticas: gráfico de frequência', () => {
  for (const slug of ['lotofacil', 'megasena', 'quina'] as const) {
    it(`${slug}: tabela alternativa bate com a contagem independente (janela 50 e Todos)`, async () => {
      const alvo = await montarEm(slug, '50');
      expect(linhasTabela(alvo).map((l) => Number(l[1]))).toEqual(frequenciaIndependente(slug, 50));
      radio(alvo, GRUPO_JANELA, 'Todos')!.click();
      flushSync();
      await pausa();
      const total = frequenciaIndependente(slug, 'total');
      expect(linhasTabela(alvo).map((l) => Number(l[1]!.replace(/\./g, '')))).toEqual(total);
      expect(subtitulo(alvo)).toBe(
        `${textos.loterias[slug]} · todos os ${cru(slug).length.toLocaleString('pt-BR')} concursos`,
      );
    });

    it(`${slug}: uma barra por dezena com vezes > 0, nos dois temas via tokens, e o rótulo do SVG é curto`, async () => {
      const alvo = await montarEm(slug, '100');
      const freq = frequenciaIndependente(slug, 100);
      const svg = alvo.querySelector('svg[role="img"]')!;
      expect(svg.querySelectorAll('.barras rect')).toHaveLength(freq.filter((v) => v > 0).length);
      expect(svg.getAttribute('viewBox')).toMatch(/^0 0 \d+ \d+$/);
      expect(svg.getAttribute('width')).toBeNull(); // responsivo: sem largura fixa
      const rotulo = svg.getAttribute('aria-label')!;
      expect(rotulo).toContain(`${PARAMETROS_LOTERIA[slug].totalDezenas} dezenas`);
      expect(rotulo).toContain('Esperado ao acaso');
      // Os extremos ficam só na legenda (já lida): o rótulo não os repete.
      expect(rotulo).not.toContain('Mais vezes');
      expect(rotulo).not.toContain('Menos vezes');
      expect(texto(alvo.querySelector('.legenda'))).toContain('Mais vezes');
      expect(rotulo.length).toBeLessThan(300);
      // Rótulos do eixo: poucos (nunca um por dezena).
      expect(svg.querySelectorAll('.eixo-x text').length).toBeLessThanOrEqual(9);
    });
  }

  it('Quina: eixo com 01, 10 ... 80; linha tracejada do esperado com texto na legenda', async () => {
    const alvo = await montarEm('quina', '100');
    const eixo = Array.from(alvo.querySelectorAll('svg[role="img"] .eixo-x text')).map(texto);
    expect(eixo).toEqual(['01', '10', '20', '30', '40', '50', '60', '70', '80']);
    expect(alvo.querySelectorAll('svg[role="img"] line.esperado')).toHaveLength(1);
    // 100 concursos x 5 / 80 = 6,25 -> 6,3 (uma casa).
    expect(texto(alvo)).toContain('Linha tracejada: 6,3 vezes, o esperado ao acaso');
  });

  it('extremos têm marca em forma (losango e círculo) e texto na legenda, não só cor', async () => {
    const alvo = await montarEm('lotofacil', '10');
    const freq = frequenciaIndependente('lotofacil', 10);
    const maximo = Math.max(...freq);
    const minimo = Math.min(...freq);
    const nMax = freq.filter((v) => v === maximo).length;
    const nMin = freq.filter((v) => v === minimo).length;
    const losangos = alvo.querySelectorAll('svg[role="img"] path.marca').length;
    const circulos = alvo.querySelectorAll('svg[role="img"] circle.marca').length;
    expect(losangos).toBe(nMax <= 3 ? nMax : 0);
    expect(circulos).toBe(nMin <= 3 ? nMin : 0);
    const legenda = texto(alvo.querySelector('.legenda'));
    expect(legenda).toContain('Mais vezes:');
    expect(legenda).toContain(`${maximo} ${maximo === 1 ? 'vez' : 'vezes'}`);
    expect(legenda).toContain('Menos vezes:');
  });

  it('tabela alternativa em <details> fechado, com cabeçalhos, diferença com sinal e região rolável focável', async () => {
    const alvo = await montarEm('quina', '50');
    const detalhes = alvo.querySelector<HTMLDetailsElement>('details.tabela-alt')!;
    expect(detalhes.open).toBe(false);
    expect(texto(detalhes.querySelector('summary'))).toBe('Ver como tabela');
    const cab = Array.from(detalhes.querySelectorAll('thead th')).map(texto);
    expect(cab).toEqual(['Dezena', 'Vezes', 'Diferença']);
    expect(detalhes.querySelectorAll('thead th[scope="col"]')).toHaveLength(3);
    expect(detalhes.querySelectorAll('tbody th[scope="row"]')).toHaveLength(80);
    expect(detalhes.querySelector('caption')?.textContent).toBe(
      'Vezes que cada dezena da Quina saiu. Esperado ao acaso: 3,1 vezes por dezena.',
    );
    const regiao = detalhes.querySelector('[role="region"]')!;
    expect(regiao.getAttribute('tabindex')).toBe('0');
    expect(regiao.getAttribute('aria-label')).toBeTruthy();
    // 50 concursos x 5 / 80 = 3,125 -> "3,1"; diferença = vezes - 3,125, com sinal.
    const freq = frequenciaIndependente('quina', 50);
    const linhas = linhasTabela(alvo);
    expect(linhas[0]!.slice(0, 2)).toEqual(['01', String(freq[0])]);
    expect(linhas[0]).toHaveLength(3);
    for (const [i, l] of linhas.entries()) {
      const dif = freq[i]! - 3.125;
      if (Math.abs(dif) >= 0.05) expect(l[2]![0]).toBe(dif > 0 ? '+' : '−');
    }
  });

  it('explicação honesta: diferenças são normais, sem previsão, sem quente/frio/devida', async () => {
    const alvo = await montarEm('megasena', '50');
    const t = texto(alvo);
    expect(t).toContain('É normal que as barras fiquem acima ou abaixo da linha tracejada');
    expect(t).toContain('por acaso');
    expect(t).toContain('Isso não mostra que alguma dezena tenha mais ou menos chance');
    expect(t).toContain(textos.avisos.naoPreveEstatisticas);
    expect(t).toContain(textos.avisos.jogoResponsavel);
    expect(t.toLowerCase()).not.toMatch(/quente|frio|frias|devida|vai sair|vão sair|palpite/);
  });
});

describe('Estatísticas: pares, repetidas, soma e atraso', () => {
  it('cartões lado a lado: média na janela e esperado ao acaso, com os valores da 7.1 formatados', async () => {
    const alvo = await montarEm('lotofacil', '100');
    const dados = cru('lotofacil').slice(-100);
    const media = dados.reduce((a, c) => a + c.dezenas.filter((d) => d % 2 === 0).length, 0) / 100;
    expect(medida(alvo, 'pares-media')).toBe(media.toFixed(1).replace('.', ','));
    // 15 x 12 / 25 = 7,2
    expect(medida(alvo, 'pares-esperado')).toBe('7,2');
    // 15 x 15 / 25 = 9,0
    expect(medida(alvo, 'repetidas-esperado')).toBe('9,0');
    let rep = 0;
    for (let i = 1; i < dados.length; i++) {
      rep += dados[i]!.dezenas.filter((d) => dados[i - 1]!.dezenas.includes(d)).length;
    }
    expect(medida(alvo, 'repetidas-media')).toBe((rep / 99).toFixed(1).replace('.', ','));
    expect(texto(cartao(alvo, 'Repetidas do anterior'))).toContain('em 99 comparações');
    expect(cartao(alvo, 'Pares por sorteio')).toBeDefined();
    expect(alvo.querySelector('.dupla')!.children).toHaveLength(2);
  });

  it('soma: média, esperado, menor e maior', async () => {
    const alvo = await montarEm('megasena', '50');
    const somas = cru('megasena')
      .slice(-50)
      .map((c) => c.dezenas.reduce((a, b) => a + b, 0));
    expect(medida(alvo, 'soma-minimo')).toBe(String(Math.min(...somas)));
    expect(medida(alvo, 'soma-maximo')).toBe(String(Math.max(...somas)));
    // 6 x 61 / 2 = 183
    expect(medida(alvo, 'soma-esperado')).toBe('183,0');
    expect(medida(alvo, 'soma-media')).toBe(
      (somas.reduce((a, b) => a + b, 0) / 50).toFixed(1).replace('.', ','),
    );
  });

  it('atraso: até 5 dezenas, com "concursos sem sair", conferidas à mão', async () => {
    const alvo = await montarEm('megasena', '100');
    const dados = cru('megasena').slice(-100);
    const atraso = new Map<number, number>();
    dados.forEach((c, i) => c.dezenas.forEach((d) => atraso.set(d, dados.length - 1 - i)));
    const esperado = [...atraso]
      .sort((a, b) => b[1] - a[1] || a[0] - b[0])
      .slice(0, 5)
      .map(([d, a]) => ({ d: String(d).padStart(2, '0'), a }));
    const itens = Array.from(alvo.querySelectorAll('.atrasos li')).map(texto);
    expect(itens).toHaveLength(5);
    expect(itens).toEqual(
      esperado.map(
        ({ d, a }) =>
          `${d} Dezena ${d}: ${a === 0 ? 'saiu no último concurso' : `${a} ${a === 1 ? 'concurso' : 'concursos'} sem sair`}`,
      ),
    );
  });

  it('janela 10 na Quina: as dezenas que não saíram aparecem JUNTO do atraso, com contagem e lista', async () => {
    const alvo = await montarEm('quina', '10');
    const dez = cru('quina').slice(-10);
    const sairam = new Set(dez.flatMap((c) => c.dezenas));
    const faltam = Array.from({ length: 80 }, (_, i) => i + 1).filter((d) => !sairam.has(d));
    const bloco = alvo.querySelector('[data-bloco="sem-aparicao"]')!;
    expect(bloco.closest('section')).toBe(cartao(alvo, 'Há mais tempo sem sair'));
    expect(texto(bloco.querySelector('.sem-aparicao-titulo'))).toBe(
      `Não saíram nos últimos 10 concursos: ${faltam.length} dezenas`,
    );
    const lista = Array.from(bloco.querySelectorAll('.lista-dezenas li')).map((li) =>
      Number(li.textContent),
    );
    expect(lista).toEqual(faltam);
    expect(texto(bloco)).toContain('passaram da janela inteira sem sair');
    // Nunca chama nenhuma de "devida" ou "atrasada para sair"; diz que o sorteio é independente.
    const card = texto(cartao(alvo, 'Há mais tempo sem sair')).toLowerCase();
    expect(card).not.toMatch(/devid|atrasad|vai sair|deve sair/);
    expect(card).toContain('cada sorteio é independente');
  });

  it('uma só dezena sem aparição: texto no singular', async () => {
    const alvo = await montarEm('quina', '50');
    const bloco = texto(alvo.querySelector('[data-bloco="sem-aparicao"]'));
    expect(bloco).toContain('Não saiu nos últimos 50 concursos: 1 dezena');
    expect(bloco).toContain('Esta dezena passou da janela inteira sem sair');
  });

  it('Todos: todas as dezenas já saíram, e o texto diz isso em vez de omitir o bloco', async () => {
    const alvo = await montarEm('lotofacil', 'total');
    const bloco = alvo.querySelector('[data-bloco="sem-aparicao"]')!;
    expect(texto(bloco)).toContain('Todas as dezenas saíram pelo menos uma vez');
    expect(bloco.querySelector('.lista-dezenas')).toBeNull();
  });
});

describe('Estatísticas: janela e loteria na URL e no aparelho', () => {
  it('trocar a janela atualiza subtítulo, anúncio, dados e a URL (replaceState, sem nova entrada)', async () => {
    const alvo = await montarEm('quina', '50');
    const entradas = history.length;
    const antes = linhasTabela(alvo).map((l) => l[1]);
    radio(alvo, GRUPO_JANELA, '10')!.click();
    flushSync();
    await pausa();
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 10']);
    expect(subtitulo(alvo)).toBe('Quina · últimos 10 concursos');
    expect(anuncio(alvo)).toBe('Mostrando Quina, últimos 10 concursos.');
    expect(location.hash).toBe('#/estatisticas?loteria=quina&janela=10');
    expect(history.length).toBe(entradas);
    expect(linhasTabela(alvo).map((l) => l[1])).not.toEqual(antes);
    expect(linhasTabela(alvo).map((l) => Number(l[1]))).toEqual(
      frequenciaIndependente('quina', 10),
    );
    radio(alvo, GRUPO_JANELA, 'Todos')!.click();
    flushSync();
    await pausa();
    expect(location.hash).toBe('#/estatisticas?loteria=quina&janela=total');
  });

  it('trocar de loteria carrega o outro histórico, guarda a escolha e mantém a janela', async () => {
    const alvo = await montarEm('quina', '100');
    radio(alvo, GRUPO_LOTERIA, 'Mega-Sena')!.click();
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Mega-Sena']));
    await vi.waitFor(() => expect(subtitulo(alvo)).toBe('Mega-Sena · últimos 100 concursos'));
    expect(location.hash).toBe('#/estatisticas?loteria=megasena&janela=100');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('megasena');
    expect(linhasTabela(alvo)).toHaveLength(60);
    expect(linhasTabela(alvo).map((l) => Number(l[1]))).toEqual(
      frequenciaIndependente('megasena', 100),
    );
    expect(anuncio(alvo)).toBe('Mostrando Mega-Sena, últimos 100 concursos.');
  });

  it('a URL vale mais que o aparelho; sem parâmetro usa o guardado; inválidos caem nos padrões', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'quina');
    location.hash = '#/estatisticas?loteria=megasena&janela=10';
    let alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrafico(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Mega-Sena']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 10']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    location.hash = '#/estatisticas';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrafico(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 50']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'loto');
    location.hash = '#/estatisticas?loteria=timemania&janela=7';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrafico(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 50']);
  });

  it('hashchange (voltar do navegador): segue loteria e janela, só na rota Estatísticas', async () => {
    const alvo = await montarEm('quina', '50');
    location.hash = '#/forca?loteria=megasena&janela=10';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await pausa(50);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    location.hash = '#/estatisticas?loteria=lotofacil&janela=10';
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']));
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 10']);
    await vi.waitFor(() => expect(linhasTabela(alvo)).toHaveLength(25));
  });

  it('resposta atrasada de uma loteria anterior não substitui a escolhida', async () => {
    const base = criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria());
    let liberar: () => void = () => undefined;
    let pediu = false;
    const portao = new Promise<void>((ok) => (liberar = ok));
    const fonte: Fonte = {
      lerCache: base.lerCache,
      garantir: async (o) => {
        if (o.loterias?.[0] === 'lotofacil') {
          pediu = true;
          await portao;
        }
        return base.garantir(o);
      },
    };
    const alvo = montar(fonte);
    await vi.waitFor(() => expect(pediu).toBe(true));
    radio(alvo, GRUPO_LOTERIA, 'Quina')!.click();
    await esperarGrafico(alvo);
    liberar();
    await pausa(40);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    expect(linhasTabela(alvo)).toHaveLength(80);
  });
});

describe('Estatísticas: acessibilidade dos controles', () => {
  it('seletores são radiogroups nomeados, com alvo mínimo e marca de seleção em texto (✓)', async () => {
    const alvo = await montarEm('quina', '50');
    for (const grupo of [GRUPO_LOTERIA, GRUPO_JANELA]) {
      const g = alvo.querySelector(`[role="radiogroup"][aria-label="${grupo}"]`)!;
      expect(g).not.toBeNull();
      expect(g.querySelectorAll('[role="radio"][aria-checked="true"]')).toHaveLength(1);
      // Uma só parada de Tab por grupo.
      expect(g.querySelectorAll('[role="radio"][tabindex="0"]')).toHaveLength(1);
    }
    expect(marcada(alvo, GRUPO_JANELA)[0]!.startsWith('✓')).toBe(true);
  });

  it('setas movem e marcam a janela (teclado)', async () => {
    const alvo = await montarEm('quina', '50');
    const r50 = radio(alvo, GRUPO_JANELA, '50')!;
    r50.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    flushSync();
    await pausa();
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 100']);
    expect(location.hash).toBe('#/estatisticas?loteria=quina&janela=100');
  });

  it('a região viva é sempre a mesma: vazia sem estatísticas e só o texto muda ao trocar de loteria', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    const regiao = alvo.querySelector('[data-anuncio]')!;
    expect(regiao).not.toBeNull();
    expect(regiao.getAttribute('role')).toBe('status');
    expect(texto(regiao)).toBe('');
    // Fora do "carregando" e dos demais condicionais: o estado de espera não fala por cima.
    expect(alvo.querySelector('[aria-busy="true"] [role="status"]')).toBeNull();
    await esperarGrafico(alvo);
    expect(alvo.querySelector('[data-anuncio]')).toBe(regiao);
    expect(texto(regiao)).toBe('Mostrando Lotofácil, últimos 50 concursos.');
    radio(alvo, GRUPO_LOTERIA, 'Quina')!.click();
    await vi.waitFor(() => expect(texto(regiao)).toBe('Mostrando Quina, últimos 50 concursos.'));
    expect(alvo.querySelector('[data-anuncio]')).toBe(regiao);
    expect(regiao.isConnected).toBe(true);
    // O estado da sincronização não fala por cima do anúncio.
    expect(alvo.querySelector('.estado')?.getAttribute('role')).toBeNull();
  });

  it('a região viva do anúncio muda só com loteria/janela, não com o estado da sincronização', async () => {
    const alvo = await montarEm('quina', '50');
    const regiao = alvo.querySelector('[data-anuncio]')!;
    const antes = regiao.textContent;
    await pausa(30);
    expect(alvo.querySelector('[data-anuncio]')).toBe(regiao);
    expect(regiao.textContent).toBe(antes);
  });
});

describe('Estatísticas: sem dados, cache e recusa', () => {
  it('sem cache e sem rede: mensagem clara e "Tentar de novo" busca de novo', async () => {
    let rede = false;
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria(), () => rede));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.sincronizacao.semDados),
    );
    expect(alvo.querySelector('svg[role="img"]')).toBeNull();
    rede = true;
    botao(alvo, textos.estatisticas.tentarDeNovo)!.click();
    await esperarGrafico(alvo);
    expect(botao(alvo, textos.estatisticas.tentarDeNovo)).toBeUndefined();
  });

  it('loteria nula no índice: aviso informativo, sem erro', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, lotofacil: null } });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.estatisticas.semLoteria));
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('histórico em cache e sem rede: mostra com a data do arquivo, sem tratar como erro', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['lotofacil'] });
    const alvo = montar(criarFonte(servidor, arm, () => false));
    await esperarGrafico(alvo);
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-estado]'))).toBe(
        'Sem rede, mostrando dados de 14/10/2026 09:30.',
      ),
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('o cache aparece antes de a rede responder', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const base = criarFonte(servidor, arm);
    await base.garantir({ arquivos: ['historico'], loterias: ['lotofacil'] });
    const alvo = montar({ lerCache: base.lerCache, garantir: () => new Promise(() => undefined) });
    await esperarGrafico(alvo);
    expect(texto(alvo.querySelector('[data-estado]'))).toContain('Verificando atualizações');
  });

  it('arquivo adulterado: Aviso de erro e as estatísticas do histórico anterior continuam', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['quina'] });
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.historico.sha256 = 'c'.repeat(64);
    servidor.gravarIndice(indice);
    location.hash = '#/estatisticas?loteria=quina';
    const alvo = montar(criarFonte(servidor, arm));
    await esperarGrafico(alvo);
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Dado recusado');
    expect(linhasTabela(alvo)).toHaveLength(80);
  });

  it('fixture adulterada na rede, sem cache: recusa clara e nenhum gráfico', async () => {
    const servidor = new ServidorFalso();
    const adulterada = lerFixtureTexto('lotofacil/historico.json').replace(
      '"schema_version": 1',
      '"schema_version": 2',
    );
    await servidor.trocar('lotofacil/historico.json', adulterada);
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
        textos.estatisticas.recusadoSemAnterior,
      ),
    );
    expect(alvo.querySelector('svg[role="img"]')).toBeNull();
  });
});

describe('Estatísticas: desempenho', () => {
  it('Quina completa (mais de 7 mil concursos): trocar as quatro janelas em sequência é imediato', async () => {
    const alvo = await montarEm('quina', 'total');
    const t0 = performance.now();
    for (const j of ['10', '50', '100', 'Todos']) {
      radio(alvo, GRUPO_JANELA, j)!.click();
      flushSync();
    }
    expect(performance.now() - t0).toBeLessThan(2000);
    expect(linhasTabela(alvo)).toHaveLength(80);
  });
});

describe('Estatísticas: gráfico responsivo', () => {
  it('o viewBox acompanha a largura medida da caixa e refaz as barras quando ela muda', async () => {
    let largura = 296;
    const observadores: (() => void)[] = [];
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(private readonly aoMudar: () => void) {}
        observe(): void {
          observadores.push(this.aoMudar);
        }
        disconnect(): void {}
      },
    );
    vi.spyOn(HTMLElement.prototype, 'clientWidth', 'get').mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.classList.contains('grafico') ? largura : 0;
    });
    const alvo = await montarEm('quina', '50');
    const svg = () => alvo.querySelector('svg[role="img"]')!;
    await vi.waitFor(() => expect(svg().getAttribute('viewBox')).toBe('0 0 296 200'));
    const larguraBarra = () => Number(svg().querySelector('.barras rect')!.getAttribute('width'));
    const estreita = larguraBarra();

    largura = 700;
    observadores.forEach((f) => f());
    await vi.waitFor(() => expect(svg().getAttribute('viewBox')).toBe('0 0 700 200'));
    expect(larguraBarra()).toBeGreaterThan(estreita * 2);
    vi.unstubAllGlobals();
  });
});
