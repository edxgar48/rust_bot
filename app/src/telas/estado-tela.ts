// Estado comum das telas que leem dados sob demanda (Força, Histórico, Conferir):
// o que a tela sabe sobre o cache e a sincronização, e o texto da linha de estado.
import type { ResultadoCache } from '../dados/cliente';
import { formatarDataHora } from '../dados/estado-sincronizacao';
import type { FalhaDaGarantia } from '../dados/sob-demanda';
import { textos } from '../lib/textos';

/** O que a tela sabe: o cache lido e em que ponto da sincronização está. */
export interface SituacaoTela {
  readonly cache: ResultadoCache | null;
  /** `true` enquanto `garantir` ainda não respondeu. */
  readonly sincronizando: boolean;
  /** Resultado de `garantir` (só vale com `sincronizando` falso). */
  readonly falha: FalhaDaGarantia;
}

export type EstadoTela = 'sincronizando' | 'atualizado' | 'sem_rede' | 'recusado';

export function estadoEData(
  situacao: SituacaoTela,
  cache: ResultadoCache,
  agora: Date,
): { estado: EstadoTela; texto: string } {
  const iso = cache.sincronizadoEm ?? cache.indiceSincronizadoEm;
  const data = iso === null ? null : formatarDataHora(iso);
  if (situacao.sincronizando) {
    return { estado: 'sincronizando', texto: textos.sincronizacao.sincronizando(data) };
  }
  if (situacao.falha === 'recusado') {
    return { estado: 'recusado', texto: textos.sincronizacao.recusado(data) };
  }
  if (situacao.falha === 'sem_rede') {
    return { estado: 'sem_rede', texto: textos.sincronizacao.semRede(data) };
  }
  return {
    estado: 'atualizado',
    texto: textos.sincronizacao.atualizado(data ?? formatarDataHora(agora.toISOString()) ?? ''),
  };
}
