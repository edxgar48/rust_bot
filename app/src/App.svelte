<script lang="ts">
  import { onMount, untrack, type Component } from 'svelte';
  import Aviso from './componentes/Aviso.svelte';
  import BarraNavegacao from './componentes/BarraNavegacao.svelte';
  import SeletorTema from './componentes/SeletorTema.svelte';
  import type { AbaNavegacao, NomeIcone } from './componentes/tipos';
  import { hashDaRota, rotaDoHash } from './lib/rotas';
  import { idsAbas, textos, type IdAba } from './lib/textos';
  import { registrarServiceWorker } from './pwa';
  import { criarSessao } from './sessao.svelte';
  import Conferir from './telas/Conferir.svelte';
  import Entenda from './telas/Entenda.svelte';
  import Estatisticas from './telas/Estatisticas.svelte';
  import Forca from './telas/Forca.svelte';
  import Inicio from './telas/Inicio.svelte';

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

  let abaAtual = $state<IdAba>(rotaDoHash(location.hash));
  let atualizacaoPronta = $state(false);
  let catalogo = $state<Component | null>(null);
  const sessao = criarSessao();

  // Catálogo de componentes: só em desenvolvimento ou num build feito com
  // VITE_CATALOGO=1 (npm run build:catalogo). No build normal a condição é
  // constante falsa e o import dinâmico sai do bundle.
  function aoMudarHash(): void {
    abaAtual = rotaDoHash(location.hash);
    if (
      (import.meta.env.DEV || import.meta.env.VITE_CATALOGO === '1') &&
      location.hash === '#catalogo'
    ) {
      void import('./catalogo/Catalogo.svelte').then((m) => (catalogo = m.default));
    } else {
      catalogo = null;
    }
  }

  // Trocar de aba muda o hash (uma entrada no histórico: o botão voltar do
  // navegador funciona); quem atualiza a aba é o evento hashchange.
  function irPara(id: IdAba): void {
    if (id !== abaAtual) location.hash = hashDaRota(id);
  }

  // Voltar à Início refaz a conferência dos jogos salvos (cache local, sem rede): reflete
  // o que mudou em Meus jogos (marcado como visto, apagado, importado).
  $effect(() => {
    if (abaAtual === 'inicio') untrack(() => void sessao.recalcularNovidades());
  });

  onMount(() => {
    aoMudarHash();
    sessao.iniciar();
    registrarServiceWorker(() => (atualizacaoPronta = true));
  });
</script>

<!-- A volta da rede tenta sincronizar de novo (não faz nada se já está sincronizando). -->
<svelte:window onhashchange={aoMudarHash} ononline={() => void sessao.atualizar()} />

{#if catalogo}
  {@const Catalogo = catalogo}
  <Catalogo />
{:else}
  <div class="app">
    <header class="topo">
      <h1 class="marca">{textos.appNome}</h1>
      <SeletorTema />
    </header>

    <main id="conteudo" aria-labelledby="titulo-aba">
      {#if sessao.aviso}
        <div class="avisos">
          <Aviso tipo="erro">{textos.sincronizacao.armazenamentoVolatil}</Aviso>
        </div>
      {/if}
      {#if abaAtual === 'inicio'}
        <Inicio
          situacao={sessao.situacao}
          ocupada={sessao.ocupada}
          aoAtualizar={() => void sessao.atualizar()}
          novidades={sessao.novidades}
        />
      {:else if abaAtual === 'forca'}
        <Forca fonte={sessao} />
      {:else if abaAtual === 'conferir'}
        <Conferir fonte={sessao} jogos={sessao.jogos} />
      {:else if abaAtual === 'entenda'}
        <Entenda
          fonte={sessao}
          situacao={sessao.situacao}
          versaoApp={__VERSAO_APP__}
          {atualizacaoPronta}
        />
      {:else}
        <Estatisticas fonte={sessao} />
      {/if}
      {#if atualizacaoPronta}
        <div class="avisos avisos-fim">
          <Aviso>{textos.sincronizacao.atualizacaoDisponivel}</Aviso>
        </div>
      {/if}
    </main>

    <BarraNavegacao {abas} atual={abaAtual} rotulo={textos.navegacao} aoSelecionar={irPara} />
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
  .marca {
    font: var(--fonte-titulo-m);
  }
  .avisos {
    display: grid;
    gap: var(--esp-3);
    margin-bottom: var(--esp-3);
  }
  .avisos-fim {
    margin-top: var(--esp-3);
    margin-bottom: 0;
  }
  main {
    flex: 1;
    width: 100%;
    max-width: 48rem;
    margin: 0 auto;
    padding: var(--esp-4);
  }
  :global(.app > nav) {
    position: sticky;
    bottom: 0;
  }
</style>
