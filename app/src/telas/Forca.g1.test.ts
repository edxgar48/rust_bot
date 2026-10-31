// G-1 na tela Força (7.8): selo, chave de ordem, URL/preferência, arquivo ausente, detalhe e aviso.
// Fixtures reais; o servidor falso reescreve o G-1 real para o snapshot da tabela base (ver
// ServidorFalso.publicarRankingPosicoes). Variações de G-1 são ilustrativas. Sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { CHAVE_ORDEM_FORCA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import Forca from './Forca.svelte';

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(servidor: ServidorFalso): Fonte {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: (url, init) => servidor.fetch(url, init),
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => new Date('2026-10-14T12:30:00Z'),
  });
  return { lerCache: (o) => cliente.lerCache(o), garantir: (o) => garantirArquivos(cliente, o) };
}

let montado: Record<string, unknown> | null = null;
function montar(servidor: ServidorFalso): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Forca as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte: criarFonte(servidor) },
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
  location.hash = '';
});

const linhas = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('ul.lista > li'));
const esperarLinhas = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(linhas(alvo)).toHaveLength(25));
const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const grupoOrdem = (alvo: HTMLElement) =>
  alvo.querySelector(`[role="radiogroup"][aria-label="${textos.forca.ordem.rotuloGrupo}"]`);
const selos = (alvo: HTMLElement) =>
  linhas(alvo).map((l) => texto(l.querySelector('.selo [aria-hidden="true"]')));
const primeiraColuna = (alvo: HTMLElement) =>
  linhas(alvo).map((l) => texto(l.querySelector('.posicao [aria-hidden="true"]')));
const dezenas = (alvo: HTMLElement) =>
  linhas(alvo).map((l) => Number(l.querySelector('[data-dezena]')?.getAttribute('data-dezena')));

/** g_1 da dezena, como o arquivo alinhado a diz (lido do que o servidor publica). */
function g1Publicado(servidor: ServidorFalso, slug = 'lotofacil'): Map<number, number> {
  const bytes = servidor.arquivos.get(`${slug}/ranking_posicoes.json`);
  const j = JSON.parse(new TextDecoder().decode(bytes));
  return new Map(j.dezenas.map((d: { dezena: number; g_1: number }) => [d.dezena, d.g_1]));
}

async function servidorComG1(): Promise<ServidorFalso> {
  const servidor = new ServidorFalso();
  await servidor.publicarRankingPosicoes();
  return servidor;
}

