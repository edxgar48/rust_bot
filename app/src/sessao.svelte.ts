// Sessão de dados do app: liga armazenamento persistente, cliente do contrato e
// a sequência de abertura (cache primeiro, sincronização em segundo plano).
import { abrirComCache, type SituacaoDados } from './dados/abertura';
import { ArmazenamentoResiliente } from './dados/armazenamento-idb';
import { criarCliente } from './dados/cliente';

export function criarSessao() {
  const armazenamento = new ArmazenamentoResiliente();
  const cliente = criarCliente({ armazenamento });
  let situacao = $state<SituacaoDados | null>(null);
  let aviso = $state<string | null>(armazenamento.aviso);

  function iniciar(): void {
    void abrirComCache(cliente, (nova) => {
      situacao = nova;
      aviso = armazenamento.aviso;
    }).catch(() => undefined);
  }

  return {
    iniciar,
    get situacao() {
      return situacao;
    },
    /** Preenchido quando os dados não podem ser guardados neste aparelho. */
    get aviso() {
      return aviso;
    },
  };
}
