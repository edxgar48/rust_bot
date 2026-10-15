// Teste de componente de Meus jogos (6.3): lista, estado vazio, renomear, apagar com
// confirmação e desfazer, exportar, importar, melhor resultado e avisos.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao, type ResultadoCache } from '../dados/cliente';
import { garantirArquivos } from '../dados/sob-demanda';
import type { Slug } from '../dados/tipos';
import { FORMATO_ARQUIVO } from '../jogos/arquivo';
import { criarJogo, type JogoSalvo } from '../jogos/jogo';
import { RepositorioEmMemoria } from '../jogos/repositorio';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import MeusJogos from './MeusJogos.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');
const QUINA_A = [5, 12, 33, 47, 71];
const QUINA_B = [1, 2, 3, 4, 5, 6];
const MEGA = [1, 2, 3, 4, 5, 6];

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const pausa = (ms = 20) => new Promise((ok) => setTimeout(ok, ms));

function jogo(
  id: string,
  loteria: Slug,
  dezenas: number[],
  nome: string | null = null,
  quando = '2026-10-14T12:00:00.000Z',
): JogoSalvo {
  const r = criarJogo({ loteria, dezenas, nome }, id, new Date(quando));
  if (!r.ok) throw new Error(`fixture inválida: ${r.motivo}`);
  return r.valor;
}

async function repoCom(...jogos: JogoSalvo[]): Promise<RepositorioEmMemoria> {
  const repo = new RepositorioEmMemoria();
  await repo.gravar(jogos);
  return repo;
}

type Fonte = { lerCache: (o: OpcoesSincronizacao) => Promise<ResultadoCache | null> };

const semCache: Fonte = { lerCache: async () => null };

/** Fonte com o histórico real já em cache (como depois de abrir a aba Histórico). */
async function fonteComHistorico(): Promise<Fonte> {
  const servidor = new ServidorFalso();
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  await garantirArquivos(cliente, { arquivos: ['historico'], loterias: ['quina'] });
  return { lerCache: (o) => cliente.lerCache(o) };
}

let montado: Record<string, unknown> | null = null;
function montar(
  jogos: RepositorioEmMemoria,
  fonte: Fonte = semCache,
  aoConferir: (j: JogoSalvo) => void = () => undefined,
): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(MeusJogos as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { jogos, fonte, aoConferir },
  });
  flushSync();
  return alvo;
}

const botao = (alvo: ParentNode, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === rotulo || b.getAttribute('aria-label')?.startsWith(rotulo),
  ) as HTMLButtonElement;
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  ) as HTMLButtonElement;
const cartoes = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('.lista > li'));
const status = (alvo: HTMLElement) => texto(alvo.querySelector('.viva [role="status"]'));
const esperarLista = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(cartoes(alvo)).toHaveLength(n));

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/conferir?loteria=quina&secao=jogos';
});

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  location.hash = '';
});

