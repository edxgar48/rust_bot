// Testes de componente sem biblioteca extra: `mount` do próprio Svelte no jsdom.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import BarraNavegacao from './BarraNavegacao.svelte';
import BolaDezena from './BolaDezena.svelte';
import Segmentado from './Segmentado.svelte';
import Volante from './Volante.svelte';

let montado: Record<string, unknown> | null = null;
function montar<P extends Record<string, unknown>>(componente: unknown, props: P): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(componente as Component<P>, { target: alvo, props });
  flushSync();
  return alvo;
}

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
});

describe('BolaDezena', () => {
  it('somente leitura: imagem com rótulo da dezena e do estado', () => {
    const alvo = montar(BolaDezena, { numero: 7, estado: 'sorteada' });
    const el = alvo.querySelector('[role="img"]');
    expect(el?.getAttribute('aria-label')).toBe('Dezena 7, sorteada');
    expect(alvo.querySelector('button')).toBeNull();
  });

  it('com aoAlternar vira botão com aria-pressed e chama o callback', () => {
    const aoAlternar = vi.fn();
    const alvo = montar(BolaDezena, { numero: 12, estado: 'selecionada', aoAlternar });
    const botao = alvo.querySelector('button');
    expect(botao?.getAttribute('aria-label')).toBe('Dezena 12');
    expect(botao?.getAttribute('aria-pressed')).toBe('true');
    botao?.click();
    expect(aoAlternar).toHaveBeenCalledWith(12);
  });

  it('não selecionada: aria-pressed false', () => {
    const alvo = montar(BolaDezena, { numero: 3, aoAlternar: () => undefined });
    expect(alvo.querySelector('button')?.getAttribute('aria-pressed')).toBe('false');
  });
});

describe('Volante', () => {
  it('seleção: um botão por dezena (60 na Mega-Sena) com aria-pressed correto', () => {
    const alvo = montar(Volante, {
      loteria: 'megasena',
      modo: 'selecao',
      selecionadas: [5, 60],
      aoAlternar: () => undefined,
    });
    const botoes = [...alvo.querySelectorAll('button')];
    expect(botoes).toHaveLength(60);
    const marcados = botoes.filter((b) => b.getAttribute('aria-pressed') === 'true');
    expect(marcados.map((b) => b.getAttribute('aria-label'))).toEqual(['Dezena 5', 'Dezena 60']);
    expect(alvo.querySelector('[role="group"]')?.getAttribute('aria-label')).toBe(
      'Volante da Mega-Sena',
    );
    expect(alvo.querySelector<HTMLElement>('.grade')?.style.getPropertyValue('--colunas')).toBe(
      '10',
    );
  });

  it('leitura: rótulo informa posição e faixa; sem botões', () => {
    const alvo = montar(Volante, {
      loteria: 'lotofacil',
      modo: 'leitura',
      posicoes: { 1: 1, 2: 25 },
      destacadas: [1],
    });
    expect(alvo.querySelectorAll('button')).toHaveLength(0);
    const imagens = [...alvo.querySelectorAll('[role="img"]')];
    expect(imagens).toHaveLength(25);
    expect(imagens[0]?.getAttribute('aria-label')).toBe(
      'Dezena 1, posição 1, faixa 1 de 5, em destaque',
    );
    expect(imagens[1]?.getAttribute('aria-label')).toBe('Dezena 2, posição 25, faixa 5 de 5');
    expect(imagens[2]?.getAttribute('aria-label')).toBe('Dezena 3, sem posição');
  });
});

