// Validação de forma dos JSON do contrato v1. Cada função recebe o valor já
// interpretado (JSON.parse), confere tipos e invariantes básicas e devolve um
// objeto novo e tipado: campos desconhecidos são ignorados, ausência vira null.
// Erros em português, com o caminho do campo.
import { SCHEMA_VERSION_SUPORTADA } from './config';
import { ErroDados, ErroSchemaNaoSuportado } from './erros';
import {
  CHAVES_ARQUIVO,
  DIRECOES_EVIDENCIA,
  GRAUS_EVIDENCIA,
  NORMALIZACOES,
  PARAMETROS_LOTERIA,
  SLUGS,
  type ChaveObrigatoria,
  type ConcursoHistorico,
  type DescritorArquivo,
  type DezenaDaTabela,
  type DezenaDoRanking,
  type DirecaoEvidencia,
  type GrauEvidencia,
  type HistoricoV1,
  type IndiceV1,
  type LoteriaNoIndice,
  type Normalizacao,
  type DezenaDoG1,
  type RankingPosicoesV1,
  type RankingTaxaV1,
  type Slug,
  type TabelaGeralV1,
  type UltimoV1,
} from './tipos';

type Objeto = Record<string, unknown>;

function falha(onde: string, mensagem: string): never {
  throw new ErroDados('forma', `${onde}: ${mensagem}`);
}

function descrever(v: unknown): string {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'lista';
  if (typeof v === 'string') {
    return `texto ${JSON.stringify(v.length > 40 ? `${v.slice(0, 40)}...` : v)}`;
  }
  if (typeof v === 'object') return 'objeto';
  return `${typeof v} ${String(v)}`;
}

function objeto(v: unknown, onde: string): Objeto {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) {
    return falha(onde, `esperado um objeto, veio ${descrever(v)}.`);
  }
  return v as Objeto;
}

function lista(v: unknown, onde: string): unknown[] {
  if (!Array.isArray(v)) return falha(onde, `esperado uma lista, veio ${descrever(v)}.`);
  return v;
}

function inteiro(v: unknown, onde: string, minimo?: number): number {
  if (typeof v !== 'number' || !Number.isInteger(v)) {
    return falha(onde, `esperado um número inteiro, veio ${descrever(v)}.`);
  }
  if (minimo !== undefined && v < minimo) {
    return falha(onde, `esperado um inteiro maior ou igual a ${minimo}, veio ${v}.`);
  }
  return v;
}

/** Campo anulável/opcional: ausente ou null viram null. */
function inteiroOuNulo(o: Objeto, chave: string, onde: string, minimo = 0): number | null {
  const v = o[chave];
  if (v === undefined || v === null) return null;
  return inteiro(v, `${onde}.${chave}`, minimo);
}

function texto(v: unknown, onde: string): string {
  if (typeof v !== 'string' || v.length === 0) {
    return falha(onde, `esperado um texto não vazio, veio ${descrever(v)}.`);
  }
  return v;
}

function numero(v: unknown, onde: string): number {
  if (typeof v !== 'number' || !Number.isFinite(v)) {
    return falha(onde, `esperado um número finito, veio ${descrever(v)}.`);
  }
  return v;
}

function sha256(v: unknown, onde: string): string {
  const t = texto(v, onde);
  if (!/^[0-9a-f]{64}$/.test(t)) {
    return falha(onde, 'esperado SHA-256 com 64 dígitos hexadecimais minúsculos.');
  }
  return t;
}

function dataIso(v: unknown, onde: string): string {
  const t = texto(v, onde);
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(t);
  if (m) {
    const [ano, mes, dia] = [Number(m[1]), Number(m[2]), Number(m[3])];
    const d = new Date(Date.UTC(ano, mes - 1, dia));
    if (d.getUTCFullYear() === ano && d.getUTCMonth() === mes - 1 && d.getUTCDate() === dia) {
      return t;
    }
  }
  return falha(onde, `esperada uma data válida AAAA-MM-DD, veio ${JSON.stringify(t)}.`);
}

