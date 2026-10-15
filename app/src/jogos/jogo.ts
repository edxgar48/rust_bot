// Modelo e validação pura dos jogos salvos no aparelho (6.3). Nada aqui toca em
// armazenamento, rede ou DOM. Um jogo é só um conjunto de dezenas escolhido pela
// pessoa: o app não o avalia, apenas o guarda e o confere contra o passado.
import { PARAMETROS_LOTERIA, SLUGS, type Slug } from '../dados/tipos';
import { QUANTIDADE_JOGO } from '../telas/conferir';

/** Tamanho máximo do nome de um jogo, em caracteres. */
export const TAMANHO_MAXIMO_NOME = 40;

export interface JogoSalvo {
  /** Estável: não muda ao renomear nem ao exportar e importar. */
  readonly id: string;
  readonly loteria: Slug;
  /** Opcional: `null` = sem nome. */
  readonly nome: string | null;
  /** Ordem crescente, sem repetição, dentro de 1..total da loteria, na quantidade válida. */
  readonly dezenas: readonly number[];
  /** ISO-8601 UTC (sempre normalizado por `validarJogoSalvo`). */
  readonly criadoEm: string;
}

export type MotivoInvalido =
  'forma' | 'id' | 'loteria' | 'nome' | 'dezenas' | 'quantidade' | 'criado_em';

export type Validacao<T> =
  | { readonly ok: true; readonly valor: T }
  | { readonly ok: false; readonly motivo: MotivoInvalido };

const ok = <T>(valor: T): Validacao<T> => ({ ok: true, valor });
const falha = <T>(motivo: MotivoInvalido): Validacao<T> => ({ ok: false, motivo });

export function ehLoteria(valor: unknown): valor is Slug {
  return typeof valor === 'string' && (SLUGS as readonly string[]).includes(valor);
}

/** Caracteres de controle (inclui quebra de linha e tabulação). */
// eslint-disable-next-line no-control-regex
const CONTROLE = /[\u0000-\u001f\u007f-\u009f]/g;

/**
 * Nome limpo: sem caracteres de controle, espaços repetidos viram um só e as
 * pontas são aparadas. Vazio = `null` (sem nome). Não corta nome longo; quem
 * valida o tamanho é `validarNome`.
 */
export function limparNome(texto: string): string | null {
  const limpo = texto.replace(CONTROLE, ' ').replace(/\s+/g, ' ').trim();
  return limpo === '' ? null : limpo;
}

/** Valida o tamanho do nome já limpo (em caracteres Unicode, não em unidades UTF-16). */
export function validarNome(nome: unknown): Validacao<string | null> {
  if (nome === null || nome === undefined) return ok(null);
  if (typeof nome !== 'string') return falha('nome');
  const limpo = limparNome(nome);
  if (limpo !== null && Array.from(limpo).length > TAMANHO_MAXIMO_NOME) return falha('nome');
  return ok(limpo);
}

/**
 * Dezenas de um jogo: inteiros distintos de 1 ao total da loteria, na quantidade
 * que a loteria aceita. Devolve em ordem crescente (aceita entrada desordenada).
 */
export function validarDezenas(slug: Slug, dezenas: unknown): Validacao<number[]> {
  if (!Array.isArray(dezenas)) return falha('dezenas');
  const { totalDezenas } = PARAMETROS_LOTERIA[slug];
  const vistas = new Set<number>();
  for (const d of dezenas as unknown[]) {
    if (typeof d !== 'number' || !Number.isInteger(d) || d < 1 || d > totalDezenas) {
      return falha('dezenas');
    }
    if (vistas.has(d)) return falha('dezenas');
    vistas.add(d);
  }
  const { minimo, maximo } = QUANTIDADE_JOGO[slug];
  if (vistas.size < minimo || vistas.size > maximo) return falha('quantidade');
  return ok([...vistas].sort((a, b) => a - b));
}

