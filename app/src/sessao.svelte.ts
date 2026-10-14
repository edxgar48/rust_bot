// Sessão de dados do app: liga armazenamento persistente, cliente do contrato e
// a sequência de abertura (cache primeiro, sincronização em segundo plano).
import { abrirComCache, type SituacaoDados } from './dados/abertura';
import { ArmazenamentoResiliente } from './dados/armazenamento-idb';
import { criarCliente, type OpcoesSincronizacao } from './dados/cliente';
import {
  OPCOES_DA_ABERTURA,
  economiaDeDados,
  esperarOciosidade,
  garantirArquivos,
  precarregar,
} from './dados/sob-demanda';

export function criarSessao() {
  const armazenamento = new ArmazenamentoResiliente();
  const cliente = criarCliente({ armazenamento });
  let situacao = $state<SituacaoDados | null>(null);
  let aviso = $state<string | null>(armazenamento.aviso);
  let ocupada = $state(false);
  let emSegundoPlano = false;

  // Depois que a Início já está atualizada: baixa o resto com baixa prioridade,
  // em silêncio (falha aqui não muda o estado mostrado na Início).
  async function baixarRestoEmSegundoPlano(): Promise<void> {
    if (emSegundoPlano || economiaDeDados()) return;
    emSegundoPlano = true;
    try {
      await precarregar(cliente, esperarOciosidade);
    } finally {
      emSegundoPlano = false;
    }
  }

  /** Cache primeiro, depois sincroniza (só o que a Início usa). Também serve para "Atualizar". */
  async function atualizar(): Promise<void> {
    if (ocupada) return;
    ocupada = true;
    try {
      const final = await abrirComCache(
        cliente,
        (nova) => {
          situacao = nova;
          aviso = armazenamento.aviso;
        },
        undefined,
        OPCOES_DA_ABERTURA,
      );
      if (final.estado.tipo === 'atualizado')
        void baixarRestoEmSegundoPlano().catch(() => undefined);
    } catch {
      /* abrirComCache não lança; a sessão segue com o que tem */
    } finally {
      ocupada = false;
    }
  }

  return {
    iniciar: (): void => void atualizar(),
    atualizar,
    /**
     * Para telas que precisam de tabela_geral ou historico: baixa o que falta
     * (ou mudou) e devolve o cache; sem rede devolve o que já estava guardado.
     */
    garantir: (opcoes: OpcoesSincronizacao) => garantirArquivos(cliente, opcoes),
    get situacao() {
      return situacao;
    },
    /** `true` enquanto a abertura/atualização da Início está em andamento. */
    get ocupada() {
      return ocupada;
    },
    /** Preenchido quando os dados não podem ser guardados neste aparelho. */
    get aviso() {
      return aviso;
    },
  };
}
