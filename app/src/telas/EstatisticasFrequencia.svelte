<script lang="ts">
  import { onMount } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Icone from '../componentes/Icone.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { PARAMETROS_LOTERIA, SLUGS, type ConcursoHistorico, type Slug } from '../dados/tipos';
  import {
    JANELAS_ESTATISTICA,
    calcularEstatisticas,
    type JanelaEstatistica,
  } from '../estatisticas/estatisticas';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import { doisDigitos, formatarDecimal, formatarInteiro } from '../lib/numeros';
  import {
    JANELA_ESTATISTICA_PADRAO,
    LOTERIA_PADRAO,
    hashDasEstatisticas,
    janelaDoHash,
    loteriaDoHash,
    rotaDoHash,
    secaoEstatisticasDoHash,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import {
    anuncioDasEstatisticas,
    montarBlocoComparacao,
    montarBlocoSoma,
    montarCartaoAtraso,
    montarVisaoEstatisticas,
    subtituloDasEstatisticas,
  } from './estatisticas';
  import ComparacaoDistribuicao from './ComparacaoDistribuicao.svelte';
  import ComparacaoSoma from './ComparacaoSoma.svelte';
  import GraficoFrequencia from './GraficoFrequencia.svelte';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  // Precedência: URL (#/estatisticas?loteria=quina&janela=50), depois a última escolha
  // guardada (loteria), depois o padrão (Lotofácil; janela de 50 concursos).
  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  let janela = $state<JanelaEstatistica>(janelaDoHash(location.hash) ?? JANELA_ESTATISTICA_PADRAO);
  // raw: o histórico tem milhares de listas; um proxy reativo em cada uma só custaria tempo.
  let situacao = $state.raw<SituacaoTela | null>(null);
  let sequencia = 0;

  const t = textos.estatisticas;

  const opcoesLoteria: readonly OpcaoSegmentada<Slug>[] = SLUGS.map((slug) => ({
    valor: slug,
    rotulo: textos.loterias[slug],
  }));
  const opcoesJanela: readonly OpcaoSegmentada<`${JanelaEstatistica}`>[] = JANELAS_ESTATISTICA.map(
    (j) => ({ valor: `${j}` as const, rotulo: t.janelas[`${j}` as const] }),
  );
  const iconesDoEstado: Record<EstadoTela, NomeIcone> = {
    sincronizando: 'atualizar',
    atualizado: 'conferir',
    sem_rede: 'sem-rede',
    recusado: 'erro',
  };

  const visao = $derived(montarVisaoEstatisticas(loteria, situacao));
  // A referência da lista só muda quando chegam dados novos: mudar o estado da
  // sincronização (atualizado, sem rede) não refaz a conta.
  const concursos = $derived<readonly ConcursoHistorico[] | null>(
    visao.tipo === 'conteudo' ? visao.concursos : null,
  );
  const stats = $derived(concursos ? calcularEstatisticas(loteria, concursos, janela) : null);
  const atraso = $derived(stats && stats.tamanho > 0 ? montarCartaoAtraso(stats) : null);
  const blocoPares = $derived(
    stats && stats.tamanho > 0 ? montarBlocoComparacao('pares', stats) : null,
  );
  const blocoRepetidas = $derived(
    stats && stats.tamanho > 1 ? montarBlocoComparacao('repetidas', stats) : null,
  );
  const blocoSoma = $derived(stats && stats.tamanho > 0 ? montarBlocoSoma(stats) : null);
  const k = $derived(PARAMETROS_LOTERIA[loteria].dezenasSorteadas);

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
    gravarLoteriaGuardada(slug);
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', hashDasEstatisticas(slug, janela));
    void carregar(slug);
  }

  function trocarJanela(valor: `${JanelaEstatistica}`): void {
    const nova = JANELAS_ESTATISTICA.find((j) => `${j}` === valor);
    if (nova === undefined || nova === janela) return;
    janela = nova;
    history.replaceState(history.state, '', hashDasEstatisticas(loteria, nova));
  }

  // Hash colado ou botão voltar com outro ?loteria= ou ?janela=: segue a URL.
  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) !== 'estatisticas') return;
    // Outra seção assume a URL: esta (que vai sair da tela) não reage.
    if (secaoEstatisticasDoHash(location.hash) !== 'frequencia') return;
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      void carregar(daUrl);
    }
    const janelaDaUrl = janelaDoHash(location.hash);
    if (janelaDaUrl !== null) janela = janelaDaUrl;
  }

  onMount(() => {
    void carregar(loteria);
    return () => {
      sequencia++;
    };
  });