function enumerado<T extends string>(v: unknown, validos: readonly T[], onde: string): T {
  if (typeof v === 'string' && (validos as readonly string[]).includes(v)) return v as T;
  return falha(
    onde,
    `valor ${descrever(v)} desconhecido para este app (esperado um de: ${validos.join(', ')}).`,
  );
}

function enumeradoOuNulo<T extends string>(
  o: Objeto,
  chave: string,
  validos: readonly T[],
  onde: string,
): T | null {
  const v = o[chave];
  if (v === undefined || v === null) return null;
  return enumerado(v, validos, `${onde}.${chave}`);
}

/** Confere schema_version ANTES de olhar o resto: outra versão tem outra forma. */
function exigirSchema(o: Objeto, onde: string): void {
  if (o.schema_version !== SCHEMA_VERSION_SUPORTADA) {
    throw new ErroSchemaNaoSuportado(onde, o.schema_version);
  }
}

/** Dezenas: inteiros, estritamente crescentes (logo sem repetição), em 1..total. */
function dezenasSorteadas(v: unknown, onde: string, loteria: Slug): readonly number[] {
  const { totalDezenas, dezenasSorteadas: quantidade } = PARAMETROS_LOTERIA[loteria];
  const itens = lista(v, onde);
  if (itens.length !== quantidade) {
    return falha(onde, `${loteria} sorteia ${quantidade} dezenas, vieram ${itens.length}.`);
  }
  const dezenas: number[] = [];
  for (const [i, item] of itens.entries()) {
    const d = inteiro(item, `${onde}[${i}]`);
    if (d < 1 || d > totalDezenas) {
      return falha(`${onde}[${i}]`, `dezena ${d} fora do intervalo 1 a ${totalDezenas}.`);
    }
    const anterior = dezenas[i - 1];
    if (anterior !== undefined && d <= anterior) {
      return falha(
        `${onde}[${i}]`,
        d === anterior
          ? `dezena ${d} repetida.`
          : `dezenas fora de ordem crescente (${d} depois de ${anterior}).`,
      );
    }
    dezenas.push(d);
  }
  return dezenas;
}

function descritor(v: unknown, onde: string): DescritorArquivo {
  const o = objeto(v, onde);
  const caminho = texto(o.caminho, `${onde}.caminho`);
  // Caminho relativo e simples: nunca deixa o índice apontar para fora da base.
  if (!/^[a-z0-9_]+(\/[a-z0-9_]+)*\.json$/.test(caminho)) {
    falha(`${onde}.caminho`, `caminho relativo inválido: ${JSON.stringify(caminho)}.`);
  }
  return {
    caminho,
    sha256: sha256(o.sha256, `${onde}.sha256`),
    bytes: inteiro(o.bytes, `${onde}.bytes`, 0),
  };
}

export function validarIndice(valor: unknown): IndiceV1 {
  const nome = 'index.json';
  const o = objeto(valor, nome);
  exigirSchema(o, nome);
  const geradoEm = texto(o.gerado_em, `${nome}.gerado_em`);
  const bruto = objeto(o.loterias, `${nome}.loterias`);
  const loterias = {} as Record<Slug, LoteriaNoIndice | null>;
  // Slugs desconhecidos no índice são ignorados; slug conhecido ausente = null.
  for (const s of SLUGS) {
    const item = bruto[s];
    if (item === undefined || item === null) {
      loterias[s] = null;
      continue;
    }
    const onde = `${nome}.loterias.${s}`;
    const l = objeto(item, onde);
    const arq = objeto(l.arquivos, `${onde}.arquivos`);
    const arquivos = {} as Record<ChaveObrigatoria, DescritorArquivo> & {
      ranking_taxa?: DescritorArquivo;
      ranking_posicoes?: DescritorArquivo;
    };
    for (const chave of CHAVES_ARQUIVO) {
      arquivos[chave] = descritor(arq[chave], `${onde}.arquivos.${chave}`);
    }
    // Opcional: a chave some quando o ranking não foi publicado (null também é tolerado).
    if (arq.ranking_taxa !== undefined && arq.ranking_taxa !== null) {
      arquivos.ranking_taxa = descritor(arq.ranking_taxa, `${onde}.arquivos.ranking_taxa`);
    }
    if (arq.ranking_posicoes !== undefined && arq.ranking_posicoes !== null) {
      arquivos.ranking_posicoes = descritor(
        arq.ranking_posicoes,
        `${onde}.arquivos.ranking_posicoes`,
      );
    }
    loterias[s] = {
      ultimoConcurso: inteiro(l.ultimo_concurso, `${onde}.ultimo_concurso`, 1),
      data: dataIso(l.data, `${onde}.data`),
      arquivos,
    };
  }
  return { schemaVersion: 1, geradoEm, loterias };
}

