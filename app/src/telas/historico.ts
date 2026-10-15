// Lógica da tela Histórico, sem Svelte: leitura da busca digitada, consulta
// (busca por concurso/data + filtro por dezenas) e janela da lista virtualizada.
// O app só LÊ historico.json aqui; nada do motor é recalculado. Datas do
// contrato são AAAA-MM-DD e nunca passam por `Date` (ver lib/datas.ts): como
// esse formato ordena igual ao calendário, comparar texto basta.
import type { ConcursoHistorico, Slug } from '../dados/tipos';
import { textos } from '../lib/textos';
import { estadoEData, type EstadoForca, type SituacaoForca } from './forca';

// ---------------------------------------------------------------- busca digitada

export type ModoBusca = 'concurso' | 'data';

export type BuscaAplicada =
  | { readonly modo: 'concurso'; readonly numero: number }
  | { readonly modo: 'data'; readonly data: string };

export type MotivoInvalido = 'formato_concurso' | 'formato_data' | 'data_inexistente';

export type InterpretacaoBusca =
  | { readonly tipo: 'vazia' }
  | { readonly tipo: 'ok'; readonly busca: BuscaAplicada }
  | { readonly tipo: 'invalida'; readonly motivo: MotivoInvalido };

function diasNoMes(ano: number, mes: number): number {
  if (mes === 2) return ano % 4 === 0 && (ano % 100 !== 0 || ano % 400 === 0) ? 29 : 28;
  return [4, 6, 9, 11].includes(mes) ? 30 : 31;
}

/**
 * Data digitada -> AAAA-MM-DD. Aceita dd/mm/aaaa (também com "." ou "-" e dia/mês
 * sem zero à esquerda) e AAAA-MM-DD. `formato`: não parece uma data; `inexistente`:
 * parece, mas não existe no calendário (31/02, mês 13...).
 */
export function lerDataDigitada(
  texto: string,
): { tipo: 'ok'; iso: string } | { tipo: 'formato' } | { tipo: 'inexistente' } {
  const t = texto.trim();
  const iso = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(t);
  const br = /^(\d{1,2})[/.-](\d{1,2})[/.-](\d{4})$/.exec(t);
  const partes = iso
    ? { ano: iso[1], mes: iso[2], dia: iso[3] }
    : br
      ? { ano: br[3], mes: br[2], dia: br[1] }
      : null;
  if (!partes) return { tipo: 'formato' };
  const ano = Number(partes.ano);
  const mes = Number(partes.mes);
  const dia = Number(partes.dia);
  if (ano < 1 || mes < 1 || mes > 12 || dia < 1 || dia > diasNoMes(ano, mes)) {
    return { tipo: 'inexistente' };
  }
  const dois = (n: number) => String(n).padStart(2, '0');
  return { tipo: 'ok', iso: `${String(ano).padStart(4, '0')}-${dois(mes)}-${dois(dia)}` };
}

/** Texto do campo -> busca pronta, vazia ou inválida (com o motivo para a mensagem). */
export function interpretarBusca(modo: ModoBusca, texto: string): InterpretacaoBusca {
  const t = texto.trim();
  if (t === '') return { tipo: 'vazia' };
  if (modo === 'concurso') {
    if (!/^\d{1,9}$/.test(t) || Number(t) < 1)
      return { tipo: 'invalida', motivo: 'formato_concurso' };
    return { tipo: 'ok', busca: { modo, numero: Number(t) } };
  }
  const data = lerDataDigitada(t);
  if (data.tipo === 'formato') return { tipo: 'invalida', motivo: 'formato_data' };
  if (data.tipo === 'inexistente') return { tipo: 'invalida', motivo: 'data_inexistente' };
  return { tipo: 'ok', busca: { modo, data: data.iso } };
}

/**
 * Máscara do campo de data enquanto se digita: só algarismos, com "/" depois do
 * dia e do mês (o teclado numérico não tem "/"). Texto colado em AAAA-MM-DD fica como veio.
 */
export function mascararData(texto: string): string {
  if (/^\d{4}-/.test(texto)) return texto.slice(0, 10);
  const d = texto.replace(/\D/g, '').slice(0, 8);
  if (d.length <= 2) return d;
  if (d.length <= 4) return `${d.slice(0, 2)}/${d.slice(2)}`;
  return `${d.slice(0, 2)}/${d.slice(2, 4)}/${d.slice(4)}`;
}

