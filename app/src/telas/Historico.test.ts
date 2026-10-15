// Teste de componente da tela Histórico: um caso por estado e por interação, com
// as fixtures reais (testes/fixtures) servidas por um servidor falso. Sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import type { Slug } from '../dados/tipos';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import Historico from './Historico.svelte';

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
  montado = mount(Historico as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte },
  });
  flushSync();
  return alvo;
}

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/conferir';
});

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  location.hash = '';
});

/** Fixture crua, lida por regex (independente do validador): [concurso, data, dezenas][]. */
function cru(slug: Slug) {
  return Array.from(
    lerFixtureTexto(`${slug}/historico.json`).matchAll(
      /\[(\d+), "(\d{4}-\d{2}-\d{2})", \[([\d, ]+)\]\]/g,
    ),
    (m) => ({
      concurso: Number(m[1]),
      data: m[2] as string,
      dezenas: (m[3] as string).split(',').map(Number),
    }),
  );
}

const linhas = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('ul.lista > li'));
const esperarLinhas = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(linhas(alvo).length).toBeGreaterThan(0));
const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const concursosDasLinhas = (alvo: HTMLElement) =>
  linhas(alvo).map((l) => Number(/Concurso (\d+)/.exec(texto(l))?.[1]));
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const botao = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(rotulo));
const selecionada = (alvo: HTMLElement) =>
  Array.from(
    alvo.querySelectorAll(
      '[role="radiogroup"][aria-label="Loteria"] [role="radio"][aria-checked="true"]',
    ),
  ).map((b) => texto(b));
const contagem = (alvo: HTMLElement) => texto(alvo.querySelector('.contagem'));
const campo = (alvo: HTMLElement) => alvo.querySelector<HTMLInputElement>('input.campo')!;
const pausa = (ms = 20) => new Promise((ok) => setTimeout(ok, ms));

async function buscar(alvo: HTMLElement, valor: string): Promise<void> {
  const c = campo(alvo);
  c.value = valor;
  c.dispatchEvent(new Event('input', { bubbles: true }));
  flushSync();
  botao(alvo, textos.historico.busca.buscar)!.click();
  flushSync();
  await pausa();
}

async function escolherDezena(alvo: HTMLElement, dezena: number): Promise<void> {
  alvo
    .querySelector<HTMLButtonElement>(`button[aria-label="Dezena ${dezena}"][aria-pressed]`)!
    .click();
  flushSync();
  await pausa();
}

async function montarEm(slug: Slug): Promise<HTMLElement> {
  location.hash = `#/conferir?loteria=${slug}`;
  const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
  await esperarLinhas(alvo);
  return alvo;
}

describe('Histórico: carregando', () => {
  it('antes de o cache ser lido mostra o esqueleto discreto, o aviso e nenhuma linha', () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(alvo.querySelector('h2')?.textContent).toBe('Histórico de concursos');
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(texto(alvo)).toContain(textos.historico.carregando);
    expect(texto(alvo)).toContain('Ele não prevê o sorteio');
    expect(linhas(alvo)).toHaveLength(0);
  });
});

