// Teste de componente da tela Força: um caso por estado, com as fixtures reais
// (testes/fixtures) servidas por um servidor falso. Sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { textos } from '../lib/textos';
import Forca from './Forca.svelte';

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
  montado = mount(Forca as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte },
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
const esperarLinhas = (alvo: HTMLElement, n: number) =>
  vi.waitFor(() => expect(linhas(alvo)).toHaveLength(n));
const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const radio = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) =>
    b.textContent?.includes(rotulo),
  );
const botao = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(rotulo));
const selecionada = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll('[role="radio"][aria-checked="true"]')).map((b) => texto(b));

describe('Força: carregando', () => {
  it('antes de o cache ser lido mostra o esqueleto discreto e nenhuma linha', () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(alvo.querySelector('h2')?.textContent).toBe('Tabela de força');
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(texto(alvo)).toContain(textos.forca.carregando);
    expect(linhas(alvo)).toHaveLength(0);
  });
});

describe('Força: atualizado (Lotofácil, padrão)', () => {
  it('título, subtítulo, perfil e janela, aviso de honestidade, cabeçalho e 25 linhas', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);

    expect(alvo.querySelector('h2')?.textContent).toBe('Tabela de força');
    expect(texto(alvo)).toContain('Próximo concurso 3804, com dados até o 3803');
    expect(texto(alvo)).toContain('Perfil padrao_v1 · janela de 50 concursos');
    expect(texto(alvo)).toContain(
      'O índice resume o passado de cada dezena. Ele não prevê o sorteio.',
    );
    expect(texto(alvo.querySelector('.cabecalho'))).toBe(
      'Pos. Dez. Variação Índice Evidência (grau · direção)',
    );
    expect(selecionada(alvo)).toEqual(['✓ Lotofácil', '✓ Lista']);
    expect(texto(alvo.querySelector('[role="status"][data-estado]'))).toBe(
      'Atualizado em 14/10/2026 09:30.',
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(alvo.querySelector('ul.lista')?.getAttribute('aria-label')).toBe(
      'Ranking das 25 dezenas da Lotofácil',
    );
  });

  it('cada linha: posição, dezena, variação com sinal, total com 3 casas e evidência por extenso', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    const [primeira, segunda] = linhas(alvo);
    expect(texto(primeira)).toContain('1º lugar');
    expect(primeira?.querySelector('[aria-label="Dezena 25"]')).not.toBeNull();
    expect(texto(primeira)).toContain('▲ +1');
    expect(texto(primeira)).toContain('subiu 1 posição');
    expect(texto(primeira)).toContain('Índice: 4,318');
    expect(texto(primeira)).toContain('Evidência: Fraca · viés');
    expect(segunda?.querySelector('[aria-label="Dezena 3"]')).not.toBeNull();
    expect(texto(segunda)).toContain('▼ −1');
    expect(texto(segunda)).toContain('caiu 1 posição');
    expect(texto(segunda)).toContain('Índice: 4,178');
    expect(texto(segunda)).toContain('Fraca · acaso');
    const posicoes = linhas(alvo).map(
      (l) => l.querySelector('.posicao [aria-hidden]')?.textContent,
    );
    expect(posicoes).toEqual(Array.from({ length: 25 }, (_, i) => `${i + 1}º`));
  });

  it('cada linha é um botão que abre o detalhe da dezena', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    expect(alvo.querySelectorAll('ul.lista button')).toHaveLength(25);
    expect(alvo.querySelector('[role="dialog"]')).toBeNull();
  });

  it('evidência nula vira "sem evidência calculada" e variação nula vira travessão', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('lotofacil/tabela_geral.json', (j) => {
      j.dezenas[0].evidencia_grau = null;
      j.dezenas[0].evidencia_direcao = null;
      j.dezenas[0].variacao_posicao = null;
    });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    const primeira = texto(linhas(alvo)[0]);
    expect(primeira).toContain('Evidência: sem evidência calculada');
    expect(primeira).toContain('sem concurso anterior para comparar');
    expect(primeira).toContain('—');
    expect(primeira).not.toContain('Fraca');
  });
});

describe('Força: troca de loteria', () => {
  it('Quina: 80 linhas, subtítulo próprio; a escolha vai para a URL e para o aparelho', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    const historicoAntes = history.length;
    radio(alvo, 'Quina')?.click();
    await esperarLinhas(alvo, 80);
    expect(texto(alvo)).toContain('Próximo concurso 7142, com dados até o 7141');
    expect(selecionada(alvo)).toContain('✓ Quina');
    expect(location.hash).toBe('#/forca?loteria=quina&visao=lista');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('quina');
    expect(history.length).toBe(historicoAntes); // replaceState: sem entrada nova
    expect(alvo.querySelector('ul.lista')?.getAttribute('aria-label')).toContain('Quina');
  });

  it('Mega-Sena: 60 linhas', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    radio(alvo, 'Mega-Sena')?.click();
    await esperarLinhas(alvo, 60);
    expect(texto(alvo)).toContain('Próximo concurso 3070, com dados até o 3069');
  });

  it('resposta atrasada de uma loteria anterior não substitui a escolhida', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const base = criarFonte(servidor, arm);
    let liberar: () => void = () => undefined;
    let pediuLotofacil = false;
    const portao = new Promise<void>((ok) => (liberar = ok));
    const fonte: Fonte = {
      lerCache: base.lerCache,
      garantir: async (o) => {
        if (o.loterias?.[0] === 'lotofacil') {
          pediuLotofacil = true;
          await portao;
        }
        return base.garantir(o);
      },
    };
    const alvo = montar(fonte);
    await vi.waitFor(() => expect(pediuLotofacil).toBe(true));
    radio(alvo, 'Quina')?.click();
    await esperarLinhas(alvo, 80);
    liberar();
    await new Promise((ok) => setTimeout(ok, 30));
    expect(linhas(alvo)).toHaveLength(80);
    expect(selecionada(alvo)).toContain('✓ Quina');
  });
});

