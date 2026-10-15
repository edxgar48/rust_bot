// Meus jogos com conferência automática (6.4): destaque dos jogos com concurso novo,
// "Marcar como visto", persistência da marca ao recarregar, download sob demanda do
// histórico e jogo sem marca. Concurso novo SIMULADO (7142 da Quina), sem rede real.
import { IDBFactory } from 'fake-indexeddb';
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE } from '../../testes/servidor-falso';
import { abrirComCache } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { OPCOES_DA_ABERTURA, garantirArquivos } from '../dados/sob-demanda';
import { criarJogo, type JogoSalvo } from '../jogos/jogo';
import type { RepositorioJogos } from '../jogos/repositorio';
import { RepositorioEmMemoria } from '../jogos/repositorio';
import { RepositorioResiliente } from '../jogos/repositorio-idb';
import { textos } from '../lib/textos';
import MeusJogos from './MeusJogos.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');
const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();

function jogoQuina(id: string, dezenas: number[], conferidoAte: number | null): JogoSalvo {
  const r = criarJogo({ loteria: 'quina', dezenas, conferidoAte }, id, T1);
  if (!r.ok) throw new Error('fixture inválida');
  return r.valor;
}

/** Cliente + fonte (como a sessão): cache sem rede e `garantir` que baixa o que falta. */
function ambiente(servidor: ServidorFalso) {
  const c = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  return {
    c,
    fonte: {
      lerCache: (o: OpcoesSincronizacao) => c.lerCache(o),
      garantir: (o: OpcoesSincronizacao) => garantirArquivos(c, o),
    },
  };
}

const abrir = (c: ReturnType<typeof ambiente>['c']) =>
  abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );

let montado: Record<string, unknown> | null = null;
type Fonte = Omit<ReturnType<typeof ambiente>['fonte'], 'garantir'> &
  Partial<Pick<ReturnType<typeof ambiente>['fonte'], 'garantir'>>;
function montar(jogos: RepositorioJogos, fonte: Fonte) {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(MeusJogos as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { jogos, fonte, aoConferir: () => undefined },
  });
  flushSync();
  return alvo;
}
function desmontar(): void {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
}

const cartoes = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('.lista > li'));
const esperarLista = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(cartoes(alvo)).toHaveLength(n));
const marcar = (li: Element) =>
  Array.from(li.querySelectorAll('button')).find((b) =>
    b.textContent?.includes(textos.novidade.marcarVisto),
  ) as HTMLButtonElement | undefined;

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/conferir?loteria=quina&secao=jogos';
});
afterEach(() => {
  desmontar();
  location.hash = '';
});

