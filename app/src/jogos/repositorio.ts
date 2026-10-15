// Armazenamento dos jogos salvos. É SEPARADO do cache de dados do contrato
// (`dados/armazenamento*.ts`): outro banco, outra interface. Nada do que atualiza
// ou limpa o cache de dados passa por aqui, e o contrário também vale.
import type { Slug } from '../dados/tipos';
import { ordenarJogos, validarNome, type JogoSalvo } from './jogo';

export interface RepositorioJogos {
  /**
   * Motivo técnico, se os jogos NÃO estão sendo guardados de forma persistente
   * (valem só até fechar o app); ausente ou `null` = tudo persiste.
   */
  readonly aviso?: string | null;
  /** Mais recentes primeiro; com `loteria`, só os dela. */
  listar(loteria?: Slug): Promise<JogoSalvo[]>;
  /** Todos os ids guardados, inclusive de registros inválidos que `listar` não devolve. */
  listarIds(): Promise<string[]>;
  /** Grava (ou substitui, pelo id) vários jogos de uma vez, tudo ou nada. */
  gravar(jogos: readonly JogoSalvo[]): Promise<void>;
  /** Troca só o nome; `null` se o jogo não existe. */
  renomear(id: string, nome: string | null): Promise<JogoSalvo | null>;
  apagar(id: string): Promise<void>;
}

export class RepositorioEmMemoria implements RepositorioJogos {
  private readonly jogos = new Map<string, JogoSalvo>();

  async listar(loteria?: Slug): Promise<JogoSalvo[]> {
    const todos = [...this.jogos.values()];
    return ordenarJogos(loteria === undefined ? todos : todos.filter((j) => j.loteria === loteria));
  }

  async listarIds(): Promise<string[]> {
    return [...this.jogos.keys()];
  }

  async gravar(jogos: readonly JogoSalvo[]): Promise<void> {
    for (const j of jogos) this.jogos.set(j.id, { ...j, dezenas: [...j.dezenas] });
  }

  async renomear(id: string, nome: string | null): Promise<JogoSalvo | null> {
    const atual = this.jogos.get(id);
    const limpo = validarNome(nome);
    if (!atual || !limpo.ok) return null;
    const novo = { ...atual, nome: limpo.valor };
    this.jogos.set(id, novo);
    return novo;
  }

  async apagar(id: string): Promise<void> {
    this.jogos.delete(id);
  }
}
