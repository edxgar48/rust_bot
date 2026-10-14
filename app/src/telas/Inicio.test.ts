// Teste de componente da tela Início: um caso por estado, com dados das
// fixtures reais (testes/fixtures) servidas por um servidor falso (sem rede).
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixture } from '../../testes/servidor-falso';
import { abrirComCache, type SituacaoDados } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { estadoInicial } from '../dados/estado-sincronizacao';
import { OPCOES_DA_ABERTURA } from '../dados/sob-demanda';
import { textos } from '../lib/textos';
import Inicio from './Inicio.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');
const semRede = async (): Promise<Response> => {
  throw new TypeError('Failed to fetch');
};

function cliente(servidor: ServidorFalso, arm: ArmazenamentoEmMemoria, rede = true) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: rede ? servidor.fetch : semRede,
    armazenamento: arm,
    agora: () => T1,
  });
}

const abrir = (c: ReturnType<typeof cliente>): Promise<SituacaoDados> =>
  abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );

let montado: Record<string, unknown> | null = null;
function montar(situacao: SituacaoDados | null, ocupada = false, aoAtualizar = () => undefined) {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Inicio as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { situacao, ocupada, aoAtualizar },
  });
  flushSync();
  return alvo;
}

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
});

const cartoes = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('section'));
const botao = (alvo: HTMLElement, texto: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(texto));

/** Em todo estado: título e o aviso de independência dos sorteios. */
function conferirBase(alvo: HTMLElement): void {
  expect(alvo.querySelector('h2')?.textContent).toBe('Resultados');
  expect(alvo.textContent).toContain('Cada sorteio é independente');
  expect(alvo.textContent).toContain('nenhuma dezena tem mais chance');
}

describe('Início: carregando', () => {
  it('cache vazio e sincronizando: placeholder discreto, sem cartões nem botão de erro', () => {
    const alvo = montar({ estado: estadoInicial(null), cache: null }, true);
    conferirBase(alvo);
    expect(alvo.textContent).toContain(textos.inicio.carregando);
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(cartoes(alvo).every((c) => c.querySelector('h3') === null)).toBe(true);
    expect(botao(alvo, textos.inicio.tentarDeNovo)).toBeUndefined();
  });

  it('antes de a abertura responder (situação nula) também carrega', () => {
    const alvo = montar(null, true);
    expect(alvo.textContent).toContain(textos.inicio.carregando);
  });
});

describe('Início: sem dados (sem cache e sem rede)', () => {
  it('mensagem clara e "Tentar de novo" dispara nova sincronização', async () => {
    const final = await abrir(cliente(new ServidorFalso(), new ArmazenamentoEmMemoria(), false));
    const aoAtualizar = vi.fn();
    const alvo = montar(final, false, aoAtualizar);
    conferirBase(alvo);
    const erro = alvo.querySelector('[role="alert"]');
    expect(erro?.textContent).toContain(textos.sincronizacao.semDados);
    expect(cartoes(alvo).some((c) => c.querySelector('h3'))).toBe(false);
    botao(alvo, textos.inicio.tentarDeNovo)?.click();
    expect(aoAtualizar).toHaveBeenCalledTimes(1);
  });

  it('enquanto tenta de novo, o botão não aparece (volta a carregar)', () => {
    const alvo = montar({ estado: estadoInicial(null), cache: null }, true);
    expect(botao(alvo, textos.inicio.tentarDeNovo)).toBeUndefined();
  });
});