describe('Força com G-1: selo e chave de ordem', () => {
  it('cada linha traz o selo "G-1 N" da sua dezena, sem mudar a ordem por força', async () => {
    const servidor = await servidorComG1();
    const alvo = montar(servidor);
    await esperarLinhas(alvo);
    const g1 = g1Publicado(servidor);
    expect(selos(alvo)).toEqual(dezenas(alvo).map((d) => `G-1 ${g1.get(d)}º`));
    expect(primeiraColuna(alvo)).toEqual(Array.from({ length: 25 }, (_, i) => `${i + 1}º`));
    // Texto falado do selo (não só visual).
    const primeira = linhas(alvo)[0];
    const d0 = dezenas(alvo)[0] ?? 0;
    expect(texto(primeira)).toContain(`G-1: ${g1.get(d0)}º lugar`);
  });

  it('a chave "Ordenar por" mostra Força | G-1, com Força marcada, e fica na Lista', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(grupoOrdem(alvo)).not.toBeNull();
    expect(radio(alvo, 'G-1')?.getAttribute('aria-checked')).toBe('false');
    expect(
      Array.from(grupoOrdem(alvo)?.querySelectorAll('[role="radio"]') ?? []).map(texto),
    ).toEqual(['✓ Força', 'G-1']);
    expect(texto(alvo)).toContain('Ordenar por');
  });

  it('ordenar por G-1: reordena, a primeira coluna mostra o G-1 e o selo mostra a posição de Força', async () => {
    const servidor = await servidorComG1();
    const alvo = montar(servidor);
    await esperarLinhas(alvo);
    const porForca = dezenas(alvo);
    radio(alvo, 'G-1')?.focus();
    radio(alvo, 'G-1')?.click();
    flushSync();
    await vi.waitFor(() => expect(primeiraColuna(alvo)[0]).toBe('1º'));
    expect(primeiraColuna(alvo)).toEqual(Array.from({ length: 25 }, (_, i) => `${i + 1}º`));
    const g1 = g1Publicado(servidor);
    const ordenadas = dezenas(alvo);
    expect(ordenadas.map((d) => g1.get(d))).toEqual(Array.from({ length: 25 }, (_, i) => i + 1));
    expect(ordenadas).not.toEqual(porForca);
    // Selo: posição de Força de cada dezena.
    expect(selos(alvo)).toEqual(ordenadas.map((d) => `Força ${porForca.indexOf(d) + 1}º`));
    expect(texto(linhas(alvo)[0])).toContain(
      `Tabela de força: ${porForca.indexOf(ordenadas[0] ?? 0) + 1}º lugar`,
    );
    expect(texto(alvo.querySelector('.cabecalho'))).toContain('G-1 Dez.');
    // Anuncia a mudança (leitor de tela) e o foco continua na chave.
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('p.so-leitor[role="status"]'))).toBe(
        textos.forca.ordem.anunciadaG1,
      ),
    );
    expect(document.activeElement).toBe(radio(alvo, 'G-1') ?? null);
  });

  it('voltar para Força restaura a ordem da tabela', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    const inicial = dezenas(alvo);
    radio(alvo, 'G-1')?.click();
    flushSync();
    radio(alvo, 'Força')?.click();
    flushSync();
    await vi.waitFor(() => expect(dezenas(alvo)).toEqual(inicial));
    expect(texto(alvo.querySelector('.cabecalho'))).toContain('Pos. Dez.');
  });

  it('a Volante não muda: sem selo nem chave', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    radio(alvo, 'Volante')?.click();
    flushSync();
    await vi.waitFor(() => expect(grupoOrdem(alvo)).toBeNull());
    expect(alvo.querySelector('.selo')).toBeNull();
  });

  it('a borda da posição sorteada (7.6) continua no número de Força; em G-1 passa ao selo', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    const marcadasPorForca = linhas(alvo).filter((l) => l.querySelector('.posicao.marcada'));
    radio(alvo, 'G-1')?.click();
    flushSync();
    await vi.waitFor(() => expect(alvo.querySelector('.selo.marcada')).not.toBeNull());
    expect(alvo.querySelectorAll('.selo.marcada')).toHaveLength(marcadasPorForca.length);
    expect(alvo.querySelector('.posicao.marcada')).toBeNull();
  });
});

describe('Força com G-1: URL e preferência guardada', () => {
  it('escolher G-1 grava no aparelho e põe &ordem=g1 na URL; voltar a Força tira da URL', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    radio(alvo, 'G-1')?.click();
    flushSync();
    expect(localStorage.getItem(CHAVE_ORDEM_FORCA)).toBe('g1');
    expect(location.hash).toContain('&ordem=g1');
    radio(alvo, 'Força')?.click();
    flushSync();
    expect(localStorage.getItem(CHAVE_ORDEM_FORCA)).toBe('forca');
    expect(location.hash).not.toContain('ordem');
  });

  it('&ordem=g1 na URL abre ordenada por G-1', async () => {
    location.hash = '#/forca?loteria=lotofacil&ordem=g1';
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(radio(alvo, 'G-1')?.getAttribute('aria-checked')).toBe('true');
    expect(texto(alvo.querySelector('.cabecalho'))).toContain('G-1');
  });

  it('a preferência guardada vale sem URL, e a URL vence a preferência', async () => {
    localStorage.setItem(CHAVE_ORDEM_FORCA, 'g1');
    let alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(radio(alvo, 'G-1')?.getAttribute('aria-checked')).toBe('true');
    void unmount(montado as Record<string, unknown>);
    montado = null;
    document.body.innerHTML = '';
    location.hash = '#/forca?loteria=lotofacil&ordem=forca';
    alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(radio(alvo, 'Força')?.getAttribute('aria-checked')).toBe('true');
  });

  it('o valor guardado inválido é ignorado', async () => {
    localStorage.setItem(CHAVE_ORDEM_FORCA, 'banana');
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(radio(alvo, 'Força')?.getAttribute('aria-checked')).toBe('true');
  });
});