function cabecalhoLoteria(o: Objeto, nome: string, esperada: Slug | undefined): Slug {
  exigirSchema(o, nome);
  const s = enumerado(o.loteria, SLUGS, `${nome}.loteria`);
  if (esperada !== undefined && s !== esperada) {
    falha(`${nome}.loteria`, `esperada ${esperada}, veio ${s}.`);
  }
  return s;
}

export function validarUltimo(valor: unknown, esperada?: Slug): UltimoV1 {
  const nome = 'ultimo.json';
  const o = objeto(valor, nome);
  const loteria = cabecalhoLoteria(o, nome, esperada);
  return {
    schemaVersion: 1,
    loteria,
    concurso: inteiro(o.concurso, `${nome}.concurso`, 1),
    data: dataIso(o.data, `${nome}.data`),
    dezenas: dezenasSorteadas(o.dezenas, `${nome}.dezenas`, loteria),
  };
}

export function validarHistorico(valor: unknown, esperada?: Slug): HistoricoV1 {
  const nome = 'historico.json';
  const o = objeto(valor, nome);
  const loteria = cabecalhoLoteria(o, nome, esperada);
  const itens = lista(o.concursos, `${nome}.concursos`);
  if (itens.length === 0) falha(`${nome}.concursos`, 'a lista de concursos está vazia.');
  const concursos: ConcursoHistorico[] = [];
  let anterior: ConcursoHistorico | undefined;
  for (const [i, item] of itens.entries()) {
    const onde = `${nome}.concursos[${i}]`;
    const linha = lista(item, onde);
    if (linha.length < 3) {
      falha(
        onde,
        `esperada uma lista de 3 posições [concurso, data, dezenas], veio com ${linha.length}.`,
      );
    }
    const concurso = inteiro(linha[0], `${onde}[0]`, 1);
    const data = dataIso(linha[1], `${onde}[1]`);
    const dezenas = dezenasSorteadas(linha[2], `${onde}[2]`, loteria);
    if (anterior) {
      if (concurso !== anterior[0] + 1) {
        falha(
          `${onde}[0]`,
          `concursos devem ser seguidos, sem buraco nem repetição (${concurso} depois de ${anterior[0]}).`,
        );
      }
      if (data < anterior[1]) {
        falha(`${onde}[1]`, `data ${data} anterior à do concurso ${anterior[0]} (${anterior[1]}).`);
      }
    }
    anterior = [concurso, data, dezenas];
    concursos.push(anterior);
  }
  return { schemaVersion: 1, loteria, concursos };
}

