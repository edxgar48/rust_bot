<script lang="ts" generics="T extends string">
  import Icone from './Icone.svelte';
  import type { AbaNavegacao } from './tipos';

  interface Props {
    abas: readonly AbaNavegacao<T>[];
    atual: T;
    rotulo: string;
    aoSelecionar: (id: T) => void;
  }

  let { abas, atual, rotulo, aoSelecionar }: Props = $props();
</script>

<nav class="barra" aria-label={rotulo}>
  {#each abas as aba (aba.id)}
    <button
      type="button"
      aria-current={atual === aba.id ? 'page' : undefined}
      onclick={() => aoSelecionar(aba.id)}
    >
      <span class="indicador"><Icone nome={aba.icone} /></span>
      <span class="rotulo">{aba.rotulo}</span>
    </button>
  {/each}
</nav>

<style>
  .barra {
    display: flex;
    background: var(--cor-cont);
    border-top: 1px solid var(--cor-contorno-var);
    padding-bottom: env(safe-area-inset-bottom);
  }
  button {
    flex: 1 1 0;
    min-width: 0;
    min-height: 4rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--esp-1);
    padding: var(--esp-2) 0;
    border: 0;
    background: transparent;
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
    cursor: pointer;
  }
  .indicador {
    display: grid;
    place-items: center;
    width: 100%;
    max-width: 4rem;
    height: 2rem;
    border-radius: var(--raio-cheio);
  }
  .rotulo {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Ativa: pílula atrás do ícone, texto em negrito e cor mais forte (e aria-current). */
  button[aria-current='page'] {
    color: var(--cor-texto);
    font-weight: 700;
  }
  button[aria-current='page'] .indicador {
    background: var(--cor-secundaria-cont);
    color: var(--cor-sobre-secundaria-cont);
  }
  button:focus-visible {
    outline-offset: -3px;
  }
</style>
