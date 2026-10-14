import { describe, expect, it } from 'vitest';
import { ROTA_PADRAO, hashDaRota, rotaDoHash } from './rotas';
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
