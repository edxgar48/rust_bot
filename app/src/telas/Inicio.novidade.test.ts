// Início com e sem o aviso de conferência automática (6.4), usando um concurso
// simulado (7142 da Quina, que não existe nas fixtures reais) e o servidor falso.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';
import { afterEach, describe, expect, it } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE } from '../../testes/servidor-falso';
import { abrirComCache, type SituacaoDados } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { OPCOES_DA_ABERTURA } from '../dados/sob-demanda';
import type { NovidadeLoteria } from '../jogos/conferencia-automatica';
import { criarJogo, type JogoSalvo } from '../jogos/jogo';
import { calcularNovidades } from '../jogos/novidades';
import { RepositorioEmMemoria } from '../jogos/repositorio';
import { textos } from '../lib/textos';
import Inicio from './Inicio.svelte';
import { montarAvisosNovidade } from './inicio';

const T1 = new Date('2026-10-14T12:30:00Z');
const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();

function jogoQuina(id: string, dezenas: number[], conferidoAte: number | null): JogoSalvo {
  const r = criarJogo({ loteria: 'quina', dezenas, conferidoAte }, id, T1);
  if (!r.ok) throw new Error('fixture inválida');
  return r.valor;
}

async function cenario(
  jogos: JogoSalvo[],
  publicar = true,
): Promise<{ situacao: SituacaoDados; novidades: NovidadeLoteria[] }> {
  const servidor = new ServidorFalso();
  if (publicar) await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
  const c = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
  const situacao = await abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );
  const repo = new RepositorioEmMemoria();
  await repo.gravar(jogos);
  return { situacao, novidades: await calcularNovidades(repo, c) };
}

let montado: Record<string, unknown> | null = null;
function montar(situacao: SituacaoDados, novidades?: readonly NovidadeLoteria[]): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  const props: Record<string, unknown> = { situacao, ocupada: false, aoAtualizar: () => undefined };
  if (novidades !== undefined) props.novidades = novidades;
  montado = mount(Inicio as unknown as Component<Record<string, unknown>>, { target: alvo, props });
  flushSync();
  return alvo;
}

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
});

const avisos = (alvo: HTMLElement) => Array.from(alvo.querySelectorAll('.novidade'));

describe('Início: aviso de conferência automática', () => {
  it('com concurso novo: "Concurso N saiu", resumo, faixa, nota de prêmio e link para Meus jogos', async () => {
    const { situacao, novidades } = await cenario([
      jogoQuina('a', [1, 2, 3, 4, 18], 7141), // 7142: 1 e 18 = 2 acertos
      jogoQuina('b', [40, 41, 42, 43, 44], 7141), // 7142: 3 acertos
    ]);
    const alvo = montar(situacao, novidades);
    expect(avisos(alvo)).toHaveLength(1);
    const secao = avisos(alvo)[0]!.closest('section')!;
    expect(texto(secao.querySelector('h3'))).toBe('Concurso 7142 saiu: seus jogos salvos da Quina');
    const t = texto(secao);
    expect(t).toContain('2 jogos conferidos; melhor: 3 acertos');
    expect(t).toContain('2 jogos na faixa com prêmio (2 ou mais acertos)');
    expect(t).toContain('não calcula valor de prêmio');
    const link = secao.querySelector('a')!;
    expect(texto(link)).toBe(textos.novidade.verJogos);
    expect(link.getAttribute('href')).toBe('#/conferir?loteria=quina&secao=jogos');
    // Linguagem honesta e jogo responsável.
    expect(texto(alvo)).toContain(textos.avisos.jogoResponsavel);
    expect(texto(alvo)).toContain(textos.avisos.naoPreveConferir);
    expect(t.toLowerCase()).not.toMatch(/ganh|prev[eê] o pr/);
    // Os cartões de resultado continuam lá.
    expect(alvo.querySelectorAll('h3').length).toBeGreaterThanOrEqual(4);
  });

  it('sem jogos salvos: nenhum aviso', async () => {
    const { situacao, novidades } = await cenario([]);
    const alvo = montar(situacao, novidades);
    expect(avisos(alvo)).toHaveLength(0);
    expect(texto(alvo)).not.toContain('saiu: seus jogos');
  });

  it('jogos em dia (nenhum concurso novo): nenhum aviso', async () => {
    const { situacao, novidades } = await cenario([jogoQuina('a', [1, 2, 3, 4, 18], 7142)]);
    const alvo = montar(situacao, novidades);
    expect(avisos(alvo)).toHaveLength(0);
  });

  it('sem a propriedade (padrão): nenhum aviso e a tela segue igual', async () => {
    const { situacao } = await cenario([]);
    const alvo = montar(situacao);
    expect(avisos(alvo)).toHaveLength(0);
    expect(texto(alvo.querySelector('h2'))).toBe('Resultados');
  });

  it('jogo que espera o histórico: avisa sem inventar acertos', async () => {
    const servidor = new ServidorFalso();
    await servidor.publicarConcursoQuina(7142, [1, 18, 40, 41, 42]);
    await servidor.publicarConcursoQuina(7143, [1, 2, 18, 60, 61], '2026-10-16');
    const c = criarCliente({
      urlBase: URL_BASE_TESTE,
      fetch: servidor.fetch,
      armazenamento: new ArmazenamentoEmMemoria(),
      agora: () => T1,
    });
    const situacao = await abrirComCache(
      c,
      () => undefined,
      () => T1,
      OPCOES_DA_ABERTURA,
    );
    const repo = new RepositorioEmMemoria();
    await repo.gravar([jogoQuina('a', [1, 2, 3, 4, 18], 7141)]);
    const alvo = montar(situacao, await calcularNovidades(repo, c));
    const secao = avisos(alvo)[0]!.closest('section')!;
    expect(texto(secao.querySelector('h3'))).toContain('Concurso 7143 saiu');
    expect(texto(secao)).toContain('aguarda o histórico de concursos');
    expect(texto(secao)).not.toContain('acertos;');
    expect(texto(secao)).not.toContain('faixa com prêmio');
  });
});

