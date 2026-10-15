// Operações dos jogos salvos que juntam as regras puras e o repositório: salvar sem
// duplicar, exportar e importar. As telas só chamam estas funções.
import {
  lerArquivoJogos,
  nomeDoArquivo,
  planejarImportacao,
  serializarJogos,
  type MotivoRecusa,
} from './arquivo';
import {
  chaveDoJogo,
  criarJogo,
  gerarId,
  type EntradaJogo,
  type JogoSalvo,
  type MotivoInvalido,
} from './jogo';
import type { RepositorioJogos } from './repositorio';

export type ResultadoSalvar =
  | {
      readonly tipo: 'salvo';
      readonly jogo: JogoSalvo;
      /** `true` se não havia nenhum jogo antes (momento de pedir armazenamento persistente). */
      readonly primeiro: boolean;
    }
  | { readonly tipo: 'duplicado'; readonly existente: JogoSalvo }
  | { readonly tipo: 'invalido'; readonly motivo: MotivoInvalido };

export async function salvarJogo(
  repo: RepositorioJogos,
  entrada: EntradaJogo,
  agora: Date = new Date(),
  novoId: () => string = gerarId,
): Promise<ResultadoSalvar> {
  const criado = criarJogo(entrada, novoId(), agora);
  if (!criado.ok) return { tipo: 'invalido', motivo: criado.motivo };
  const existentes = await repo.listar();
  const chave = chaveDoJogo(criado.valor);
  const repetido = existentes.find((j) => chaveDoJogo(j) === chave);
  if (repetido) return { tipo: 'duplicado', existente: repetido };
  await repo.gravar([criado.valor]);
  return { tipo: 'salvo', jogo: criado.valor, primeiro: existentes.length === 0 };
}

/**
 * "Marcar como visto": o jogo passa a valer como conferido até `concurso`. Só avança
 * (nunca volta a marca) e devolve `null` se o jogo não existe mais.
 */
export async function marcarComoVisto(
  repo: RepositorioJogos,
  id: string,
  concurso: number,
): Promise<JogoSalvo | null> {
  // Leitura e gravação no mesmo passo do repositório: uma renomeação concorrente não é desfeita.
  const [novo] = await repo.avancarMarcas(new Map([[id, concurso]]));
  return novo ?? null;
}

export interface ArquivoExportado {
  readonly nome: string;
  readonly conteudo: string;
  readonly quantidade: number;
}

/** Todos os jogos de todas as loterias, como arquivo de backup. */
export async function exportarJogos(
  repo: RepositorioJogos,
  agora: Date = new Date(),
): Promise<ArquivoExportado> {
  const jogos = await repo.listar();
  return {
    nome: nomeDoArquivo(agora),
    conteudo: serializarJogos(jogos, agora),
    quantidade: jogos.length,
  };
}

export type ResultadoImportacao =
  | { readonly tipo: 'recusado'; readonly motivo: MotivoRecusa }
  | {
      readonly tipo: 'importado';
      readonly entraram: number;
      readonly ignorados: number;
      readonly invalidos: number;
    };

/**
 * Importa o texto de um arquivo de backup. Arquivo malformado, de outro formato ou de
 * versão desconhecida é recusado sem tocar no repositório; itens ruins dentro de um
 * arquivo bom são só contados.
 */
export async function importarJogos(
  repo: RepositorioJogos,
  texto: string,
  novoId: () => string = gerarId,
): Promise<ResultadoImportacao> {
  const leitura = lerArquivoJogos(texto);
  if (!leitura.ok) return { tipo: 'recusado', motivo: leitura.motivo };
  const plano = planejarImportacao(
    await repo.listar(),
    leitura.itens,
    novoId,
    await repo.listarIds(),
  );
  await repo.gravar(plano.novos);
  return {
    tipo: 'importado',
    entraram: plano.novos.length,
    ignorados: plano.ignorados,
    invalidos: plano.invalidos,
  };
}

/**
 * Pede ao navegador que não apague os dados do site sem avisar (best-effort).
 * Devolve `true` (concedido), `false` (negado) ou `null` (sem suporte ou erro).
 * Nunca lança e nunca bloqueia a gravação.
 */
export async function pedirArmazenamentoPersistente(
  armazenamento: Pick<StorageManager, 'persist' | 'persisted'> | undefined = globalThis.navigator
    ?.storage,
): Promise<boolean | null> {
  try {
    if (!armazenamento || typeof armazenamento.persist !== 'function') return null;
    if (typeof armazenamento.persisted === 'function' && (await armazenamento.persisted())) {
      return true;
    }
    return await armazenamento.persist();
  } catch {
    return null;
  }
}
