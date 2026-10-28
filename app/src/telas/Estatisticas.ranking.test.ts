// Aba Estatísticas, seção "Taxa histórica" (7.7): dados idênticos ao ranking_taxa.json (leitura
// independente do JSON cru), ordem pela Posição A, nota B rotulada experimental, sem evidência,
// chave opcional ausente, download só ao abrir a seção, arquivo recusado e honestidade dos textos.
// Servidor falso; nada de rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { SLUGS, type Slug } from '../dados/tipos';
import { textos } from '../lib/textos';
import Estatisticas from './Estatisticas.svelte';
import { formatarDecimal, formatarDiferenca } from '../lib/numeros';

const T1 = new Date('2026-10-14T12:30:00Z');

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(
  servidor: ServidorFalso,
  arm = new ArmazenamentoEmMemoria(),
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

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const GRUPO_SECAO = textos.estatisticas.rotuloSecao;
const GRUPO_LOTERIA = textos.estatisticas.rotuloLoteria;
const radio = (alvo: HTMLElement, grupo: string, rotulo: string) =>
  Array.from(
    alvo.querySelectorAll<HTMLButtonElement>(
      `[role="radiogroup"][aria-label="${grupo}"] [role="radio"]`,
    ),
  ).find((b) => b.textContent?.includes(rotulo));
const linhas = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLElement>('[data-ranking] [role="row"][data-dezena]'));
const esperarTabela = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(linhas(alvo).length).toBeGreaterThan(0));
/** Valor de cada célula, sem o rótulo visual do layout estreito (span aria-hidden). */
const celulas = (linha: Element) =>
  Array.from(linha.querySelectorAll('[role="cell"]')).map((c) => {
    const copia = c.cloneNode(true) as Element;
    copia.querySelectorAll('.rotulo').forEach((x) => x.remove());
    return texto(copia);
  });

interface Cru {
  taxa_acaso: number;
  concurso: number;
  ultimo_sorteado: number;
  dezenas: {
    dezena: number;
    posicao_indice: number;
    nota_a: number;
    diferenca_acaso: number;
    nota_b: number;
    posicao_a: number;
    posicao_b: number;
  }[];
}
const crua = (slug: Slug) => JSON.parse(lerFixtureTexto(`${slug}/ranking_taxa.json`)) as Cru;

/** Formatação independente (Intl direto), para não repetir a função testada. */
const f4 = (n: number) => {
  const t = new Intl.NumberFormat('pt-BR', {
    minimumFractionDigits: 4,
    maximumFractionDigits: 4,
    useGrouping: false,
  }).format(n);
  return t === '-0,0000' ? '0,0000' : t;
};
const fdif = (n: number) =>
  f4(n) === '0,0000' ? f4(n) : n > 0 ? `+${f4(n)}` : `−${f4(Math.abs(n))}`;

async function abrir(slug: Slug, servidor: ServidorFalso, fonte?: Fonte): Promise<HTMLElement> {
  location.hash = `#/estatisticas?loteria=${slug}&secao=ranking`;
  return montar(fonte ?? criarFonte(servidor));
}

describe('formatação', () => {
  it('4 casas, vírgula decimal, sem "-0,0000"', () => {
    expect(formatarDecimal(0.590189065, 4)).toBe('0,5902');
    expect(formatarDiferenca(0.01778232, 4)).toBe('+0,0178');
    expect(formatarDiferenca(-0.009810935, 4)).toBe('−0,0098');
    expect(formatarDiferenca(0, 4)).toBe('0,0000');
    expect(formatarDiferenca(0.00001, 4)).toBe('0,0000');
    expect(formatarDiferenca(-0.00001, 4)).toBe('0,0000');
  });
});