describe('Meus jogos: aviso, vazio e lista', () => {
  it('estado vazio honesto: diz que não há jogos, orienta e leva a Conferir jogo', async () => {
    const alvo = montar(await repoCom());
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.jogos.vazio('Quina')));
    expect(texto(alvo)).toContain(textos.jogos.vazioOrienta);
    expect(cartoes(alvo)).toHaveLength(0);
    expect(botao(alvo, textos.jogos.exportar).disabled).toBe(true);
    botao(alvo, textos.jogos.irParaConferir).click();
    expect(location.hash).toBe('#/conferir?loteria=quina');
  });

  it('diz que os jogos ficam só neste aparelho, que limpar os dados do navegador apaga e que exportar é o backup', async () => {
    const alvo = montar(await repoCom());
    const t = texto(alvo);
    expect(t).toContain('só neste aparelho');
    expect(t).toContain('Limpar os dados do navegador apaga os jogos');
    expect(t).toContain('O backup é o arquivo gerado por Exportar');
    expect(t).toContain('não há conta nem nuvem');
    expect(t).toContain(textos.avisos.jogoResponsavel);
    expect(t).toContain(textos.avisos.naoPreveConferir);
  });

  it('lista só os jogos da loteria escolhida, do mais recente ao mais antigo, com dezenas e nome', async () => {
    const repo = await repoCom(
      jogo('a', 'quina', QUINA_A, 'Aniversário', '2026-10-10T00:00:00Z'),
      jogo('b', 'quina', QUINA_B.slice(0, 5), null, '2026-10-12T00:00:00Z'),
      jogo('m', 'megasena', MEGA, 'Mega'),
    );
    const alvo = montar(repo);
    await esperarLista(alvo, 2);
    const [primeiro, segundo] = cartoes(alvo);
    expect(texto(primeiro!.querySelector('.nome'))).toBe(textos.jogos.semNome);
    expect(texto(segundo!.querySelector('.nome'))).toBe('Aniversário');
    expect(texto(segundo!.querySelector('.resumo'))).toBe('Quina · 5 dezenas');
    expect(texto(segundo!.querySelector('.so-leitor'))).toBe('Dezenas do jogo: 5, 12, 33, 47, 71.');
    expect(Array.from(segundo!.querySelectorAll('.dezena')).map(texto)).toEqual([
      '05',
      '12',
      '33',
      '47',
      '71',
    ]);
    expect(texto(alvo.querySelector('.titulo-lista'))).toBe('Jogos salvos da Quina: 2 jogos');

    radio(alvo, 'Mega-Sena').click();
    flushSync();
    await esperarLista(alvo, 1);
    expect(texto(cartoes(alvo)[0]!.querySelector('.nome'))).toBe('Mega');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('megasena');
    expect(location.hash).toBe('#/conferir?loteria=megasena&secao=jogos');
  });

  it('cada botão tem nome acessível que começa pelo texto visível e identifica o jogo', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A, 'Aniversário')));
    await esperarLista(alvo, 1);
    const nomes = Array.from(cartoes(alvo)[0]!.querySelectorAll('button')).map((b) => [
      texto(b),
      b.getAttribute('aria-label'),
    ]);
    expect(nomes).toEqual([
      ['Conferir', 'Conferir: Aniversário'],
      ['Renomear', 'Renomear: Aniversário'],
      ['Apagar', 'Apagar: Aniversário'],
    ]);
  });

  it('"Conferir" entrega o jogo à tela pai', async () => {
    const aoConferir = vi.fn();
    const j = jogo('a', 'quina', QUINA_A, 'Aniversário');
    const alvo = montar(await repoCom(j), semCache, aoConferir);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Conferir').click();
    expect(aoConferir).toHaveBeenCalledWith(j);
  });

  it('jogo salvo mostra a data do salvamento', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A, null, '2026-10-14T12:30:00Z')));
    await esperarLista(alvo, 1);
    expect(texto(cartoes(alvo)[0]!)).toContain('Salvo em 14/10/2026 09:30');
  });
});

describe('Meus jogos: melhor resultado histórico', () => {
  it('com o histórico em cache mostra o melhor resultado igual ao cálculo independente', async () => {
    const dados = Array.from(
      lerFixtureTexto('quina/historico.json').matchAll(
        /\[(\d+), "\d{4}-\d{2}-\d{2}", \[([\d, ]+)\]\]/g,
      ),
      (m) => ({ concurso: Number(m[1]), dezenas: (m[2] as string).split(',').map(Number) }),
    );
    const acertos = dados.map((c) => c.dezenas.filter((d) => QUINA_A.includes(d)).length);
    const melhor = Math.max(...acertos);
    const recente = dados.filter((_, i) => acertos[i] === melhor).at(-1)!.concurso;

    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A)), await fonteComHistorico());
    await esperarLista(alvo, 1);
    await vi.waitFor(() =>
      expect(texto(cartoes(alvo)[0]!.querySelector('.melhor'))).toBe(
        textos.jogos.melhor(melhor, recente),
      ),
    );
  });

  it('sem histórico em cache: omite a linha, sem erro nem aviso', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A)), semCache);
    await esperarLista(alvo, 1);
    await pausa();
    expect(alvo.querySelector('.melhor')).toBeNull();
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('cache que falha ao ler: omite a linha e a lista continua', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A)), {
      lerCache: async () => {
        throw new Error('banco quebrado');
      },
    });
    await esperarLista(alvo, 1);
    await pausa();
    expect(alvo.querySelector('.melhor')).toBeNull();
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });
});

