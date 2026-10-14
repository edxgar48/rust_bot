<script lang="ts">
  import { idsAbas, textos, type IdAba } from './lib/textos';

  let abaAtual = $state<IdAba>('inicio');
</script>

<header class="topo">
  <h1>{textos.appNome}</h1>
</header>

<main id="conteudo" aria-labelledby="titulo-aba">
  <h2 id="titulo-aba">{textos.abas[abaAtual]}</h2>
  <p>{textos.placeholder}</p>
</main>

<nav class="abas" aria-label={textos.navegacao}>
  {#each idsAbas as id (id)}
    <button
      type="button"
      aria-current={abaAtual === id ? 'page' : undefined}
      onclick={() => (abaAtual = id)}
    >
      {textos.abas[id]}
    </button>
  {/each}
</nav>

<style>
  .topo {
    padding: 1rem;
    background: var(--cor-superficie-var);
  }
  h1 {
    margin: 0;
    font-size: 1.25rem;
  }
  main {
    padding: 1rem;
    padding-bottom: 5rem;
    max-width: 48rem;
    margin: 0 auto;
  }
  .abas {
    position: fixed;
    inset: auto 0 0 0;
    display: flex;
    background: var(--cor-superficie-var);
    border-top: 1px solid var(--cor-contorno);
  }
  .abas button {
    flex: 1;
    min-height: 3.5rem;
    padding: 0.5rem 0.25rem;
    border: 0;
    background: transparent;
    color: var(--cor-texto);
    font: inherit;
    font-size: 0.8125rem;
    cursor: pointer;
  }
  /* Aba ativa: negrito e sublinhado, não só cor. */
  .abas button[aria-current='page'] {
    color: var(--cor-primaria);
    font-weight: 700;
    text-decoration: underline;
    text-underline-offset: 0.35rem;
    text-decoration-thickness: 2px;
  }
  .abas button:focus-visible {
    outline: 3px solid var(--cor-primaria);
    outline-offset: -3px;
  }
</style>