function dezenaDaTabela(v: unknown, onde: string, total: number): DezenaDaTabela {
  const o = objeto(v, onde);
  const dezena = inteiro(o.dezena, `${onde}.dezena`);
  if (dezena < 1 || dezena > total) {
    falha(`${onde}.dezena`, `dezena ${dezena} fora do intervalo 1 a ${total}.`);
  }
  const grau: GrauEvidencia | null = enumeradoOuNulo(o, 'evidencia_grau', GRAUS_EVIDENCIA, onde);
  const direcao: DirecaoEvidencia | null = enumeradoOuNulo(
    o,
    'evidencia_direcao',
    DIRECOES_EVIDENCIA,
    onde,
  );
  if ((grau === null) !== (direcao === null)) {
    falha(onde, 'evidencia_grau e evidencia_direcao devem vir juntos (ou ambos ausentes).');
  }
  return {
    dezena,
    posicao: inteiro(o.posicao, `${onde}.posicao`, 1),
    total: numero(o.total, `${onde}.total`),
    variacaoPosicao:
      o.variacao_posicao === undefined || o.variacao_posicao === null
        ? null
        : inteiro(o.variacao_posicao, `${onde}.variacao_posicao`),
    frequencia: inteiro(o.frequencia, `${onde}.frequencia`, 0),
    maiorEspaco: inteiroOuNulo(o, 'maior_espaco', onde),
    maiorEspacoOcorrencias: inteiroOuNulo(o, 'maior_espaco_ocorrencias', onde),
    menorEspaco: inteiroOuNulo(o, 'menor_espaco', onde),
    menorEspacoOcorrencias: inteiroOuNulo(o, 'menor_espaco_ocorrencias', onde),
    somaVariacoes: inteiroOuNulo(o, 'soma_variacoes', onde),
    espacoFinal: inteiroOuNulo(o, 'espaco_final', onde),
    repeticoesTotal: inteiro(o.repeticoes_total, `${onde}.repeticoes_total`, 0),
    repeticaoAtual: inteiro(o.repeticao_atual, `${onde}.repeticao_atual`, 0),
    evidenciaGrau: grau,
    evidenciaDirecao: direcao,
  };
}

export function validarTabelaGeral(valor: unknown, esperada?: Slug): TabelaGeralV1 {
  const nome = 'tabela_geral.json';
  const o = objeto(valor, nome);
  const loteria = cabecalhoLoteria(o, nome, esperada);
  const total = PARAMETROS_LOTERIA[loteria].totalDezenas;
  const normalizacao: Normalizacao = enumerado(
    o.normalizacao,
    NORMALIZACOES,
    `${nome}.normalizacao`,
  );
  const concurso = inteiro(o.concurso, `${nome}.concurso`, 1);
  const ultimoSorteado = inteiro(o.ultimo_sorteado, `${nome}.ultimo_sorteado`, 1);
  if (concurso !== ultimoSorteado + 1) {
    falha(
      `${nome}.concurso`,
      `deve ser ultimo_sorteado + 1 (${ultimoSorteado + 1}), veio ${concurso}.`,
    );
  }
  const itens = lista(o.dezenas, `${nome}.dezenas`);
  if (itens.length !== total) {
    falha(`${nome}.dezenas`, `${loteria} tem ${total} dezenas, vieram ${itens.length}.`);
  }
  const dezenas: DezenaDaTabela[] = [];
  const vistas = new Set<number>();
  for (const [i, item] of itens.entries()) {
    const onde = `${nome}.dezenas[${i}]`;
    const d = dezenaDaTabela(item, onde, total);
    if (d.posicao !== i + 1) {
      falha(
        `${onde}.posicao`,
        `esperada a posição ${i + 1} (lista em ordem de posição), veio ${d.posicao}.`,
      );
    }
    if (vistas.has(d.dezena)) falha(`${onde}.dezena`, `dezena ${d.dezena} repetida.`);
    vistas.add(d.dezena);
    const anterior = dezenas[i - 1];
    if (anterior && d.total > anterior.total) {
      falha(`${onde}.total`, 'o índice deve ser decrescente ao longo das posições.');
    }
    dezenas.push(d);
  }
  return {
    schemaVersion: 1,
    loteria,
    perfil: texto(o.perfil, `${nome}.perfil`),
    hashToml: sha256(o.hash_toml, `${nome}.hash_toml`),
    janela: inteiro(o.janela, `${nome}.janela`, 0),
    normalizacao,
    featureVersion: inteiro(o.feature_version, `${nome}.feature_version`, 1),
    concurso,
    ultimoSorteado,
    dezenas,
  };
}

