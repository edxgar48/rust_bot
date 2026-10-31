// Cliente dos JSON do contrato v1: fluxo do index.json (docs/contrato-v1.md,
// "Como o app deve usar o index.json"). Regras:
//  - o hash e o tamanho são conferidos sobre os bytes recebidos, ANTES de
//    interpretar o JSON;
//  - só se baixa o arquivo cujo sha256 mudou em relação ao guardado;
//  - dado que falhou em qualquer conferência nunca é guardado nem devolvido
//    como novo: o último arquivo válido continua valendo.
import { CAMINHO_INDICE, type Armazenamento } from './armazenamento';
import { URL_BASE_PADRAO, normalizarUrlBase } from './config';
import { ErroDados } from './erros';
import { sha256Hex } from './hash';
import {
  CHAVES_ARQUIVO,
  SLUGS,
  type ChaveArquivo,
  type DadosPorChave,
  type DescritorArquivo,
  type IndiceV1,
  type LoteriaNoIndice,
  type Slug,
} from './tipos';
import {
  validarHistorico,
  validarIndice,
  validarRankingPosicoes,
  validarRankingTaxa,
  validarTabelaGeral,
  validarUltimo,
} from './validacao';

export type FetchInjetavel = (url: string, init?: RequestInit) => Promise<Response>;

export interface OpcoesCliente {
  armazenamento: Armazenamento;
  /** Padrão: URL_BASE_PADRAO (config.ts). */
  urlBase?: string;
  /** Padrão: fetch global. Injetável para testes (sem rede). */
  fetch?: FetchInjetavel;
  /** Relógio injetável (testes). Padrão: `new Date()`. */
  agora?: () => Date;
}

export interface OpcoesSincronizacao {
  /** Padrão: todas as loterias. */
  loterias?: readonly Slug[];
  /** Padrão: os três arquivos. O app pode pedir só o que a tela usa. */
  arquivos?: readonly ChaveArquivo[];
}

export type ResultadoArquivo<K extends ChaveArquivo> =
  | {
      /** `baixado`: sha256 novo, baixado e conferido. `em_cache`: o guardado já valia. */
      readonly estado: 'baixado' | 'em_cache';
      readonly dados: DadosPorChave[K];
      readonly sha256: string;
    }
  | {
      readonly estado: 'falhou';
      readonly erro: ErroDados;
      /** Último arquivo válido guardado, se houver (nunca o que falhou). */
      readonly dados: DadosPorChave[K] | null;
    };

export interface ResultadoLoteria {
  readonly indice: LoteriaNoIndice;
  readonly arquivos: { readonly [K in ChaveArquivo]?: ResultadoArquivo<K> };
}

export interface ResultadoSincronizacao {
  readonly indice: IndiceV1;
  /** `null` = loteria sem arquivos no índice (tolerado). */
  readonly loterias: { readonly [S in Slug]?: ResultadoLoteria | null };
}

function interpretar<K extends ChaveArquivo>(
  chave: K,
  bytes: Uint8Array,
  slug: Slug,
): DadosPorChave[K] {
  let texto: string;
  try {
    texto = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    throw new ErroDados('json_invalido', `${chave}: os bytes não são UTF-8 válido.`);
  }
  let valor: unknown;
  try {
    valor = JSON.parse(texto);
  } catch (e) {
    const detalhe = e instanceof Error ? ` (${e.message})` : '';
    throw new ErroDados('json_invalido', `${chave}: o conteúdo não é JSON válido${detalhe}.`);
  }
  // Os ramos do switch devolvem o tipo exato de cada chave.
  const dados =
    chave === 'ultimo'
      ? validarUltimo(valor, slug)
      : chave === 'tabela_geral'
        ? validarTabelaGeral(valor, slug)
        : chave === 'ranking_taxa'
          ? validarRankingTaxa(valor, slug)
          : chave === 'ranking_posicoes'
            ? validarRankingPosicoes(valor, slug)
            : validarHistorico(valor, slug);
  return dados as DadosPorChave[K];
}

/** O que há guardado, lido sem rede (reconferido contra o sha256 guardado). */
export interface ResultadoCache {
  /** index.json válido mais recente guardado. */
  readonly indice: IndiceV1;
  /** Quando o índice foi guardado (ISO UTC), se conhecido. */
  readonly indiceSincronizadoEm: string | null;
  /**
   * Instante mais antigo entre as últimas confirmações dos arquivos presentes
   * no cache (o dado "vale desde" ali); `null` se desconhecido.
   */
  readonly sincronizadoEm: string | null;
  /** Por loteria e arquivo: dados guardados, ou `null` se ausentes/corrompidos. */
  readonly loterias: {
    readonly [S in Slug]?: {
      readonly indice: LoteriaNoIndice;
      readonly dados: { readonly [K in ChaveArquivo]?: DadosPorChave[K] | null };
    } | null;
  };
}

