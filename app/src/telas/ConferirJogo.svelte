<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Icone from '../componentes/Icone.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import Volante from '../componentes/Volante.svelte';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { SLUGS, type Slug } from '../dados/tipos';
  import type { RepositorioJogos } from '../jogos/repositorio';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDoJogo,
    loteriaDoHash,
    rotaDoHash,
    secaoConferirDoHash,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import {
    QUANTIDADE_JOGO,
    acertosNoUltimoConcurso,
    alternarDezenaDoJogo,
    avaliarQuantidade,
    conferirJogo,
    faixasDoResultado,
    montarVisaoConferir,
  } from './conferir';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import type { JogoInicial } from './jogo-inicial';
  import FaixaAcertos from './FaixaAcertos.svelte';
  import SalvarJogo from './SalvarJogo.svelte';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
    /** Onde os jogos salvos ficam (separado do cache de dados). */
    jogos: RepositorioJogos;
    /** Jogo salvo a carregar no volante ao abrir (vindo de Meus jogos). */
    jogoInicial?: JogoInicial | null;
    /** Chamado quando o jogo inicial já foi carregado (a tela pai o esquece). */
    aoUsarJogoInicial?: () => void;
    /** Leva para a lista Meus jogos. */
    aoVerJogos?: () => void;
  }

  let { fonte, jogos, jogoInicial = null, aoUsarJogoInicial, aoVerJogos }: Props = $props();

  /** Quantos concursos do melhor resultado a lista mostra. */
  const LIMITE_LISTA = 10;
  /** Quantos concursos cada faixa de acertos lista ao expandir. */
  const LIMITE_FAIXA = 20;

  // Precedência: URL (#/conferir?loteria=quina), depois a última escolha guardada, depois Lotofácil.
  // Um jogo vindo de Meus jogos manda na loteria e nas dezenas iniciais.
  const inicial = untrack(() => jogoInicial);
  let loteria = $state<Slug>(
    inicial?.loteria ?? loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  let avisoCarregado = $state<string | null>(
    inicial ? textos.conferir.salvar.carregado(inicial.nome) : null,
  );
  // raw: o histórico tem milhares de listas; um proxy reativo em cada uma só custaria tempo.
  let situacao = $state.raw<SituacaoTela | null>(null);
  let sequencia = 0;
  let jogo = $state<number[]>(inicial ? [...inicial.dezenas] : []);

  const opcoesLoteria: readonly OpcaoSegmentada<Slug>[] = SLUGS.map((slug) => ({
    valor: slug,
    rotulo: textos.loterias[slug],
  }));
  const iconesDoEstado: Record<EstadoTela, NomeIcone> = {
    sincronizando: 'atualizar',
    atualizado: 'conferir',
    sem_rede: 'sem-rede',
    recusado: 'erro',
  };

  const visao = $derived(montarVisaoConferir(loteria, situacao));
  const limites = $derived(QUANTIDADE_JOGO[loteria]);
  const quantidade = $derived(avaliarQuantidade(loteria, jogo.length));
  const noLimite = $derived(jogo.length >= limites.maximo);
  // O resultado só existe com quantidade válida e dados carregados.
  const resultado = $derived(
    visao.tipo === 'conteudo' && quantidade.tipo === 'valida'
      ? conferirJogo(loteria, jogo, visao.concursos)
      : null,
  );
  const faixas = $derived(resultado ? faixasDoResultado(resultado) : []);
  const historico = $derived(visao.tipo === 'conteudo' ? visao.concursos : []);
  const ultimo = $derived(resultado ? acertosNoUltimoConcurso(loteria, jogo, historico) : null);
  const melhor = $derived(resultado?.melhor ?? null);
  const concursosDoMelhor = $derived(melhor ? melhor.concursos.slice(0, LIMITE_LISTA) : []);
  const resumoLista = $derived(
    melhor
      ? textos.conferir.resultado.listaMelhorResumo(
          concursosDoMelhor.length,
          melhor.concursos.length,
        )
      : '',
  );

  const mensagemQuantidade = $derived.by(() => {
    if (quantidade.tipo === 'vazia') {
      return textos.conferir.quantidade.vazia(limites.minimo, limites.maximo);
    }
    if (quantidade.tipo === 'faltam') {
      return textos.conferir.quantidade.faltam(quantidade.faltam, limites.minimo);
    }
    if (quantidade.tipo === 'excede') {
      return textos.conferir.quantidade.excede(quantidade.sobram, limites.maximo);
    }
    return null;
  });

  /** Mostra o cache na hora e só depois sincroniza; respostas de uma loteria antiga são descartadas. */
  async function carregar(slug: Slug): Promise<void> {
    const minha = ++sequencia;
    const opcoes: OpcoesSincronizacao = { arquivos: ['historico'], loterias: [slug] };
    situacao = null;
    const cache = await fonte.lerCache(opcoes);
    if (minha !== sequencia) return;
    situacao = { cache, sincronizando: true, falha: 'nenhuma' };
    const garantia = await fonte.garantir(opcoes);
    if (minha !== sequencia) return;
    situacao = { cache: garantia.cache, sincronizando: false, falha: garantia.falha };
  }

  function trocarLoteria(slug: Slug): void {
    if (slug === loteria) return;
    loteria = slug;
    jogo = [];
    avisoCarregado = null;
    gravarLoteriaGuardada(slug);
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', hashDoJogo(slug));
    void carregar(slug);
  }

  function alternar(dezena: number): void {
    avisoCarregado = null;
    jogo = alternarDezenaDoJogo(loteria, jogo, dezena);
  }

  // Hash colado ou botão voltar com outro ?loteria=: segue a URL.
  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) !== 'conferir' || secaoConferirDoHash(location.hash) !== 'jogo') {
      return;
    }
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      jogo = [];
      avisoCarregado = null;
      void carregar(daUrl);
    }
  }

  onMount(() => {
    if (inicial) aoUsarJogoInicial?.();
    void carregar(loteria);
    return () => {
      sequencia++;
    };
  });
