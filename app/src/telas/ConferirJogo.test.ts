// Teste de componente da tela Conferir jogo: estados, seleção no volante, quantidade
// válida, resultado conferido contra um cálculo independente. Fixtures reais, sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import type { Slug } from '../dados/tipos';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import ConferirJogo from './ConferirJogo.svelte';

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
  montado = mount(ConferirJogo as unknown as Component<Record<string, unknown>>, {
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

/** Fixture crua, lida por regex (independente do validador). */
function cru(slug: Slug) {
  return Array.from(
    lerFixtureTexto(`${slug}/historico.json`).matchAll(
      /\[(\d+), "\d{4}-\d{2}-\d{2}", \[([\d, ]+)\]\]/g,
    ),
    (m) => ({ concurso: Number(m[1]), dezenas: (m[2] as string).split(',').map(Number) }),
  );
}

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const pausa = (ms = 20) => new Promise((ok) => setTimeout(ok, ms));
const bolas = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('button[aria-pressed]'));
const bola = (alvo: HTMLElement, dezena: number) =>
  alvo.querySelector<HTMLButtonElement>(
    `button[aria-label="Dezena ${dezena}"][aria-pressed]`,
  ) as HTMLButtonElement;
const botao = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(rotulo));
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const esperarVolante = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(bolas(alvo).length).toBeGreaterThan(0));
const contador = (alvo: HTMLElement) => texto(alvo.querySelector('.contador'));
const resultado = (alvo: HTMLElement) =>
  alvo.querySelector('section[aria-labelledby]:has(.faixas)');
const faixas = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll('.faixas > li')).map((l) => texto(l));

async function marcar(alvo: HTMLElement, dezenas: number[]): Promise<void> {
  for (const d of dezenas) bola(alvo, d).click();
  flushSync();
  await pausa();
}

async function montarEm(slug: Slug): Promise<HTMLElement> {
  location.hash = `#/conferir?loteria=${slug}`;
  const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
  await esperarVolante(alvo);
  return alvo;
}

describe('Conferir jogo: carregando e aviso', () => {
  it('antes do cache: título, esqueleto discreto, aviso de jogo responsável e nenhuma bola', () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(alvo.querySelector('h2')?.textContent).toBe('Conferir jogo');
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(texto(alvo)).toContain(textos.conferir.carregando);
    expect(texto(alvo)).toContain(textos.avisos.naoPreveConferir);
    expect(texto(alvo)).toContain('Jogue com responsabilidade');
    expect(bolas(alvo)).toHaveLength(0);
  });
});