describe('Meus jogos: concurso novo', () => {
  it('destaca o jogo com concurso novo, com selo em texto, acertos e faixa; o jogo em dia fica sem destaque', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    const { c, fonte } = ambiente(servidor);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([
      jogoQuina('novo', [1, 2, 3, 4, 18], 7141), // 2 acertos: faixa
      jogoQuina('emdia', [10, 11, 12, 13, 14], 7142),
      jogoQuina('sem', [50, 51, 52, 53, 54], 7141), // 0 acertos
    ]);
    const alvo = montar(repo, fonte);
    await esperarLista(alvo, 3);
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(2),
    );

    const lis = cartoes(alvo);
    const comNovo = lis.filter((li) => li.classList.contains('com-novidade'));
    const emDia = lis.find((li) => !li.classList.contains('com-novidade'))!;
    expect(emDia.querySelector('.novidade')).toBeNull();
    expect(marcar(emDia)).toBeUndefined();

    const faixa = comNovo.find((li) => texto(li).includes('Faixa com prêmio'))!;
    expect(texto(faixa.querySelector('.selo'))).toBe('Novo: concurso 7142');
    expect(texto(faixa)).toContain('Concurso 7142: 2 acertos');
    expect(texto(faixa)).toContain('Faixa com prêmio: 2 ou mais acertos nesta loteria');
    const zero = comNovo.find((li) => !texto(li).includes('Faixa com prêmio'))!;
    expect(texto(zero)).toContain('Concurso 7142: 0 acertos');

    const t = texto(alvo);
    expect(t).toContain('2 jogos com concurso novo');
    expect(t).toContain('não calcula valor de prêmio');
    expect(t).toContain(textos.avisos.jogoResponsavel);
    // Ao entrar em Meus jogos, o histórico (que não estava em cache) é baixado sob demanda.
    await vi.waitFor(() => expect(servidor.pedidos).toContain('quina/historico.json'));
  });

  it('"Marcar como visto" atualiza conferidoAte, tira o destaque e a marca persiste ao recarregar', async () => {
    const fabrica = new IDBFactory();
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    const { c, fonte } = ambiente(servidor);
    await abrir(c);
    const repo = new RepositorioResiliente(fabrica);
    await repo.gravar([
      jogoQuina('a', [1, 2, 3, 4, 18], 7141),
      jogoQuina('b', [40, 41, 42, 43, 44], 7141),
    ]);

    let alvo = montar(repo, fonte);
    await esperarLista(alvo, 2);
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(2),
    );

    // Abrir a tela não marca nada sozinho.
    expect((await repo.listar()).map((j) => j.conferidoAte)).toEqual([7141, 7141]);

    const li = cartoes(alvo).find((x) => texto(x).includes('Concurso 7142: 3 acertos'))!;
    marcar(li)!.click();
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(1),
    );
    expect(texto(alvo.querySelector('.viva [role="status"]'))).toBe(textos.novidade.marcado);

    const salvos = await repo.listar();
    expect(salvos.find((j) => j.id === 'b')?.conferidoAte).toBe(7142);
    expect(salvos.find((j) => j.id === 'a')?.conferidoAte).toBe(7141);

    // "Recarregar a página": tela e repositório novos sobre o mesmo IndexedDB.
    desmontar();
    alvo = montar(new RepositorioResiliente(fabrica), fonte);
    await esperarLista(alvo, 2);
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(1),
    );
    const restante = alvo.querySelector('.com-novidade')!;
    expect(texto(restante)).toContain('Concurso 7142: 2 acertos');
  });

  it('marcar o último jogo pendente de uma loteria remove o resumo de novidade', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    const { c, fonte } = ambiente(servidor);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 18], 7141)]);
    const alvo = montar(repo, fonte);
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(1),
    );
    marcar(cartoes(alvo)[0]!)!.click();
    await vi.waitFor(() => expect(alvo.querySelectorAll('.com-novidade')).toHaveLength(0));
    expect(texto(alvo)).not.toContain('com concurso novo');
    expect(texto(alvo)).not.toContain(textos.novidade.premioNota);
  });

  it('jogo sem marca (salvo antes da 6.4) não vira novidade por abrir a tela', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    const { c, fonte } = ambiente(servidor);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 18], null)]);
    const alvo = montar(repo, fonte);
    await esperarLista(alvo, 1);
    await new Promise((ok) => setTimeout(ok, 30));
    expect(alvo.querySelectorAll('.com-novidade')).toHaveLength(0);
  });

  it('vários concursos novos: lista os mais recentes, o melhor e o total de anteriores', async () => {
    const servidor = new ServidorFalso();
    const dezenas = [
      [1, 2, 60, 61, 62],
      [1, 2, 3, 61, 62],
      [10, 11, 12, 13, 14],
      [20, 21, 22, 23, 24],
      [30, 31, 32, 33, 34],
      [40, 41, 42, 43, 44],
      [50, 51, 52, 53, 54],
    ];
    for (const [i, d] of dezenas.entries()) {
      await servidor.publicarConcursoQuina(7142 + i, d, '2026-10-15');
    }
    const { c, fonte } = ambiente(servidor);
    await abrir(c);
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 5], 7141)]);
    const alvo = montar(repo, fonte);
    // Primeiro aparece pendente; com o histórico baixado sob demanda, vira conferido.
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados li')).toHaveLength(5),
    );
    const li = alvo.querySelector('.com-novidade')!;
    expect(texto(li.querySelector('.resultados li'))).toBe('Concurso 7148: 0 acertos');
    expect(texto(li)).toContain('e mais 2 concursos anteriores');
    expect(texto(li)).toContain('Melhor entre os novos: 3 acertos (concurso 7143)');
    expect(texto(li.querySelector('.selo'))).toBe('Novo: concurso 7148');
  });

  it('sem rede nem histórico: avisa que falta o histórico, sem acertos inventados e sem botão de marcar', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    await servidor.publicarConcursoQuina(7143, [1, 2, 18, 60, 61], '2026-10-16');
    const { c } = ambiente(servidor);
    await abrir(c);
    const offline = {
      lerCache: (o: OpcoesSincronizacao) => c.lerCache(o),
      garantir: async () => ({ cache: null, falhou: true, falha: 'sem_rede' as const }),
    };
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 18], 7141)]);
    const alvo = montar(repo, offline);
    await vi.waitFor(() => expect(alvo.querySelectorAll('.com-novidade')).toHaveLength(1));
    const li = alvo.querySelector('.com-novidade')!;
    expect(texto(li)).toContain('o histórico ainda não está neste aparelho');
    expect(texto(li)).not.toContain('acertos');
    expect(marcar(li)).toBeUndefined();
  });

  it('sem `garantir` na fonte, a tela só lê o cache e nunca baixa', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    const { c } = ambiente(servidor);
    await abrir(c);
    servidor.pedidos.length = 0;
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 18], 7141)]);
    const alvo = montar(repo, { lerCache: (o: OpcoesSincronizacao) => c.lerCache(o) });
    await vi.waitFor(() =>
      expect(alvo.querySelectorAll('.com-novidade .resultados')).toHaveLength(1),
    );
    expect(servidor.pedidos).toEqual([]);
    expect(texto(alvo.querySelector('.com-novidade .resultados'))).toBe('Concurso 7142: 2 acertos');
  });
});