</script>

<svelte:window onhashchange={aoMudarHash} />

<h2 id="titulo-aba">{textos.conferir.titulo}</h2>
{#if visao.tipo === 'conteudo'}
  <p class="subtitulo">{visao.subtitulo}</p>
{/if}

<div class="pilha">
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={textos.conferir.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>

  <Aviso>{textos.avisos.naoPreveConferir} {textos.avisos.jogoResponsavel}</Aviso>

  {#if visao.tipo === 'carregando'}
    <div class="carregando" aria-busy="true">
      <p class="discreto" role="status">{textos.conferir.carregando}</p>
      {#each [0, 1, 2, 3] as i (i)}
        <span class="barra" aria-hidden="true"></span>
      {/each}
    </div>
  {:else if visao.tipo === 'sem_dados'}
    <Aviso tipo={visao.erro ? 'erro' : 'informativo'}>{visao.texto}</Aviso>
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void carregar(loteria)}>
        <Icone nome="atualizar" tamanho={1.125} />
        {textos.conferir.tentarDeNovo}
      </button>
    </div>
  {:else}
    {#if visao.erro}
      <Aviso tipo="erro">{visao.texto}</Aviso>
    {:else}
      <p class="estado" role="status" data-estado={visao.estado}>
        <Icone nome={iconesDoEstado[visao.estado]} tamanho={1.25} />
        <span>{visao.texto}</span>
      </p>
    {/if}

    <Cartao variante="contorno" titulo={textos.conferir.volanteTitulo}>
      <div class="volante">
        <Volante
          {loteria}
          modo="selecao"
          selecionadas={jogo}
          aoAlternar={alternar}
          bloquearNovas={noLimite}
        />
      </div>
      <div class="rodape-volante">
        <div class="contador" role="status">
          <p class="marcadas">{textos.conferir.marcadas(jogo.length)}</p>
          {#if mensagemQuantidade !== null}
            <p class="mensagem" data-tipo={quantidade.tipo}>{mensagemQuantidade}</p>
          {/if}
          {#if noLimite}
            <p class="mensagem">{textos.conferir.limite(limites.maximo)}</p>
          {/if}
        </div>
        <button
          type="button"
          class="botao"
          disabled={jogo.length === 0}
          onclick={() => {
            jogo = [];
            avisoCarregado = null;
          }}
        >
          {textos.conferir.limpar}
        </button>
      </div>
      {#if avisoCarregado !== null}
        <p class="mensagem carregado" role="status">{avisoCarregado}</p>
      {/if}
      <SalvarJogo
        {loteria}
        dezenas={jogo}
        valido={quantidade.tipo === 'valida'}
        {jogos}
        aoVerJogos={() => aoVerJogos?.()}
      />
    </Cartao>

    {#if resultado}
      <Cartao
        variante="elevado"
        titulo={textos.conferir.resultado.titulo(resultado.totalConcursos)}
      >
        {#if melhor}
          {@const maisRecente = melhor.concursos[0] as number}
          <p class="melhor">{textos.conferir.resultado.melhor(melhor.acertos, maisRecente)}</p>
          {#if ultimo}
            <p class="ultimo">
              {textos.conferir.resultado.ultimo(ultimo.concurso, ultimo.acertos)}
            </p>
          {/if}
          {#if melhor.concursos.length > 1}
            <p class="discreto">
              {textos.conferir.resultado.melhorRepetido(melhor.concursos.length)}
            </p>
            <details class="lista-melhor">
              <summary>{textos.conferir.resultado.listaMelhor}</summary>
              <ul>
                {#each concursosDoMelhor as concurso (concurso)}
                  <li>{textos.conferir.resultado.concursoItem(concurso)}</li>
                {/each}
              </ul>
              {#if resumoLista !== ''}
                <p class="discreto">{resumoLista}</p>
              {/if}
            </details>
          {/if}
          <ul class="faixas" aria-label={textos.conferir.resultado.faixas}>
            {#each faixas as faixa (faixa.acertos)}
              <li>
                <FaixaAcertos
                  {faixa}
                  melhor={faixa.acertos === melhor.acertos}
                  {loteria}
                  {jogo}
                  concursos={historico}
                  limite={LIMITE_FAIXA}
                />
              </li>
            {/each}
          </ul>
          <p class="discreto nota">{textos.conferir.resultado.nota}</p>
        {:else}
          <p>{textos.conferir.resultado.semConcursos}</p>
        {/if}
      </Cartao>
    {/if}
  {/if}
</div>

<style>
  h2 {
    font: var(--fonte-titulo-g);
  }
  .subtitulo {
    margin-top: var(--esp-1);
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .pilha {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--esp-3);
    margin-top: var(--esp-4);
  }
  .controle {
    max-width: 26rem;
  }
  .estado {
    display: flex;
    align-items: flex-start;
    gap: var(--esp-2);
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .estado :global(svg) {
    flex: none;
  }
  .estado span {
    min-width: 0;
    padding-top: 0.125rem;
    overflow-wrap: anywhere;
  }
  .discreto {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }

  /* Volante e contador */
  .volante {
    margin-top: var(--esp-3);
    max-width: 30rem;
  }
  .rodape-volante {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--esp-3);
    margin-top: var(--esp-3);
  }
  .contador {
    flex: 1 1 14rem;
    display: grid;
    gap: var(--esp-1);
    min-width: 0;
  }
  .marcadas {
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
  }
  .mensagem {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .carregado {
    margin-top: var(--esp-2);
  }
  .mensagem[data-tipo='faltam'],
  .mensagem[data-tipo='excede'] {
    color: var(--cor-texto);
    font-weight: 600;
  }

  /* Resultado */
  .melhor {
    margin-top: var(--esp-2);
    font: var(--fonte-titulo-m);
  }
  .lista-melhor {
    margin-top: var(--esp-2);
  }
  .lista-melhor summary {
    display: flex;
    align-items: center;
    min-height: var(--alvo-minimo);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .lista-melhor ul {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-1) var(--esp-3);
    margin: 0;
    padding: 0;
    list-style: none;
    font: var(--fonte-corpo-m);
    font-variant-numeric: tabular-nums;
  }
  .faixas {
    display: grid;
    gap: var(--esp-1);
    margin: var(--esp-4) 0 0;
    padding: 0;
    list-style: none;
  }
  .ultimo {
    margin-top: var(--esp-1);
    font: var(--fonte-corpo-m);
  }
  .nota {
    margin-top: var(--esp-3);
  }

  .acoes {
    display: flex;
  }
  .botao {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--esp-2);
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .botao:disabled {
    color: var(--cor-texto-var);
    cursor: not-allowed;
    opacity: 0.6;
  }
  .carregando {
    display: grid;
    gap: var(--esp-3);
  }
  .barra {
    display: block;
    height: 3rem;
    border-radius: var(--raio-m);
    background: var(--cor-cont-altissimo);
    animation: pulso 1.6s ease-in-out infinite;
  }
  @keyframes pulso {
    50% {
      opacity: 0.45;
    }
  }
</style>