// ---------------------------------------------------------------- consulta

export interface Consulta {
  readonly busca: BuscaAplicada | null;
  /** Dezenas do filtro: o concurso precisa conter TODAS. */
  readonly dezenas: readonly number[];
}

export interface Marco {
  readonly concurso: number;
  /** AAAA-MM-DD */
  readonly data: string;
}

export type AvisoConsulta =
  | {
      readonly tipo: 'concurso_inexistente';
      readonly numero: number;
      readonly primeiro: number;
      readonly ultimo: number;
    }
  | {
      readonly tipo: 'data_sem_sorteio';
      readonly data: string;
      /** Último concurso de data anterior; `null` se a data é anterior a todos. */
      readonly anterior: Marco | null;
      /** Primeiro concurso de data posterior; `null` se a data é posterior a todos. */
      readonly seguinte: Marco | null;
    }
  | { readonly tipo: 'sem_resultado' };

export interface ResultadoConsulta {
  /** Posições em `concursos`, do mais recente para o mais antigo. */
  readonly indices: readonly number[];
  readonly aviso: AvisoConsulta | null;
}

/** Primeira posição cuja data é >= `data` (as datas do histórico não decrescem). */
function primeiraComDataMaiorOuIgual(
  concursos: readonly ConcursoHistorico[],
  data: string,
): number {
  let baixo = 0;
  let alto = concursos.length;
  while (baixo < alto) {
    const meio = (baixo + alto) >> 1;
    if ((concursos[meio] as ConcursoHistorico)[1] < data) baixo = meio + 1;
    else alto = meio;
  }
  return baixo;
}

/** Primeira posição cuja data é > `data`. */
function primeiraComDataMaior(concursos: readonly ConcursoHistorico[], data: string): number {
  let baixo = 0;
  let alto = concursos.length;
  while (baixo < alto) {
    const meio = (baixo + alto) >> 1;
    if ((concursos[meio] as ConcursoHistorico)[1] <= data) baixo = meio + 1;
    else alto = meio;
  }
  return baixo;
}

const marco = (c: ConcursoHistorico | undefined): Marco | null =>
  c ? { concurso: c[0], data: c[1] } : null;

/**
 * Aplica busca e filtro (os dois valem ao mesmo tempo). `concursos` segue o
 * contrato: crescente por concurso, sem buraco, datas que não decrescem.
 */
export function consultarHistorico(
  concursos: readonly ConcursoHistorico[],
  consulta: Consulta,
): ResultadoConsulta {
  const n = concursos.length;
  const primeiro = concursos[0]?.[0] ?? 0;
  const ultimo = concursos[n - 1]?.[0] ?? 0;
  // Faixa de posições [de, ate) candidata, antes do filtro por dezenas.
  let de = 0;
  let ate = n;
  const { busca } = consulta;
  if (busca?.modo === 'concurso') {
    const posicao = busca.numero - primeiro;
    if (n === 0 || posicao < 0 || posicao >= n || concursos[posicao]?.[0] !== busca.numero) {
      return {
        indices: [],
        aviso: { tipo: 'concurso_inexistente', numero: busca.numero, primeiro, ultimo },
      };
    }
    de = posicao;
    ate = posicao + 1;
  } else if (busca?.modo === 'data') {
    de = primeiraComDataMaiorOuIgual(concursos, busca.data);
    ate = primeiraComDataMaior(concursos, busca.data);
    if (de === ate) {
      return {
        indices: [],
        aviso: {
          tipo: 'data_sem_sorteio',
          data: busca.data,
          anterior: marco(concursos[de - 1]),
          seguinte: marco(concursos[de]),
        },
      };
    }
  }

  const exigidas = consulta.dezenas;
  const indices: number[] = [];
  for (let i = ate - 1; i >= de; i--) {
    const sorteadas = (concursos[i] as ConcursoHistorico)[2];
    if (exigidas.every((d) => sorteadas.includes(d))) indices.push(i);
  }
  return { indices, aviso: indices.length === 0 ? { tipo: 'sem_resultado' } : null };
}