describe('Início: região anunciada do aviso', () => {
  it('a região role="status" existe antes do aviso e é o mesmo elemento quando o cartão chega', async () => {
    const { situacao, novidades } = await cenario([jogoQuina('a', [1, 2, 3, 4, 18], 7141)]);
    expect(novidades).toHaveLength(1);
    const estado = new SvelteMap<string, readonly NovidadeLoteria[]>([['n', []]]);
    const alvo = document.createElement('div');
    document.body.appendChild(alvo);
    montado = mount(Inicio as unknown as Component<Record<string, unknown>>, {
      target: alvo,
      props: {
        situacao,
        ocupada: false,
        aoAtualizar: () => undefined,
        get novidades() {
          return estado.get('n');
        },
      },
    });
    flushSync();

    const regiao = alvo.querySelector('.novidades');
    expect(regiao?.getAttribute('role')).toBe('status');
    expect(texto(regiao)).toBe('');

    estado.set('n', novidades);
    flushSync();
    expect(alvo.querySelector('.novidades')).toBe(regiao);
    expect(texto(regiao?.querySelector('h3'))).toBe(
      'Concurso 7142 saiu: seus jogos salvos da Quina',
    );
  });
});

describe('montarAvisosNovidade', () => {
  it('lista vazia: nada', () => {
    expect(montarAvisosNovidade([])).toEqual([]);
  });

  it('mais de um concurso novo informa a quantidade; um acerto no singular', () => {
    const [a] = montarAvisosNovidade([
      {
        slug: 'megasena',
        ultimoConcurso: 3071,
        jogos: [
          {
            jogoId: 'x',
            estado: 'conferido',
            conferidoAte: 3069,
            ultimoConcurso: 3071,
            resultados: [
              { concurso: 3071, acertos: 1 },
              { concurso: 3070, acertos: 0 },
            ],
            totalConcursos: 2,
            melhor: { concurso: 3071, acertos: 1 },
            naFaixaPremio: false,
          },
        ],
        conferidos: 1,
        pendentes: 0,
        melhorAcertos: 1,
        naFaixaPremio: 0,
        concursosNovos: 2,
      },
    ]);
    expect(a?.titulo).toBe('Concurso 3071 saiu: seus jogos salvos da Mega-Sena');
    expect(a?.resumo).toBe('1 jogo conferido; melhor: 1 acerto');
    expect(a?.concursos).toContain('2 concursos novos');
    expect(a?.faixa).toBeNull();
    expect(a?.pendente).toBeNull();
    expect(a?.href).toBe('#/conferir?loteria=megasena&secao=jogos');
  });
});
