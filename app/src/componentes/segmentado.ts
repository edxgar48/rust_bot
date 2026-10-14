// Navegação por teclado de um grupo de opções exclusivas (padrão radiogroup).

/** Índice a focar/selecionar após `tecla`; `null` se a tecla não navega. */
export function proximoIndice(atual: number, tecla: string, total: number): number | null {
  if (total <= 0) return null;
  switch (tecla) {
    case 'ArrowRight':
    case 'ArrowDown':
      return (atual + 1) % total;
    case 'ArrowLeft':
    case 'ArrowUp':
      return (atual - 1 + total) % total;
    case 'Home':
      return 0;
    case 'End':
      return total - 1;
    default:
      return null;
  }
}