describe('Histórico: lista (Quina, o maior histórico)', () => {
  it('título, subtítulo, estado, contagem; do mais recente ao mais antigo', async () => {
    const dados = cru('quina');
    const ultimo = dados[dados.length - 1]!;
    const alvo = await montarEm('quina');
    expect(alvo.querySelector('h2')?.textContent).toBe('Histórico de concursos');
    expect(texto(alvo)).toContain(
      `${dados.length.toLocaleString('pt-BR')} concursos, do 1 ao ${ultimo.concurso}`,
    );
    expect(selecionada(alvo)).toEqual(['✓ Quina']);
    expect(texto(alvo.querySelector('[role="status"][data-estado]'))).toBe(
      'Atualizado em 14/10/2026 09:30.',
    );
    expect(contagem(alvo)).toBe(
      `${dados.length.toLocaleString('pt-BR')} concursos, do mais recente ao mais antigo`,
    );
    const ordem = concursosDasLinhas(alvo);
    expect(ordem[0]).toBe(ultimo.concurso);
    expect(ordem).toEqual(ordem.map((_, i) => ultimo.concurso - i));
  });

  it('só uma janela de linhas existe no DOM (muito menos que os milhares de concursos)', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    const n = linhas(alvo).length;
    expect(dados.length).toBeGreaterThan(7000);
    expect(n).toBeGreaterThan(3);
    expect(n).toBeLessThan(30);
    expect(alvo.querySelectorAll('ul.lista .disco').length).toBeLessThan(30 * 5);
  });

  it('cada linha: concurso, data dd/mm/aaaa, 5 bolas e frase para leitor de tela; posição e tamanho da lista', async () => {
    const dados = cru('quina');
    const ultimo = dados[dados.length - 1]!;
    const alvo = await montarEm('quina');
    const primeira = linhas(alvo)[0]!;
    const [a, m, d] = ultimo.data.split('-');
    expect(texto(primeira)).toContain(`Concurso ${ultimo.concurso}`);
    expect(texto(primeira)).toContain(`${d}/${m}/${a}`);
    expect(primeira.querySelectorAll('.disco')).toHaveLength(5);
    expect(primeira.querySelector('.bolas')?.getAttribute('aria-hidden')).toBe('true');
    expect(texto(primeira.querySelector('.so-leitor'))).toBe(
      `Dezenas sorteadas: ${ultimo.dezenas.map((x) => String(x).padStart(2, '0')).join(', ')}.`,
    );
    expect(primeira.getAttribute('aria-posinset')).toBe('1');
    expect(primeira.getAttribute('aria-setsize')).toBe(String(dados.length));
    expect(alvo.querySelector('ul.lista')?.getAttribute('aria-label')).toBe(
      `Concursos da Quina: ${dados.length.toLocaleString('pt-BR')} resultados`,
    );
    // A altura fixa (rem) é o que a rolagem virtual usa.
    expect((primeira as HTMLElement).style.height).toBe('6.25rem');
  });

  it('Lotofácil (padrão): 15 bolas por linha e altura própria', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo);
    const dados = cru('lotofacil');
    const primeira = linhas(alvo)[0]!;
    expect(texto(primeira)).toContain(`Concurso ${dados[dados.length - 1]!.concurso}`);
    expect(primeira.querySelectorAll('.disco')).toHaveLength(15);
    expect((primeira as HTMLElement).style.height).toBe('7.5rem');
  });

  it('aviso de honestidade presente; nenhum texto de previsão', async () => {
    const alvo = await montarEm('quina');
    expect(texto(alvo)).toContain(textos.avisos.naoPreveHistorico);
  });
});

describe('Histórico: rolagem virtual', () => {
  it('rolar muda as linhas desenhadas, mantém o total pequeno e o espaço da lista inteira', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    const ultimo = dados[dados.length - 1]!.concurso;
    const lista = alvo.querySelector<HTMLElement>('ul.lista')!;
    const alturaTotal = () =>
      (parseFloat(lista.style.paddingTop) +
        parseFloat(lista.style.paddingBottom) +
        linhas(alvo).length * 6.25) /
      6.25;
    expect(Math.round(alturaTotal())).toBe(dados.length);

    // Simula 3000 linhas (de 100 px) já roladas: o topo da lista fica 300000 px acima da tela.
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: HTMLElement,
    ) {
      const top = this.classList.contains('lista') ? -300_000 : 0;
      return { top, bottom: top, left: 0, right: 0, width: 0, height: 0, x: 0, y: top } as DOMRect;
    });
    window.dispatchEvent(new Event('scroll'));
    await vi.waitFor(() => expect(concursosDasLinhas(alvo)[0]).toBeLessThan(ultimo - 2900));

    const ordem = concursosDasLinhas(alvo);
    // Linha de posição 3000 (0-based) é o concurso ultimo - 3000; com folga de 3 linhas a janela começa em 2997.
    expect(ordem[0]).toBe(ultimo - 2997);
    expect(ordem).toContain(ultimo - 3000);
    expect(ordem.length).toBeLessThan(30);
    expect(linhas(alvo)[0]!.getAttribute('aria-posinset')).toBe('2998');
    // O espaço total da lista não mudou.
    expect(Math.round(alturaTotal())).toBe(dados.length);
  });
});