describe('Força: loteria guardada', () => {
  it('a URL (?loteria=) vale mais que o aparelho', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'quina');
    location.hash = '#/forca?loteria=megasena';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 60);
    expect(selecionada(alvo)).toContain('✓ Mega-Sena');
  });

  it('sem parâmetro usa a última escolha guardada', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'quina');
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 80);
  });

  it('parâmetro e valor guardado inválidos caem na Lotofácil', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'loto');
    location.hash = '#/forca?loteria=timemania';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
  });

  it('localStorage bloqueado não quebra a tela', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('bloqueado');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('bloqueado');
    });
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    radio(alvo, 'Quina')?.click();
    await esperarLinhas(alvo, 80);
  });

  it('mudar o hash para outra loteria (voltar do navegador) troca a tabela', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarLinhas(alvo, 25);
    location.hash = '#/forca?loteria=quina';
    await esperarLinhas(alvo, 80);
    expect(selecionada(alvo)).toContain('✓ Quina');
  });
});

describe('Força: sem dados', () => {
  it('sem cache e sem rede: mensagem clara e "Tentar de novo" busca de novo', async () => {
    let rede = false;
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria(), () => rede));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.sincronizacao.semDados),
    );
    expect(linhas(alvo)).toHaveLength(0);
    expect(alvo.querySelector('[role="status"][data-estado]')).toBeNull();
    rede = true;
    botao(alvo, textos.forca.tentarDeNovo)?.click();
    await esperarLinhas(alvo, 25);
    expect(botao(alvo, textos.forca.tentarDeNovo)).toBeUndefined();
  });

  it('loteria null no índice: aviso informativo, sem erro', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, lotofacil: null } });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.forca.semLoteria));
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });
});

describe('Força: dados em cache e sem rede', () => {
  it('mostra a tabela guardada com a data do arquivo, sem tratar como erro', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({
      arquivos: ['tabela_geral'],
      loterias: ['lotofacil'],
    });
    const alvo = montar(criarFonte(servidor, arm, () => false));
    await esperarLinhas(alvo, 25);
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-estado]'))).toBe(
        'Sem rede, mostrando dados de 14/10/2026 09:30.',
      ),
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(texto(alvo)).toContain('Próximo concurso 3804, com dados até o 3803');
  });

  it('o cache aparece antes de a rede responder', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const base = criarFonte(servidor, arm);
    await base.garantir({ arquivos: ['tabela_geral'], loterias: ['lotofacil'] });
    const fonte: Fonte = { lerCache: base.lerCache, garantir: () => new Promise(() => undefined) };
    const alvo = montar(fonte);
    await esperarLinhas(alvo, 25);
    expect(texto(alvo.querySelector('[data-estado]'))).toContain('Verificando atualizações');
  });
});

describe('Força: dado recusado', () => {
  it('arquivo adulterado: Aviso de erro e a tabela anterior continua', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['tabela_geral'], loterias: ['quina'] });
    // Índice com hash novo, mas o conteúdo servido não confere.
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.tabela_geral.sha256 = 'c'.repeat(64);
    servidor.gravarIndice(indice);
    location.hash = '#/forca?loteria=quina';
    const alvo = montar(criarFonte(servidor, arm));
    await esperarLinhas(alvo, 80);
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    const erro = texto(alvo.querySelector('[role="alert"]'));
    expect(erro).toContain('Erro:');
    expect(erro).toContain('Dado recusado');
    expect(erro).toContain('14/10/2026 09:30');
    expect(linhas(alvo)).toHaveLength(80);
  });

  it('valor de evidência desconhecido: arquivo recusado, nunca chega à tela', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('lotofacil/tabela_geral.json', (j) => {
      j.dezenas[0].evidencia_grau = 'enorme';
    });
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
        textos.forca.recusadoSemAnterior,
      ),
    );
    expect(linhas(alvo)).toHaveLength(0);
    expect(texto(alvo)).not.toContain('enorme');
  });

  it('fixture adulterada na rede com cache: a tabela válida anterior segue', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({
      arquivos: ['tabela_geral'],
      loterias: ['lotofacil'],
    });
    const adulterada = lerFixtureTexto('lotofacil/tabela_geral.json').replace(
      '"fraca"',
      '"enorme"',
    );
    await servidor.trocar('lotofacil/tabela_geral.json', adulterada);
    const alvo = montar(criarFonte(servidor, arm));
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(linhas(alvo)).toHaveLength(25);
    expect(texto(alvo)).not.toContain('enorme');
  });
});