export function criarCliente(opcoes: OpcoesCliente) {
  const urlBase = normalizarUrlBase(opcoes.urlBase ?? URL_BASE_PADRAO);
  const buscar: FetchInjetavel = opcoes.fetch ?? ((url, init) => globalThis.fetch(url, init));
  const { armazenamento } = opcoes;
  const agora = opcoes.agora ?? (() => new Date());
  const instante = () => agora().toISOString();

  // Último conteúdo já validado por caminho. Os bytes são sempre reconferidos pelo
  // sha256; se não mudaram, reaproveita o objeto interpretado em vez de validar de
  // novo (o histórico tem centenas de KB e a tela o lê mais de uma vez ao abrir).
  const validados = new Map<string, { sha256: string; dados: unknown }>();

  /** Gravar nunca derruba nem invalida um dado já conferido. */
  async function guardarSemFalhar(arquivo: Parameters<Armazenamento['guardar']>[0]): Promise<void> {
    try {
      await armazenamento.guardar(arquivo);
    } catch {
      /* armazenamento indisponível: o dado conferido segue valendo nesta sessão */
    }
  }

  async function baixarBytes(caminho: string): Promise<Uint8Array> {
    const url = new URL(caminho, urlBase).href;
    let resposta: Response;
    try {
      // Sem cache HTTP do navegador: quem decide se algo mudou é o sha256 do índice.
      resposta = await buscar(url, { cache: 'no-store' });
    } catch (e) {
      const detalhe = e instanceof Error ? `: ${e.message}` : '';
      throw new ErroDados('rede', `Não foi possível baixar ${caminho}${detalhe}.`);
    }
    if (!resposta.ok) {
      throw new ErroDados('http', `Falha ao baixar ${caminho}: resposta HTTP ${resposta.status}.`);
    }
    return new Uint8Array(await resposta.arrayBuffer());
  }

  /** Índice: sempre baixado; não tem hash próprio, então só a forma é conferida. */
  async function baixarIndice(): Promise<IndiceV1> {
    const bytes = await baixarBytes('index.json');
    let valor: unknown;
    try {
      valor = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    } catch {
      throw new ErroDados('json_invalido', 'index.json: o conteúdo não é JSON válido.');
    }
    const indice = validarIndice(valor);
    // Só o índice que passou na validação vira o "último índice válido".
    await guardarSemFalhar({
      caminho: CAMINHO_INDICE,
      sha256: await sha256Hex(bytes),
      bytes,
      sincronizadoEm: instante(),
    });
    return indice;
  }

  async function conferirBytes(descritor: DescritorArquivo, bytes: Uint8Array): Promise<void> {
    if (bytes.byteLength !== descritor.bytes) {
      throw new ErroDados(
        'bytes',
        `${descritor.caminho}: tamanho recebido ${bytes.byteLength} bytes, o índice declara ${descritor.bytes}.`,
      );
    }
    const hash = await sha256Hex(bytes);
    if (hash !== descritor.sha256) {
      throw new ErroDados(
        'hash',
        `${descritor.caminho}: SHA-256 recebido (${hash}) difere do índice (${descritor.sha256}).`,
      );
    }
  }

  /** Último arquivo válido guardado, reconferido; `null` se não há ou se corrompeu. */
  async function lerDoCache<K extends ChaveArquivo>(
    chave: K,
    slug: Slug,
    caminho: string,
  ): Promise<{
    dados: DadosPorChave[K];
    sha256: string;
    bytes: Uint8Array;
    sincronizadoEm: string | null;
  } | null> {
    try {
      const guardado = await armazenamento.ler(caminho);
      if (!guardado) return null;
      const hash = await sha256Hex(guardado.bytes);
      if (hash !== guardado.sha256) return null;
      const anterior = validados.get(caminho);
      let dados: DadosPorChave[K];
      if (anterior && anterior.sha256 === guardado.sha256) {
        dados = anterior.dados as DadosPorChave[K];
      } else {
        dados = interpretar(chave, guardado.bytes, slug);
        validados.set(caminho, { sha256: guardado.sha256, dados });
      }
      return {
        dados,
        sha256: guardado.sha256,
        bytes: guardado.bytes,
        sincronizadoEm: guardado.sincronizadoEm ?? null,
      };
    } catch {
      return null;
    }
  }

  async function obterArquivo<K extends ChaveArquivo>(
    chave: K,
    slug: Slug,
    entrada: LoteriaNoIndice,
    descritor: DescritorArquivo,
  ): Promise<ResultadoArquivo<K>> {
    const guardado = await lerDoCache(chave, slug, descritor.caminho);
    if (guardado && guardado.sha256 === descritor.sha256) {
      // O índice atual confirma que este arquivo continua o vigente.
      await guardarSemFalhar({
        caminho: descritor.caminho,
        sha256: guardado.sha256,
        bytes: guardado.bytes,
        sincronizadoEm: instante(),
      });
      return { estado: 'em_cache', dados: guardado.dados, sha256: guardado.sha256 };
    }
    try {
      const bytes = await baixarBytes(descritor.caminho);
      await conferirBytes(descritor, bytes);
      const dados = interpretar(chave, bytes, slug);
      validados.set(descritor.caminho, { sha256: descritor.sha256, dados });
      if (
        chave === 'ultimo' &&
        ((dados as DadosPorChave['ultimo']).concurso !== entrada.ultimoConcurso ||
          (dados as DadosPorChave['ultimo']).data !== entrada.data)
      ) {
        throw new ErroDados(
          'forma',
          `${descritor.caminho}: concurso/data não conferem com o índice (${entrada.ultimoConcurso}, ${entrada.data}).`,
        );
      }
      await guardarSemFalhar({
        caminho: descritor.caminho,
        sha256: descritor.sha256,
        bytes,
        sincronizadoEm: instante(),
      });
      return { estado: 'baixado', dados, sha256: descritor.sha256 };
    } catch (e) {
      const erro =
        e instanceof ErroDados
          ? e
          : new ErroDados(
              'rede',
              `${descritor.caminho}: ${e instanceof Error ? e.message : String(e)}`,
            );
      return { estado: 'falhou', erro, dados: guardado?.dados ?? null };
    }
  }

  async function obterLoteria(
    slug: Slug,
    entrada: LoteriaNoIndice,
    chaves: readonly ChaveArquivo[],
  ): Promise<ResultadoLoteria> {
    const arquivos: { [K in ChaveArquivo]?: ResultadoArquivo<K> } = {};
    await Promise.all(
      chaves.map(async (chave) => {
        // Arquivo opcional que o índice não lista (ranking_taxa, ranking_posicoes): não há o que baixar.
        const descritor = entrada.arquivos[chave];
        if (!descritor) return;
        // Atribuição por chave: o tipo do resultado acompanha a chave.
        Object.assign(arquivos, { [chave]: await obterArquivo(chave, slug, entrada, descritor) });
      }),
    );
    return { indice: entrada, arquivos };
  }

  /**
   * Baixa o índice (lança ErroDados se falhar ou se schema_version != 1) e
   * atualiza só os arquivos cujo sha256 mudou. Falha de um arquivo não afeta os
   * outros nem o cache.
   */
  async function sincronizar(
    opcoesSync: OpcoesSincronizacao = {},
  ): Promise<ResultadoSincronizacao> {
    const indice = await baixarIndice();
    const slugs = opcoesSync.loterias ?? SLUGS;
    const chaves = opcoesSync.arquivos ?? CHAVES_ARQUIVO;
    const loterias: { [S in Slug]?: ResultadoLoteria | null } = {};
    await Promise.all(
      slugs.map(async (slug) => {
        const entrada = indice.loterias[slug];
        loterias[slug] = entrada ? await obterLoteria(slug, entrada, chaves) : null;
      }),
    );
    return { indice, loterias };
  }

  /**
   * Lê só o armazenamento, sem rede: último índice válido e os arquivos
   * guardados. `null` se nunca houve sincronização (ou o índice guardado
   * corrompeu). Nunca lança.
   */
  async function lerCache(opcoesCache: OpcoesSincronizacao = {}): Promise<ResultadoCache | null> {
    let indice: IndiceV1;
    let indiceSincronizadoEm: string | null;
    try {
      const guardado = await armazenamento.ler(CAMINHO_INDICE);
      if (!guardado || (await sha256Hex(guardado.bytes)) !== guardado.sha256) return null;
      indice = validarIndice(
        JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(guardado.bytes)),
      );
      indiceSincronizadoEm = guardado.sincronizadoEm ?? null;
    } catch {
      return null;
    }
    const slugs = opcoesCache.loterias ?? SLUGS;
    const chaves = opcoesCache.arquivos ?? CHAVES_ARQUIVO;
    const loterias: { -readonly [S in Slug]?: ResultadoCache['loterias'][S] } = {};
    const datas: string[] = [];
    let faltouData = false;
    for (const slug of slugs) {
      const entrada = indice.loterias[slug];
      if (!entrada) {
        loterias[slug] = null;
        continue;
      }
      const dados: { [K in ChaveArquivo]?: DadosPorChave[K] | null } = {};
      for (const chave of chaves) {
        const descritor = entrada.arquivos[chave];
        const lido = descritor ? await lerDoCache(chave, slug, descritor.caminho) : null;
        Object.assign(dados, { [chave]: lido?.dados ?? null });
        if (lido) {
          if (lido.sincronizadoEm) datas.push(lido.sincronizadoEm);
          else faltouData = true;
        }
      }
      loterias[slug] = { indice: entrada, dados };
    }
    datas.sort();
    const sincronizadoEm = faltouData ? null : (datas[0] ?? indiceSincronizadoEm);
    return { indice, indiceSincronizadoEm, sincronizadoEm, loterias };
  }

  return { urlBase, sincronizar, lerCache };
}

export type ClienteDados = ReturnType<typeof criarCliente>;