describe('Meus jogos: renomear', () => {
  it('abre o campo com o nome atual e foco; salva, anuncia e atualiza a lista', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Antigo'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Renomear').click();
    await vi.waitFor(() => expect(alvo.querySelector('.edicao input')).not.toBeNull());
    const campo = alvo.querySelector<HTMLInputElement>('.edicao input')!;
    expect(campo.value).toBe('Antigo');
    expect(campo.maxLength).toBe(40);
    expect(document.activeElement).toBe(campo);
    expect(alvo.querySelector(`label[for="${campo.id}"]`)).not.toBeNull();

    campo.value = '  Novo   nome ';
    campo.dispatchEvent(new Event('input', { bubbles: true }));
    alvo.querySelector('.edicao')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() => expect(alvo.querySelector('.edicao')).toBeNull());
    expect(status(alvo)).toBe(textos.jogos.nomeAtualizado);
    expect(texto(cartoes(alvo)[0]!.querySelector('.nome'))).toBe('Novo nome');
    expect((await repo.listar())[0]!.nome).toBe('Novo nome');
  });

  it('nome vazio remove o nome; nome longo demais mostra erro e não grava', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Antigo'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Renomear').click();
    await vi.waitFor(() => expect(alvo.querySelector('.edicao input')).not.toBeNull());
    const campo = alvo.querySelector<HTMLInputElement>('.edicao input')!;

    campo.value = 'x'.repeat(41);
    campo.dispatchEvent(new Event('input', { bubbles: true }));
    alvo.querySelector('.edicao')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() => expect(alvo.querySelector('.edicao [role="alert"]')).not.toBeNull());
    expect(texto(alvo.querySelector('.edicao [role="alert"]'))).toBe(textos.jogos.nomeInvalido(40));
    expect(campo.getAttribute('aria-invalid')).toBe('true');
    expect((await repo.listar())[0]!.nome).toBe('Antigo');

    campo.value = '   ';
    campo.dispatchEvent(new Event('input', { bubbles: true }));
    alvo.querySelector('.edicao')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() => expect(alvo.querySelector('.edicao')).toBeNull());
    expect((await repo.listar())[0]!.nome).toBeNull();
    expect(texto(cartoes(alvo)[0]!.querySelector('.nome'))).toBe(textos.jogos.semNome);
  });

  it('Esc e Cancelar descartam a edição', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Antigo'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Renomear').click();
    await vi.waitFor(() => expect(alvo.querySelector('.edicao input')).not.toBeNull());
    alvo
      .querySelector('.edicao input')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    expect(alvo.querySelector('.edicao')).toBeNull();
    // O foco volta ao botão Renomear que abriu a edição.
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(botao(cartoes(alvo)[0]!, 'Renomear')),
    );

    botao(cartoes(alvo)[0]!, 'Renomear').click();
    await vi.waitFor(() => expect(alvo.querySelector('.edicao input')).not.toBeNull());
    botao(alvo, textos.jogos.cancelar).click();
    flushSync();
    expect(alvo.querySelector('.edicao')).toBeNull();
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(botao(cartoes(alvo)[0]!, 'Renomear')),
    );
    expect((await repo.listar())[0]!.nome).toBe('Antigo');
  });

  it('salvar o nome devolve o foco ao botão Renomear', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A, 'Antigo')));
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Renomear').click();
    await vi.waitFor(() => expect(alvo.querySelector('.edicao input')).not.toBeNull());
    alvo.querySelector('.edicao')!.dispatchEvent(new Event('submit', { cancelable: true }));
    await vi.waitFor(() => expect(alvo.querySelector('.edicao')).toBeNull());
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(botao(cartoes(alvo)[0]!, 'Renomear')),
    );
  });
});