describe('Início: atualizado', () => {
  it('um cartão por loteria com concurso, data e as dezenas; Lotofácil em grade', async () => {
    const final = await abrir(cliente(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    const alvo = montar(final);
    conferirBase(alvo);
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(alvo.querySelector('[role="status"]')?.textContent).toContain(
      'Atualizado em 14/10/2026 09:30.',
    );

    const secoes = cartoes(alvo);
    expect(secoes.map((s) => s.querySelector('h3')?.textContent)).toEqual([
      'Lotofácil',
      'Mega-Sena',
      'Quina',
    ]);
    expect(secoes[0]?.textContent).toContain('Concurso 3803 · 13/10/2026');
    expect(secoes[1]?.textContent).toContain('Concurso 3069 · ');
    expect(secoes[2]?.textContent).toContain('Concurso 7141 · ');
    const bolas = secoes.map((s) => s.querySelectorAll('[role="img"]').length);
    expect(bolas).toEqual([15, 6, 5]);
    expect(secoes[0]?.querySelector('ul')?.getAttribute('data-disposicao')).toBe('grade');
    expect(secoes[1]?.querySelector('ul')?.getAttribute('data-disposicao')).toBe('linha');
    // As dezenas mostradas são as do ultimo.json (em ordem), com 2 dígitos.
    const esperado = JSON.parse(new TextDecoder().decode(lerFixture('lotofacil/ultimo.json')));
    const mostradas = Array.from(secoes[0]?.querySelectorAll('.disco') ?? []).map((d) =>
      Number(d.textContent?.trim()),
    );
    expect(mostradas).toEqual(esperado.dezenas);
  });

  it('"Atualizar" chama a sincronização e fica desativado enquanto ocupada', async () => {
    const final = await abrir(cliente(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    const aoAtualizar = vi.fn();
    const alvo = montar(final, false, aoAtualizar);
    botao(alvo, textos.inicio.atualizar)?.click();
    expect(aoAtualizar).toHaveBeenCalledTimes(1);

    const ocupado = montar(final, true);
    const b = botao(ocupado, textos.inicio.atualizando);
    expect(b?.disabled).toBe(true);
  });
});

describe('Início: dados em cache e sem rede', () => {
  it('mostra os dados e a data, sem tratar como erro', async () => {
    const arm = new ArmazenamentoEmMemoria();
    await abrir(cliente(new ServidorFalso(), arm));
    const final = await abrir(cliente(new ServidorFalso(), arm, false));
    const alvo = montar(final);
    conferirBase(alvo);
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(alvo.querySelector('[role="status"]')?.textContent).toContain(
      'Sem rede, mostrando dados de 14/10/2026 09:30.',
    );
    expect(alvo.textContent).toContain('Concurso 3803 · 13/10/2026');
    expect(alvo.querySelectorAll('section [role="img"]')).toHaveLength(26);
  });
});

describe('Início: dado recusado', () => {
  it('Aviso de erro e os dados anteriores continuam nos cartões', async () => {
    const arm = new ArmazenamentoEmMemoria();
    const servidor = new ServidorFalso();
    await abrir(cliente(servidor, arm));
    await servidor.trocar('megasena/ultimo.json', lerFixture('megasena/historico.json'));
    const adulterado = lerFixture('megasena/ultimo.json').slice();
    adulterado[adulterado.length - 3] = (adulterado[adulterado.length - 3] ?? 0) ^ 1;
    servidor.arquivos.set('megasena/ultimo.json', adulterado);
    const final = await abrir(cliente(servidor, arm));
    expect(final.estado.tipo).toBe('recusado');

    const alvo = montar(final);
    conferirBase(alvo);
    const erro = alvo.querySelector('[role="alert"]');
    expect(erro?.textContent).toContain('Erro:');
    expect(erro?.textContent).toContain('Dado recusado');
    expect(alvo.textContent).toContain('Concurso 3069 · ');
    expect(alvo.querySelectorAll('section [role="img"]')).toHaveLength(26);
    expect(botao(alvo, textos.inicio.atualizar)).toBeDefined();
  });
});

describe('Início: loteria null no índice', () => {
  it('cartão diz "Ainda sem dados desta loteria" e os outros seguem normais', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, megasena: null } });
    const final = await abrir(cliente(servidor, new ArmazenamentoEmMemoria()));
    const alvo = montar(final);
    const secoes = cartoes(alvo);
    expect(secoes[1]?.querySelector('h3')?.textContent).toBe('Mega-Sena');
    expect(secoes[1]?.textContent).toContain('Ainda sem dados desta loteria');
    expect(secoes[1]?.querySelectorAll('[role="img"]')).toHaveLength(0);
    expect(secoes[1]?.textContent).not.toContain('Concurso');
    expect(secoes[0]?.querySelectorAll('[role="img"]')).toHaveLength(15);
  });
});
