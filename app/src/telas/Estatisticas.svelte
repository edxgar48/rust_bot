<script lang="ts">
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    JANELA_ESTATISTICA_PADRAO,
    LOTERIA_PADRAO,
    hashDaSecaoEstatisticas,
    janelaDoHash,
    loteriaDoHash,
    rotaDoHash,
    secaoEstatisticasDoHash,
    type SecaoEstatisticas,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import EstatisticasEvidencia from './EstatisticasEvidencia.svelte';
  import EstatisticasFrequencia from './EstatisticasFrequencia.svelte';
  import EstatisticasRankingTaxa from './EstatisticasRankingTaxa.svelte';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  // A seção vive no hash (#/estatisticas?loteria=quina&secao=evidencia): link e botão voltar
  // funcionam. Sem `secao` (URLs antigas) vale Frequência e atraso.
  let secao = $state<SecaoEstatisticas>(secaoEstatisticasDoHash(location.hash));

  const t = textos.estatisticas;
  const opcoes: readonly OpcaoSegmentada<SecaoEstatisticas>[] = [
    { valor: 'frequencia', rotulo: t.secaoFrequencia },
    { valor: 'evidencia', rotulo: t.secaoEvidencia },
    { valor: 'ranking', rotulo: t.secaoRanking },
  ];

  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) === 'estatisticas')
      secao = secaoEstatisticasDoHash(location.hash);
  }

  function trocar(nova: SecaoEstatisticas): void {
    if (nova === secao) return;
    const slug = loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO;
    const janela = janelaDoHash(location.hash) ?? JANELA_ESTATISTICA_PADRAO;
    location.hash = hashDaSecaoEstatisticas(nova, slug, janela);
  }
</script>

<svelte:window onhashchange={aoMudarHash} />

<h2 id="titulo-aba">{t.titulo}</h2>
<div class="secoes">
  <Segmentado {opcoes} valor={secao} rotuloGrupo={t.rotuloSecao} aoMudar={trocar} />
</div>

{#if secao === 'evidencia'}
  <EstatisticasEvidencia {fonte} />
{:else if secao === 'ranking'}
  <EstatisticasRankingTaxa {fonte} />
{:else}
  <EstatisticasFrequencia {fonte} />
{/if}

<style>
  h2 {
    font: var(--fonte-titulo-g);
  }
  .secoes {
    max-width: 26rem;
    margin-top: var(--esp-3);
  }
</style>
