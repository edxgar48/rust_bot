// Download sob demanda. A abertura baixa só o necessário para a tela Início
// (index.json + ultimo.json das 3 loterias, uns 150 bytes cada, mais o índice de ~2 KB);
// tabela_geral (11 a 36 KB) e historico (156 a 338 KB) são baixados:
//  1. quando a tela que os usa abre (`garantirArquivos`), e
//  2. em segundo plano, com baixa prioridade, depois que a Início já está
//     visível (`precarregar`), para o app inteiro funcionar sem rede mesmo que
//     o usuário nunca tenha aberto as outras telas online.
// Sem Svelte aqui, para testar sem navegador.
import type { ClienteDados, OpcoesSincronizacao, ResultadoCache } from './cliente';
import { ErroDados, ehErroDeRede } from './erros';
import type { ChaveArquivo } from './tipos';

/** O que a abertura sincroniza e lê: só o que a Início usa. */
export const OPCOES_DA_ABERTURA: OpcoesSincronizacao = { arquivos: ['ultimo'] };

/** Arquivos baixados em segundo plano, do menor para o maior. */
export const ARQUIVOS_EM_SEGUNDO_PLANO: readonly ChaveArquivo[] = ['tabela_geral', 'historico'];

/** `sem_rede`: não deu para falar com o servidor; `recusado`: veio, mas falhou na conferência. */
export type FalhaDaGarantia = 'nenhuma' | 'sem_rede' | 'recusado';

export interface ResultadoGarantia {
  /** O que há guardado para o pedido (pode ser o cache antigo se a rede falhou). */
  readonly cache: ResultadoCache | null;
  /** `true` se o índice ou algum arquivo pedido não pôde ser atualizado. */
  readonly falhou: boolean;
  /** Por que falhou; recusa (dado inválido) tem precedência sobre falta de rede. */
  readonly falha: FalhaDaGarantia;
}

/**
 * Garante os arquivos pedidos: baixa o que falta ou mudou (conferido por
 * SHA-256) e devolve o que há em cache. Nunca lança; sem rede, devolve o cache
 * (o que já foi baixado continua disponível).
 */
export async function garantirArquivos(
  cliente: Pick<ClienteDados, 'lerCache' | 'sincronizar'>,
  opcoes: OpcoesSincronizacao,
): Promise<ResultadoGarantia> {
  const erros: ErroDados[] = [];
  try {
    const resultado = await cliente.sincronizar(opcoes);
    for (const loteria of Object.values(resultado.loterias)) {
      if (!loteria) continue;
      for (const arquivo of Object.values(loteria.arquivos)) {
        if (arquivo?.estado === 'falhou') erros.push(arquivo.erro);
      }
    }
  } catch (e) {
    erros.push(e instanceof ErroDados ? e : new ErroDados('rede', String(e)));
  }
  const falha: FalhaDaGarantia =
    erros.length === 0 ? 'nenhuma' : erros.every(ehErroDeRede) ? 'sem_rede' : 'recusado';
  return { cache: await cliente.lerCache(opcoes), falhou: erros.length > 0, falha };
}

/** Baixa o resto dos arquivos, um tipo por vez, cedendo a vez entre eles. Nunca lança. */
export async function precarregar(
  cliente: Pick<ClienteDados, 'lerCache' | 'sincronizar'>,
  cedeVez: () => Promise<void>,
  arquivos: readonly ChaveArquivo[] = ARQUIVOS_EM_SEGUNDO_PLANO,
): Promise<void> {
  for (const arquivo of arquivos) {
    await cedeVez();
    await garantirArquivos(cliente, { arquivos: [arquivo] });
  }
}

/** Espera o navegador ficar ocioso (ou um tempo, onde não há requestIdleCallback). */
export function esperarOciosidade(): Promise<void> {
  return new Promise((ok) => {
    const g = globalThis as { requestIdleCallback?: (f: () => void, o?: object) => number };
    if (typeof g.requestIdleCallback === 'function')
      g.requestIdleCallback(() => ok(), { timeout: 5000 });
    else setTimeout(ok, 1500);
  });
}

/** O usuário pediu economia de dados (Save-Data): nada de download em segundo plano. */
export function economiaDeDados(): boolean {
  const conexao = (globalThis.navigator as { connection?: { saveData?: boolean } } | undefined)
    ?.connection;
  return conexao?.saveData === true;
}
