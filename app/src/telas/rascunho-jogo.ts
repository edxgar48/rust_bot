// Rascunho de jogo: dezenas separadas na tela Força (botão "Inserir") para o volante de
// Conferir jogo. Vive só em memória de módulo: some ao recarregar a página e nunca vai
// para o disco.
//
// Regras:
// - O rascunho é de uma loteria; inserir em outra loteria começa um rascunho novo.
// - Não duplica e respeita o máximo de dezenas do jogo da loteria.
// - Conferir jogo só consome o rascunho depois de `pedirAbertura()` (atalho "Abrir em
//   Conferir"). Abrir a aba Conferir por outro caminho não mexe no rascunho nem usa as
//   dezenas dele. O consumo acontece na montagem da tela, que parte do volante vazio:
//   por isso nunca sobrescreve um jogo que o usuário esteja editando.
// - Um jogo vindo de Meus jogos tem precedência: o rascunho fica guardado, intacto.
import type { Slug } from '../dados/tipos';
import { QUANTIDADE_JOGO, alternarDezenaDoJogo } from './conferir';
import type { JogoInicial } from './jogo-inicial';

export interface Rascunho {
  readonly loteria: Slug;
  readonly dezenas: readonly number[];
}

export type ResultadoAlternancia =
  { readonly tipo: 'inserida' | 'removida' } | { readonly tipo: 'cheio'; readonly maximo: number };

let atual: Rascunho | null = null;
let aberturaPedida = false;

/** Dezenas do rascunho da loteria (vazio se o rascunho é de outra). */
export function dezenasDoRascunho(loteria: Slug): readonly number[] {
  return atual !== null && atual.loteria === loteria ? atual.dezenas : [];
}

/** Insere a dezena (ou a remove, se já estava); `cheio` quando passaria do máximo. */
export function alternarNoRascunho(loteria: Slug, dezena: number): ResultadoAlternancia {
  const base = dezenasDoRascunho(loteria);
  if (base.includes(dezena)) {
    const restantes = base.filter((d) => d !== dezena);
    atual = restantes.length === 0 ? null : { loteria, dezenas: restantes };
    aberturaPedida = false;
    return { tipo: 'removida' };
  }
  const { maximo } = QUANTIDADE_JOGO[loteria];
  if (base.length >= maximo) return { tipo: 'cheio', maximo };
  atual = { loteria, dezenas: alternarDezenaDoJogo(loteria, base, dezena) };
  aberturaPedida = false;
  return { tipo: 'inserida' };
}

/** Marca que o usuário pediu para abrir o rascunho em Conferir jogo. */
export function pedirAbertura(): void {
  if (atual !== null) aberturaPedida = true;
}

/** Entrega o rascunho como jogo inicial (sem nome) se a abertura foi pedida; apaga-o. */
export function consumirRascunho(): JogoInicial | null {
  if (atual === null || !aberturaPedida) return null;
  const jogo: JogoInicial = { loteria: atual.loteria, dezenas: atual.dezenas, nome: null };
  limparRascunho();
  return jogo;
}

export function limparRascunho(): void {
  atual = null;
  aberturaPedida = false;
}