describe('Histórico: busca por concurso', () => {
  it('mostra só o concurso pedido, com contagem e "Limpar busca" volta à lista toda', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    await buscar(alvo, '3000');
    expect(concursosDasLinhas(alvo)).toEqual([3000]);
    expect(contagem(alvo)).toContain('1 concurso encontrado');
    expect(texto(alvo)).toContain('Busca: concurso 3000');
    const esperado = dados.find((d) => d.concurso === 3000)!;
    expect(texto(linhas(alvo)[0]!.querySelector('.so-leitor'))).toBe(
      `Dezenas sorteadas: ${esperado.dezenas.map((x) => String(x).padStart(2, '0')).join(', ')}.`,
    );

    botao(alvo, textos.historico.busca.limpar)!.click();
    flushSync();
    await pausa();
    expect(concursosDasLinhas(alvo)[0]).toBe(dados[dados.length - 1]!.concurso);
    expect(texto(alvo)).not.toContain('Busca: concurso');
    expect(campo(alvo).value).toBe('');
  });

  it('concurso que não existe: mensagem clara com a faixa que existe, sem lista', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    await buscar(alvo, '99999');
    expect(linhas(alvo)).toHaveLength(0);
    expect(texto(alvo)).toContain(
      `Não existe o concurso 99999. Esta loteria tem os concursos de 1 a ${dados.length}.`,
    );
  });

  it('entrada que não é número: erro no campo (aria-invalid e descrição), lista intacta', async () => {
    const alvo = await montarEm('quina');
    const antes = concursosDasLinhas(alvo);
    await buscar(alvo, 'abc');
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Digite o número do concurso');
    expect(campo(alvo).getAttribute('aria-invalid')).toBe('true');
    expect(campo(alvo).getAttribute('aria-describedby')).not.toBeNull();
    expect(concursosDasLinhas(alvo)).toEqual(antes);
    // Digitar de novo limpa o erro.
    campo(alvo).value = '30';
    campo(alvo).dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(campo(alvo).getAttribute('aria-invalid')).toBe('false');
  });

  it('enviar o campo vazio limpa a busca', async () => {
    const alvo = await montarEm('quina');
    await buscar(alvo, '10');
    expect(concursosDasLinhas(alvo)).toEqual([10]);
    await buscar(alvo, '');
    expect(concursosDasLinhas(alvo).length).toBeGreaterThan(5);
  });
});

describe('Histórico: busca por data', () => {
  async function modoData(alvo: HTMLElement): Promise<void> {
    radio(alvo, textos.historico.busca.modoData)!.click();
    flushSync();
    await pausa();
  }

  it('data digitada com máscara encontra o concurso do dia; mostra a data no formato brasileiro', async () => {
    const dados = cru('quina');
    const primeiro = dados[0]!; // 1994-03-13
    const alvo = await montarEm('quina');
    await modoData(alvo);
    expect(alvo.querySelector('label.rotulo-campo')?.textContent).toBe('Data do sorteio');
    expect(campo(alvo).placeholder).toBe('dd/mm/aaaa');
    const c = campo(alvo);
    c.value = '13031994';
    c.dispatchEvent(new Event('input', { bubbles: true }));
    flushSync();
    expect(c.value).toBe('13/03/1994');
    botao(alvo, textos.historico.busca.buscar)!.click();
    flushSync();
    await pausa();
    expect(primeiro.data).toBe('1994-03-13');
    expect(concursosDasLinhas(alvo)).toEqual([primeiro.concurso]);
    expect(texto(alvo)).toContain('Busca: sorteios de 13/03/1994');
  });

  it('data com mais de um concurso no dia lista todos, o mais recente primeiro', async () => {
    const dados = cru('quina');
    const contagemPorData = new Map<string, number[]>();
    for (const d of dados)
      contagemPorData.set(d.data, [...(contagemPorData.get(d.data) ?? []), d.concurso]);
    const repetida = [...contagemPorData].find(([, cs]) => cs.length > 1);
    const alvo = await montarEm('quina');
    await modoData(alvo);
    const [dia, concursos] = repetida ?? ['1994-03-13', [1]];
    const [a, m, d] = dia.split('-');
    await buscar(alvo, `${d}/${m}/${a}`);
    expect(concursosDasLinhas(alvo)).toEqual([...concursos].reverse());
  });

  it('data sem sorteio: mensagem com o sorteio anterior e o seguinte mais próximos', async () => {
    const alvo = await montarEm('quina');
    await modoData(alvo);
    await buscar(alvo, '14/03/1994'); // entre os concursos 1 (13/03) e 2 (17/03)
    expect(linhas(alvo)).toHaveLength(0);
    const t = texto(alvo);
    expect(t).toContain('Não houve sorteio em 14/03/1994.');
    expect(t).toContain('Sorteio anterior mais próximo: concurso 1, em 13/03/1994.');
    expect(t).toContain('Sorteio seguinte mais próximo: concurso 2, em 17/03/1994.');
  });

  it('data inexistente no calendário e formato errado dão erro, sem tocar na lista', async () => {
    const alvo = await montarEm('quina');
    await modoData(alvo);
    await buscar(alvo, '31/02/2020');
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
      '31/02/2020 não é uma data do calendário.',
    );
    await buscar(alvo, '13/03');
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Digite a data como dia/mês/ano');
    expect(linhas(alvo).length).toBeGreaterThan(5);
  });

  it('trocar o modo limpa o campo e os erros, mas mantém a busca já aplicada', async () => {
    const alvo = await montarEm('quina');
    await buscar(alvo, '3000');
    await modoData(alvo);
    expect(campo(alvo).value).toBe('');
    expect(concursosDasLinhas(alvo)).toEqual([3000]);
  });
});

