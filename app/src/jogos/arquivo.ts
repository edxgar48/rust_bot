// Arquivo de backup dos jogos (formato próprio do app, não é o contrato de dados):
//
//   { "formato": "forca-loterias-jogos", "versao": 1, "exportadoEm": "...",
//     "jogos": [ { "id", "loteria", "nome", "dezenas", "criadoEm",
//                 "conferidoAte" } ] }
//
// "conferidoAte" (6.4) é opcional: arquivos antigos sem ele seguem válidos (vira `null`,
// sem marca) e apps antigos ignoram o campo; por isso a versão continua 1.
// Campos desconhecidos são ignorados (compatível para a frente dentro da versão 1);
// versão maior que a conhecida é recusada. Importar valida item a item e nunca
// altera nada quando o arquivo como um todo é recusado.
import { chaveDoJogo, gerarId, validarJogoSalvo, type JogoSalvo } from './jogo';

export const FORMATO_ARQUIVO = 'forca-loterias-jogos';
export const VERSAO_ARQUIVO = 1;
/** Acima disto o arquivo é recusado antes de ser lido (um backup real tem poucos KB). */
export const TAMANHO_MAXIMO_ARQUIVO = 2 * 1024 * 1024;

/** Máximo de jogos num arquivo de backup; acima disto o arquivo é recusado (motivo `grande`). */
export const MAXIMO_ITENS_ARQUIVO = 1000;

export function serializarJogos(jogos: readonly JogoSalvo[], agora: Date): string {
  const corpo = {
    formato: FORMATO_ARQUIVO,
    versao: VERSAO_ARQUIVO,
    exportadoEm: agora.toISOString(),
    jogos: jogos.map((j) => ({
      id: j.id,
      loteria: j.loteria,
      nome: j.nome,
      dezenas: [...j.dezenas],
      criadoEm: j.criadoEm,
      conferidoAte: j.conferidoAte,
    })),
  };
  return `${JSON.stringify(corpo, null, 2)}\n`;
}

/** Nome do arquivo baixado: jogos-forca-loterias-AAAA-MM-DD.json (data do aparelho, UTC). */
export function nomeDoArquivo(agora: Date): string {
  return `jogos-forca-loterias-${agora.toISOString().slice(0, 10)}.json`;
}

export type MotivoRecusa = 'grande' | 'malformado' | 'formato' | 'versao';

export type LeituraArquivo =
  | { readonly ok: true; readonly itens: readonly unknown[] }
  | { readonly ok: false; readonly motivo: MotivoRecusa };

/** Confere o envelope do arquivo; não valida os jogos (isso é por item). */
export function lerArquivoJogos(texto: string): LeituraArquivo {
  if (texto.length > TAMANHO_MAXIMO_ARQUIVO) return { ok: false, motivo: 'grande' };
  let bruto: unknown;
  try {
    // Com BOM (editores do Windows) o JSON.parse falha; tira antes.
    bruto = JSON.parse(texto.charCodeAt(0) === 0xfeff ? texto.slice(1) : texto);
  } catch {
    return { ok: false, motivo: 'malformado' };
  }
  if (typeof bruto !== 'object' || bruto === null || Array.isArray(bruto)) {
    return { ok: false, motivo: 'formato' };
  }
  const o = bruto as Record<string, unknown>;
  if (o.formato !== FORMATO_ARQUIVO) return { ok: false, motivo: 'formato' };
  if (typeof o.versao !== 'number' || !Number.isInteger(o.versao) || o.versao < 1) {
    return { ok: false, motivo: 'formato' };
  }
  if (o.versao > VERSAO_ARQUIVO) return { ok: false, motivo: 'versao' };
  if (!Array.isArray(o.jogos)) return { ok: false, motivo: 'formato' };
  // Teto de itens: um arquivo acima disto é recusado inteiro (não é um backup real).
  if (o.jogos.length > MAXIMO_ITENS_ARQUIVO) return { ok: false, motivo: 'grande' };
  return { ok: true, itens: o.jogos as unknown[] };
}

export interface PlanoImportacao {
  /** Jogos a gravar (já com id livre). */
  readonly novos: readonly JogoSalvo[];
  /** Jogos válidos que o aparelho já tinha (mesma loteria e dezenas), ou repetidos no arquivo. */
  readonly ignorados: number;
  /** Itens que não passaram na validação. */
  readonly invalidos: number;
}

/**
 * Decide o que entra. Idêntico (loteria + dezenas) a um jogo existente ou a um item
 * anterior do arquivo é ignorado. Se o id do arquivo já existe com outro conteúdo, o
 * jogo entra com id novo (nunca sobrescreve um jogo do aparelho). `idsOcupados` são
 * todos os ids que existem no armazenamento, inclusive de registros inválidos que a
 * lista não mostra: um `put` com esse id os sobrescreveria.
 */
export function planejarImportacao(
  existentes: readonly JogoSalvo[],
  itens: readonly unknown[],
  novoId: () => string = gerarId,
  idsOcupados: Iterable<string> = [],
): PlanoImportacao {
  const chaves = new Set(existentes.map(chaveDoJogo));
  const ids = new Set([...existentes.map((j) => j.id), ...idsOcupados]);
  const novos: JogoSalvo[] = [];
  let ignorados = 0;
  let invalidos = 0;

  for (const item of itens) {
    const v = validarJogoSalvo(item);
    if (!v.ok) {
      invalidos++;
      continue;
    }
    const chave = chaveDoJogo(v.valor);
    if (chaves.has(chave)) {
      ignorados++;
      continue;
    }
    let id = v.valor.id;
    while (ids.has(id)) id = novoId();
    chaves.add(chave);
    ids.add(id);
    novos.push(id === v.valor.id ? v.valor : { ...v.valor, id });
  }
  return { novos, ignorados, invalidos };
}