describe('Meus jogos: apagar com confirmação e desfazer', () => {
  it('pede confirmação em diálogo; cancelar mantém o jogo e devolve o foco ao botão', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Aniversário'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    const apagar = botao(cartoes(alvo)[0]!, 'Apagar');
    apagar.focus();
    apagar.click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    const dialogo = document.querySelector('[role="dialog"]')!;
    expect(dialogo.getAttribute('aria-modal')).toBe('true');
    expect(texto(dialogo)).toContain(textos.jogos.confirmaTitulo);
    expect(texto(dialogo)).toContain('Aniversário');
    expect(texto(dialogo)).toContain('não há outra cópia');
    expect(await repo.listar()).toHaveLength(1);

    botao(dialogo, textos.jogos.cancelar).click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).toBeNull());
    expect(await repo.listar()).toHaveLength(1);
    expect(document.activeElement).toBe(apagar);
  });

  it('Esc fecha o diálogo sem apagar', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Apagar').click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    document
      .querySelector('[role="dialog"]')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).toBeNull());
    expect(await repo.listar()).toHaveLength(1);
  });

  it('confirmar apaga, anuncia, e "Desfazer" traz o mesmo jogo de volta', async () => {
    const original = jogo('a', 'quina', QUINA_A, 'Aniversário');
    const repo = await repoCom(original, jogo('b', 'quina', [10, 20, 30, 40, 50]));
    const alvo = montar(repo);
    await esperarLista(alvo, 2);
    const idx = cartoes(alvo).findIndex((c) => texto(c.querySelector('.nome')) === 'Aniversário');
    botao(cartoes(alvo)[idx]!, 'Apagar').click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    botao(document.querySelector('[role="dialog"]')!, textos.jogos.confirmaApagar).click();
    await esperarLista(alvo, 1);
    expect(status(alvo)).toBe(textos.jogos.apagado);
    expect(await repo.listar()).toHaveLength(1);
    expect(document.activeElement).toBe(alvo.querySelector('.titulo-lista'));

    botao(alvo, textos.jogos.desfazer).click();
    await esperarLista(alvo, 2);
    expect(status(alvo)).toBe(textos.jogos.desfeito);
    expect(botao(alvo, textos.jogos.desfazer)).toBeUndefined();
    expect((await repo.listar()).find((j) => j.id === 'a')).toEqual(original);
  });
});

describe('Meus jogos: exportar', () => {
  function espiarDownload() {
    const blobs: Blob[] = [];
    const nomes: string[] = [];
    URL.createObjectURL = vi.fn((b: Blob | MediaSource) => {
      blobs.push(b as Blob);
      return 'blob:teste';
    });
    URL.revokeObjectURL = vi.fn();
    vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      nomes.push(this.download);
    });
    return { blobs, nomes };
  }

  it('baixa um arquivo JSON versionado com todos os jogos (de todas as loterias) e anuncia', async () => {
    const { blobs, nomes } = espiarDownload();
    const repo = await repoCom(
      jogo('a', 'quina', QUINA_A, 'Aniversário'),
      jogo('m', 'megasena', MEGA),
    );
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(alvo, textos.jogos.exportar).click();
    await vi.waitFor(() => expect(blobs).toHaveLength(1));
    expect(nomes[0]).toMatch(/^jogos-forca-loterias-\d{4}-\d{2}-\d{2}\.json$/);
    expect(blobs[0]!.type).toContain('application/json');
    const conteudo = JSON.parse(await blobs[0]!.text()) as { formato: string; jogos: unknown[] };
    expect(conteudo.formato).toBe(FORMATO_ARQUIVO);
    expect(conteudo.jogos).toHaveLength(2);
    await vi.waitFor(() => expect(status(alvo)).toBe(textos.jogos.exportado(2, nomes[0]!)));
  });

  it('sem jogos, o botão fica desabilitado e nada é baixado', async () => {
    const { blobs } = espiarDownload();
    const alvo = montar(await repoCom());
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.jogos.vazio('Quina')));
    const exportar = botao(alvo, textos.jogos.exportar);
    expect(exportar.disabled).toBe(true);
    exportar.click();
    expect(blobs).toHaveLength(0);
  });
});

