<script lang="ts">
  interface Props {
    rotulo: string;
    selecionado?: boolean;
    desabilitado?: boolean;
    aoAlternar?: () => void;
  }

  let { rotulo, selecionado = false, desabilitado = false, aoAlternar }: Props = $props();
</script>

<!-- Botão de alternância: o estado vai em aria-pressed e na marca ✓ (não só na cor). -->
<button
  type="button"
  class="chip"
  aria-pressed={selecionado}
  disabled={desabilitado}
  onclick={() => aoAlternar?.()}
>
  <span class="corpo">
    {#if selecionado}<span aria-hidden="true">✓</span>{/if}
    {rotulo}
  </span>
</button>

<style>
  /* O botão tem 44 px de altura (alvo de toque); o chip visível tem 32 px. */
  .chip {
    display: inline-flex;
    align-items: center;
    min-height: var(--alvo-minimo);
    padding: 0;
    border: 0;
    background: transparent;
    font: inherit;
    cursor: pointer;
  }
  .corpo {
    display: inline-flex;
    align-items: center;
    gap: var(--esp-2);
    height: 2rem;
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-p);
    background: transparent;
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-g);
  }
  .chip[aria-pressed='true'] .corpo {
    background: var(--cor-secundaria-cont);
    border-color: transparent;
    color: var(--cor-sobre-secundaria-cont);
  }
  .chip:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }
</style>
