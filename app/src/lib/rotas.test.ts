import { describe, expect, it } from 'vitest';
import {
  EXIBICAO_PADRAO,
  LOTERIA_PADRAO,
  dezenaDoHash,
  exibicaoDoHash,
  ROTA_PADRAO,
  ehSlug,
  hashDaForca,
  hashDaRota,
  hashDoHistorico,
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

describe('forma de exibição e dezena na URL da Força', () => {
  it('padrão é a lista; visao aceita só lista e volante', () => {
    expect(EXIBICAO_PADRAO).toBe('lista');
    expect(exibicaoDoHash('#/forca?loteria=quina&visao=volante')).toBe('volante');
    expect(exibicaoDoHash('#/forca?visao=lista&loteria=quina')).toBe('lista');
    for (const h of [
      '',
      '#/forca',
      '#/forca?visao=',
      '#/forca?visao=Volante',
      '#/forca?visao=grade',
    ]) {
      expect(exibicaoDoHash(h)).toBeNull();
    }
  });

  it('dezena: inteiro positivo; ausente, zero, negativo, texto ou longo demais = null', () => {
    expect(dezenaDoHash('#/forca?loteria=quina&visao=volante&dezena=20')).toBe(20);
    expect(dezenaDoHash('#/forca?dezena=7&loteria=quina')).toBe(7);
    expect(dezenaDoHash('#/forca?dezena=80#topo')).toBe(80);
    for (const h of [
      '',
      '#/forca?dezena=',
      '#/forca?dezena=0',
      '#/forca?dezena=-3',
      '#/forca?dezena=abc',
      '#/forca?dezena=2x',
      '#/forca?dezena=1234',
      '#/forca?dezena=1.5',
    ]) {
      expect(dezenaDoHash(h)).toBeNull();
    }
  });

  it('hashDaForca monta loteria, visao e dezena nessa ordem e a ida e volta é identidade', () => {
    expect(hashDaForca('quina', { visao: 'volante', dezena: 20 })).toBe(
      '#/forca?loteria=quina&visao=volante&dezena=20',
    );
    expect(hashDaForca('megasena', { visao: 'lista' })).toBe(
      '#/forca?loteria=megasena&visao=lista',
    );
    expect(hashDaForca('lotofacil', { visao: 'lista', dezena: null })).toBe(
      '#/forca?loteria=lotofacil&visao=lista',
    );
    const h = hashDaForca('quina', { visao: 'volante', dezena: 20 });
    expect(rotaDoHash(h)).toBe('forca');
    expect(loteriaDoHash(h)).toBe('quina');
    expect(exibicaoDoHash(h)).toBe('volante');
    expect(dezenaDoHash(h)).toBe(20);
  });

  it('hashDoHistorico leva à aba Conferir com a loteria, e a ida e volta é identidade', () => {
    expect(hashDoHistorico('quina')).toBe('#/conferir?loteria=quina');
    expect(rotaDoHash(hashDoHistorico('megasena'))).toBe('conferir');
    expect(loteriaDoHash(hashDoHistorico('megasena'))).toBe('megasena');
  });
});