describe('Força sem o arquivo do G-1', () => {
  it('sem a chave no índice: sem selo, sem chave, tela como antes e aviso antigo', async () => {
    const alvo = montar(new ServidorFalso());
    await esperarLinhas(alvo);
    expect(alvo.querySelector('.selo')).toBeNull();
    expect(grupoOrdem(alvo)).toBeNull();
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(texto(alvo)).toContain(textos.avisos.naoPreveForca);
    expect(texto(alvo)).not.toContain('G-1');
  });

  it('preferência g1 guardada mas sem arquivo: lista por força, sem erro', async () => {
    localStorage.setItem(CHAVE_ORDEM_FORCA, 'g1');
    const alvo = montar(new ServidorFalso());
    await esperarLinhas(alvo);
    expect(primeiraColuna(alvo)[0]).toBe('1º');
    expect(texto(alvo.querySelector('.cabecalho'))).toContain('Pos.');
    expect(grupoOrdem(alvo)).toBeNull();
  });

  it('G-1 de outro snapshot (concurso diferente da tabela): ignorado, sem erro', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingPosicoes(['lotofacil'], { alinhado: false });
    const alvo = montar(servidor);
    await esperarLinhas(alvo);
    expect(alvo.querySelector('.selo')).toBeNull();
    expect(grupoOrdem(alvo)).toBeNull();
  });

  it('arquivo inválido: o G-1 não aparece, a tabela continua e o erro é dito', async () => {
    const servidor = await servidorComG1();
    await servidor.trocar(
      'lotofacil/ranking_posicoes.json',
      lerFixtureTexto('lotofacil/ranking_posicoes.json').replace(
        '"schema_version": 1',
        '"schema_version": 9',
      ),
    );
    const alvo = montar(servidor);
    await esperarLinhas(alvo);
    expect(alvo.querySelector('.selo')).toBeNull();
    expect(grupoOrdem(alvo)).toBeNull();
    expect(alvo.querySelector('[role="alert"]')).not.toBeNull();
  });
});

describe('Força com G-1: aviso e detalhe', () => {
  it('o aviso geral passa a citar o G-1 e continua dizendo que não prevê', async () => {
    const alvo = montar(await servidorComG1());
    await esperarLinhas(alvo);
    expect(texto(alvo)).toContain(textos.avisos.naoPreveForcaG1);
    expect(textos.avisos.naoPreveForcaG1).toContain('G-1');
    expect(textos.avisos.naoPreveForcaG1).toContain('não preveem');
    expect(texto(alvo)).not.toMatch(/experimental/i);
  });

  it('o detalhe da dezena mostra o G-1 e a variação da dezena', async () => {
    const servidor = await servidorComG1();
    const alvo = montar(servidor);
    await esperarLinhas(alvo);
    const g1 = g1Publicado(servidor);
    const dezena = dezenas(alvo)[1] ?? 0; // dezena 2..N; variação ilustrativa (dezena % 5) - 2
    alvo.querySelector<HTMLButtonElement>(`button.linha[data-dezena="${dezena}"]`)?.click();
    flushSync();
    await vi.waitFor(() => expect(document.querySelector('#det-g1')).not.toBeNull());
    const bloco = texto(document.querySelector('section.g1'));
    expect(bloco).toContain(`${g1.get(dezena)}º lugar`);
    expect(bloco).toContain(textos.detalhe.g1.variacao);
    expect(bloco).toContain(textos.detalhe.g1.nota);
    expect(texto(document.body)).toContain(textos.avisos.naoPreveDetalheG1);
  });

  it('sem G-1 o detalhe não ganha o bloco', async () => {
    const alvo = montar(new ServidorFalso());
    await esperarLinhas(alvo);
    alvo.querySelector<HTMLButtonElement>('button.linha')?.click();
    flushSync();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    expect(document.querySelector('section.g1')).toBeNull();
    expect(texto(document.body)).toContain(textos.avisos.naoPreveDetalhe);
  });
});