describe('Conferir jogo: volante e quantidade', () => {
  it('Lotofácil (padrão): subtítulo, 25 bolas-botão com rótulo "Dezena 07", nada marcado, orientação', async () => {
    const alvo = await montarEm('lotofacil');
    expect(texto(alvo.querySelector('.subtitulo'))).toBe('Lotofácil · escolha de 15 a 20 dezenas');
    expect(bolas(alvo)).toHaveLength(25);
    expect(bola(alvo, 7).getAttribute('aria-label')).toBe('Dezena 7');
    expect(bolas(alvo).every((b) => b.getAttribute('aria-pressed') === 'false')).toBe(true);
    expect(contador(alvo)).toContain('0 dezenas marcadas');
    expect(contador(alvo)).toContain(textos.conferir.quantidade.vazia(15, 20));
    expect(resultado(alvo)).toBeNull();
    expect(texto(alvo.querySelector('[role="status"].estado, .estado'))).toContain('Atualizado');
  });

  it('o contador está numa região viva (role=status) e acompanha as marcações', async () => {
    const alvo = await montarEm('quina');
    expect(alvo.querySelector('.contador')?.getAttribute('role')).toBe('status');
    await marcar(alvo, [3]);
    expect(contador(alvo)).toContain('1 dezena marcada');
    await marcar(alvo, [9, 20]);
    expect(contador(alvo)).toContain('3 dezenas marcadas');
    expect(bola(alvo, 9).getAttribute('aria-pressed')).toBe('true');
  });

  it('quantidade abaixo do mínimo: mensagem clara e nenhum resultado', async () => {
    const alvo = await montarEm('quina');
    await marcar(alvo, [1, 2, 3, 4]);
    expect(contador(alvo)).toContain('4 dezenas marcadas');
    expect(contador(alvo)).toContain(textos.conferir.quantidade.faltam(1, 5));
    expect(resultado(alvo)).toBeNull();
    expect(texto(alvo)).not.toContain('Melhor resultado');
  });

  it('no mínimo o resultado aparece; ao desmarcar uma, some de novo', async () => {
    const alvo = await montarEm('quina');
    await marcar(alvo, [1, 2, 3, 4, 5]);
    expect(resultado(alvo)).not.toBeNull();
    await marcar(alvo, [5]);
    expect(resultado(alvo)).toBeNull();
    expect(bola(alvo, 5).getAttribute('aria-pressed')).toBe('false');
  });

  it('no máximo da loteria as demais bolas ficam desabilitadas, com aviso; desmarcar libera', async () => {
    const alvo = await montarEm('lotofacil');
    await marcar(
      alvo,
      Array.from({ length: 20 }, (_, i) => i + 1),
    );
    expect(contador(alvo)).toContain('20 dezenas marcadas');
    expect(contador(alvo)).toContain(textos.conferir.limite(20));
    expect(bola(alvo, 21).disabled).toBe(true);
    expect(bola(alvo, 20).disabled).toBe(false);
    expect(resultado(alvo)).not.toBeNull();
    await marcar(alvo, [20]);
    expect(bola(alvo, 21).disabled).toBe(false);
  });

  it('"Limpar" zera a seleção e esconde o resultado; fica desabilitado sem seleção', async () => {
    const alvo = await montarEm('quina');
    expect(botao(alvo, 'Limpar')!.disabled).toBe(true);
    await marcar(alvo, [1, 2, 3, 4, 5, 6]);
    expect(botao(alvo, 'Limpar')!.disabled).toBe(false);
    botao(alvo, 'Limpar')!.click();
    flushSync();
    expect(contador(alvo)).toContain('0 dezenas marcadas');
    expect(resultado(alvo)).toBeNull();
  });

  it('não há botão "Salvar jogo" (persistência é da 6.3)', async () => {
    const alvo = await montarEm('quina');
    expect(botao(alvo, 'Salvar')).toBeUndefined();
  });
});