/** "05, 12, 33": dezenas com dois dígitos, como nas bolas. */
export function listarDezenas(dezenas: readonly number[]): string {
  return dezenas.map((d) => String(d).padStart(2, '0')).join(', ');
}

/** Dezenas do concurso que fazem parte do filtro, em ordem crescente. */
export function dezenasDoFiltroNoConcurso(
  sorteadas: readonly number[],
  filtro: readonly number[],
): number[] {
  return sorteadas.filter((d) => filtro.includes(d));
}

// ---------------------------------------------------------------- virtualização

/** Altura (rem) de cada linha: fixa por loteria, o que torna a janela uma conta simples. */
export const ALTURA_LINHA_REM: Readonly<Record<Slug, number>> = {
  lotofacil: 7.5,
  megasena: 6.25,
  quina: 6.25,
};

export interface EntradaJanela {
  readonly total: number;
  /** Altura de uma linha, em px. */
  readonly alturaLinha: number;
  /** Quanto da lista já passou do topo da tela, em px (negativo: a lista ainda está abaixo). */
  readonly rolagem: number;
  /** Altura da área visível, em px. */
  readonly alturaVisivel: number;
  /** Linhas extras de cada lado, para a rolagem rápida não mostrar vazio. */
  readonly extra: number;
}

export interface JanelaVirtual {
  /** Primeira linha desenhada. */
  readonly inicio: number;
  /** Uma depois da última desenhada. */
  readonly fim: number;
}

/**
 * Linhas que precisam existir no DOM: as que cruzam a área visível mais `extra`
 * linhas de cada lado. O tamanho da janela depende da tela, nunca de `total`.
 */
export function janelaVirtual(e: EntradaJanela): JanelaVirtual {
  if (e.total <= 0 || e.alturaLinha <= 0) return { inicio: 0, fim: 0 };
  const folga = e.extra * e.alturaLinha;
  const limitar = (v: number) => Math.min(Math.max(v, 0), e.total);
  const inicio = limitar(Math.floor((e.rolagem - folga) / e.alturaLinha));
  const fim = Math.max(
    inicio,
    limitar(Math.ceil((e.rolagem + e.alturaVisivel + folga) / e.alturaLinha)),
  );
  return { inicio, fim };
}

// ---------------------------------------------------------------- visão da tela

export type VisaoHistorico =
  | { readonly tipo: 'carregando' }
  | { readonly tipo: 'sem_dados'; readonly texto: string; readonly erro: boolean }
  | {
      readonly tipo: 'conteudo';
      readonly nomeLoteria: string;
      /** "7.141 concursos, do 1 ao 7141". */
      readonly subtitulo: string;
      readonly estado: EstadoForca;
      readonly texto: string;
      readonly erro: boolean;
      readonly concursos: readonly ConcursoHistorico[];
      readonly primeiro: number;
      readonly ultimo: number;
    };

export function montarVisaoHistorico(
  slug: Slug,
  situacao: SituacaoForca | null,
  agora: Date = new Date(),
): VisaoHistorico {
  if (situacao === null) return { tipo: 'carregando' };
  const entrada = situacao.cache?.loterias[slug] ?? null;
  const historico = entrada?.dados.historico ?? null;

  if (situacao.cache === null || historico === null) {
    if (situacao.sincronizando) return { tipo: 'carregando' };
    if (situacao.falha === 'recusado') {
      return { tipo: 'sem_dados', texto: textos.historico.recusadoSemAnterior, erro: true };
    }
    if (situacao.falha === 'nenhuma' && situacao.cache !== null && entrada === null) {
      return { tipo: 'sem_dados', texto: textos.historico.semLoteria, erro: false };
    }
    return { tipo: 'sem_dados', texto: textos.sincronizacao.semDados, erro: true };
  }

  const { estado, texto } = estadoEData(situacao, situacao.cache, agora);
  const concursos = historico.concursos;
  const primeiro = concursos[0]?.[0] ?? 0;
  const ultimo = concursos[concursos.length - 1]?.[0] ?? 0;
  return {
    tipo: 'conteudo',
    nomeLoteria: textos.loterias[slug],
    subtitulo: textos.historico.subtitulo(concursos.length, primeiro, ultimo),
    estado,
    texto,
    erro: estado === 'recusado',
    concursos,
    primeiro,
    ultimo,
  };
}
