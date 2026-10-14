<script lang="ts" generics="T extends string">
  import { tick } from 'svelte';
  import type { OpcaoSegmentada } from './tipos';
  import { proximoIndice } from './segmentado';

  interface Props {
    opcoes: readonly OpcaoSegmentada<T>[];
    valor: T;
    /** Nome do grupo para leitor de tela. */
    rotuloGrupo: string;
    aoMudar?: (valor: T) => void;
  }

  let { opcoes, valor = $bindable(), rotuloGrupo, aoMudar }: Props = $props();

  let botoes: (HTMLButtonElement | undefined)[] = [];
  const indiceAtual = $derived(
    Math.max(
      0,
      opcoes.findIndex((o) => o.valor === valor),
    ),
  );

  function escolher(novo: T): void {
    if (novo === valor) return;
    valor = novo;
    aoMudar?.(novo);
  }

  async function aoTeclar(evento: KeyboardEvent): Promise<void> {
    const destino = proximoIndice(indiceAtual, evento.key, opcoes.length);
    const opcao = destino === null ? undefined : opcoes[destino];
    if (destino === null || !opcao) return;
    evento.preventDefault();
    escolher(opcao.valor);
    await tick();
    botoes[destino]?.focus();
  }
</script>

<!-- Grupo de rádio: uma só parada de Tab (a opção marcada); setas movem e marcam. -->
<div class="segmentado" role="radiogroup" aria-label={rotuloGrupo}>
  {#each opcoes as opcao, i (opcao.valor)}
    {@const marcada = opcao.valor === valor}
    <button
      bind:this={botoes[i]}
      type="button"
      role="radio"
      aria-checked={marcada}
      tabindex={i === indiceAtual ? 0 : -1}
      onclick={() => escolher(opcao.valor)}
      onkeydown={aoTeclar}
    >
      {#if marcada}<span aria-hidden="true">✓</span>{/if}
      {opcao.rotulo}
    </button>
  {/each}
</div>

<style>
  .segmentado {
    display: flex;
    width: 100%;
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    overflow: hidden;
  }
  button {
    flex: 1 1 0;
    min-width: 0;
    min-height: var(--alvo-minimo);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--esp-2);
    padding: 0 var(--esp-2);
    border: 0;
    border-left: 1px solid var(--cor-contorno);
    background: transparent;
    color: var(--cor-texto);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  button:first-child {
    border-left: 0;
  }
  button[aria-checked='true'] {
    background: var(--cor-secundaria-cont);
    color: var(--cor-sobre-secundaria-cont);
  }
  /* Foco por dentro: o contêiner corta (overflow) um anel externo. */
  button:focus-visible {
    outline-offset: -3px;
  }
</style>