describe('Meus jogos: importar', () => {
  async function importar(alvo: HTMLElement, conteudo: string, nome = 'jogos.json') {
    const campo = alvo.querySelector<HTMLInputElement>('input[type="file"]')!;
    const arquivo = new File([conteudo], nome, { type: 'application/json' });
    Object.defineProperty(campo, 'files', { value: [arquivo], configurable: true });
    campo.dispatchEvent(new Event('change', { bubbles: true }));
    await vi.waitFor(() => expect(status(alvo)).not.toBe(''));
    await pausa();
  }
  const arquivoCom = (itens: unknown[]) =>
    JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 1, jogos: itens });

  it('o botão Importar abre o seletor de arquivo (campo escondido do teclado e do leitor de tela)', async () => {
    const alvo = montar(await repoCom());
    const campo = alvo.querySelector<HTMLInputElement>('input[type="file"]')!;
    const abrir = vi.spyOn(campo, 'click');
    botao(alvo, textos.jogos.importar).click();
    expect(abrir).toHaveBeenCalled();
    expect(campo.accept).toContain('.json');
    expect(campo.getAttribute('aria-hidden')).toBe('true');
  });

  it('importa, não duplica o que já existe e relata entraram, ignorados e inválidos', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Já tenho'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    await importar(
      alvo,
      arquivoCom([
        { ...jogo('x', 'quina', QUINA_A, 'Igual'), extra: 1 },
        jogo('y', 'quina', [10, 20, 30, 40, 50], 'Novo'),
        { id: 'z', loteria: 'quina', dezenas: [1, 2] },
        'lixo',
      ]),
    );
    expect(status(alvo)).toBe(textos.jogos.resultadoImportacao(1, 1, 2));
    expect(status(alvo)).toBe('1 jogo importado; 1 ignorado, pois já estava salvo; 2 inválidos.');
    await esperarLista(alvo, 2);
    expect((await repo.listar()).map((j) => j.nome).sort()).toEqual(['Já tenho', 'Novo']);
  });

  it('importa jogos de outra loteria e a lista mostra ao trocar de loteria', async () => {
    const repo = await repoCom();
    const alvo = montar(repo);
    await importar(alvo, arquivoCom([jogo('m', 'megasena', MEGA, 'Mega')]));
    expect(status(alvo)).toBe(textos.jogos.resultadoImportacao(1, 0, 0));
    expect(cartoes(alvo)).toHaveLength(0); // a lista é da Quina
    radio(alvo, 'Mega-Sena').click();
    flushSync();
    await esperarLista(alvo, 1);
  });

  it.each([
    ['malformado', '{"formato": "forca', 'malformado'],
    ['outro formato', JSON.stringify({ qualquer: 1 }), 'formato'],
    ['versão futura', JSON.stringify({ formato: FORMATO_ARQUIVO, versao: 7, jogos: [] }), 'versao'],
  ] as const)(
    'arquivo %s é recusado com mensagem e não altera nada',
    async (_n, conteudo, motivo) => {
      const repo = await repoCom(jogo('a', 'quina', QUINA_A, 'Intacto'));
      const alvo = montar(repo);
      await esperarLista(alvo, 1);
      const antes = await repo.listar();
      await importar(alvo, conteudo);
      expect(status(alvo)).toBe(textos.jogos.recusa[motivo]);
      expect(alvo.querySelector('.viva [data-tipo="erro"]')).not.toBeNull();
      expect(await repo.listar()).toEqual(antes);
      expect(cartoes(alvo)).toHaveLength(1);
    },
  );

  it('o seletor é limpo depois de usar, para poder escolher o mesmo arquivo de novo', async () => {
    const alvo = montar(await repoCom());
    await importar(alvo, arquivoCom([]));
    const campo = alvo.querySelector<HTMLInputElement>('input[type="file"]')!;
    expect(campo.value).toBe('');
  });
});