describe('Histórico: filtro por dezenas', () => {
  it('uma dezena: contagem igual à independente; dezena destacada em cada linha (anel e leitura)', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    await escolherDezena(alvo, 5);
    const esperados = dados.filter((d) => d.dezenas.includes(5));
    expect(contagem(alvo)).toBe(
      `${esperados.length.toLocaleString('pt-BR')} concursos encontrados, de ${dados.length.toLocaleString('pt-BR')}, do mais recente ao mais antigo`,
    );
    expect(texto(alvo)).toContain('Dezenas escolhidas: 05');
    expect(concursosDasLinhas(alvo)[0]).toBe(esperados[esperados.length - 1]!.concurso);
    // Cada linha mostrada tem exatamente a dezena 05 em destaque, e o texto de leitura diz isso.
    for (const l of linhas(alvo)) {
      const emDestaque = Array.from(l.querySelectorAll('.disco[data-destaque="true"]'));
      expect(emDestaque.map((e) => texto(e))).toEqual(['05']);
      expect(texto(l.querySelector('.so-leitor'))).toContain(
        'Dezenas do filtro neste concurso: 05.',
      );
    }
    // Sem filtro, nada em destaque e sem frase de filtro.
    await escolherDezena(alvo, 5);
    expect(alvo.querySelectorAll('.disco[data-destaque="true"]')).toHaveLength(0);
    expect(texto(linhas(alvo)[0]!.querySelector('.so-leitor'))).not.toContain('filtro');
  });

  it('duas dezenas: só concursos com as duas; as duas destacadas', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    await escolherDezena(alvo, 5);
    await escolherDezena(alvo, 12);
    const esperados = dados
      .filter((d) => d.dezenas.includes(5) && d.dezenas.includes(12))
      .map((d) => d.concurso);
    expect(esperados.length).toBeGreaterThan(0);
    expect(contagem(alvo)).toContain(
      `${esperados.length.toLocaleString('pt-BR')} ${esperados.length === 1 ? 'concurso encontrado' : 'concursos encontrados'}`,
    );
    expect(concursosDasLinhas(alvo)[0]).toBe(Math.max(...esperados));
    for (const l of linhas(alvo)) {
      expect(
        Array.from(l.querySelectorAll('.disco[data-destaque="true"]')).map((e) => texto(e)),
      ).toEqual(['05', '12']);
    }
    expect(texto(alvo)).toContain('Dezenas escolhidas: 05, 12');
    expect(texto(alvo.querySelector('summary'))).toContain('2 escolhidas');
  });

  it('combinação que nunca saiu junta: mensagem clara, sem lista', async () => {
    const dados = cru('quina');
    // Conjunto das trincas que já saíram (cada concurso tem 10); procura uma que nunca saiu.
    const vistas = new Set<string>();
    for (const d of dados) {
      for (let i = 0; i < 5; i++) {
        for (let j = i + 1; j < 5; j++) {
          for (let k = j + 1; k < 5; k++) {
            vistas.add(`${d.dezenas[i]}-${d.dezenas[j]}-${d.dezenas[k]}`);
          }
        }
      }
    }
    let trinca: [number, number, number] | null = null;
    for (let a = 1; a <= 80 && !trinca; a++) {
      for (let b = a + 1; b <= 80 && !trinca; b++) {
        for (let c = b + 1; c <= 80 && !trinca; c++) {
          if (!vistas.has(`${a}-${b}-${c}`)) trinca = [a, b, c];
        }
      }
    }
    expect(trinca).not.toBeNull();
    const alvo = await montarEm('quina');
    for (const d of trinca!) await escolherDezena(alvo, d);
    expect(linhas(alvo)).toHaveLength(0);
    expect(texto(alvo)).toContain(textos.historico.contagem.nenhum(false, true));
    expect(contagem(alvo)).toBe(textos.historico.contagem.nenhumCurto);
  });

  it('limite: com tantas dezenas quanto o sorteio, as demais ficam desabilitadas e há aviso', async () => {
    const alvo = await montarEm('quina');
    for (const d of [1, 2, 3, 4, 5]) await escolherDezena(alvo, d);
    const outra = alvo.querySelector<HTMLButtonElement>('button[aria-label="Dezena 6"]')!;
    expect(outra.disabled).toBe(true);
    expect(texto(alvo)).toContain('Limite de 5 dezenas');
    // As marcadas seguem ativas para trocar.
    expect(alvo.querySelector<HTMLButtonElement>('button[aria-label="Dezena 5"]')!.disabled).toBe(
      false,
    );
  });

  it('"Limpar dezenas" volta à lista inteira', async () => {
    const dados = cru('quina');
    const alvo = await montarEm('quina');
    await escolherDezena(alvo, 5);
    botao(alvo, textos.historico.filtro.limpar)!.click();
    flushSync();
    await pausa();
    expect(contagem(alvo)).toContain(
      `${dados.length.toLocaleString('pt-BR')} concursos, do mais recente`,
    );
    expect(botao(alvo, textos.historico.filtro.limpar)).toBeUndefined();
  });

  it('filtro e busca valem juntos (concurso que tem a dezena aparece; que não tem, não)', async () => {
    const dados = cru('quina');
    const c = dados.find((d) => d.concurso === 3000)!;
    const tem = c.dezenas[0]!;
    const naoTem = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13].find((n) => !c.dezenas.includes(n))!;
    const alvo = await montarEm('quina');
    await buscar(alvo, '3000');
    await escolherDezena(alvo, tem);
    expect(concursosDasLinhas(alvo)).toEqual([3000]);
    await escolherDezena(alvo, tem);
    await escolherDezena(alvo, naoTem);
    expect(linhas(alvo)).toHaveLength(0);
    expect(texto(alvo)).toContain(textos.historico.contagem.nenhum(true, true));
    expect(contagem(alvo)).toBe(textos.historico.contagem.nenhumCurto);
  });

  it('o filtro usa as dezenas da loteria: Lotofácil tem 25 e limite de 15', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo);
    expect(alvo.querySelectorAll('button[aria-pressed][aria-label^="Dezena"]')).toHaveLength(25);
    for (let d = 1; d <= 15; d++) await escolherDezena(alvo, d);
    expect(alvo.querySelector<HTMLButtonElement>('button[aria-label="Dezena 16"]')!.disabled).toBe(
      true,
    );
    expect(texto(alvo)).toContain('Limite de 15 dezenas');
  });
});

