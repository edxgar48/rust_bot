<script lang="ts">
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDaSecaoConferir,
    loteriaDoHash,
    rotaDoHash,
    secaoConferirDoHash,
    type SecaoConferir,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import ConferirJogo from './ConferirJogo.svelte';
  import Historico from './Historico.svelte';

  interface Props {
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  // A seção vive no hash (#/conferir?loteria=quina&secao=historico): link e botão voltar funcionam.
  let secao = $state<SecaoConferir>(secaoConferirDoHash(location.hash));

  const opcoes: readonly OpcaoSegmentada<SecaoConferir>[] = [
    { valor: 'jogo', rotulo: textos.conferir.secaoJogo },
    { valor: 'historico', rotulo: textos.conferir.secaoHistorico },
  ];

  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) === 'conferir') secao = secaoConferirDoHash(location.hash);
  }

  function trocar(nova: SecaoConferir): void {
    const slug = loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO;
    location.hash = hashDaSecaoConferir(nova, slug);
  }
</script>

<svelte:window onhashchange={aoMudarHash} />

<div class="secoes">
  <Segmentado {opcoes} valor={secao} rotuloGrupo={textos.conferir.rotuloSecao} aoMudar={trocar} />
</div>

{#if secao === 'historico'}
  <Historico {fonte} />
{:else}
  <ConferirJogo {fonte} />
{/if}

<style>
  .secoes {
    max-width: 26rem;
    margin-bottom: var(--esp-4);
  }
</style>
