// Gerador mínimo de PDF (texto e linhas), sem dependências e sem rede. Só o que o app precisa
// para imprimir uma tabela: páginas A4, Helvetica / Helvetica-Bold (fontes padrão do PDF,
// codificação WinAnsi) e linhas retas. Determinístico: a mesma entrada gera os mesmos bytes.

export const LARGURA_A4 = 595.28;
export const ALTURA_A4 = 841.89;

export type Fonte = 'normal' | 'negrito';

type Operacao = string;

/** Uma página: coordenadas em pontos, origem no canto superior esquerdo (como numa folha). */
export class PaginaPdf {
  readonly operacoes: Operacao[] = [];

  /** Texto alinhado à esquerda a partir de `x`; `y` é a linha de base. */
  texto(x: number, y: number, tamanho: number, texto: string, fonte: Fonte = 'normal'): void {
    this.operacoes.push(
      `BT /${fonte === 'negrito' ? 'F2' : 'F1'} ${n(tamanho)} Tf ${n(x)} ${n(ALTURA_A4 - y)} Td (${escapar(texto)}) Tj ET`,
    );
  }

  /** Texto cuja borda direita fica em `x`. */
  textoDireita(
    x: number,
    y: number,
    tamanho: number,
    texto: string,
    fonte: Fonte = 'normal',
  ): void {
    this.texto(x - larguraDoTexto(texto, tamanho, fonte), y, tamanho, texto, fonte);
  }

  /** Texto centrado em `x`. */
  textoCentro(x: number, y: number, tamanho: number, texto: string, fonte: Fonte = 'normal'): void {
    this.texto(x - larguraDoTexto(texto, tamanho, fonte) / 2, y, tamanho, texto, fonte);
  }

  linha(x1: number, y1: number, x2: number, y2: number, espessura = 0.5): void {
    this.operacoes.push(
      `${n(espessura)} w ${n(x1)} ${n(ALTURA_A4 - y1)} m ${n(x2)} ${n(ALTURA_A4 - y2)} l S`,
    );
  }

  /** Contorno de retângulo (borda), sem preenchimento. */
  retanguloContorno(x: number, y: number, largura: number, altura: number, espessura = 1): void {
    this.operacoes.push(
      `${n(espessura)} w ${n(x)} ${n(ALTURA_A4 - y - altura)} ${n(largura)} ${n(altura)} re S`,
    );
  }

  /** Retângulo preenchido com cinza (0 = preto, 1 = branco). */
  retanguloCinza(x: number, y: number, largura: number, altura: number, cinza: number): void {
    this.operacoes.push(
      `${n(cinza)} g ${n(x)} ${n(ALTURA_A4 - y - altura)} ${n(largura)} ${n(altura)} re f 0 g`,
    );
  }
}

/** Número em texto PDF: sem notação científica e com no máximo 2 casas. */
function n(valor: number): string {
  return String(Math.round(valor * 100) / 100);
}

// Larguras da Helvetica (milésimos de em) dos caracteres que a tabela usa; o resto cai em 556.
const LARGURA_HELVETICA: Readonly<Record<string, number>> = {
  ' ': 278,
  '+': 584,
  '-': 333,
  '.': 278,
  ',': 278,
  ':': 278,
  '/': 278,
  '(': 333,
  ')': 333,
};

/** Largura aproximada do texto (suficiente para alinhar números à direita e centralizar). */
export function larguraDoTexto(texto: string, tamanho: number, fonte: Fonte = 'normal'): number {
  let soma = 0;
  for (const c of texto) soma += LARGURA_HELVETICA[c] ?? 556;
  return (soma / 1000) * tamanho * (fonte === 'negrito' ? 1.05 : 1);
}

/** Troca o que a WinAnsi não tem pelo equivalente mais próximo e escapa os delimitadores. */
function escapar(texto: string): string {
  let saida = '';
  for (const c of texto) {
    const troca = c === '−' || c === '‑' || c === '–' || c === '—' ? '-' : c;
    const codigo = troca.charCodeAt(0);
    if (troca === '\\' || troca === '(' || troca === ')') saida += `\\${troca}`;
    else if (codigo >= 32 && codigo <= 255) saida += troca;
    else saida += '?';
  }
  return saida;
}

function paraBytes(texto: string): Uint8Array {
  const bytes = new Uint8Array(texto.length);
  for (let i = 0; i < texto.length; i++) bytes[i] = texto.charCodeAt(i) & 0xff;
  return bytes;
}

export interface MetadadosPdf {
  readonly titulo: string;
  /** Data de criação (UTC) gravada no arquivo. */
  readonly criadoEm: Date;
}

function dataPdf(data: Date): string {
  const p = (v: number) => String(v).padStart(2, '0');
  return `D:${data.getUTCFullYear()}${p(data.getUTCMonth() + 1)}${p(data.getUTCDate())}${p(data.getUTCHours())}${p(data.getUTCMinutes())}${p(data.getUTCSeconds())}Z`;
}

/** Monta o arquivo PDF com as páginas dadas, na ordem. */
export function montarPdf(paginas: readonly PaginaPdf[], meta: MetadadosPdf): Uint8Array {
  // Objetos: 1 catálogo, 2 páginas, 3 e 4 fontes, 5 info; depois, por página, (página, conteúdo).
  const corpos: string[] = [];
  const idPagina = (i: number) => 6 + i * 2;
  corpos[1] = '<< /Type /Catalog /Pages 2 0 R >>';
  corpos[2] = `<< /Type /Pages /Count ${paginas.length} /Kids [${paginas.map((_, i) => `${idPagina(i)} 0 R`).join(' ')}] >>`;
  corpos[3] = '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>';
  corpos[4] =
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>';
  corpos[5] = `<< /Title (${escapar(meta.titulo)}) /Producer (App Força Loterias) /CreationDate (${dataPdf(meta.criadoEm)}) >>`;
  paginas.forEach((pagina, i) => {
    const conteudo = pagina.operacoes.join('\n');
    corpos[idPagina(i)] =
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${n(LARGURA_A4)} ${n(ALTURA_A4)}] ` +
      `/Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents ${idPagina(i) + 1} 0 R >>`;
    corpos[idPagina(i) + 1] = `<< /Length ${conteudo.length} >>\nstream\n${conteudo}\nendstream`;
  });

  let saida = '%PDF-1.4\n';
  const deslocamentos: number[] = [];
  for (let id = 1; id < corpos.length; id++) {
    deslocamentos[id] = saida.length;
    saida += `${id} 0 obj\n${corpos[id]}\nendobj\n`;
  }
  const inicioXref = saida.length;
  saida += `xref\n0 ${corpos.length}\n0000000000 65535 f \n`;
  for (let id = 1; id < corpos.length; id++) {
    saida += `${String(deslocamentos[id]).padStart(10, '0')} 00000 n \n`;
  }
  saida += `trailer\n<< /Size ${corpos.length} /Root 1 0 R /Info 5 0 R >>\nstartxref\n${inicioXref}\n%%EOF\n`;
  return paraBytes(saida);
}
