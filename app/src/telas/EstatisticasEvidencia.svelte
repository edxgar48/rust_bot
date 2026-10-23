<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import BolaDezena from '../componentes/BolaDezena.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import FolhaModal from '../componentes/FolhaModal.svelte';
  import Icone from '../componentes/Icone.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import {
    SETAS_VARIACAO,
    formatarVariacao,
    sentidoVariacao,
    textoFaladoDaVariacao,
  } from '../componentes/ranking';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { SLUGS, type Slug } from '../dados/tipos';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    dezenaDoHash,
    hashDaEvidencia,
    janelaDoHash,
    loteriaDoHash,
    rotaDoHash,
    secaoEstatisticasDoHash,
  } from '../lib/rotas';
  import { fraseEvidencia, textos } from '../lib/textos';
  import DetalheDezena from './DetalheDezena.svelte';
  import { montarDetalheDaDezena } from './detalhe-dezena';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import { montarVisaoEvidencia } from './evidencia';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  const t = textos.evidencia;

  // Precedência: URL (&loteria=quina), depois a última escolha guardada, depois Lotofácil.
  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  // Dezena com o detalhe aberto (&dezena=20 na URL, para o botão voltar fechar).
  let dezenaAberta = $state<number | null>(dezenaDoHash(location.hash));
  let raiz: HTMLElement | undefined = $state();
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

  const visao = $derived(montarVisaoEvidencia(loteria, situacao));
  const detalhe = $derived(
    visao.tipo === 'conteudo' && dezenaAberta !== null
      ? montarDetalheDaDezena(visao.tabela, dezenaAberta)
      : null,
  );

  // A janela da URL (escolhida em Frequência e atraso) é ignorada aqui, mas mantida.
  const urlAtual = (dezena: number | null = null): string =>
    hashDaEvidencia(loteria, dezena, janelaDoHash(location.hash));

  /** Mostra o cache na hora e só depois sincroniza; respostas de uma loteria antiga são descartadas. */
  async function carregar(slug: Slug): Promise<void> {
    const minha = ++sequencia;
    const opcoes: OpcoesSincronizacao = { arquivos: ['tabela_geral'], loterias: [slug] };
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
    dezenaAberta = null;
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', urlAtual());
    void carregar(slug);
  }

  // Abrir o detalhe cria uma entrada no histórico: o botão voltar fecha.
  function abrirDetalhe(dezena: number): void {
    dezenaAberta = dezena;
    history.pushState({ detalheEvidencia: true }, '', urlAtual(dezena));
  }

  function fecharDetalhe(): void {
    if (history.state?.detalheEvidencia) {
      // Entrada criada por abrirDetalhe: voltar a desfaz (o hashchange atualiza a tela).
      history.back();
    } else {
      // Aberto direto pela URL: só tira o parâmetro.
      history.replaceState(history.state, '', urlAtual());
    }
    dezenaAberta = null;
  }

  // Dezena da URL que a tabela não tem: ignora e limpa o parâmetro.
  $effect(() => {
    if (visao.tipo === 'conteudo' && dezenaAberta !== null && detalhe === null) {
      dezenaAberta = null;
      history.replaceState(history.state, '', urlAtual());
    }
  });

  // Ao fechar o detalhe, o foco volta à dezena de origem.
  let dezenaAnterior: number | null = null;
  $effect(() => {
    const atual = dezenaAberta;
    const fechada = dezenaAnterior;
    dezenaAnterior = atual;
    if (atual === null && fechada !== null) {
      void tick().then(() => {
        raiz?.querySelector<HTMLElement>(`button[data-dezena="${fechada}"]`)?.focus();
      });
    }
  });

  // Hash colado ou botão voltar com outro &loteria=: segue a URL (só nesta seção).
  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) !== 'estatisticas') return;
    if (secaoEstatisticasDoHash(location.hash) !== 'evidencia') return;
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      void carregar(daUrl);
    }
    dezenaAberta = dezenaDoHash(location.hash);
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
  {visao.tipo === 'conteudo' ? t.anuncio(visao.nomeLoteria) : ''}
