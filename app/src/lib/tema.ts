// Tema claro/escuro: preferência do usuário (automático segue o sistema) e
// persistência tolerante a falha. Funções puras, sem estado global; o script
// inline de index.html repete só a leitura mínima para evitar o "flash".

export const PREFERENCIAS_TEMA = ['automatico', 'claro', 'escuro'] as const;
export type PreferenciaTema = (typeof PREFERENCIAS_TEMA)[number];
export type TemaResolvido = 'claro' | 'escuro';

/** Chave do localStorage. Precisa ser a mesma do script inline de index.html. */
export const CHAVE_TEMA = 'tema';

export type LeitorArmazenamento = Pick<Storage, 'getItem'>;
export type EscritorArmazenamento = Pick<Storage, 'setItem' | 'removeItem'>;

export function ehPreferenciaTema(valor: unknown): valor is PreferenciaTema {
  return PREFERENCIAS_TEMA.some((p) => p === valor);
}

export function resolverTema(preferencia: PreferenciaTema, sistemaEscuro: boolean): TemaResolvido {
  if (preferencia === 'automatico') return sistemaEscuro ? 'escuro' : 'claro';
  return preferencia;
}

/** Próxima preferência no ciclo automático, claro, escuro. */
export function proximaPreferencia(atual: PreferenciaTema): PreferenciaTema {
  const i = PREFERENCIAS_TEMA.indexOf(atual);
  return PREFERENCIAS_TEMA[(i + 1) % PREFERENCIAS_TEMA.length] ?? 'automatico';
}

/** localStorage, ou `null` se o navegador o bloqueia (acessar já pode lançar). */
export function armazenamentoLocal(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

/** Lê a preferência; qualquer falha ou valor estranho vira `automatico`. */
export function lerPreferencia(
  armazenamento: LeitorArmazenamento | null = armazenamentoLocal(),
): PreferenciaTema {
  try {
    const valor = armazenamento?.getItem(CHAVE_TEMA);
    return ehPreferenciaTema(valor) ? valor : 'automatico';
  } catch {
    return 'automatico';
  }
}

/** Grava a preferência; devolve `false` se não foi possível (o app segue funcionando). */
export function gravarPreferencia(
  preferencia: PreferenciaTema,
  armazenamento: EscritorArmazenamento | null = armazenamentoLocal(),
): boolean {
  try {
    if (!armazenamento) return false;
    if (preferencia === 'automatico') armazenamento.removeItem(CHAVE_TEMA);
    else armazenamento.setItem(CHAVE_TEMA, preferencia);
    return true;
  } catch {
    return false;
  }
}

/** Cores da barra do topo (--cor-cont dos tokens); manifesto.ts tem a versão clara. */
export const COR_BARRA: Readonly<Record<TemaResolvido, string>> = {
  claro: '#E8EFEC',
  escuro: '#1A211F',
};

/**
 * Aplica no <html>: `data-tema` só existe quando o usuário escolheu claro/escuro.
 * As metas `theme-color` seguem o mesmo critério: com tema explícito as duas (que têm
 * `media` do sistema) recebem a cor escolhida; no automático voltam à cor de cada `media`.
 */
export function aplicarPreferencia(preferencia: PreferenciaTema, raiz: HTMLElement): void {
  if (preferencia === 'automatico') raiz.removeAttribute('data-tema');
  else raiz.setAttribute('data-tema', preferencia);
  for (const meta of raiz.ownerDocument.querySelectorAll('meta[name="theme-color"]')) {
    const mediaEscura = (meta.getAttribute('media') ?? '').includes('dark');
    const tema: TemaResolvido =
      preferencia === 'automatico' ? (mediaEscura ? 'escuro' : 'claro') : preferencia;
    meta.setAttribute('content', COR_BARRA[tema]);
  }
}