/** Data ISO-8601 com fuso (o que `toISOString()` produz); aceita só datas reais. */
export function ehDataIso(valor: unknown): valor is string {
  if (typeof valor !== 'string') return false;
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(valor)) {
    return false;
  }
  if (Number.isNaN(Date.parse(valor))) return false;
  // O JavaScript aceita 31 de fevereiro e rola para março: confere o dia no calendário.
  const [ano, mes, dia] = valor.slice(0, 10).split('-').map(Number) as [number, number, number];
  const d = new Date(Date.UTC(ano, mes - 1, dia));
  return d.getUTCFullYear() === ano && d.getUTCMonth() === mes - 1 && d.getUTCDate() === dia;
}

/** Id: texto curto sem espaços (o que `gerarId` produz, ou o que veio de um arquivo). */
export function ehIdValido(valor: unknown): valor is string {
  return typeof valor === 'string' && /^[A-Za-z0-9_-]{1,64}$/.test(valor);
}

/** Id novo e estável. Usa o UUID do navegador; sem ele, data + sorteio local. */
export function gerarId(): string {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === 'function') return c.randomUUID();
  return `j${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Valida um registro completo (lido do banco ou de um arquivo importado), tratado
 * como não confiável. Campos desconhecidos são ignorados; a saída só tem os do modelo.
 */
export function validarJogoSalvo(valor: unknown): Validacao<JogoSalvo> {
  if (typeof valor !== 'object' || valor === null || Array.isArray(valor)) return falha('forma');
  const v = valor as Record<string, unknown>;
  if (!ehIdValido(v.id)) return falha('id');
  if (!ehLoteria(v.loteria)) return falha('loteria');
  const dezenas = validarDezenas(v.loteria, v.dezenas);
  if (!dezenas.ok) return falha(dezenas.motivo);
  const nome = validarNome(v.nome);
  if (!nome.ok) return falha('nome');
  if (!ehDataIso(v.criadoEm)) return falha('criado_em');
  const criadoEm = new Date(v.criadoEm).toISOString();
  // Um deslocamento de fuso pode empurrar o ano para fora de 4 dígitos.
  if (!ehDataIso(criadoEm)) return falha('criado_em');
  return ok({
    id: v.id,
    loteria: v.loteria,
    nome: nome.valor,
    dezenas: dezenas.valor,
    // Normaliza para UTC: "…-03:00" e "…Z" do mesmo instante viram o mesmo texto, e a
    // ordenação por texto em `ordenarJogos` passa a coincidir com a ordem no tempo.
    criadoEm,
  });
}

export interface EntradaJogo {
  readonly loteria: Slug;
  readonly dezenas: readonly number[];
  readonly nome?: string | null;
}

/** Monta um jogo novo a partir do que a tela tem (valida tudo; não grava). */
export function criarJogo(
  entrada: EntradaJogo,
  id: string = gerarId(),
  agora: Date = new Date(),
): Validacao<JogoSalvo> {
  return validarJogoSalvo({
    id,
    loteria: entrada.loteria,
    nome: entrada.nome ?? null,
    dezenas: entrada.dezenas,
    criadoEm: agora.toISOString(),
  });
}

/** Dois jogos são o mesmo se têm a mesma loteria e as mesmas dezenas (o nome não conta). */
export function chaveDoJogo(jogo: Pick<JogoSalvo, 'loteria' | 'dezenas'>): string {
  return `${jogo.loteria}:${[...jogo.dezenas].sort((a, b) => a - b).join(',')}`;
}

/** Mais recentes primeiro; empate pelo id, para a ordem ser sempre a mesma. */
export function ordenarJogos(jogos: readonly JogoSalvo[]): JogoSalvo[] {
  return [...jogos].sort((a, b) =>
    a.criadoEm === b.criadoEm ? a.id.localeCompare(b.id) : a.criadoEm < b.criadoEm ? 1 : -1,
  );
}