/** Decimal do contrato: finito e com no máximo 9 casas (tolerância a erro de ponto flutuante). */
function decimal9(v: unknown, onde: string): number {
  const n = numero(v, onde);
  if (Math.abs(n * 1e9 - Math.round(n * 1e9)) > 1e-3) {
    falha(onde, `esperado um decimal com no máximo 9 casas, veio ${n}.`);
  }
  return n;
}

/** Posições 1..N sem repetição (a lista inteira é uma permutação). */
function exigirPermutacao(valores: readonly number[], onde: string): void {
  const vistos = new Set(valores);
  if (vistos.size !== valores.length || valores.some((v) => v < 1 || v > valores.length)) {
    falha(onde, `as posições devem ser 1 a ${valores.length}, sem repetição.`);
  }
}

export function validarRankingTaxa(valor: unknown, esperada?: Slug): RankingTaxaV1 {
  const nome = 'ranking_taxa.json';
  const o = objeto(valor, nome);
  const loteria = cabecalhoLoteria(o, nome, esperada);
  const total = PARAMETROS_LOTERIA[loteria].totalDezenas;
  const normalizacao: Normalizacao = enumerado(
    o.normalizacao,
    NORMALIZACOES,
    `${nome}.normalizacao`,
  );
  const concurso = inteiro(o.concurso, `${nome}.concurso`, 1);
  const ultimoSorteado = inteiro(o.ultimo_sorteado, `${nome}.ultimo_sorteado`, 1);
  if (concurso !== ultimoSorteado + 1) {
    falha(
      `${nome}.concurso`,
      `deve ser ultimo_sorteado + 1 (${ultimoSorteado + 1}), veio ${concurso}.`,
    );
  }
  const taxaAcaso = decimal9(o.taxa_acaso, `${nome}.taxa_acaso`);
  if (taxaAcaso <= 0 || taxaAcaso >= 1) {
    falha(`${nome}.taxa_acaso`, `esperado um número entre 0 e 1, veio ${taxaAcaso}.`);
  }
  const itens = lista(o.dezenas, `${nome}.dezenas`);
  if (itens.length !== total) {
    falha(`${nome}.dezenas`, `${loteria} tem ${total} dezenas, vieram ${itens.length}.`);
  }
  const dezenas: DezenaDoRanking[] = [];
  for (const [i, item] of itens.entries()) {
    const onde = `${nome}.dezenas[${i}]`;
    const e = objeto(item, onde);
    const dezena = inteiro(e.dezena, `${onde}.dezena`);
    if (dezena !== i + 1) {
      falha(
        `${onde}.dezena`,
        `esperada a dezena ${i + 1} (lista em ordem crescente, sem repetição), veio ${dezena}.`,
      );
    }
    const notaA = decimal9(e.nota_a, `${onde}.nota_a`);
    const diferencaAcaso = decimal9(e.diferenca_acaso, `${onde}.diferenca_acaso`);
    if (Math.abs(notaA - taxaAcaso - diferencaAcaso) > 2e-9) {
      falha(`${onde}.diferenca_acaso`, 'deve ser nota_a menos taxa_acaso.');
    }
    dezenas.push({
      dezena,
      posicaoIndice: inteiro(e.posicao_indice, `${onde}.posicao_indice`, 1),
      notaA,
      diferencaAcaso,
      notaB: decimal9(e.nota_b, `${onde}.nota_b`),
      posicaoA: inteiro(e.posicao_a, `${onde}.posicao_a`, 1),
      posicaoB: inteiro(e.posicao_b, `${onde}.posicao_b`, 1),
    });
  }
  exigirPermutacao(
    dezenas.map((d) => d.posicaoIndice),
    `${nome}.dezenas[].posicao_indice`,
  );
  exigirPermutacao(
    dezenas.map((d) => d.posicaoA),
    `${nome}.dezenas[].posicao_a`,
  );
  exigirPermutacao(
    dezenas.map((d) => d.posicaoB),
    `${nome}.dezenas[].posicao_b`,
  );
  return {
    schemaVersion: 1,
    loteria,
    perfil: texto(o.perfil, `${nome}.perfil`),
    hashToml: sha256(o.hash_toml, `${nome}.hash_toml`),
    janela: inteiro(o.janela, `${nome}.janela`, 0),
    normalizacao,
    featureVersion: inteiro(o.feature_version, `${nome}.feature_version`, 1),
    concurso,
    ultimoSorteado,
    taxaAcaso,
    dezenas,
  };
}

