<script lang="ts">
  import { onMount, type Component } from 'svelte';
  import Aviso from './componentes/Aviso.svelte';
  import BarraNavegacao from './componentes/BarraNavegacao.svelte';
  import BolaDezena from './componentes/BolaDezena.svelte';
  import Cartao from './componentes/Cartao.svelte';
  import SeletorTema from './componentes/SeletorTema.svelte';
  import type { AbaNavegacao, NomeIcone } from './componentes/tipos';
  import { formatarDataSorteio } from './dados/estado-sincronizacao';
  import { SLUGS } from './dados/tipos';
  import { idsAbas, textos, type IdAba } from './lib/textos';
  import { registrarServiceWorker } from './pwa';
  import { criarSessao } from './sessao.svelte';

  const iconesDasAbas: Record<IdAba, NomeIcone> = {
    inicio: 'inicio',
    forca: 'forca',
    conferir: 'conferir',
    estatisticas: 'estatisticas',
    entenda: 'entenda',
  };
  const abas: AbaNavegacao<IdAba>[] = idsAbas.map((id) => ({
    id,
    rotulo: textos.abas[id],
    icone: iconesDasAbas[id],
  }));

  let abaAtual = $state<IdAba>('inicio');
  let atualizacaoPronta = $state(false);
  let catalogo = $state<Component | null>(null);
  const sessao = criarSessao();

  // Catálogo de componentes: só em desenvolvimento ou num build feito com
  // VITE_CATALOGO=1 (npm run build:catalogo). No build normal a condição é
  // constante falsa e o import dinâmico sai do bundle.
  function atualizarCatalogo(): void {
    if (
      (import.meta.env.DEV || import.meta.env.VITE_CATALOGO === '1') &&
      location.hash === '#catalogo'
    ) {
      void import('./catalogo/Catalogo.svelte').then((m) => (catalogo = m.default));
    } else {
      catalogo = null;
    }
  }

  onMount(() => {
    atualizarCatalogo();
    sessao.iniciar();
    registrarServiceWorker(() => (atualizacaoPronta = true));
  });

  const estadoComErro = $derived(
    sessao.situacao?.estado.tipo === 'recusado' || sessao.situacao?.estado.tipo === 'sem_dados',
  );
</script>

<svelte:window onhashchange={atualizarCatalogo} />

{#if catalogo}
  {@const Catalogo = catalogo}
  <Catalogo />
{:else}
  <div class="app">
    <header class="topo">
      <h1>{textos.appNome}</h1>
      <SeletorTema />
    </header>

    <main id="conteudo" aria-labelledby="titulo-aba">
      <h2 id="titulo-aba">{textos.abas[abaAtual]}</h2>
      {#if abaAtual === 'inicio'}
        <div class="pilha">
          <Aviso tipo={estadoComErro ? 'erro' : 'informativo'}>
            {sessao.situacao?.estado.texto ?? textos.sincronizacao.sincronizando(null)}
          </Aviso>
          {#if sessao.aviso}
            <Aviso tipo="erro">{textos.sincronizacao.armazenamentoVolatil}</Aviso>
          {/if}
          {#if atualizacaoPronta}
            <Aviso>{textos.sincronizacao.atualizacaoDisponivel}</Aviso>
          {/if}
          {#if sessao.situacao?.cache}
            {@const cache = sessao.situacao.cache}
            {#each SLUGS as slug (slug)}
              {@const entrada = cache.indice.loterias[slug]}
              {@const ultimo = cache.loterias[slug]?.dados.ultimo ?? null}
              {#if entrada}
                <Cartao variante="elevado" titulo={textos.loterias[slug]}>
                  <p class="detalhe">
                    {textos.sincronizacao.ultimoConcurso(
                      textos.inicio.ultimoResultado,
                      entrada.ultimoConcurso,
                      formatarDataSorteio(entrada.data),
                    )}
                  </p>
                  {#if ultimo}
                    <ul class="bolas" aria-label={textos.inicio.dezenasSorteadas}>
                      {#each ultimo.dezenas as dezena (dezena)}
                        <li><BolaDezena numero={dezena} tamanho="p" estado="sorteada" /></li>
                      {/each}
                    </ul>
                  {/if}
                </Cartao>
              {/if}
            {/each}
          {/if}
        </div>
      {:else}
        <p>{textos.placeholder}</p>
      {/if}
    </main>

    <BarraNavegacao
      {abas}
      atual={abaAtual}
      rotulo={textos.navegacao}
      aoSelecionar={(id) => (abaAtual = id)}
    />
  </div>
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    min-height: 100dvh;
  }
  .topo {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--esp-2);
    padding: var(--esp-2) var(--esp-2) var(--esp-2) var(--esp-4);
    background: var(--cor-cont);
  }
  h1 {
    font: var(--fonte-titulo-g);
  }
  main {
    flex: 1;
    width: 100%;
    max-width: 48rem;
    margin: 0 auto;
    padding: var(--esp-4);
  }
  h2 {
    margin-bottom: var(--esp-4);
    font: var(--fonte-titulo-g);
  }
  .pilha {
    display: grid;
    gap: var(--esp-3);
  }
  .detalhe {
    margin-bottom: var(--esp-3);
    color: var(--cor-texto-var);
  }
  .bolas {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  :global(.app > nav) {
    position: sticky;
    bottom: 0;
  }
</style>