describe('Histórico: troca e escolha de loteria', () => {
  it('trocar para a Mega-Sena carrega o outro histórico, guarda a escolha e zera busca e filtro', async () => {
    const alvo = await montarEm('quina');
    await buscar(alvo, '3000');
    await escolherDezena(alvo, 5);
    const entradas = history.length;
    radio(alvo, 'Mega-Sena')!.click();
    await vi.waitFor(() => expect(selecionada(alvo)).toEqual(['✓ Mega-Sena']));
    await vi.waitFor(() => expect(texto(alvo)).toContain('Concurso 3069'));
    const dados = cru('megasena');
    expect(concursosDasLinhas(alvo)[0]).toBe(dados[dados.length - 1]!.concurso);
    expect(location.hash).toBe('#/conferir?loteria=megasena&secao=historico');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('megasena');
    expect(history.length).toBe(entradas); // replaceState
    expect(texto(alvo)).not.toContain('Busca: concurso');
    expect(texto(alvo)).not.toContain('Dezenas escolhidas');
    expect(linhas(alvo)[0]!.querySelectorAll('.disco')).toHaveLength(6);
  });

  it('hashchange: só segue a URL quando a seção é Histórico (a guarda de Conferir jogo)', async () => {
    const alvo = await montarEm('quina');
    // Seção Conferir jogo (padrão): outra loteria no hash não mexe nesta tela.
    location.hash = '#/conferir?loteria=megasena';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await pausa(50);
    expect(selecionada(alvo)).toEqual(['✓ Quina']);
    // Seção Histórico: a tela segue.
    location.hash = '#/conferir?loteria=megasena&secao=historico';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await vi.waitFor(() => expect(selecionada(alvo)).toEqual(['✓ Mega-Sena']));
  });

  it('a URL vale mais que o aparelho; sem parâmetro usa o guardado; inválidos caem na Lotofácil', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'quina');
    location.hash = '#/conferir?loteria=megasena';
    let alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo);
    expect(selecionada(alvo)).toEqual(['✓ Mega-Sena']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    location.hash = '#/conferir';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo);
    expect(selecionada(alvo)).toEqual(['✓ Quina']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'loto');
    location.hash = '#/conferir?loteria=timemania';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo);
    expect(selecionada(alvo)).toEqual(['✓ Lotofácil']);
  });

  it('mudar o hash para outra loteria (voltar do navegador) troca o histórico', async () => {
    const alvo = await montarEm('quina');
    location.hash = '#/conferir?loteria=lotofacil&secao=historico';
    await vi.waitFor(() => expect(selecionada(alvo)).toEqual(['✓ Lotofácil']));
    await vi.waitFor(() => expect(linhas(alvo)[0]?.querySelectorAll('.disco').length).toBe(15));
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
    radio(alvo, 'Quina')!.click();
    await esperarLinhas(alvo);
    liberar();
    await pausa(40);
    expect(selecionada(alvo)).toEqual(['✓ Quina']);
    expect(linhas(alvo)[0]!.querySelectorAll('.disco')).toHaveLength(5);
  });
});

