// Ordem da lista da Força (7.8): URL (&ordem=), preferência guardada e textos do G-1.
import { describe, expect, it } from 'vitest';
import { CHAVE_ORDEM_FORCA, gravarOrdemGuardada, lerOrdemGuardada } from './loteria-guardada';
import { hashDaForca, ordemDoHash } from './rotas';
import { textos } from './textos';

describe('rotas: &ordem=', () => {
  it('só g1 entra na URL; a ordem por força é a padrão', () => {
    expect(hashDaForca('quina', { visao: 'lista', ordem: 'g1' })).toBe(
      '#/forca?loteria=quina&visao=lista&ordem=g1',
    );
    expect(hashDaForca('quina', { visao: 'lista', ordem: 'forca' })).toBe(
      '#/forca?loteria=quina&visao=lista',
    );
    expect(hashDaForca('quina', { ordem: 'g1', dezena: 7 })).toBe(
      '#/forca?loteria=quina&ordem=g1&dezena=7',
    );
  });

  it('lê ordem válida; ausente ou inválida = null', () => {
    expect(ordemDoHash('#/forca?loteria=quina&ordem=g1')).toBe('g1');
    expect(ordemDoHash('#/forca?ordem=forca&visao=lista')).toBe('forca');
    expect(ordemDoHash('#/forca?loteria=quina')).toBeNull();
    expect(ordemDoHash('#/forca?ordem=qualquer')).toBeNull();
  });
});

describe('preferência de ordem guardada', () => {
  const memoria = (inicial: string | null) => {
    let valor = inicial;
    return {
      getItem: () => valor,
      setItem: (_: string, v: string) => {
        valor = v;
      },
    };
  };

  it('grava e lê; valor inválido ou armazenamento que falha = null/false', () => {
    const m = memoria(null);
    expect(lerOrdemGuardada(m)).toBeNull();
    expect(gravarOrdemGuardada('g1', m)).toBe(true);
    expect(lerOrdemGuardada(m)).toBe('g1');
    expect(lerOrdemGuardada(memoria('x'))).toBeNull();
    const quebrado = {
      getItem: () => {
        throw new Error('bloqueado');
      },
      setItem: () => {
        throw new Error('bloqueado');
      },
    };
    expect(lerOrdemGuardada(quebrado)).toBeNull();
    expect(gravarOrdemGuardada('g1', quebrado)).toBe(false);
    expect(gravarOrdemGuardada('g1', null)).toBe(false);
    expect(CHAVE_ORDEM_FORCA).toBe('forca.ordem');
  });
});

describe('textos do G-1', () => {
  it('avisos citam o G-1 e dizem que não preveem; sem rótulo de experimental', () => {
    for (const aviso of [textos.avisos.naoPreveForcaG1, textos.avisos.naoPreveDetalheG1]) {
      expect(aviso).toContain('G-1');
      expect(aviso).toContain('não preveem');
      expect(aviso).toContain('independente');
    }
    expect(JSON.stringify(textos.forca)).not.toMatch(/experimental/i);
  });

  it('a Entenda explica o G-1: passado, sorteios independentes, sem vantagem sobre o acaso', () => {
    const tudo = textos.entenda.g1.paragrafos.join(' ');
    expect(tudo).toContain('passado');
    expect(tudo).toContain('independentes');
    expect(tudo).toContain('não encontrou vantagem sobre o acaso');
  });

  it('selos escritos e falados', () => {
    expect(textos.forca.g1.selo(5)).toBe('G-1 5º');
    expect(textos.forca.g1.seloFalado(5)).toBe('G-1: 5º lugar');
    expect(textos.forca.g1.seloForca(3)).toBe('Força 3º');
    expect(textos.forca.g1.seloForcaFalado(3)).toBe('Tabela de força: 3º lugar');
  });
});
