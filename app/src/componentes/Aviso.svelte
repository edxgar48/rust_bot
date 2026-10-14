<script lang="ts">
  import type { Snippet } from 'svelte';
  import { textos } from '../lib/textos';
  import Icone from './Icone.svelte';
  import type { TipoAviso } from './tipos';

  interface Props {
    tipo?: TipoAviso;
    children: Snippet;
  }

  let { tipo = 'informativo', children }: Props = $props();
</script>

<!-- Erro: role="alert" (anunciado na hora). Informativo: role="status" (educado). -->
<div class="aviso" data-tipo={tipo} role={tipo === 'erro' ? 'alert' : 'status'}>
  <Icone nome={tipo === 'erro' ? 'erro' : 'info'} />
  <p>
    <strong class="rotulo">{textos.aviso[tipo]}:</strong>
    {@render children()}
  </p>
</div>

<style>
  .aviso {
    display: flex;
    align-items: flex-start;
    gap: var(--esp-3);
    padding: var(--esp-3) var(--esp-4);
    border-radius: var(--raio-m);
    border: 1px solid transparent;
    font: var(--fonte-corpo-m);
  }
  .aviso :global(svg) {
    flex: none;
    margin-top: 0.0625rem;
  }
  .aviso[data-tipo='informativo'] {
    background: var(--cor-secundaria-cont);
    color: var(--cor-sobre-secundaria-cont);
  }
  /* Erro: rótulo "Erro:", ícone de triângulo e borda grossa à esquerda, além da cor. */
  .aviso[data-tipo='erro'] {
    background: var(--cor-erro-cont);
    color: var(--cor-sobre-erro-cont);
    border-color: var(--cor-erro);
    border-left-width: 0.375rem;
  }
  .rotulo {
    font-weight: 700;
  }
  p {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
