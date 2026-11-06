// PDF da tabela G-1 (dezenas em ordem de G-1), para imprimir e comparar com o sorteio depois.
// Só formata o que a tela Força já tem: nada é recalculado. Sem Svelte e sem rede.
import { montarPdf, PaginaPdf, ALTURA_A4, LARGURA_A4 } from '../lib/pdf';
import { doisDigitos } from '../lib/numeros';
import { textos } from '../lib/textos';
import type { TabelaGeralV1 } from '../dados/tipos';
import type { G1DaDezena } from './forca';

const MARGEM = 40;
const TOPO_TABELA = 154;
const ALTURA_LINHA = 16.5;
const LINHAS_POR_BLOCO = 35;
const COLUNAS_POR_PAGINA = 2;
const RODAPE = ALTURA_A4 - 48;

export interface EntradaPdfG1 {
  readonly nomeLoteria: string;
  readonly tabela: TabelaGeralV1;
  readonly g1: ReadonlyMap<number, G1DaDezena>;
  /** Posições de força que o último concurso sorteou (borda na tela); some a borda se vazio. */
  readonly posicoesSorteadas?: ReadonlySet<number>;
  readonly geradoEm: Date;
}

interface LinhaPdf {
  readonly g1: number;
  readonly dezena: number;
  readonly forca: number;
  readonly variacao: number | null;
}

function variacaoTexto(variacao: number | null): string {
  if (variacao === null) return '-';
  if (variacao === 0) return '0';
  return `${variacao > 0 ? '+' : '-'}${Math.abs(variacao)}`;
}

/** Dezenas por G-1 (1 = melhor); empate desempata pela posição de força e pela dezena. */
export function linhasDoPdf(
  tabela: TabelaGeralV1,
  g1: ReadonlyMap<number, G1DaDezena>,
): LinhaPdf[] {
  return tabela.dezenas
    .map((d) => ({
      g1: g1.get(d.dezena)?.g1 ?? Number.POSITIVE_INFINITY,
      dezena: d.dezena,
      forca: d.posicao,
      variacao: g1.get(d.dezena)?.variacao ?? null,
    }))
    .sort((a, b) => a.g1 - b.g1 || a.forca - b.forca || a.dezena - b.dezena);
}

/** "slug do jogo" no nome do arquivo: tabela-g1-<loteria>-concurso-<n>.pdf. */
export function nomeDoArquivoG1(slug: string, concurso: number): string {
  return `tabela-g1-${slug}-concurso-${concurso}.pdf`;
}

function cabecalho(pagina: PaginaPdf, entrada: EntradaPdfG1): void {
  const { tabela } = entrada;
  pagina.texto(MARGEM, 62, 20, textos.forca.pdf.titulo(entrada.nomeLoteria), 'negrito');
  pagina.texto(MARGEM, 82, 11, textos.forca.pdf.subtitulo(tabela.concurso, tabela.ultimoSorteado));
  pagina.texto(MARGEM, 97, 9, textos.forca.perfilJanela(tabela.perfil, tabela.janela));
  pagina.texto(
    MARGEM,
    110,
    9,
    textos.forca.pdf.geradoEm(
      entrada.geradoEm.toLocaleString('pt-BR', {
        dateStyle: 'short',
        timeStyle: 'short',
        timeZone: 'America/Sao_Paulo',
      }),
    ),
  );
}

function bloco(
  pagina: PaginaPdf,
  x: number,
  largura: number,
  linhas: readonly LinhaPdf[],
  sorteadas: ReadonlySet<number>,
): void {
  const colG1 = x + 28;
  const colDezena = x + 74;
  const colForca = x + 122;
  const colVar = x + largura;
  const c = textos.forca.pdf.colunas;
  const yCab = TOPO_TABELA;
  pagina.retanguloCinza(x, yCab - 12, largura, 17, 0.9);
  pagina.textoDireita(colG1, yCab, 9, c.g1, 'negrito');
  pagina.textoDireita(colDezena, yCab, 9, c.dezena, 'negrito');
  pagina.textoDireita(colForca, yCab, 9, c.forca, 'negrito');
  pagina.textoDireita(colVar, yCab, 9, c.variacao, 'negrito');
  linhas.forEach((l, i) => {
    const y = yCab + 18 + i * ALTURA_LINHA;
    if (i % 2 === 1) pagina.retanguloCinza(x, y - 11.5, largura, ALTURA_LINHA, 0.96);
    pagina.textoDireita(colG1, y, 10, String(l.g1), 'normal');
    pagina.textoDireita(colDezena, y, 11, doisDigitos(l.dezena), 'negrito');
    pagina.textoDireita(colForca, y, 10, String(l.forca));
    if (sorteadas.has(l.forca)) pagina.retanguloContorno(colForca - 17, y - 11, 22, 14, 1.2);
    pagina.textoDireita(colVar, y, 10, variacaoTexto(l.variacao));
  });
  pagina.linha(
    x,
    yCab + 18 + linhas.length * ALTURA_LINHA - 11.5,
    x + largura,
    yCab + 18 + linhas.length * ALTURA_LINHA - 11.5,
  );
}

/** Gera os bytes do PDF: um bloco de até 40 linhas por coluna, duas colunas por página. */
export function gerarPdfG1(entrada: EntradaPdfG1): Uint8Array {
  const linhas = linhasDoPdf(entrada.tabela, entrada.g1);
  const blocos: LinhaPdf[][] = [];
  for (let i = 0; i < linhas.length; i += LINHAS_POR_BLOCO) {
    blocos.push(linhas.slice(i, i + LINHAS_POR_BLOCO));
  }
  const total = Math.max(1, Math.ceil(blocos.length / COLUNAS_POR_PAGINA));
  const largura = (LARGURA_A4 - 2 * MARGEM - 24) / 2;
  const sorteadas = entrada.posicoesSorteadas ?? new Set<number>();
  const paginas: PaginaPdf[] = [];
  for (let p = 0; p < total; p++) {
    const pagina = new PaginaPdf();
    cabecalho(pagina, entrada);
    if (sorteadas.size > 0) {
      pagina.retanguloContorno(MARGEM, 120, 22, 14, 1.2);
      pagina.texto(
        MARGEM + 30,
        131,
        9,
        textos.forca.legendaPosicoesSorteadas(entrada.tabela.ultimoSorteado),
      );
    }
    for (let k = 0; k < COLUNAS_POR_PAGINA; k++) {
      const dados = blocos[p * COLUNAS_POR_PAGINA + k];
      if (dados !== undefined)
        bloco(pagina, MARGEM + k * (largura + 24), largura, dados, sorteadas);
    }
    pagina.linha(MARGEM, RODAPE - 12, LARGURA_A4 - MARGEM, RODAPE - 12);
    pagina.texto(MARGEM, RODAPE, 8, textos.forca.pdf.aviso);
    pagina.textoDireita(LARGURA_A4 - MARGEM, RODAPE + 14, 8, textos.forca.pdf.pagina(p + 1, total));
    paginas.push(pagina);
  }
  return montarPdf(paginas, {
    titulo: textos.forca.pdf.titulo(entrada.nomeLoteria),
    criadoEm: entrada.geradoEm,
  });
}
