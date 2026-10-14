// Sequência de abertura do app: mostrar o cache de imediato, depois sincronizar
// em segundo plano. Sem Svelte aqui, para testar sem navegador.
import type { ClienteDados, ResultadoCache } from './cliente';
import {
  estadoInicial,
  estadoSincronizacao,
  type EstadoSincronizacao,
} from './estado-sincronizacao';

export interface SituacaoDados {
  readonly estado: EstadoSincronizacao;
  readonly cache: ResultadoCache | null;
}

export async function abrirComCache(
  cliente: Pick<ClienteDados, 'lerCache' | 'sincronizar'>,
  aoMudar: (situacao: SituacaoDados) => void,
  agora: () => Date = () => new Date(),
): Promise<SituacaoDados> {
  // 1. Cache primeiro: aparece na hora, mesmo sem rede.
  const antes = await cliente.lerCache();
  aoMudar({ estado: estadoInicial(antes), cache: antes });

  // 2. Sincroniza; nenhuma falha sai daqui (o app segue com o cache).
  let final: SituacaoDados;
  try {
    const resultado = await cliente.sincronizar();
    const cache = await cliente.lerCache();
    final = { estado: estadoSincronizacao({ resultado, cache, agora: agora() }), cache };
  } catch (erro) {
    const cache = await cliente.lerCache();
    final = { estado: estadoSincronizacao({ erro, cache, agora: agora() }), cache };
  }
  aoMudar(final);
  return final;
}