describe('Histórico: sem dados, cache e recusa', () => {
  it('sem cache e sem rede: mensagem clara e "Tentar de novo" busca de novo', async () => {
    let rede = false;
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria(), () => rede));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.sincronizacao.semDados),
    );
    expect(linhas(alvo)).toHaveLength(0);
    expect(alvo.querySelector('input.campo')).toBeNull();
    rede = true;
    botao(alvo, textos.historico.tentarDeNovo)!.click();
    await esperarLinhas(alvo);
    expect(botao(alvo, textos.historico.tentarDeNovo)).toBeUndefined();
  });

  it('loteria nula no índice: aviso informativo, sem erro', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, lotofacil: null } });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.historico.semLoteria));
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('histórico em cache e sem rede: mostra com a data do arquivo, sem tratar como erro', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['lotofacil'] });
    const alvo = montar(criarFonte(servidor, arm, () => false));
    await esperarLinhas(alvo);
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
    await esperarLinhas(alvo);
    expect(texto(alvo.querySelector('[data-estado]'))).toContain('Verificando atualizações');
  });

  it('arquivo adulterado: Aviso de erro e o histórico anterior continua', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['quina'] });
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.historico.sha256 = 'c'.repeat(64);
    servidor.gravarIndice(indice);
    location.hash = '#/conferir?loteria=quina';
    const alvo = montar(criarFonte(servidor, arm));
    await esperarLinhas(alvo);
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Dado recusado');
    expect(linhas(alvo).length).toBeGreaterThan(3);
  });

  it('fixture adulterada na rede, sem cache: recusa clara e nenhuma linha', async () => {
    const servidor = new ServidorFalso();
    const adulterada = lerFixtureTexto('lotofacil/historico.json').replace(
      '"schema_version": 1',
      '"schema_version": 2',
    );
    await servidor.trocar('lotofacil/historico.json', adulterada);
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
        textos.historico.recusadoSemAnterior,
      ),
    );
    expect(linhas(alvo)).toHaveLength(0);
  });
});