describe('Conferir jogo: resultado', () => {
  it('Quina: título com o total, melhor resultado e barras iguais ao cálculo independente', async () => {
    const alvo = await montarEm('quina');
    const jogo = [5, 12, 33, 47, 71];
    await marcar(alvo, jogo);
    const dados = cru('quina');
    const acertosPorConcurso = dados.map((c) => ({
      concurso: c.concurso,
      a: c.dezenas.filter((d) => jogo.includes(d)).length,
    }));
    const melhor = Math.max(...acertosPorConcurso.map((x) => x.a));
    const ultimoMelhor = acertosPorConcurso.filter((x) => x.a === melhor).at(-1)!.concurso;
    const nMelhor = acertosPorConcurso.filter((x) => x.a === melhor).length;

    const secao = resultado(alvo)!;
    expect(texto(secao.querySelector('h3'))).toBe(
      `Este jogo em todos os ${dados.length.toLocaleString('pt-BR')} concursos`,
    );
    expect(texto(secao.querySelector('.melhor'))).toBe(
      textos.conferir.resultado.melhor(melhor, ultimoMelhor),
    );
    const linhas = faixas(alvo);
    expect(linhas).toHaveLength(6); // 5 a 0 acertos
    let soma = 0;
    for (let k = 5; k >= 0; k--) {
      const esperado = acertosPorConcurso.filter((x) => x.a === k).length;
      soma += esperado;
      const linha = linhas[5 - k] as string;
      expect(linha).toContain(textos.conferir.resultado.acertos(k));
      expect(linha).toContain(textos.conferir.resultado.concursos(esperado));
    }
    expect(soma).toBe(dados.length);
    // A faixa do melhor resultado é marcada com texto, não só com cor.
    expect(linhas.filter((l) => l.includes('melhor resultado'))).toHaveLength(1);
    expect(linhas[5 - melhor]).toContain('melhor resultado');
    if (nMelhor > 1) {
      expect(texto(secao)).toContain(textos.conferir.resultado.melhorRepetido(nMelhor));
      expect(secao.querySelectorAll('details li').length).toBe(Math.min(nMelhor, 10));
    }
  });

  it('Mega-Sena com 20 dezenas: melhor resultado e sete faixas (6 a 0)', async () => {
    const alvo = await montarEm('megasena');
    const jogo = Array.from({ length: 20 }, (_, i) => i * 3 + 1);
    await marcar(alvo, jogo);
    const dados = cru('megasena');
    const melhor = Math.max(...dados.map((c) => c.dezenas.filter((d) => jogo.includes(d)).length));
    expect(texto(resultado(alvo)!.querySelector('.melhor'))).toContain(
      `Melhor resultado: ${melhor} ${melhor === 1 ? 'acerto' : 'acertos'}`,
    );
    expect(faixas(alvo)).toHaveLength(7); // 6 a 0
  });

  it('cada faixa expande (details) e lista os concursos dela, com limite e "e mais N"', async () => {
    const alvo = await montarEm('quina');
    const jogo = [5, 12, 33, 47, 71];
    await marcar(alvo, jogo);
    const dados = cru('quina');
    const acertos = (c: { dezenas: number[] }) => c.dezenas.filter((d) => jogo.includes(d)).length;
    const detalhes = Array.from(alvo.querySelectorAll<HTMLDetailsElement>('.faixas details'));
    expect(detalhes).toHaveLength(6);
    // Fechadas: nada calculado nem renderizado; o resumo tem alvo de toque e nome acessível.
    expect(alvo.querySelectorAll('.faixas .lista-faixa')).toHaveLength(0);
    expect(detalhes[0]!.querySelector('summary')!.getAttribute('aria-label')).toMatch(
      /^5 acertos: \d+ concursos?/,
    );

    const abrir = async (k: number) => {
      const d = detalhes[5 - k]!;
      d.open = true;
      d.dispatchEvent(new Event('toggle'));
      flushSync();
      await pausa();
      return d;
    };

    // Faixa grande (1 acerto): 20 mais recentes, do mais recente ao mais antigo, e o excedente.
    const ids = dados.filter((c) => acertos(c) === 1).map((c) => c.concurso);
    expect(ids.length).toBeGreaterThan(20);
    const d1 = await abrir(1);
    const itens = Array.from(d1.querySelectorAll('.lista-faixa li:not(.mais)')).map((l) =>
      texto(l),
    );
    expect(itens).toEqual(
      ids
        .slice(-20)
        .reverse()
        .map((c) => `Concurso ${c}`),
    );
    expect(texto(d1.querySelector('.mais'))).toBe(
      textos.conferir.resultado.faixaMais(ids.length - 20),
    );

    // Faixa vazia (5 acertos, a menos que o jogo tenha saído): mensagem em vez de lista.
    if (!dados.some((c) => acertos(c) === 5)) {
      const d5 = await abrir(5);
      expect(texto(d5)).toContain(textos.conferir.resultado.faixaSemConcursos);
    }

    // Fechar de novo descarta a lista.
    d1.open = false;
    d1.dispatchEvent(new Event('toggle'));
    flushSync();
    expect(d1.querySelector('.lista-faixa')).toBeNull();
  });

  it('mostra o resultado no último concurso do arquivo', async () => {
    const alvo = await montarEm('quina');
    const jogo = [5, 12, 33, 47, 71];
    await marcar(alvo, jogo);
    const dados = cru('quina');
    const ultimo = dados[dados.length - 1]!;
    const n = ultimo.dezenas.filter((d) => jogo.includes(d)).length;
    expect(texto(alvo.querySelector('.ultimo'))).toBe(
      textos.conferir.resultado.ultimo(ultimo.concurso, n),
    );
  });

  it('Lotofácil com 15 dezenas não oferece faixas abaixo de 5 acertos', async () => {
    const alvo = await montarEm('lotofacil');
    await marcar(
      alvo,
      Array.from({ length: 15 }, (_, i) => i + 1),
    );
    const linhas = faixas(alvo);
    expect(linhas).toHaveLength(11); // 15 a 5
    expect(linhas.at(-1)).toContain(textos.conferir.resultado.acertos(5));
  });

  it('o resultado traz a nota de consulta ao passado e nenhum texto de previsão', async () => {
    const alvo = await montarEm('quina');
    await marcar(alvo, [1, 2, 3, 4, 5]);
    expect(texto(alvo)).toContain('É uma consulta ao passado');
    expect(texto(alvo)).toContain('não aumenta a chance de acertar');
    expect(texto(alvo)).not.toMatch(/vai sair|pr[oó]ximo sorteio.*(mais|maior)/i);
  });
});

