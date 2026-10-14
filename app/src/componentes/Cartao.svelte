<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { VarianteCartao } from './tipos';

  interface Props {
    variante?: VarianteCartao;
    titulo?: string;
    /** Nível do título (h2, h3 ou h4) conforme a posição na página. */
    nivel?: 2 | 3 | 4;
    children: Snippet;
  }

  let { variante = 'preenchido', titulo, nivel = 3, children }: Props = $props();
  const idTitulo = $props.id();
</script>

<section class="cartao" data-variante={variante} aria-labelledby={titulo ? idTitulo : undefined}>
  {#if titulo}
    <svelte:element this={`h${nivel}`} id={idTitulo} class="titulo">{titulo}</svelte:element>
  {/if}
  {@render children()}
</section>

<style>
  .cartao {
    padding: var(--esp-4);
    border-radius: var(--raio-m);
    color: var(--cor-texto);
    border: 1px solid transparent;
  }
  .cartao[data-variante='elevado'] {
    background: var(--cor-cont-baixo);
    box-shadow: var(--elev-1);
  }
  .cartao[data-variante='preenchido'] {
    background: var(--cor-cont-altissimo);
  }
  .cartao[data-variante='contorno'] {
    background: var(--cor-superficie);
    border-color: var(--cor-contorno-var);
  }
  .titulo {
    margin: 0 0 var(--esp-3);
    font: var(--fonte-titulo-m);
  }
</style>
