<script lang="ts">
  import { onMount } from 'svelte';
  import { formatarDataSorteio } from './dados/estado-sincronizacao';
  import { SLUGS } from './dados/tipos';
  import { idsAbas, textos, type IdAba } from './lib/textos';
  import { registrarServiceWorker } from './pwa';
  import { criarSessao } from './sessao.svelte';

  let abaAtual = $state<IdAba>('inicio');
  let atualizacaoPronta = $state(false);
  const sessao = criarSessao();

  onMount(() => {
    sessao.iniciar();
    registrarServiceWorker(() => (atualizacaoPronta = true));
  });
</script>

<header class="topo">
  <h1>{textos.appNome}</h1>
</header>

<main id="conteudo" aria-labelledby="titulo-aba">
  <h2 id="titulo-aba">{textos.abas[abaAtual]}</h2>
  {#if abaAtual === 'inicio'}
    <section aria-labelledby="titulo-dados">
      <h3 id="titulo-dados">{textos.sincronizacao.titulo}</h3>
      <p role="status" class="estado" data-tipo={sessao.situacao?.estado.tipo ?? 'sincronizando'}>
        {sessao.situacao?.estado.texto ?? textos.sincronizacao.sincronizando(null)}
      </p>
      {#if sessao.aviso}
        <p role="alert" class="aviso">{textos.sincronizacao.armazenamentoVolatil}</p>
      {/if}
      {#if atualizacaoPronta}
        <p class="aviso">{textos.sincronizacao.atualizacaoDisponivel}</p>
      {/if}
      {#if sessao.situacao?.cache}
        <ul>
          {#each SLUGS as slug (slug)}
            {@const entrada = sessao.situacao.cache.indice.loterias[slug]}
            {#if entrada}
              <li>
                {textos.sincronizacao.ultimoConcurso(
                  textos.loterias[slug],
                  entrada.ultimoConcurso,
                  formatarDataSorteio(entrada.data),
                )}
              </li>
            {/if}
          {/each}
        </ul>
      {/if}
    </section>
  {:else}
    <p>{textos.placeholder}</p>
  {/if}
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
  h3 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
  }
  .estado,
  .aviso {
    margin: 0 0 0.75rem;
    padding: 0.75rem;
    border: 1px solid var(--cor-contorno);
    border-radius: 0.5rem;
  }
  ul {
    padding-left: 1.25rem;
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
