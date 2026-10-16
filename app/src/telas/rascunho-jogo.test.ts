// Rascunho de jogo (7.5): estado em memória de módulo, sem rede e sem disco.
import { beforeEach, describe, expect, it } from 'vitest';
import { QUANTIDADE_JOGO } from './conferir';
import {
  alternarNoRascunho,
  consumirRascunho,
  dezenasDoRascunho,
  limparRascunho,
  pedirAbertura,
} from './rascunho-jogo';

beforeEach(() => {
  limparRascunho();
  localStorage.clear();
});

describe('rascunho de jogo', () => {
  it('começa vazio para toda loteria', () => {
    expect(dezenasDoRascunho('lotofacil')).toEqual([]);
    expect(dezenasDoRascunho('quina')).toEqual([]);
    expect(consumirRascunho()).toBeNull();
  });

  it('insere sem duplicar, em ordem crescente; tocar de novo remove', () => {
    expect(alternarNoRascunho('quina', 33)).toEqual({ tipo: 'inserida' });
    expect(alternarNoRascunho('quina', 5)).toEqual({ tipo: 'inserida' });
    expect(dezenasDoRascunho('quina')).toEqual([5, 33]);
    expect(alternarNoRascunho('quina', 33)).toEqual({ tipo: 'removida' });
    expect(dezenasDoRascunho('quina')).toEqual([5]);
    expect(alternarNoRascunho('quina', 5)).toEqual({ tipo: 'removida' });
    expect(dezenasDoRascunho('quina')).toEqual([]);
  });

  it.each(['lotofacil', 'megasena', 'quina'] as const)(
    '%s: ao chegar no máximo do volante não adiciona e avisa "cheio"',
    (slug) => {
      const maximo = QUANTIDADE_JOGO[slug].maximo;
      for (let d = 1; d <= maximo; d++)
        expect(alternarNoRascunho(slug, d)).toEqual({ tipo: 'inserida' });
      expect(alternarNoRascunho(slug, maximo + 1)).toEqual({ tipo: 'cheio', maximo });
      expect(dezenasDoRascunho(slug)).toHaveLength(maximo);
      // Cheio ainda deixa remover.
      expect(alternarNoRascunho(slug, 1)).toEqual({ tipo: 'removida' });
      expect(alternarNoRascunho(slug, maximo + 1)).toEqual({ tipo: 'inserida' });
    },
  );

  it('inserir em outra loteria começa um rascunho novo', () => {
    alternarNoRascunho('quina', 5);
    alternarNoRascunho('quina', 6);
    expect(alternarNoRascunho('megasena', 10)).toEqual({ tipo: 'inserida' });
    expect(dezenasDoRascunho('megasena')).toEqual([10]);
    expect(dezenasDoRascunho('quina')).toEqual([]);
  });

  it('só é consumido depois de pedirAbertura, como jogo inicial sem nome, e uma única vez', () => {
    alternarNoRascunho('megasena', 40);
    alternarNoRascunho('megasena', 7);
    expect(consumirRascunho()).toBeNull(); // abrir Conferir por outro caminho não usa o rascunho
    expect(dezenasDoRascunho('megasena')).toEqual([7, 40]); // continua guardado
    pedirAbertura();
    expect(consumirRascunho()).toEqual({ loteria: 'megasena', dezenas: [7, 40], nome: null });
    expect(consumirRascunho()).toBeNull();
    expect(dezenasDoRascunho('megasena')).toEqual([]);
  });

  it('mexer no rascunho depois de pedir a abertura cancela o pedido', () => {
    alternarNoRascunho('quina', 5);
    pedirAbertura();
    alternarNoRascunho('quina', 6);
    expect(consumirRascunho()).toBeNull();
  });

  it('pedirAbertura sem rascunho não deixa pedido pendente', () => {
    pedirAbertura();
    alternarNoRascunho('quina', 5);
    expect(consumirRascunho()).toBeNull();
  });

  it('não grava nada no aparelho', () => {
    alternarNoRascunho('quina', 5);
    expect(localStorage.length).toBe(0);
  });
});
