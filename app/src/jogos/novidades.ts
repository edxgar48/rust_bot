// Liga a conferência automática (6.4) ao armazenamento: lê os jogos salvos e o cache de
// dados (sem rede), define a marca dos jogos que ainda não têm e calcula as novidades.
// A rede fica com quem chama (a abertura sincroniza o `ultimo.json`; o histórico vem em
// segundo plano ou ao entrar em Meus jogos).
import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
import { SLUGS, type Slug, type UltimoV1 } from '../dados/tipos';
import { novidadesDaLoteria, type NovidadeLoteria } from './conferencia-automatica';
import type { RepositorioJogos } from './repositorio';

export interface FonteDoCache {
  lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
}

/**
 * Novidades de todas as loterias que têm jogos salvos. Jogo sem marca (anterior à 6.4 ou
 * importado de arquivo antigo) recebe como marca o último concurso conhecido agora: o
 * passado não vira novidade. Só lê o cache: não baixa nada, e o histórico só é lido das
 * loterias que têm concurso novo a conferir.
 */
export async function calcularNovidades(
  repo: RepositorioJogos,
  fonte: FonteDoCache,
): Promise<NovidadeLoteria[]> {
  const todos = await repo.listar();
  const loterias = SLUGS.filter((s) => todos.some((j) => j.loteria === s));
  if (loterias.length === 0) return [];

  const comUltimo = await fonte.lerCache({ arquivos: ['ultimo'], loterias });
  const ultimos = new Map<Slug, UltimoV1 | null>();
  for (const s of loterias) ultimos.set(s, comUltimo?.loterias[s]?.dados.ultimo ?? null);

  // Marca inicial dos jogos sem ela.
  let jogos = todos;
  const novasMarcas = new Map<string, number>();
  for (const j of todos) {
    const concurso = ultimos.get(j.loteria)?.concurso;
    if (j.conferidoAte === null && concurso !== undefined) novasMarcas.set(j.id, concurso);
  }
  if (novasMarcas.size > 0) {
    try {
      // Só `conferidoAte`, e só onde ainda não há marca, lido e gravado no mesmo passo:
      // uma renomeação ou "marcar como visto" feitos desde a listagem não são desfeitos.
      const gravados = new Map((await repo.avancarMarcas(novasMarcas, true)).map((j) => [j.id, j]));
      jogos = todos.map((j) => gravados.get(j.id) ?? j);
    } catch {
      /* sem gravar, a marca fica para a próxima abertura; nada é conferido às cegas */
    }
  }

  // O histórico só é lido (e reconferido pelo hash) onde algum jogo está mais de um
  // concurso atrás do último: com um só concurso novo, o `ultimo.json` já basta.
  const precisamDoHistorico = loterias.filter((s) => {
    const ultimo = ultimos.get(s);
    return (
      ultimo &&
      jogos.some((j) => j.loteria === s && (j.conferidoAte ?? Infinity) < ultimo.concurso - 1)
    );
  });
  const comHistorico =
    precisamDoHistorico.length > 0
      ? await fonte.lerCache({ arquivos: ['historico'], loterias: precisamDoHistorico })
      : null;

  const saida: NovidadeLoteria[] = [];
  for (const s of loterias) {
    const n = novidadesDaLoteria(
      s,
      jogos,
      ultimos.get(s) ?? null,
      comHistorico?.loterias[s]?.dados.historico?.concursos ?? null,
    );
    if (n) saida.push(n);
  }
  return saida;
}