</p>
{#if visao.tipo === 'conteudo'}
  <p class="subtitulo">{visao.subtitulo}</p>
  <p class="meta" data-perfil-janela>{visao.perfilJanela}</p>
{/if}

<div class="pilha" bind:this={raiz}>
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
  {:else}
    {#if visao.erro}
      <Aviso tipo="erro">{visao.texto}</Aviso>
    {:else}
      <p class="estado" data-estado={visao.estado}>
        <Icone nome={iconesDoEstado[visao.estado]} tamanho={1.25} />
        <span>{visao.texto}</span>
      </p>
    {/if}

    {#if visao.grupos.length === 0}
      <Aviso>{t.semDezenas}</Aviso>
    {:else}
      <Cartao variante="contorno" titulo={t.resumo.titulo}>
        <p class="nota nota-inicial">{t.intro}</p>
        <ul class="resumo" aria-label={t.resumo.lista(visao.total)} data-resumo>
          {#each visao.grupos as grupo (grupo.chave)}
            <li data-grupo-resumo={grupo.chave}>
              <span class="resumo-n">{grupo.dezenas.length}</span>
              <span>{grupo.rotulo}</span>
            </li>
          {/each}
        </ul>
        <p class="nota" data-fracos>{t.resumo.fracos(visao.fracos, visao.total)}</p>
        <p class="nota" data-explicacao>{t.explicacao}</p>
      </Cartao>

      <p class="nota ordem" data-ordem>{t.ordem}</p>

      {#each visao.grupos as grupo (grupo.chave)}
        <div data-grupo={grupo.chave}>
          <Cartao variante="contorno" titulo={grupo.rotulo}>
            <p class="nota nota-inicial" data-nota-grupo>
              {grupo.grau !== null && grupo.direcao !== null
                ? fraseEvidencia(grupo.grau, grupo.direcao)
                : t.grupo.semEvidenciaNota}
            </p>
            <p class="contagem">{t.grupo.contagem(grupo.dezenas.length)}</p>
            <ul class="dezenas" aria-label={t.grupo.lista(grupo.rotulo, grupo.dezenas.length)}>
              {#each grupo.dezenas as linha (linha.dezena)}
                {@const sentido = sentidoVariacao(linha.variacao)}
                <li>
                  <button
                    type="button"
                    class="item"
                    data-dezena={linha.dezena}
                    aria-haspopup="dialog"
                    aria-label={`${textos.bola.dezena(linha.dezena)}, ${textos.ranking.posicao(linha.posicao)}, ${textoFaladoDaVariacao(linha.variacao)}`}
                    onclick={() => abrirDetalhe(linha.dezena)}
                  >
                    <BolaDezena numero={linha.dezena} />
                    <span class="posicao" aria-hidden="true">{linha.posicao}º</span>
                    <span class="variacao" data-sentido={sentido} aria-hidden="true">
                      {SETAS_VARIACAO[sentido]}
                      {formatarVariacao(linha.variacao)}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          </Cartao>
        </div>
      {/each}
    {/if}

    <Aviso>{textos.avisos.jogoResponsavel}</Aviso>
  {/if}

  <Aviso>{textos.avisos.naoPreveEvidencia}</Aviso>
</div>

{#if detalhe}
  <FolhaModal titulo={detalhe.titulo} aoFechar={fecharDetalhe}>
    <DetalheDezena {detalhe} />
  </FolhaModal>
{/if}

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
  .ordem {
    margin: 0;
  }

  /* Resumo: uma linha de "chips" com a contagem em texto (sem depender de cor). */
  .resumo {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-2);
    margin: var(--esp-3) 0 0;
    padding: 0;
    list-style: none;
  }
  .resumo li {
    display: inline-flex;
    align-items: center;
    gap: var(--esp-2);
    min-width: 0;
    padding: 0.25rem var(--esp-3) 0.25rem 0.25rem;
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-p);
    font: var(--fonte-rotulo-g);
  }
  .resumo-n {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.75rem;
    height: 1.75rem;
    padding: 0 0.375rem;
    border-radius: var(--raio-p);
    background: var(--cor-secundaria-cont);
    color: var(--cor-sobre-secundaria-cont);
    font-variant-numeric: tabular-nums;
  }

  .contagem {
    margin: var(--esp-3) 0;
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-g);
  }
  .dezenas {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(4.25rem, 1fr));
    gap: var(--esp-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .item {
    display: grid;
    justify-items: center;
    gap: 0.125rem;
    width: 100%;
    min-height: var(--alvo-minimo);
    padding: var(--esp-2) var(--esp-1);
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-m);
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .posicao {
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
  }
  .variacao {
    font: var(--fonte-rotulo-m);
    font-variant-numeric: tabular-nums;
    color: var(--cor-texto-var);
    white-space: nowrap;
  }
  /* Subiu/caiu: seta, sinal e texto falado; a cor só reforça. */
  .variacao[data-sentido='subiu'] {
    color: var(--cor-primaria);
  }
  .variacao[data-sentido='caiu'] {
    color: var(--cor-erro);
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
