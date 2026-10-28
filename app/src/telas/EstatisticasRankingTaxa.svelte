<script lang="ts">
  import { onMount } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import BolaDezena from '../componentes/BolaDezena.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Icone from '../componentes/Icone.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { SLUGS, type Slug } from '../dados/tipos';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDoRankingTaxa,
    janelaDoHash,
    loteriaDoHash,
    rotaDoHash,
    secaoEstatisticasDoHash,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import { montarVisaoRankingTaxa } from './ranking-taxa';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  const t = textos.rankingTaxa;
  const c = t.colunas;

  // Precedência: URL (&loteria=quina), depois a última escolha guardada, depois Lotofácil.
  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  let situacao = $state<SituacaoTela | null>(null);
  let sequencia = 0;

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

  const visao = $derived(montarVisaoRankingTaxa(loteria, situacao));

  /**
   * Só aqui o ranking é pedido (a abertura do app baixa apenas index + ultimo). Mostra o cache
   * na hora e só depois sincroniza; respostas de uma loteria antiga são descartadas.
   */
  async function carregar(slug: Slug): Promise<void> {
    const minha = ++sequencia;
    const opcoes: OpcoesSincronizacao = { arquivos: ['ranking_taxa'], loterias: [slug] };
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
    gravarLoteriaGuardada(slug);
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', hashDoRankingTaxa(slug, janelaDoHash(location.hash)));
    void carregar(slug);
  }

  // Hash colado ou botão voltar com outro &loteria=: segue a URL (só nesta seção).
  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) !== 'estatisticas') return;
    if (secaoEstatisticasDoHash(location.hash) !== 'ranking') return;
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      void carregar(daUrl);
    }
  }

  onMount(() => {
    void carregar(loteria);
    return () => {
      sequencia++;
    };
  });
</script>

<svelte:window onhashchange={aoMudarHash} />

<!-- Região viva SEMPRE montada: só o texto muda ao trocar de loteria. -->
<p class="so-leitor" role="status" data-anuncio>
  {visao.tipo === 'conteudo' || visao.tipo === 'nao_publicado' ? t.anuncio(visao.nomeLoteria) : ''}