describe('Estatísticas: Taxa histórica com ranking publicado', () => {
  it.each(SLUGS)(
    '%s: todas as dezenas, na ordem da Posição A, valores idênticos ao arquivo',
    async (slug) => {
      const servidor = new ServidorFalso();
      await servidor.publicarRankingTaxa();
      const alvo = await abrir(slug, servidor);
      await esperarTabela(alvo);
      const cru = crua(slug);
      const esperado = cru.dezenas.toSorted((a, b) => a.posicao_a - b.posicao_a);
      const vistas = linhas(alvo);
      expect(vistas).toHaveLength(cru.dezenas.length);
      vistas.forEach((l, i) => {
        const e = esperado[i]!;
        expect(Number(l.dataset.dezena)).toBe(e.dezena);
        // [dezena, Posição A, Nota A, Dif., No índice, Posição B, Nota B]
        expect(celulas(l).slice(1)).toEqual([
          `${e.posicao_a}º`,
          f4(e.nota_a),
          fdif(e.diferenca_acaso),
          `${e.posicao_indice}º`,
          `${e.posicao_b}º`,
          f4(e.nota_b),
        ]);
      });
      // Posição A cresce de 1 a N (sem repetição).
      expect(esperado.map((e) => e.posicao_a)).toEqual(esperado.map((_, i) => i + 1));
      expect(texto(alvo.querySelector('[data-taxa-acaso]'))).toContain(f4(cru.taxa_acaso));
      expect(texto(alvo)).toContain(
        `Próximo concurso ${cru.concurso}, com dados até o ${cru.ultimo_sorteado}`,
      );
    },
  );

  it('cabeçalho completo, nota B rotulada "experimental" e sem evidência nem faixa', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const alvo = await abrir('quina', servidor);
    await esperarTabela(alvo);
    const cab = Array.from(alvo.querySelectorAll('[role="columnheader"]')).map((c) => texto(c));
    expect(cab).toEqual([
      'Dezena',
      'Posição A',
      'Nota A',
      'Dif. do acaso',
      'No índice',
      'Posição B',
      'Nota B (experimental)',
    ]);
    const rotuloB = alvo.querySelector('[role="cell"].experimental .rotulo');
    expect(rotuloB?.textContent).toContain('experimental');
    // O rótulo da célula é só visual: o leitor de tela já tem o columnheader.
    expect(rotuloB?.getAttribute('aria-hidden')).toBe('true');
    expect(texto(alvo.querySelector('[data-legenda]'))).toContain('Nota B (experimental)');
    // Nenhuma coluna nem célula de grau, evidência ou faixa na tabela.
    const tabela = alvo.querySelector('[data-ranking]');
    expect(tabela).not.toBeNull();
    expect(texto(tabela)).not.toMatch(/muito fraca|moderada|forte|faixa|tercil|evid[êe]ncia|grau/i);
    expect(cab.some((h) => /faixa|grau|evid[êe]ncia/i.test(h))).toBe(false);
    expect(tabela!.querySelectorAll('[role="columnheader"]')).toHaveLength(7);
    // A seção traz a nota de não previsão e o aviso de jogo responsável.
    expect(texto(alvo)).toContain(textos.avisos.naoPreveRanking);
    expect(texto(alvo)).toContain(textos.avisos.jogoResponsavel);
  });

  it('todos os rótulos da legenda estão presentes', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const alvo = await abrir('lotofacil', servidor);
    await esperarTabela(alvo);
    const leg = texto(alvo.querySelector('[data-legenda]'));
    const { titulo, ...itens } = textos.rankingTaxa.legenda;
    expect(titulo).toBeTruthy();
    for (const t of Object.values(itens)) expect(leg).toContain(t);
  });

  it('baixa o ranking só ao abrir a seção (e só o da loteria mostrada)', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    location.hash = '#/estatisticas?loteria=quina';
    const alvo = montar(criarFonte(servidor));
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    expect(servidor.pedidos.some((p) => p.includes('ranking_taxa'))).toBe(false);
    radio(alvo, GRUPO_SECAO, 'Taxa histórica')!.click();
    await esperarTabela(alvo);
    expect(location.hash).toBe('#/estatisticas?loteria=quina&secao=ranking&janela=50');
    expect(servidor.pedidos.filter((p) => p.includes('ranking_taxa'))).toEqual([
      'quina/ranking_taxa.json',
    ]);
  });

  it('trocar de loteria mantém a seção e atualiza a tabela', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const alvo = await abrir('quina', servidor);
    await esperarTabela(alvo);
    expect(linhas(alvo)).toHaveLength(80);
    radio(alvo, GRUPO_LOTERIA, 'Lotofácil')!.click();
    await vi.waitFor(() => expect(linhas(alvo)).toHaveLength(25));
    expect(location.hash).toBe('#/estatisticas?loteria=lotofacil&secao=ranking');
  });

  it('funciona sem rede com o ranking já guardado', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const arm = new ArmazenamentoEmMemoria();
    let rede = true;
    const fonte = criarFonte(servidor, arm, () => rede);
    let alvo = await abrir('megasena', servidor, fonte);
    await esperarTabela(alvo);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';
    rede = false;
    alvo = await abrir('megasena', servidor, fonte);
    await esperarTabela(alvo);
    expect(linhas(alvo)).toHaveLength(60);
    await vi.waitFor(() => expect(alvo.querySelector('[data-estado="sem_rede"]')).not.toBeNull());
  });
});

