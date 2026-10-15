// Formatação de números para a tela (pt-BR: vírgula decimal, ponto de milhar).

/** 3.1 -> "3,1" (casas fixas); `null` -> "-". */
export function formatarDecimal(valor: number | null, casas = 1): string {
  if (valor === null || !Number.isFinite(valor)) return '-';
  return valor.toLocaleString('pt-BR', {
    minimumFractionDigits: casas,
    maximumFractionDigits: casas,
  });
}

/** 2282 -> "2.282". */
export function formatarInteiro(valor: number): string {
  return valor.toLocaleString('pt-BR', { maximumFractionDigits: 0 });
}

/** Diferença com sinal explícito: "+1,9", "−0,4" (sinal de menos de verdade) e "0,0". */
export function formatarDiferenca(valor: number, casas = 1): string {
  const texto = formatarDecimal(Math.abs(valor), casas);
  // Arredondou para zero: sem sinal.
  if (Number(Math.abs(valor).toFixed(casas)) === 0) return texto;
  return `${valor > 0 ? '+' : '−'}${texto}`;
}

/** 7 -> "07". */
export function doisDigitos(n: number): string {
  return String(n).padStart(2, '0');
}

/** 0.234 -> "23%"; abaixo de 10% uma casa ("4,5%"); positivo mínimo vira "<0,1%". */
export function formatarPercentual(fracao: number): string {
  const v = fracao * 100;
  if (!Number.isFinite(v) || v <= 0) return '0%';
  if (v < 0.05) return '<0,1%';
  return `${formatarDecimal(v, v < 10 ? 1 : 0)}%`;
}