</p>
{#if visao.tipo === 'conteudo'}
  <p class="subtitulo">{visao.subtitulo}</p>
  <p class="meta" data-perfil-janela>{visao.perfilJanela}</p>
{/if}

<div class="pilha">
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={textos.estatisticas.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>

  {#if visao.tipo === 'carregando'}
    <div class="carregando" aria-busy="true">
      <p class="discreto">{t.carregando}</p>
      {#each [0, 1, 2] as i (i)}
        <span class="esqueleto" aria-hidden="true"></span>
      {/each}
    </div>
  {:else if visao.tipo === 'sem_dados'}
    <Aviso tipo={visao.erro ? 'erro' : 'informativo'}>{visao.texto}</Aviso>
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void carregar(loteria)}>
        <Icone nome="atualizar" tamanho={1.125} />
        {t.tentarDeNovo}
      </button>
    </div>
  {:else if visao.tipo === 'nao_publicado'}
    <div data-nao-publicado>
      <Aviso>{visao.texto}</Aviso>
    </div>
  {:else}
    {#if visao.erro}
      <Aviso tipo="erro">{visao.texto}</Aviso>
    {:else}
      <p class="estado" data-estado={visao.estado}>
        <Icone nome={iconesDoEstado[visao.estado]} tamanho={1.25} />
        <span>{visao.texto}</span>
      </p>
    {/if}

    {#if visao.linhas.length === 0}
      <Aviso>{t.semDezenas}</Aviso>
    {:else}
      <Cartao variante="contorno" titulo={textos.estatisticas.secaoRanking}>
        <p class="nota nota-inicial">{t.intro}</p>
        <p class="nota" data-taxa-acaso>{t.taxaAcaso(visao.taxaAcaso)}</p>
        <p class="nota" data-aviso-tabela>{t.ordemAviso}</p>
      </Cartao>

      <!-- Papéis explícitos: no celular as linhas viram blocos em grade e, sem eles, o leitor
           de tela perderia a semântica de tabela. -->
      <div
        class="tabela"
        role="table"
        aria-label={t.tabela(visao.nomeLoteria, visao.total)}
        data-ranking
      >
        <div class="cabeca" role="row">
          <span role="columnheader">{c.dezena}</span>
          <span role="columnheader">{c.posicaoA}</span>
          <span role="columnheader">{c.notaA}</span>
          <span role="columnheader">{c.diferenca}</span>
          <span role="columnheader">{c.indice}</span>
          <span role="columnheader">{c.posicaoB}</span>
          <span role="columnheader" class="experimental">{c.notaB}</span>
        </div>
        {#each visao.linhas as linha (linha.dezena)}
          <div class="linha" role="row" data-dezena={linha.dezena}>
            <span role="cell" class="c-dezena"><BolaDezena numero={linha.dezena} /></span>
            <span role="cell" class="c-posicao-a"
              ><span class="rotulo" aria-hidden="true">{c.posicaoA}</span>{linha.posicaoA}º</span
            >
            <span role="cell" class="c-nota-a"
              ><span class="rotulo" aria-hidden="true">{c.notaA}</span>{linha.notaA}</span
            >
            <span role="cell" class="c-diferenca"
              ><span class="rotulo" aria-hidden="true">{c.diferenca}</span>{linha.diferenca}</span
            >
            <span role="cell" class="c-indice"
              ><span class="rotulo" aria-hidden="true">{c.indice}</span>{linha.posicaoIndice}º</span
            >
            <span role="cell" class="c-posicao-b"
              ><span class="rotulo" aria-hidden="true">{c.posicaoB}</span>{linha.posicaoB}º</span
            >
            <span role="cell" class="c-nota-b experimental"
              ><span class="rotulo" aria-hidden="true">{c.notaB}</span>{linha.notaB}</span
            >
          </div>
        {/each}
      </div>

      <Cartao variante="contorno" titulo={t.legenda.titulo}>
        <ul class="legenda" data-legenda>
          <li>{t.legenda.posicaoA}</li>
          <li>{t.legenda.notaA}</li>
          <li>{t.legenda.diferenca}</li>
          <li>{t.legenda.indice}</li>
          <li>{t.legenda.posicaoB}</li>
          <li>{t.legenda.notaB}</li>
          <li>{t.legenda.formato}</li>
        </ul>
      </Cartao>
    {/if}

    <Aviso>{textos.avisos.jogoResponsavel}</Aviso>
  {/if}

  <Aviso>{textos.avisos.naoPreveRanking}</Aviso>
</div>

<style>
  .subtitulo {
    margin-top: var(--esp-3);
    font: var(--fonte-corpo-m);
  }
  .meta {
    margin-top: var(--esp-1);
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
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
  .nota {
    margin: var(--esp-3) 0 0;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .nota-inicial {
    margin-top: 0;
  }

  /* Tabela: no celular cada dezena é um bloco (bola + 6 valores rotulados em 3 colunas); a partir de
     40rem vira tabela de 7 colunas com cabeçalho. Nunca rola na horizontal. */
  .tabela {
    display: grid;
    gap: var(--esp-2);
    min-width: 0;
  }
  .cabeca {
    /* Só leitor de tela no celular: cada célula já traz o próprio rótulo. */
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .linha {
    display: grid;
    grid-template-columns: 2.5rem repeat(3, minmax(0, 1fr));
    grid-template-areas:
      'dez pa na df'
      'dez ix pb nb';
    gap: var(--esp-2) var(--esp-2);
    align-items: start;
    padding: var(--esp-2) var(--esp-3);
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-m);
  }
  .c-dezena {
    grid-area: dez;
    align-self: center;
  }
  .c-posicao-a {
    grid-area: pa;
  }
  .c-nota-a {
    grid-area: na;
  }
  .c-diferenca {
    grid-area: df;
  }
  .c-indice {
    grid-area: ix;
  }
  .c-posicao-b {
    grid-area: pb;
  }
  .c-nota-b {
    grid-area: nb;
  }
  [role='cell'] {
    min-width: 0;
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
    overflow-wrap: anywhere;
  }
  /* Rótulo da célula (celular): texto pequeno acima do valor. */
  .rotulo {
    display: block;
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  /* Nota B: "experimental" vem sempre em texto; o itálico reforça sem depender de cor. */
  .experimental {
    font-style: italic;
  }

  @media (min-width: 40rem) {
    .tabela {
      gap: 0;
      border: 1px solid var(--cor-contorno-var);
      border-radius: var(--raio-m);
      overflow: hidden;
    }
    .cabeca,
    .linha {
      position: static;
      width: auto;
      height: auto;
      margin: 0;
      clip-path: none;
      overflow: visible;
      white-space: normal;
      display: grid;
      grid-template-columns: 3.5rem repeat(6, minmax(0, 1fr));
      grid-template-areas: none;
      gap: var(--esp-3);
      align-items: center;
      padding: var(--esp-2) var(--esp-4);
      border: 0;
      border-radius: 0;
    }
    .cabeca {
      color: var(--cor-texto-var);
      font: var(--fonte-rotulo-m);
      border-bottom: 1px solid var(--cor-contorno);
    }
    .linha + .linha,
    .cabeca + .linha {
      border-top: 1px solid var(--cor-contorno-var);
    }
    .c-dezena,
    .c-posicao-a,
    .c-nota-a,
    .c-diferenca,
    .c-indice,
    .c-posicao-b,
    .c-nota-b {
      grid-area: auto;
    }
    .rotulo {
      display: none;
    }
    [role='cell'] {
      font: var(--fonte-corpo-m);
    }
  }

  .legenda {
    display: grid;
    gap: var(--esp-2);
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
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
  .carregando {
    display: grid;
    gap: var(--esp-3);
  }
  .esqueleto {
    display: block;
    height: 6rem;
    border-radius: var(--raio-m);
    background: var(--cor-cont-altissimo);
    animation: pulso 1.6s ease-in-out infinite;
  }
  @keyframes pulso {
    50% {
      opacity: 0.45;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .esqueleto {
      animation: none;
    }
  }
</style>