describe('Estatísticas: Taxa histórica sem ranking publicado', () => {
  it('explica em texto que não foi publicado, sem erro, e as outras seções seguem', async () => {
    const servidor = new ServidorFalso(); // índice sem a chave ranking_taxa
    const alvo = await abrir('quina', servidor);
    await vi.waitFor(() => expect(alvo.querySelector('[data-nao-publicado]')).not.toBeNull());
    expect(texto(alvo.querySelector('[data-nao-publicado]'))).toContain(
      textos.rankingTaxa.naoPublicado,
    );
    expect(alvo.querySelector('[data-ranking]')).toBeNull();
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(servidor.pedidos.some((p) => p.includes('ranking_taxa'))).toBe(false);
    // A navegação continua: volta para Frequência e atraso e o gráfico aparece.
    radio(alvo, GRUPO_SECAO, 'Frequência e atraso')!.click();
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    // O aviso de não previsão acompanha a seção em qualquer estado.
  });

  it('o aviso de não previsão aparece também no estado não publicado', async () => {
    const alvo = await abrir('quina', new ServidorFalso());
    await vi.waitFor(() => expect(alvo.querySelector('[data-nao-publicado]')).not.toBeNull());
    expect(texto(alvo)).toContain(textos.avisos.naoPreveRanking);
  });

  it('publicado só para uma loteria: as demais mostram "não publicado"', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa(['quina']);
    const alvo = await abrir('quina', servidor);
    await esperarTabela(alvo);
    radio(alvo, GRUPO_LOTERIA, 'Mega-Sena')!.click();
    await vi.waitFor(() => expect(alvo.querySelector('[data-nao-publicado]')).not.toBeNull());
    expect(alvo.querySelector('[data-ranking]')).toBeNull();
  });
});

describe('Estatísticas: Taxa histórica com arquivo inválido', () => {
  it('arquivo adulterado: erro da aba, sem tabela, e nada é guardado', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    await servidor.trocar(
      'quina/ranking_taxa.json',
      lerFixtureTexto('quina/ranking_taxa.json').replace('0.0625', '0.0626'),
      false,
    );
    const arm = new ArmazenamentoEmMemoria();
    const alvo = await abrir('quina', servidor, criarFonte(servidor, arm));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.rankingTaxa.recusadoSemAnterior));
    expect(alvo.querySelector('[data-ranking]')).toBeNull();
    expect(alvo.querySelector('[data-nao-publicado]')).toBeNull();
    // Nada do arquivo recusado foi para o armazenamento.
    expect(await arm.ler('quina/ranking_taxa.json')).toBeNull();
  });

  it('forma inválida com hash correto (uma dezena a menos): recusado', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    await servidor.alterarJson('quina/ranking_taxa.json', (j) => j.dezenas.pop());
    const alvo = await abrir('quina', servidor);
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.rankingTaxa.recusadoSemAnterior));
    expect(alvo.querySelector('[data-ranking]')).toBeNull();
  });

  it('arquivo novo inválido não substitui o ranking válido em cache', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarRankingTaxa();
    const arm = new ArmazenamentoEmMemoria();
    let alvo = await abrir('quina', servidor, criarFonte(servidor, arm));
    await esperarTabela(alvo);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';
    await servidor.alterarJson('quina/ranking_taxa.json', (j) => j.dezenas.pop());
    alvo = await abrir('quina', servidor, criarFonte(servidor, arm));
    await vi.waitFor(() =>
      expect(alvo.querySelector('[data-estado="recusado"], [role="alert"]')).not.toBeNull(),
    );
    expect(linhas(alvo)).toHaveLength(80);
  });
});