describe('Meus jogos: falhas do armazenamento', () => {
  it('leitura que falha mostra o erro, não "Nenhum jogo salvo", e bloqueia Exportar; tentar de novo recupera', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A));
    const listar = vi.spyOn(repo, 'listar').mockRejectedValueOnce(new Error('idb'));
    const alvo = montar(repo);
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.jogos.falhaListar));
    expect(texto(alvo)).not.toContain(textos.jogos.vazio('Quina'));
    expect(alvo.querySelector('[role="alert"]')).not.toBeNull();
    expect(alvo.querySelector('.lista')).toBeNull();
    expect(botao(alvo, textos.jogos.exportar).disabled).toBe(true);
    // Não é o aviso de armazenamento volátil (modo privado).
    expect(texto(alvo)).not.toContain(textos.jogos.volatil);

    botao(alvo, textos.jogos.tentarDeNovo).click();
    await esperarLista(alvo, 1);
    expect(listar).toHaveBeenCalledTimes(2);
    expect(texto(alvo)).not.toContain(textos.jogos.falhaListar);
    expect(botao(alvo, textos.jogos.exportar).disabled).toBe(false);
  });

  it('apagar que falha usa o texto próprio e o jogo continua na lista', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A));
    vi.spyOn(repo, 'apagar').mockRejectedValue(new Error('idb'));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Apagar').click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    botao(document.querySelector('[role="dialog"]')!, textos.jogos.confirmaApagar).click();
    await vi.waitFor(() => expect(status(alvo)).toBe(textos.jogos.apagarFalhou));
    expect(cartoes(alvo)).toHaveLength(1);
    expect(textos.jogos.apagarFalhou).not.toBe(textos.jogos.falhaListar);
  });

  it('desfazer que falha usa o texto próprio', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A));
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    botao(cartoes(alvo)[0]!, 'Apagar').click();
    await vi.waitFor(() => expect(document.querySelector('[role="dialog"]')).not.toBeNull());
    botao(document.querySelector('[role="dialog"]')!, textos.jogos.confirmaApagar).click();
    await vi.waitFor(() => expect(botao(alvo, textos.jogos.desfazer)).toBeTruthy());
    vi.spyOn(repo, 'gravar').mockRejectedValue(new Error('idb'));
    botao(alvo, textos.jogos.desfazer).click();
    await vi.waitFor(() => expect(status(alvo)).toBe(textos.jogos.desfazerFalhou));
    expect(textos.jogos.desfazerFalhou).not.toBe(textos.jogos.falhaListar);
  });
});

describe('Meus jogos: muitos jogos', () => {
  it('calcula o melhor resultado de todos aos poucos, sem travar, e igual ao cálculo independente', async () => {
    const todos = Array.from({ length: 65 }, (_, i) =>
      jogo(
        `j${i}`,
        'quina',
        [i + 1, i + 2, i + 3, i + 4, i + 5],
        null,
        `2026-10-14T12:${String(i % 60).padStart(2, '0')}:00.000Z`,
      ),
    );
    const dados = Array.from(
      lerFixtureTexto('quina/historico.json').matchAll(
        /\[(\d+), "\d{4}-\d{2}-\d{2}", \[([\d, ]+)\]\]/g,
      ),
      (m) => (m[2] as string).split(',').map(Number),
    );
    const alvo = montar(await repoCom(...todos), await fonteComHistorico());
    await esperarLista(alvo, 65);
    await vi.waitFor(() => expect(alvo.querySelectorAll('.melhor')).toHaveLength(65));
    for (const c of cartoes(alvo)) {
      const dezenas = Array.from(c.querySelectorAll('.dezena'), (e) => Number(e.textContent));
      const melhor = Math.max(...dados.map((d) => d.filter((x) => dezenas.includes(x)).length));
      expect(texto(c.querySelector('.melhor'))).toContain(
        `${melhor} ${melhor === 1 ? 'acerto' : 'acertos'}`,
      );
    }
  });
});

describe('Meus jogos: armazenamento volátil', () => {
  it('avisa (com alerta) quando o navegador não guarda os jogos neste aparelho', async () => {
    const repo = await repoCom(jogo('a', 'quina', QUINA_A));
    Object.defineProperty(repo, 'aviso', { value: 'IndexedDB indisponível', configurable: true });
    const alvo = montar(repo);
    await esperarLista(alvo, 1);
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.jogos.volatil);
  });

  it('armazenamento normal: nenhum alerta', async () => {
    const alvo = montar(await repoCom(jogo('a', 'quina', QUINA_A)));
    await esperarLista(alvo, 1);
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });
});
