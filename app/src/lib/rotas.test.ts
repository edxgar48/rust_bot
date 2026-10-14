import { describe, expect, it } from 'vitest';
import {
  LOTERIA_PADRAO,
  ROTA_PADRAO,
  ehSlug,
  hashDaForca,
  hashDaRota,
  loteriaDoHash,
  rotaDoHash,
} from './rotas';
import { idsAbas } from './textos';

describe('rotas por hash', () => {
  it('cada aba tem a sua rota e ida e volta é identidade', () => {
    expect(idsAbas.map(hashDaRota)).toEqual([
      '#/inicio',
      '#/forca',
      '#/conferir',
      '#/estatisticas',
      '#/entenda',
    ]);
    for (const id of idsAbas) expect(rotaDoHash(hashDaRota(id))).toBe(id);
  });

  it('vazio, "#", "#/" e desconhecido caem no padrão (#/inicio)', () => {
    expect(ROTA_PADRAO).toBe('inicio');
    for (const h of ['', '#', '#/', '#/nada', '#forca', '#/forca/extra', '#catalogo', '/forca']) {
      expect(rotaDoHash(h)).toBe('inicio');
    }
  });

  it('tolera barra final e parâmetros', () => {
    expect(rotaDoHash('#/forca/')).toBe('forca');
    expect(rotaDoHash('#/entenda?x=1')).toBe('entenda');
    expect(rotaDoHash('#/conferir#ancora')).toBe('conferir');
  });

  it('a rota é minúscula', () => {
    expect(rotaDoHash('#/Forca')).toBe('inicio');
  });
});

describe('loteria na URL da Força', () => {
  it('padrão é a Lotofácil', () => {
    expect(LOTERIA_PADRAO).toBe('lotofacil');
  });

  it('ida e volta para as 3 loterias; a rota continua sendo a Força', () => {
    for (const slug of ['lotofacil', 'megasena', 'quina'] as const) {
      expect(loteriaDoHash(hashDaForca(slug))).toBe(slug);
      expect(rotaDoHash(hashDaForca(slug))).toBe('forca');
    }
    expect(hashDaForca('quina')).toBe('#/forca?loteria=quina');
  });

  it('ausente, vazio, desconhecido ou fora de maiúsculas/minúsculas vira null', () => {
    for (const h of [
      '',
      '#/forca',
      '#/forca?',
      '#/forca?loteria=',
      '#/forca?loteria=timemania',
      '#/forca?loteria=Quina',
      '#/forca?loteria=quinaa',
    ]) {
      expect(loteriaDoHash(h)).toBeNull();
    }
  });

  it('tolera outros parâmetros e âncora', () => {
    expect(loteriaDoHash('#/forca?x=1&loteria=megasena')).toBe('megasena');
    expect(loteriaDoHash('#/forca?loteria=quina&x=1')).toBe('quina');
    expect(loteriaDoHash('#/forca?loteria=quina#topo')).toBe('quina');
  });

  it('ehSlug só aceita os três slugs', () => {
    expect(ehSlug('quina')).toBe(true);
    expect(ehSlug('QUINA')).toBe(false);
    expect(ehSlug(null)).toBe(false);
    expect(ehSlug(undefined)).toBe(false);
  });
});
