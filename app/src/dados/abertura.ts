// Sequência de abertura do app: mostrar o cache de imediato, depois sincronizar
// em segundo plano. Sem Svelte aqui, para testar sem navegador.
import type { ClienteDados, OpcoesSincronizacao, ResultadoCache } from './cliente';
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
  /**
   * O que ler e sincronizar. Padrão: tudo. O app passa `{ arquivos: ['ultimo'] }`
   * na abertura (só o que a tela Início usa); o resto vem sob demanda
   * (sob-demanda.ts). O estado e a data "dados de" valem para esses arquivos.
   */
  opcoes: OpcoesSincronizacao = {},
): Promise<SituacaoDados> {
  // 1. Cache primeiro: aparece na hora, mesmo sem rede.
  const antes = await cliente.lerCache(opcoes);
  aoMudar({ estado: estadoInicial(antes), cache: antes });

  // 2. Sincroniza; nenhuma falha sai daqui (o app segue com o cache).
  let final: SituacaoDados;
  try {
    const resultado = await cliente.sincronizar(opcoes);
    const cache = await cliente.lerCache(opcoes);
    final = { estado: estadoSincronizacao({ resultado, cache, agora: agora() }), cache };
  } catch (erro) {
    const cache = await cliente.lerCache(opcoes);
    final = { estado: estadoSincronizacao({ erro, cache, agora: agora() }), cache };
  }
  aoMudar(final);
  return final;
}
