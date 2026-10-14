// Visão pura do detalhe de uma dezena (sem Svelte): monta, a partir da entrada de
// tabela_geral.json, tudo o que o cartão mostra. O app NÃO recalcula o motor: índice,
// posição, componentes e evidência vêm prontos. `null` de componente é "sem dados na
// janela", nunca zero. Testada sem navegador.
import {
  formatarIndice,
  formatarVariacao,
  segmentosDoGrau,
  sentidoVariacao,
  type SentidoVariacao,
} from '../componentes/ranking';
import {
  CHAVES_COMPONENTES,
  type ChaveComponente,
  type DirecaoEvidencia,
  type GrauEvidencia,
  type TabelaGeralV1,
} from '../dados/tipos';
import {
  fraseEvidencia,
  rotuloEvidenciaDirecao,
  rotuloEvidenciaGrau,
  textos,
  textosComponentes,
} from '../lib/textos';

export interface ItemComponente {
  readonly chave: ChaveComponente;
  readonly rotulo: string;
  readonly explicacao: string;
  /** Valor bruto do contrato; `null` = sem dados. */
  readonly valor: number | null;
  /** Texto mostrado: valor com unidade ou "sem dados na janela". */
  readonly texto: string;
  readonly semDados: boolean;
  /** Por que falta (só quando `semDados`). */
  readonly motivo: string | null;
}

export interface EvidenciaDetalhe {
  readonly grau: GrauEvidencia;
  readonly direcao: DirecaoEvidencia;
  /** "Fraca · viés". */
  readonly rotulo: string;
  readonly frase: string;
  /** 1 a 5 segmentos acesos. */
  readonly segmentos: number;
}

export interface VariacaoDetalhe {
  readonly sentido: SentidoVariacao;
  /** "▲ +2", "▼ −1", "■ 0" ou "—". */
  readonly sinal: string;
  /** "subiu 2 posições", "caiu", "manteve a posição" ou "sem concurso anterior...". */
  readonly texto: string;
}

export interface DetalheDezena {
  readonly dezena: number;
  readonly posicao: number;
  /** "Dezena 20 · 1º lugar". */
  readonly titulo: string;
  /** Índice com 3 casas. */
  readonly indice: string;
  readonly variacao: VariacaoDetalhe;
  /** `null` = sem evidência calculada. */
  readonly evidencia: EvidenciaDetalhe | null;
  readonly janela: string;
  readonly componentes: readonly ItemComponente[];
}

const SETAS: Readonly<Record<SentidoVariacao, string>> = {
  subiu: '▲',
  caiu: '▼',
  igual: '■',
  sem: '',
};

export function descreverVariacao(variacao: number | null): VariacaoDetalhe {
  const sentido = sentidoVariacao(variacao);
  const sinal = `${SETAS[sentido]} ${formatarVariacao(variacao)}`.trim();
  const abs = Math.abs(variacao ?? 0);
  const texto =
    sentido === 'subiu'
      ? textos.ranking.subiu(abs)
      : sentido === 'caiu'
        ? textos.ranking.caiu(abs)
        : sentido === 'igual'
          ? textos.ranking.igual
          : textos.ranking.semVariacao;
  return { sentido, sinal, texto };
}

export function descreverEvidencia(
  grau: GrauEvidencia | null,
  direcao: DirecaoEvidencia | null,
): EvidenciaDetalhe | null {
  if (grau === null || direcao === null) return null;
  return {
    grau,
    direcao,
    rotulo: `${rotuloEvidenciaGrau(grau)} · ${rotuloEvidenciaDirecao(direcao)}`,
    frase: fraseEvidencia(grau, direcao),
    segmentos: segmentosDoGrau(grau),
  };
}

export function descreverComponente(chave: ChaveComponente, valor: number | null): ItemComponente {
  const t = textosComponentes[chave];
  const semDados = valor === null;
  return {
    chave,
    rotulo: t.rotulo,
    explicacao: t.explicacao,
    valor,
    // Espaço final nulo = nunca saiu (não depende da janela): dizer "sem dados na
    // janela" seria falso, porque o componente não é limitado pela janela.
    texto: semDados
      ? chave === 'espacoFinal'
        ? textos.detalhe.nuncaSaiu
        : textos.detalhe.semDados
      : t.formatar(valor),
    semDados,
    motivo: semDados ? (t.motivoSemDados ?? null) : null,
  };
}

/** Detalhe da dezena, ou `null` se a tabela não a tem. */
export function montarDetalheDaDezena(tabela: TabelaGeralV1, dezena: number): DetalheDezena | null {
  const d = tabela.dezenas.find((x) => x.dezena === dezena);
  if (d === undefined) return null;
  return {
    dezena: d.dezena,
    posicao: d.posicao,
    titulo: textos.detalhe.titulo(d.dezena, d.posicao),
    indice: formatarIndice(d.total),
    variacao: descreverVariacao(d.variacaoPosicao),
    evidencia: descreverEvidencia(d.evidenciaGrau, d.evidenciaDirecao),
    janela: textos.detalhe.janela(tabela.janela),
    componentes: CHAVES_COMPONENTES.map((chave) => descreverComponente(chave, d[chave])),
  };
}