describe('Conferir jogo: troca e escolha de loteria', () => {
  it('trocar para a Mega-Sena zera o jogo, guarda a escolha e usa replaceState', async () => {
    const alvo = await montarEm('quina');
    await marcar(alvo, [1, 2, 3, 4, 5]);
    const entradas = history.length;
    radio(alvo, 'Mega-Sena')!.click();
    flushSync();
    await vi.waitFor(() => expect(bolas(alvo)).toHaveLength(60));
    expect(contador(alvo)).toContain('0 dezenas marcadas');
    expect(resultado(alvo)).toBeNull();
    expect(texto(alvo.querySelector('.subtitulo'))).toBe('Mega-Sena · escolha de 6 a 20 dezenas');
    expect(location.hash).toBe('#/conferir?loteria=megasena');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('megasena');
    expect(history.length).toBe(entradas);
  });

  it('a URL vale mais que o aparelho; sem parâmetro usa o guardado', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'megasena');
    location.hash = '#/conferir?loteria=quina';
    let alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarVolante(alvo);
    expect(bolas(alvo)).toHaveLength(80);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';
    location.hash = '#/conferir';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarVolante(alvo);
    expect(bolas(alvo)).toHaveLength(60);
  });

  it('mudar o hash para outra loteria (voltar do navegador) troca o volante e zera o jogo', async () => {
    const alvo = await montarEm('quina');
    await marcar(alvo, [1, 2]);
    location.hash = '#/conferir?loteria=lotofacil';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await vi.waitFor(() => expect(bolas(alvo)).toHaveLength(25));
    expect(contador(alvo)).toContain('0 dezenas marcadas');
  });
});

describe('Conferir jogo: sem dados, cache e recusa', () => {
  it('sem cache e sem rede: mensagem clara, sem volante, e "Tentar de novo" busca de novo', async () => {
    let rede = false;
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria(), () => rede));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.sincronizacao.semDados),
    );
    expect(bolas(alvo)).toHaveLength(0);
    rede = true;
    botao(alvo, textos.conferir.tentarDeNovo)!.click();
    await esperarVolante(alvo);
    expect(botao(alvo, textos.conferir.tentarDeNovo)).toBeUndefined();
  });

  it('loteria nula no índice: aviso informativo, sem erro', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, lotofacil: null } });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.conferir.semLoteria));
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('histórico em cache e sem rede: funciona, com a data do arquivo, sem tratar como erro', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['lotofacil'] });
    const alvo = montar(criarFonte(servidor, arm, () => false));
    await esperarVolante(alvo);
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-estado]'))).toBe(
        'Sem rede, mostrando dados de 14/10/2026 09:30.',
      ),
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    await marcar(
      alvo,
      Array.from({ length: 15 }, (_, i) => i + 1),
    );
    expect(resultado(alvo)).not.toBeNull();
  });

  it('arquivo adulterado: Aviso de erro e o histórico anterior continua servindo', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['historico'], loterias: ['quina'] });
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.historico.sha256 = 'c'.repeat(64);
    servidor.gravarIndice(indice);
    location.hash = '#/conferir?loteria=quina';
    const alvo = montar(criarFonte(servidor, arm));
    await esperarVolante(alvo);
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Dado recusado');
    await marcar(alvo, [1, 2, 3, 4, 5]);
    expect(resultado(alvo)).not.toBeNull();
  });

  it('fixture adulterada na rede, sem cache: recusa clara e nenhuma bola', async () => {
    const servidor = new ServidorFalso();
    const adulterada = lerFixtureTexto('lotofacil/historico.json').replace(
      '"schema_version": 1',
      '"schema_version": 2',
    );
    await servidor.trocar('lotofacil/historico.json', adulterada);
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
        textos.conferir.recusadoSemAnterior,
      ),
    );
    expect(bolas(alvo)).toHaveLength(0);
  });
});
