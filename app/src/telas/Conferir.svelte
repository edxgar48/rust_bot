<script lang="ts">
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import type { RepositorioJogos } from '../jogos/repositorio';
  import type { JogoSalvo } from '../jogos/jogo';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDaSecaoConferir,
    hashDoJogo,
    hashDosJogos,
    loteriaDoHash,
    rotaDoHash,
    secaoConferirDoHash,
    type SecaoConferir,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import ConferirJogo from './ConferirJogo.svelte';
  import Historico from './Historico.svelte';
  import type { JogoInicial } from './jogo-inicial';
  import MeusJogos from './MeusJogos.svelte';

  interface Props {
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
    /** Jogos salvos no aparelho (banco separado do cache de dados). */
    jogos: RepositorioJogos;
  }

  let { fonte, jogos }: Props = $props();

  // A seção vive no hash (#/conferir?loteria=quina&secao=historico): link e botão voltar funcionam.
  let secao = $state<SecaoConferir>(secaoConferirDoHash(location.hash));

  const opcoes: readonly OpcaoSegmentada<SecaoConferir>[] = [
    { valor: 'jogo', rotulo: textos.conferir.secaoJogo },
    { valor: 'historico', rotulo: textos.conferir.secaoHistorico },
    { valor: 'jogos', rotulo: textos.conferir.secaoJogos },
  ];

  // Jogo salvo que o volante deve carregar na próxima abertura de Conferir jogo.
  let jogoInicial = $state.raw<JogoInicial | null>(null);

  function conferirSalvo(jogo: JogoSalvo): void {
    jogoInicial = { loteria: jogo.loteria, dezenas: jogo.dezenas, nome: jogo.nome };
    gravarLoteriaGuardada(jogo.loteria);
    location.hash = hashDoJogo(jogo.loteria);
  }

  function verMeusJogos(): void {
    const slug = loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO;
    location.hash = hashDosJogos(slug);
  }

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
{:else if secao === 'jogos'}
  <MeusJogos {jogos} {fonte} aoConferir={conferirSalvo} />
{:else}
  <ConferirJogo
    {fonte}
    {jogos}
    {jogoInicial}
    aoUsarJogoInicial={() => (jogoInicial = null)}
    aoVerJogos={verMeusJogos}
  />
{/if}

<style>
  .secoes {
    max-width: 32rem;
    margin-bottom: var(--esp-4);
  }
</style>
