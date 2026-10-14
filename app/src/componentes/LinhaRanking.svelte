<script lang="ts">
  import type { DirecaoEvidencia, GrauEvidencia } from '../dados/tipos';
  import { rotuloEvidenciaDirecao, rotuloEvidenciaGrau, textos } from '../lib/textos';
  import BolaDezena from './BolaDezena.svelte';
  import { formatarIndice, formatarVariacao, segmentosDoGrau, sentidoVariacao } from './ranking';

  interface Props {
    posicao: number;
    numero: number;
    /** Posições ganhas (+) ou perdidas (−); `null` = sem concurso anterior. */
    variacao: number | null;
    indice: number;
    grau: GrauEvidencia | null;
    direcao: DirecaoEvidencia | null;
    /** Se informado, a linha vira botão e chama isto com a dezena; sem ele, não é interativa. */
    aoEscolher?: (numero: number) => void;
    /** `false` quando um cabeçalho de coluna já diz "Evidência" (o leitor de tela ainda ouve). */
    rotuloEvidenciaVisivel?: boolean;
  }

  let {
    posicao,
    numero,
    variacao,
    indice,
    grau,
    direcao,
    aoEscolher,
    rotuloEvidenciaVisivel = true,
  }: Props = $props();

  const sentido = $derived(sentidoVariacao(variacao));
  const setas = { subiu: '▲', caiu: '▼', igual: '■', sem: '' } as const;
  const falado = $derived(
    sentido === 'subiu'
      ? textos.ranking.subiu(variacao ?? 0)
      : sentido === 'caiu'
        ? textos.ranking.caiu(Math.abs(variacao ?? 0))
        : sentido === 'igual'
          ? textos.ranking.igual
          : textos.ranking.semVariacao,
  );
  const acesos = $derived(segmentosDoGrau(grau));
  const textoEvidencia = $derived(
    grau === null || direcao === null
      ? textos.ranking.semEvidencia
      : `${rotuloEvidenciaGrau(grau)} · ${rotuloEvidenciaDirecao(direcao)}`,
  );
</script>

{#snippet corpo()}
  <span class="posicao">
    <span aria-hidden="true">{posicao}º</span>
    <span class="so-leitor">{textos.ranking.posicao(posicao)}</span>
  </span>
  <BolaDezena {numero} />
  <span class="variacao" data-sentido={sentido}>
    <span aria-hidden="true">{setas[sentido]} {formatarVariacao(variacao)}</span>
    <span class="so-leitor">{falado}</span>
  </span>
  <span class="indice">
    <span class="so-leitor">{textos.ranking.indice}:</span>
    {formatarIndice(indice)}
  </span>
  <span class="evidencia">
    <span class="segmentos" aria-hidden="true">
      {#each [1, 2, 3, 4, 5] as i (i)}
        <span class="segmento" data-aceso={i <= acesos}></span>
      {/each}
    </span>
    {#if rotuloEvidenciaVisivel}
      <span>{textos.ranking.evidencia}: {textoEvidencia}</span>
    {:else}
      <span class="so-leitor">{textos.ranking.evidencia}:</span>
      <span>{textoEvidencia}</span>
    {/if}
  </span>
{/snippet}

<li>
  {#if aoEscolher}
    <button
      type="button"
      class="linha"
      data-dezena={numero}
      aria-haspopup="dialog"
      onclick={() => aoEscolher(numero)}
    >
      {@render corpo()}
    </button>
  {:else}
    <div class="linha">
      {@render corpo()}
    </div>
  {/if}
</li>

<style>
  li {
    border-bottom: 1px solid var(--cor-contorno-var);
    list-style: none;
  }
  .linha {
    display: grid;
    grid-template-columns: 2.5rem auto 1fr auto;
    align-items: center;
    column-gap: var(--esp-3);
    row-gap: var(--esp-1);
    width: 100%;
    padding: var(--esp-2) var(--esp-4);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
  }
  /* Interativa (botão): alvo de toque inteiro e foco por dentro da linha. */
  button.linha {
    min-height: var(--alvo-minimo);
    cursor: pointer;
  }
  button.linha:focus-visible {
    outline-offset: -3px;
  }
  .posicao {
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
  }
  .variacao {
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
    color: var(--cor-texto-var);
  }
  /* Subiu/caiu: seta, sinal e texto falado; a cor só reforça. */
  .variacao[data-sentido='subiu'] {
    color: var(--cor-primaria);
  }
  .variacao[data-sentido='caiu'] {
    color: var(--cor-erro);
  }
  .indice {
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .evidencia {
    grid-column: 2 / -1;
    display: flex;
    align-items: center;
    gap: var(--esp-2);
    font: var(--fonte-rotulo-m);
    color: var(--cor-texto-var);
  }
  .segmentos {
    display: inline-flex;
    gap: 2px;
    flex: none;
  }
  .segmento {
    width: 0.375rem;
    height: 0.875rem;
    border: 1px solid var(--cor-contorno);
    border-radius: 1px;
  }
  .segmento[data-aceso='true'] {
    background: var(--cor-texto-var);
    border-color: var(--cor-texto-var);
  }
</style>
