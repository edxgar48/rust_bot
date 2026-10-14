// Estado de sincronização para a interface: função pura que, dado o resultado
// de `sincronizar` (ou o erro que ele lançou) e o que há no cache, diz em que
// situação estão os dados e o texto em português (src/lib/textos.ts).
import { textos } from '../lib/textos';
import type { ResultadoCache, ResultadoSincronizacao } from './cliente';
import { ErroDados } from './erros';

export type TipoEstadoSincronizacao =
  'sincronizando' | 'atualizado' | 'sem_rede' | 'recusado' | 'sem_dados';

export interface EstadoSincronizacao {
  readonly tipo: TipoEstadoSincronizacao;
  /** Desde quando valem os dados exibidos (ISO UTC), se conhecido. */
  readonly dataDosDados: string | null;
  readonly texto: string;
}

const FUSO = 'America/Sao_Paulo';

/** "14/10/2026 09:30" no horário de Brasília, independente do fuso do aparelho. */
export function formatarDataHora(iso: string): string | null {
  const instante = new Date(iso);
  if (Number.isNaN(instante.getTime())) return null;
  const partes = Object.fromEntries(
    new Intl.DateTimeFormat('pt-BR', {
      timeZone: FUSO,
      day: '2-digit',
      month: '2-digit',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
      hourCycle: 'h23',
    })
      .formatToParts(instante)
      .map((p) => [p.type, p.value]),
  );
  return `${partes.day}/${partes.month}/${partes.year} ${partes.hour}:${partes.minute}`;
}

/** "2026-10-07" -> "07/10/2026" (data de sorteio do contrato, sem fuso). */
export function formatarDataSorteio(data: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(data);
  return m ? `${m[3]}/${m[2]}/${m[1]}` : data;
}

/** Data a exibir quando os dados vêm do cache. */
function dataDoCache(cache: ResultadoCache | null): string | null {
  return cache ? (cache.sincronizadoEm ?? cache.indiceSincronizadoEm) : null;
}

function comData<T>(iso: string | null, f: (data: string | null) => T): T {
  return f(iso === null ? null : formatarDataHora(iso));
}

/** Estado enquanto a sincronização em segundo plano ainda roda. */
export function estadoInicial(cache: ResultadoCache | null): EstadoSincronizacao {
  const data = dataDoCache(cache);
  if (cache === null) {
    return {
      tipo: 'sincronizando',
      dataDosDados: null,
      texto: textos.sincronizacao.sincronizando(null),
    };
  }
  return {
    tipo: 'sincronizando',
    dataDosDados: data,
    texto: comData(data, textos.sincronizacao.sincronizando),
  };
}

export interface EntradaEstado {
  /** Resultado de `sincronizar`; ausente se ele lançou. */
  readonly resultado?: ResultadoSincronizacao;
  /** O que `sincronizar` lançou, se lançou. */
  readonly erro?: unknown;
  /** Cache lido DEPOIS da tentativa (traz a data da última confirmação). */
  readonly cache: ResultadoCache | null;
  readonly agora: Date;
}

/** Códigos de erro que significam "não consegui falar com o servidor". */
const ERROS_DE_REDE = new Set(['rede', 'http']);

function errosDoResultado(resultado: ResultadoSincronizacao): ErroDados[] {
  const erros: ErroDados[] = [];
  for (const loteria of Object.values(resultado.loterias)) {
    if (!loteria) continue;
    for (const arquivo of Object.values(loteria.arquivos)) {
      if (arquivo?.estado === 'falhou') erros.push(arquivo.erro);
    }
  }
  return erros;
}

export function estadoSincronizacao(entrada: EntradaEstado): EstadoSincronizacao {
  const { resultado, erro, cache, agora } = entrada;
  const erros = resultado
    ? errosDoResultado(resultado)
    : [erro instanceof ErroDados ? erro : new ErroDados('rede', String(erro))];

  if (erros.length === 0) {
    const iso = agora.toISOString();
    const texto = formatarDataHora(iso);
    return {
      tipo: 'atualizado',
      dataDosDados: iso,
      texto: textos.sincronizacao.atualizado(texto ?? iso),
    };
  }

  // Sem nada guardado não há o que mostrar, seja qual for a causa da falha.
  if (cache === null) {
    return { tipo: 'sem_dados', dataDosDados: null, texto: textos.sincronizacao.semDados };
  }

  const data = dataDoCache(cache);
  const recusado = erros.some((e) => !ERROS_DE_REDE.has(e.codigo));
  return recusado
    ? { tipo: 'recusado', dataDosDados: data, texto: comData(data, textos.sincronizacao.recusado) }
    : { tipo: 'sem_rede', dataDosDados: data, texto: comData(data, textos.sincronizacao.semRede) };
}