describe('Volante: células que abrem o detalhe', () => {
  // Posição inversa à dezena (a dezena N é a 1ª): prova que a célula não confunde as duas.
  const posicoesDe = (total: number): Record<number, number> =>
    Object.fromEntries(Array.from({ length: total }, (_, i) => [i + 1, total - i]));

  it.each([
    ['lotofacil', 25, 5],
    ['megasena', 60, 10],
    ['quina', 80, 10],
  ] as const)(
    '%s: %i botões na ordem do volante, rótulo "Dezena N, P lugar"',
    (slug, total, colunas) => {
      const aoEscolher = vi.fn();
      const alvo = montar(Volante, {
        loteria: slug,
        modo: 'leitura',
        posicoes: posicoesDe(total),
        aoEscolher,
      });
      const botoes = [...alvo.querySelectorAll('button')];
      expect(botoes).toHaveLength(total);
      expect(botoes.map((b) => b.getAttribute('aria-label'))).toEqual(
        Array.from({ length: total }, (_, i) => `Dezena ${i + 1}, ${total - i}º lugar`),
      );
      expect(alvo.querySelector<HTMLElement>('.grade')?.style.getPropertyValue('--colunas')).toBe(
        String(colunas),
      );
      // Cada célula mostra a dezena e a posição.
      expect(botoes[0]?.textContent?.replace(/\s+/g, ' ').trim()).toBe(`01 ${total}º`);
      botoes[2]?.click();
      expect(aoEscolher).toHaveBeenCalledWith(3);
      // Faixa pelo atributo (cor) e legenda das 5 faixas.
      expect(botoes[0]?.querySelector('[data-faixa]')?.getAttribute('data-faixa')).toBe('5');
      expect(botoes[total - 1]?.querySelector('[data-faixa]')?.getAttribute('data-faixa')).toBe(
        '1',
      );
      expect(alvo.querySelectorAll('.legenda li')).toHaveLength(5);
    },
  );

  it('dezena sem posição fica somente leitura (nenhum botão que não leva a nada)', () => {
    const alvo = montar(Volante, {
      loteria: 'lotofacil',
      modo: 'leitura',
      posicoes: { 1: 1 },
      aoEscolher: () => undefined,
    });
    expect(alvo.querySelectorAll('button')).toHaveLength(1);
    expect(alvo.querySelectorAll('[role="img"]')).toHaveLength(24);
  });
});

describe('Segmentado', () => {
  const opcoes = [
    { valor: 'a', rotulo: 'A' },
    { valor: 'b', rotulo: 'B' },
    { valor: 'c', rotulo: 'C' },
  ];

  it('radiogroup com aria-checked e uma só parada de Tab', () => {
    const alvo = montar(Segmentado, { opcoes, valor: 'b', rotuloGrupo: 'Teste' });
    expect(alvo.querySelector('[role="radiogroup"]')?.getAttribute('aria-label')).toBe('Teste');
    const radios = [...alvo.querySelectorAll('[role="radio"]')];
    expect(radios.map((r) => r.getAttribute('aria-checked'))).toEqual(['false', 'true', 'false']);
    expect(radios.map((r) => r.getAttribute('tabindex'))).toEqual(['-1', '0', '-1']);
  });

  it('seta para a direita marca a próxima opção', () => {
    const aoMudar = vi.fn();
    const alvo = montar(Segmentado, { opcoes, valor: 'a', rotuloGrupo: 'Teste', aoMudar });
    const primeiro = alvo.querySelector('[role="radio"]');
    primeiro?.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    flushSync();
    expect(aoMudar).toHaveBeenCalledWith('b');
  });
});

describe('BarraNavegacao', () => {
  it('marca só a aba atual com aria-current="page"', () => {
    const abas = [
      { id: 'x', rotulo: 'X', icone: 'inicio' },
      { id: 'y', rotulo: 'Y', icone: 'forca' },
    ] as const;
    const alvo = montar(BarraNavegacao, {
      abas,
      atual: 'y',
      rotulo: 'Principal',
      aoSelecionar: () => undefined,
    });
    expect(alvo.querySelector('nav')?.getAttribute('aria-label')).toBe('Principal');
    const botoes = [...alvo.querySelectorAll('button')];
    expect(botoes.map((b) => b.getAttribute('aria-current'))).toEqual([null, 'page']);
  });
});