export function validarRankingPosicoes(valor: unknown, esperada?: Slug): RankingPosicoesV1 {
  const nome = 'ranking_posicoes.json';
  const o = objeto(valor, nome);
  const loteria = cabecalhoLoteria(o, nome, esperada);
  const total = PARAMETROS_LOTERIA[loteria].totalDezenas;
  const normalizacao: Normalizacao = enumerado(
    o.normalizacao,
    NORMALIZACOES,
    `${nome}.normalizacao`,
  );
  const normalizacaoPosicoes: Normalizacao = enumerado(
    o.normalizacao_posicoes,
    NORMALIZACOES,
    `${nome}.normalizacao_posicoes`,
  );
  const concurso = inteiro(o.concurso, `${nome}.concurso`, 1);
  const ultimoSorteado = inteiro(o.ultimo_sorteado, `${nome}.ultimo_sorteado`, 1);
  if (concurso !== ultimoSorteado + 1) {
    falha(
      `${nome}.concurso`,
      `deve ser ultimo_sorteado + 1 (${ultimoSorteado + 1}), veio ${concurso}.`,
    );
  }
  const itens = lista(o.dezenas, `${nome}.dezenas`);
  if (itens.length !== total) {
    falha(`${nome}.dezenas`, `${loteria} tem ${total} dezenas, vieram ${itens.length}.`);
  }
  const dezenas: DezenaDoG1[] = [];
  for (const [i, item] of itens.entries()) {
    const onde = `${nome}.dezenas[${i}]`;
    const e = objeto(item, onde);
    const dezena = inteiro(e.dezena, `${onde}.dezena`);
    if (dezena !== i + 1) {
      falha(
        `${onde}.dezena`,
        `esperada a dezena ${i + 1} (lista em ordem crescente, sem repetição), veio ${dezena}.`,
      );
    }
    dezenas.push({
      dezena,
      posicaoIndice: inteiro(e.posicao_indice, `${onde}.posicao_indice`, 1),
      g1: inteiro(e.g_1, `${onde}.g_1`, 1),
      // Ausente ou null = sem concurso anterior (nunca zero).
      variacaoG1:
        e.variacao_g_1 === undefined || e.variacao_g_1 === null
          ? null
          : inteiro(e.variacao_g_1, `${onde}.variacao_g_1`),
    });
  }
  exigirPermutacao(
    dezenas.map((d) => d.posicaoIndice),
    `${nome}.dezenas[].posicao_indice`,
  );
  exigirPermutacao(
    dezenas.map((d) => d.g1),
    `${nome}.dezenas[].g_1`,
  );
  return {
    schemaVersion: 1,
    loteria,
    perfil: texto(o.perfil, `${nome}.perfil`),
    hashToml: sha256(o.hash_toml, `${nome}.hash_toml`),
    janela: inteiro(o.janela, `${nome}.janela`, 0),
    normalizacao,
    featureVersion: inteiro(o.feature_version, `${nome}.feature_version`, 1),
    concurso,
    ultimoSorteado,
    perfilPosicoes: texto(o.perfil_posicoes, `${nome}.perfil_posicoes`),
    hashTomlPosicoes: sha256(o.hash_toml_posicoes, `${nome}.hash_toml_posicoes`),
    janelaPosicoes: inteiro(o.janela_posicoes, `${nome}.janela_posicoes`, 0),
    normalizacaoPosicoes,
    dezenas,
  };
}
