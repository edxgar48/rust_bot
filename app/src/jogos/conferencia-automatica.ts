// Conferência automática (6.4): para cada jogo salvo, o resultado dos concursos que
// saíram depois do último que a pessoa já viu (`conferidoAte`). Funções puras, sem
// armazenamento, rede ou DOM. Só conta acertos do passado; não diz nada sobre o
// próximo sorteio e não calcula valor de prêmio.
import type { ConcursoHistorico, Slug, UltimoV1 } from '../dados/tipos';
import { resumirAcertosRecentes } from '../telas/conferir';
import type { JogoSalvo } from './jogo';

/**
 * Menor número de acertos que cai numa faixa com prêmio em cada loteria (Lotofácil 11,
 * Mega-Sena 4 = quadra, Quina 2 = duque). O app não calcula o valor do prêmio.
 */
export const FAIXA_PREMIO: Readonly<Record<Slug, number>> = {
  lotofacil: 11,
  megasena: 4,
  quina: 2,
};

/** Quantos concursos novos cada jogo lista (os mais recentes); o resto só entra na contagem. */
export const LIMITE_RESULTADOS_LISTADOS = 5;

export interface ConcursosNovos {
  /** Concursos depois de `conferidoAte`, em ordem crescente. */
  readonly concursos: readonly ConcursoHistorico[];
  /**
   * `false` quando pode faltar algum concurso no meio: o histórico em cache é mais
   * antigo que o `ultimo.json` e o salto é maior que um concurso. Aí a conferência
   * espera o histórico (nunca finge ter conferido o que não viu).
   */
  readonly completo: boolean;
  /** Maior concurso conhecido (do `ultimo.json` ou do histórico); `null` sem nenhum dado. */
  readonly ultimoConcurso: number | null;
}

/**
 * Concursos posteriores a `conferidoAte`. O `ultimo.json` (sempre atualizado na
 * abertura) entra como concurso extra se o histórico em cache ainda não o tem, então
 * um único concurso novo já é conferido sem baixar o histórico.
 */
export function concursosPosteriores(
  conferidoAte: number,
  ultimo: UltimoV1 | null,
  historico: readonly ConcursoHistorico[] | null,
): ConcursosNovos {
  const lista = historico ?? [];
  const ultimoDoHistorico =
    lista.length > 0 ? (lista[lista.length - 1] as ConcursoHistorico)[0] : null;
  const conhecidos = [ultimo?.concurso ?? null, ultimoDoHistorico].filter(
    (n): n is number => n !== null,
  );
  if (conhecidos.length === 0) return { concursos: [], completo: true, ultimoConcurso: null };
  const ultimoConcurso = Math.max(...conhecidos);
  if (conferidoAte >= ultimoConcurso) return { concursos: [], completo: true, ultimoConcurso };

  // O histórico é crescente: acha o começo andando do fim (custo proporcional aos novos).
  let inicio = lista.length;
  while (inicio > 0 && (lista[inicio - 1] as ConcursoHistorico)[0] > conferidoAte) inicio--;
  const novos = lista.slice(inicio);
  if (ultimo && ultimo.concurso > conferidoAte && !novos.some((c) => c[0] === ultimo.concurso)) {
    novos.push([ultimo.concurso, ultimo.data, ultimo.dezenas]);
  }
  const historicoCobre = ultimoDoHistorico !== null && ultimoDoHistorico >= ultimoConcurso;
  // Histórico atrasado: só se sabe que nada falta se os números vão sem buraco até o último.
  const semBuraco = novos.every((x, i) => x[0] === conferidoAte + 1 + i);
  return {
    concursos: novos,
    completo: historicoCobre || (semBuraco && novos.length === ultimoConcurso - conferidoAte),
    ultimoConcurso,
  };
}

export interface ResultadoNoConcurso {
  readonly concurso: number;
  readonly acertos: number;
}

export interface ConferenciaDosNovos {
  /**
   * Só os `LIMITE_RESULTADOS_LISTADOS` mais recentes, do mais recente ao mais antigo
   * (a lista completa nunca é montada: com muitos jogos e uma marca antiga pesaria).
   */
  readonly resultados: readonly ResultadoNoConcurso[];
  /** Quantos concursos novos foram conferidos (inclui os que não estão em `resultados`). */
  readonly totalConcursos: number;
  /** Mais acertos; em empate, o concurso mais recente. `null` sem concursos. */
  readonly melhor: ResultadoNoConcurso | null;
  /** O melhor resultado atingiu `FAIXA_PREMIO` da loteria. */
  readonly naFaixaPremio: boolean;
}

