// Jogo salvo a carregar no volante de Conferir jogo (passado de Meus jogos).
import type { Slug } from '../dados/tipos';

export interface JogoInicial {
  readonly loteria: Slug;
  readonly dezenas: readonly number[];
  readonly nome: string | null;
}
