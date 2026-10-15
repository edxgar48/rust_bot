// Sessão de dados do app: liga armazenamento persistente, cliente do contrato e
// a sequência de abertura (cache primeiro, sincronização em segundo plano).
import { abrirComCache, type SituacaoDados } from './dados/abertura';
import { ArmazenamentoResiliente } from './dados/armazenamento-idb';
import { criarCliente, type OpcoesSincronizacao } from './dados/cliente';
import type { NovidadeLoteria } from './jogos/conferencia-automatica';
import { calcularNovidades } from './jogos/novidades';
import { RepositorioResiliente } from './jogos/repositorio-idb';
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
  // Jogos salvos: banco próprio, fora do cache de dados (atualizar o cache não os toca).
  const jogos = new RepositorioResiliente();
  let situacao = $state<SituacaoDados | null>(null);
  let aviso = $state<string | null>(armazenamento.aviso);
  let ocupada = $state(false);
  let emSegundoPlano = false;
  // Conferência automática (6.4): resultado dos concursos novos para os jogos salvos.
  let novidades = $state.raw<readonly NovidadeLoteria[]>([]);

  /** Só lê os jogos e o cache (sem rede). Falha aqui não muda nada na tela. */
  let sequenciaNovidades = 0;
  async function recalcularNovidades(): Promise<void> {
    // Só vale o cálculo da chamada mais recente: um resultado antigo, que terminou
    // depois, não pode sobrescrever o novo.
    const minha = ++sequenciaNovidades;
    try {
      const calculado = await calcularNovidades(jogos, cliente);
      if (minha === sequenciaNovidades) novidades = calculado;
    } catch {
      /* mantém o que já estava */
    }
  }

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
      // Com o ultimo.json já atualizado, confere os jogos salvos; o histórico (que pode
      // faltar) chega em segundo plano e então a conferência é refeita.
      await recalcularNovidades();
      if (final.estado.tipo === 'atualizado') {
        void baixarRestoEmSegundoPlano()
          .then(recalcularNovidades)
          .catch(() => undefined);
      }
    } catch {
      /* abrirComCache não lança; a sessão segue com o que tem */
    } finally {
      ocupada = false;
    }
  }

  return {
    jogos,
    iniciar: (): void => void atualizar(),
    atualizar,
    /**
     * Para telas que precisam de tabela_geral ou historico: baixa o que falta
     * (ou mudou) e devolve o cache; sem rede devolve o que já estava guardado.
     */
    garantir: (opcoes: OpcoesSincronizacao) => garantirArquivos(cliente, opcoes),
    /** Só o que está guardado, sem rede (para mostrar de imediato antes de `garantir`). */
    lerCache: (opcoes: OpcoesSincronizacao) => cliente.lerCache(opcoes),
    /** Refaz a conferência automática (depois de marcar jogos como vistos). */
    recalcularNovidades,
    /** Loterias com jogos salvos que têm concurso novo desde a última marca. */
    get novidades() {
      return novidades;
    },
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