/** Acertos do jogo em cada concurso novo (função pura; `concursos` em ordem crescente). */
export function conferirConcursosNovos(
  slug: Slug,
  dezenas: readonly number[],
  concursos: readonly ConcursoHistorico[],
): ConferenciaDosNovos {
  const { recentes, total, melhor } = resumirAcertosRecentes(
    slug,
    dezenas,
    concursos,
    LIMITE_RESULTADOS_LISTADOS,
  );
  return {
    resultados: recentes,
    totalConcursos: total,
    melhor,
    naFaixaPremio: melhor !== null && melhor.acertos >= FAIXA_PREMIO[slug],
  };
}

export interface NovidadeJogo extends ConferenciaDosNovos {
  readonly jogoId: string;
  /** `pendente`: falta o histórico para conferir tudo; `resultados` fica vazio. */
  readonly estado: 'conferido' | 'pendente';
  readonly conferidoAte: number;
  readonly ultimoConcurso: number;
}

export interface NovidadeLoteria {
  readonly slug: Slug;
  /** Concurso mais recente conhecido da loteria. */
  readonly ultimoConcurso: number;
  /** Só os jogos com concurso novo (os sem marca ou já em dia não entram). */
  readonly jogos: readonly NovidadeJogo[];
  readonly conferidos: number;
  readonly pendentes: number;
  /** Maior número de acertos entre os jogos conferidos; `null` se nenhum foi conferido. */
  readonly melhorAcertos: number | null;
  /** Quantos jogos conferidos atingiram a faixa com prêmio. */
  readonly naFaixaPremio: number;
  /** Maior quantidade de concursos novos entre os jogos conferidos. */
  readonly concursosNovos: number;
}

/**
 * Novidades de uma loteria: um registro por jogo com `conferidoAte` abaixo do
 * concurso mais recente conhecido. Jogo sem marca (`null`) não tem novidade.
 * `null` se nenhum jogo tem concurso novo.
 */
export function novidadesDaLoteria(
  slug: Slug,
  jogos: readonly JogoSalvo[],
  ultimo: UltimoV1 | null,
  historico: readonly ConcursoHistorico[] | null,
): NovidadeLoteria | null {
  const lista: NovidadeJogo[] = [];
  let ultimoConcurso: number | null = null;

  for (const jogo of jogos) {
    if (jogo.loteria !== slug || jogo.conferidoAte === null) continue;
    // Sem guardar por marca: a lista de cada jogo é descartada logo após conferir.
    const novos = concursosPosteriores(jogo.conferidoAte, ultimo, historico);
    if (novos.ultimoConcurso === null || jogo.conferidoAte >= novos.ultimoConcurso) continue;
    ultimoConcurso = novos.ultimoConcurso;
    if (novos.completo) {
      lista.push({
        jogoId: jogo.id,
        estado: 'conferido',
        conferidoAte: jogo.conferidoAte,
        ultimoConcurso: novos.ultimoConcurso,
        ...conferirConcursosNovos(slug, jogo.dezenas, novos.concursos),
      });
    } else {
      lista.push({
        jogoId: jogo.id,
        estado: 'pendente',
        conferidoAte: jogo.conferidoAte,
        ultimoConcurso: novos.ultimoConcurso,
        resultados: [],
        totalConcursos: 0,
        melhor: null,
        naFaixaPremio: false,
      });
    }
  }
  if (lista.length === 0 || ultimoConcurso === null) return null;

  const conferidos = lista.filter((j) => j.estado === 'conferido');
  const acertos = conferidos.map((j) => j.melhor?.acertos ?? 0);
  return {
    slug,
    ultimoConcurso,
    jogos: lista,
    conferidos: conferidos.length,
    pendentes: lista.length - conferidos.length,
    melhorAcertos: acertos.length === 0 ? null : acertos.reduce((a, b) => Math.max(a, b)),
    naFaixaPremio: conferidos.filter((j) => j.naFaixaPremio).length,
    concursosNovos: conferidos.reduce((m, j) => Math.max(m, j.totalConcursos), 0),
  };
}
