<script lang="ts">
  import type { DirecaoEvidencia, GrauEvidencia } from '../dados/tipos';
  import { rotuloEvidenciaDirecao, rotuloEvidenciaGrau, textos } from '../lib/textos';
  import { doisDigitos } from '../lib/numeros';
  import BolaDezena from './BolaDezena.svelte';
  import {
    SETAS_VARIACAO,
    formatarIndice,
    formatarVariacao,
    segmentosDoGrau,
    sentidoVariacao,
    textoFaladoDaVariacao,
  } from './ranking';

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
    /** Se informado, mostra o botão Inserir entre a variação e o índice (só com `aoEscolher`). */
    aoInserir?: (numero: number) => void;
    /** Dezena já inserida no jogo: o botão fica marcado e tocar de novo a remove. */
    inserida?: boolean;
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
    aoInserir,
    inserida = false,
  }: Props = $props();

  const comInserir = $derived(aoEscolher !== undefined && aoInserir !== undefined);
  const dezenaTexto = $derived(doisDigitos(numero));

  const sentido = $derived(sentidoVariacao(variacao));
  const setas = SETAS_VARIACAO;
  const falado = $derived(textoFaladoDaVariacao(variacao));
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
  {#if comInserir}
    <!-- Reserva a coluna do botão Inserir, que fica por cima (um botão não contém outro). -->
    <span class="vaga-inserir" aria-hidden="true"></span>
  {/if}
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

<li class:com-inserir={comInserir}>
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
    {#if aoInserir}
      <button
        type="button"
        class="inserir"
        data-inserir={numero}
        aria-pressed={inserida}
        onclick={() => aoInserir(numero)}
      >
        <span>{inserida ? `✓ ${textos.ranking.inserida}` : textos.ranking.inserir}</span>
        <span class="so-leitor">{textos.ranking.paraODezena(dezenaTexto)}</span>
      </button>
    {/if}
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
  /*
   * Com o botão Inserir: o <li> é a grade (posição, dezena, variação, Inserir, índice); a linha-botão
   * ocupa todas as colunas por subgrade e o Inserir fica por cima, na 4ª coluna (botão não
   * aninha botão). A coluna do Inserir tem largura fixa, igual à do cabeçalho da tabela.
   */
  li.com-inserir {
    display: grid;
    grid-template-columns: 2.5rem auto 1fr 5.5rem auto;
  }
  li.com-inserir .linha {
    grid-column: 1 / -1;
    grid-row: 1;
    grid-template-columns: subgrid;
  }
  /* Sem subgrade: li e linha-botão usam as mesmas 5 colunas fixas e o recuo passa para o li. */
  @supports not (grid-template-columns: subgrid) {
    li.com-inserir {
      grid-template-columns: 2.5rem 2.75rem minmax(0, 1fr) 5.5rem 4rem;
      column-gap: var(--esp-3);
      padding: 0 var(--esp-4);
    }
    li.com-inserir .linha {
      grid-template-columns: 2.5rem 2.75rem minmax(0, 1fr) 5.5rem 4rem;
      padding-inline: 0;
    }
  }
  .vaga-inserir {
    min-width: 0;
  }
  .inserir {
    position: relative;
    grid-column: 4;
    grid-row: 1;
    align-self: start;
    justify-self: stretch;
    z-index: 1;
    box-sizing: border-box;
    min-width: var(--alvo-minimo);
    min-height: var(--alvo-minimo);
    margin-top: var(--esp-1);
    padding: 0 var(--esp-1);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-m);
    white-space: nowrap;
    cursor: pointer;
  }
  /* Marcada: borda mais grossa, fundo e "✓" (a cor nunca é a única informação). */
  .inserir[aria-pressed='true'] {
    border-width: 2px;
    background: var(--cor-cont-altissimo);
    color: var(--cor-texto);
    font-weight: 600;
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
