import { afterEach, describe, expect, it } from 'vitest';
import { formatarDataSorteio } from './datas';

describe('formatarDataSorteio', () => {
  it('AAAA-MM-DD -> dd/mm/aaaa', () => {
    expect(formatarDataSorteio('2026-10-13')).toBe('13/10/2026');
    expect(formatarDataSorteio('2026-01-01')).toBe('01/01/2026');
    expect(formatarDataSorteio('2024-02-29')).toBe('29/02/2024');
  });

  it('texto fora do formato volta como veio', () => {
    for (const t of ['', 'lixo', '2026-10-13T00:00:00Z', '13/10/2026', '2026-1-3']) {
      expect(formatarDataSorteio(t)).toBe(t);
    }
  });

  describe('independe do fuso do aparelho', () => {
    const original = process.env.TZ;
    afterEach(() => {
      if (original === undefined) delete process.env.TZ;
      else process.env.TZ = original;
    });

    it.each([
      'UTC',
      'America/Sao_Paulo',
      'America/Los_Angeles',
      'Pacific/Pago_Pago', // UTC-11
      'Pacific/Kiritimati', // UTC+14
    ])('2026-10-13 -> 13/10/2026 em %s', (fuso) => {
      process.env.TZ = fuso;
      expect(formatarDataSorteio('2026-10-13')).toBe('13/10/2026');
    });

    it('contraprova: via Date o dia muda em fuso a oeste de UTC', () => {
      process.env.TZ = 'Pacific/Pago_Pago';
      expect(new Date('2026-10-13').getDate()).toBe(12);
      expect(formatarDataSorteio('2026-10-13')).toBe('13/10/2026');
    });
  });
});