</script>

<svelte:window onhashchange={aoMudarHash} />

<!-- Região viva SEMPRE montada (fora de qualquer {#if}): só o texto muda ao trocar loteria ou
     janela, e é isso que o leitor de tela anuncia. Vazia enquanto não há estatísticas. -->
<p class="so-leitor" role="status" data-anuncio>
  {stats && stats.tamanho > 0 ? anuncioDasEstatisticas(stats) : ''}
</p>
{#if stats && stats.tamanho > 0}
  <p class="subtitulo">{subtituloDasEstatisticas(stats)}</p>
{/if}

<div class="pilha">
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={t.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>
  <div class="controle">
    <Segmentado
      opcoes={opcoesJanela}
      valor={`${janela}` as const}
      rotuloGrupo={t.rotuloJanela}
      aoMudar={trocarJanela}
    />
  </div>

  <Aviso>{textos.avisos.naoPreveEstatisticas}</Aviso>

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

    {#if stats && stats.tamanho > 0}
      <Cartao variante="contorno" titulo={t.frequencia.titulo}>
        <GraficoFrequencia estatisticas={stats} />
      </Cartao>

      <div class="dupla">
        <Cartao variante="contorno" titulo={t.pares.titulo}>
          <dl class="medidas">
            <div>
              <dt>{t.pares.media}</dt>
              <dd data-medida="pares-media">{formatarDecimal(stats.mediaPares)}</dd>
            </div>
            <div>
              <dt>{t.pares.esperado}</dt>
              <dd data-medida="pares-esperado">{formatarDecimal(stats.paresEsperado)}</dd>
            </div>
          </dl>
          <p class="nota">{t.pares.nota(k)}</p>
          {#if blocoPares}<ComparacaoDistribuicao bloco={blocoPares} />{/if}
        </Cartao>

        <Cartao variante="contorno" titulo={t.repetidas.titulo}>
          {#if stats.repetidas.media === null}
            <p class="nota">{t.repetidas.semDados}</p>
          {:else}
            <dl class="medidas">
              <div>
                <dt>{t.repetidas.media}</dt>
                <dd data-medida="repetidas-media">{formatarDecimal(stats.repetidas.media)}</dd>
              </div>
              <div>
                <dt>{t.repetidas.esperado}</dt>
                <dd data-medida="repetidas-esperado">
                  {formatarDecimal(stats.repetidas.esperada)}
                </dd>
              </div>
            </dl>
            <p class="nota">{t.repetidas.nota(stats.repetidas.comparacoes)}</p>
            {#if blocoRepetidas}<ComparacaoDistribuicao bloco={blocoRepetidas} />{/if}
          {/if}
        </Cartao>
      </div>

      {#if stats.soma}
        <Cartao variante="contorno" titulo={t.soma.titulo}>
          <dl class="medidas medidas-quatro">
            <div>
              <dt>{t.soma.media}</dt>
              <dd data-medida="soma-media">{formatarDecimal(stats.soma.media)}</dd>
            </div>
            <div>
              <dt>{t.soma.esperado}</dt>
              <dd data-medida="soma-esperado">{formatarDecimal(stats.somaEsperada)}</dd>
            </div>
            <div>
              <dt>{t.soma.minimo}</dt>
              <dd data-medida="soma-minimo">{formatarInteiro(stats.soma.minimo)}</dd>
            </div>
            <div>
              <dt>{t.soma.maximo}</dt>
              <dd data-medida="soma-maximo">{formatarInteiro(stats.soma.maximo)}</dd>
            </div>
          </dl>
          <p class="nota">{t.soma.nota}</p>
          {#if blocoSoma}<ComparacaoSoma bloco={blocoSoma} />{/if}
        </Cartao>
      {/if}

      {#if atraso}
        <Cartao variante="contorno" titulo={t.atraso.titulo}>
          <p class="nota">{t.atraso.intro}</p>
          <ul class="atrasos" aria-label={t.atraso.listaRotulo}>
            {#each atraso.itens as item (item.dezena)}
              <li data-dezena={item.dezena}>
                <span class="disco" aria-hidden="true">{item.rotulo}</span>
                <span class="item-texto">
                  {item.texto}
                  {#if item.chance}<span class="chance" data-chance-atraso>{item.chance}</span>{/if}
                </span>
              </li>
            {/each}
          </ul>
          {#if atraso.resumoMaior}<p class="nota" data-resumo-atraso>{atraso.resumoMaior}</p>{/if}

          <!-- Sempre junto do ranking: dezenas que nem apareceram na janela têm atraso maior. -->
          <div class="sem-aparicao" data-bloco="sem-aparicao">
            <p class="sem-aparicao-titulo">{atraso.textoSemAparicao}</p>
            <p class="nota" data-chance-sem-aparicao>{atraso.chanceSemAparicao}</p>
            {#if atraso.semAparicao.length > 0}
              <p class="nota">{t.atraso.semAparicaoAviso(atraso.semAparicao.length)}</p>
              <ul class="lista-dezenas" aria-label={atraso.textoSemAparicao}>
                {#each atraso.semAparicao as d (d)}
                  <li>{doisDigitos(d)}</li>
                {/each}
              </ul>
            {/if}
          </div>
          <p class="nota">{t.atraso.nota}</p>
          <p class="nota" data-independente>{atraso.independente}</p>
        </Cartao>
      {/if}
    {:else}
      <Aviso>{t.semConcursos}</Aviso>
    {/if}

    <Aviso>{textos.avisos.jogoResponsavel}</Aviso>
  {/if}
</div>

<style>
  .subtitulo {
    margin-top: var(--esp-3);
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

  /* Uma coluna no celular: as mini-barras emparelhadas precisam da largura toda. */
  .dupla {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--esp-3);
  }
  @media (min-width: 48rem) {
    .dupla {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      align-items: start;
    }
  }

  .medidas {
    display: grid;
    gap: var(--esp-3);
    margin: 0;
  }
  .medidas-quatro {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .medidas dt {
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .medidas dd {
    margin: 0;
    font: var(--fonte-titulo-g);
    font-variant-numeric: tabular-nums;
  }
  .nota {
    margin: var(--esp-3) 0 0;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }

  .atrasos {
    display: grid;
    gap: var(--esp-2);
    margin: var(--esp-3) 0 0;
    padding: 0;
    list-style: none;
  }
  .atrasos li {
    display: flex;
    align-items: center;
    gap: var(--esp-3);
    font: var(--fonte-corpo-g);
  }
  .item-texto {
    display: grid;
    gap: 0.125rem;
    min-width: 0;
  }
  .chance {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .disco {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
  }
  .sem-aparicao {
    margin-top: var(--esp-4);
    padding-top: var(--esp-3);
    border-top: 1px solid var(--cor-contorno-var);
  }
  .sem-aparicao-titulo {
    font: var(--fonte-titulo-p);
  }
  .sem-aparicao .nota {
    margin-top: var(--esp-1);
  }
  .lista-dezenas {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-1) var(--esp-2);
    margin: var(--esp-2) 0 0;
    padding: 0;
    list-style: none;
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
  }
  .lista-dezenas li {
    min-width: 1.75rem;
    padding: 0.125rem 0.375rem;
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-p);
    text-align: center;
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
