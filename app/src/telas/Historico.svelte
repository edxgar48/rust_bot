<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Icone from '../componentes/Icone.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import Volante from '../componentes/Volante.svelte';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { PARAMETROS_LOTERIA, SLUGS, type ConcursoHistorico, type Slug } from '../dados/tipos';
  import { formatarDataSorteio } from '../lib/datas';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDoHistorico,
    loteriaDoHash,
    rotaDoHash,
    secaoConferirDoHash,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import {
    ALTURA_LINHA_REM,
    consultarHistorico,
    interpretarBusca,
    janelaVirtual,
    listarDezenas,
    mascararData,
    montarVisaoHistorico,
    type BuscaAplicada,
    type JanelaVirtual,
    type ModoBusca,
    type MotivoInvalido,
  } from './historico';
  import LinhaConcurso from './LinhaConcurso.svelte';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  /** Linhas extras de cada lado da área visível. */
  const LINHAS_EXTRAS = 3;

  // Precedência: URL (#/conferir?loteria=quina), depois a última escolha guardada, depois Lotofácil.
  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  // raw: o histórico tem milhares de listas; um proxy reativo em cada uma só custaria tempo.
  let situacao = $state.raw<SituacaoTela | null>(null);
  let sequencia = 0;

  let modoBusca = $state<ModoBusca>('concurso');
  let textoBusca = $state('');
  let buscaAplicada = $state<BuscaAplicada | null>(null);
  let motivoInvalido = $state<MotivoInvalido | null>(null);
  let dezenasFiltro = $state<number[]>([]);

  let secaoResultados: HTMLElement | undefined = $state();
  let lista: HTMLElement | undefined = $state();
  let janela = $state.raw<JanelaVirtual>({ inicio: 0, fim: 12 });
  let quadroPendente = false;
  // Tamanho da rem em px: lido uma vez e de novo só no resize (getComputedStyle força estilo).
  let pxPorRem = 16;

  const idCampo = $props.id();
  const idErro = `${idCampo}-erro`;

  const opcoesLoteria: readonly OpcaoSegmentada<Slug>[] = SLUGS.map((slug) => ({
    valor: slug,
    rotulo: textos.loterias[slug],
  }));
  const opcoesModo: readonly OpcaoSegmentada<ModoBusca>[] = [
    { valor: 'concurso', rotulo: textos.historico.busca.modoConcurso },
    { valor: 'data', rotulo: textos.historico.busca.modoData },
  ];
  const iconesDoEstado: Record<EstadoTela, NomeIcone> = {
    sincronizando: 'atualizar',
    atualizado: 'conferir',
    sem_rede: 'sem-rede',
    recusado: 'erro',
  };

  const visao = $derived(montarVisaoHistorico(loteria, situacao));
  const resultado = $derived(
    visao.tipo === 'conteudo'
      ? consultarHistorico(visao.concursos, { busca: buscaAplicada, dezenas: dezenasFiltro })
      : null,
  );
  const total = $derived(resultado?.indices.length ?? 0);
  const alturaRem = $derived(ALTURA_LINHA_REM[loteria]);
  const maximoDezenas = $derived(PARAMETROS_LOTERIA[loteria].dezenasSorteadas);
  const filtroAtivo = $derived(dezenasFiltro.length > 0);
  const filtroOrdenado = $derived([...dezenasFiltro].sort((a, b) => a - b));
  // Janela já limitada ao tamanho atual (a lista pode encolher antes de recalcular).
  const fim = $derived(Math.min(janela.fim, total));
  const inicio = $derived(Math.min(janela.inicio, fim));
  const visiveis = $derived.by(() => {
    if (visao.tipo !== 'conteudo' || !resultado) return [];
    const saida: { posicao: number; concurso: ConcursoHistorico }[] = [];
    for (let posicao = inicio; posicao < fim; posicao++) {
      const concurso = visao.concursos[resultado.indices[posicao] ?? -1];
      if (concurso) saida.push({ posicao, concurso });
    }
    return saida;
  });

  const rotuloCampo = $derived(
    modoBusca === 'concurso'
      ? textos.historico.busca.campoConcurso
      : textos.historico.busca.campoData,
  );
  const exemploCampo = $derived(
    modoBusca === 'concurso'
      ? textos.historico.busca.exemploConcurso
      : textos.historico.busca.exemploData,
  );
  const textoBuscaAtiva = $derived(
    buscaAplicada === null
      ? null
      : buscaAplicada.modo === 'concurso'
        ? textos.historico.busca.ativaConcurso(buscaAplicada.numero)
        : textos.historico.busca.ativaData(formatarDataSorteio(buscaAplicada.data)),
  );
  const textoInvalido = $derived.by(() => {
    if (motivoInvalido === 'formato_concurso') return textos.historico.busca.formatoConcurso;
    if (motivoInvalido === 'formato_data') return textos.historico.busca.formatoData;
    if (motivoInvalido === 'data_inexistente') {
      return textos.historico.busca.dataInexistente(textoBusca.trim());
    }
    return null;
  });
  const contagem = $derived.by(() => {
    if (!resultado || visao.tipo !== 'conteudo') return '';
    if (buscaAplicada === null && !filtroAtivo) {
      return textos.historico.contagem.tudo(visao.concursos.length);
    }
    return resultado.indices.length === 0
      ? textos.historico.contagem.nenhumCurto
      : textos.historico.contagem.encontrados(resultado.indices.length, visao.concursos.length);
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

  function limparConsulta(): void {
    buscaAplicada = null;
    motivoInvalido = null;
    textoBusca = '';
    dezenasFiltro = [];
  }

  function trocarLoteria(slug: Slug): void {
    if (slug === loteria) return;
    loteria = slug;
    gravarLoteriaGuardada(slug);
    limparConsulta();
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', hashDoHistorico(slug));
    void carregar(slug);
  }

  function trocarModo(novo: ModoBusca): void {
    modoBusca = novo;
    textoBusca = '';
    motivoInvalido = null;
  }

  function aoDigitar(evento: Event & { currentTarget: HTMLInputElement }): void {
    const digitado = evento.currentTarget.value;
    textoBusca = modoBusca === 'data' ? mascararData(digitado) : digitado;
    if (textoBusca !== digitado) evento.currentTarget.value = textoBusca;
    motivoInvalido = null;
  }

  function buscar(evento: SubmitEvent): void {
    evento.preventDefault();
    const leitura = interpretarBusca(modoBusca, textoBusca);
    if (leitura.tipo === 'invalida') {
      motivoInvalido = leitura.motivo;
      return;
    }
    motivoInvalido = null;
    buscaAplicada = leitura.tipo === 'ok' ? leitura.busca : null;
    void voltarAoTopoDaLista();
  }

  function limparBusca(): void {
    buscaAplicada = null;
    motivoInvalido = null;
    textoBusca = '';
    void voltarAoTopoDaLista();
  }

  function alternarDezena(dezena: number): void {
    dezenasFiltro = dezenasFiltro.includes(dezena)
      ? dezenasFiltro.filter((d) => d !== dezena)
      : dezenasFiltro.length < maximoDezenas
        ? [...dezenasFiltro, dezena]
        : dezenasFiltro;
    void voltarAoTopoDaLista();
  }

  /** Mudou a consulta: se a lista já passou do topo da tela, volta ao começo dela. */
  async function voltarAoTopoDaLista(): Promise<void> {
    await tick();
    if (secaoResultados && secaoResultados.getBoundingClientRect().top < 0) {
      secaoResultados.scrollIntoView({ block: 'start' });
    }
  }

  // --- Lista virtual: só as linhas perto da área visível existem no DOM.
  function recalcularJanela(): void {
    if (!lista) return;
    const nova = janelaVirtual({
      total,
      alturaLinha: alturaRem * pxPorRem,
      rolagem: -lista.getBoundingClientRect().top,
      alturaVisivel: window.innerHeight,
      extra: LINHAS_EXTRAS,
    });
    const atual = untrack(() => janela);
    if (nova.inicio !== atual.inicio || nova.fim !== atual.fim) janela = nova;
  }

  function medirRem(): void {
    pxPorRem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  }

  function aoRedimensionar(): void {
    medirRem();
    agendarJanela();
  }

  /** Uma conta por quadro, por mais eventos de rolagem que cheguem. */
  function agendarJanela(): void {
    if (quadroPendente) return;
    quadroPendente = true;
    const rodar = () => {
      quadroPendente = false;
      recalcularJanela();
    };
    if (typeof requestAnimationFrame === 'function') requestAnimationFrame(rodar);
    else setTimeout(rodar, 16);
  }

  // A lista mudou de tamanho, de altura de linha ou apareceu: refaz a janela.
  $effect(() => {
    void total;
    void alturaRem;
    void lista;
    if (lista) medirRem();
    recalcularJanela();
  });

  // Hash colado ou botão voltar com outro ?loteria=: segue a URL.
  function aoMudarHash(): void {
    if (
      rotaDoHash(location.hash) !== 'conferir' ||
      secaoConferirDoHash(location.hash) !== 'historico'
    ) {
      return;
    }
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      limparConsulta();
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

<svelte:window onhashchange={aoMudarHash} onscroll={agendarJanela} onresize={aoRedimensionar} />

<h2 id="titulo-aba">{textos.historico.titulo}</h2>
{#if visao.tipo === 'conteudo'}
  <p class="subtitulo">{visao.subtitulo}</p>
{/if}

<div class="pilha">
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={textos.historico.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>

  <Aviso>{textos.avisos.naoPreveHistorico}</Aviso>

  {#if visao.tipo === 'carregando'}
    <div class="carregando" aria-busy="true">
      <p class="discreto" role="status">{textos.historico.carregando}</p>
      {#each [0, 1, 2, 3] as i (i)}
        <span class="barra" aria-hidden="true"></span>
      {/each}
    </div>
  {:else if visao.tipo === 'sem_dados'}
    <Aviso tipo={visao.erro ? 'erro' : 'informativo'}>{visao.texto}</Aviso>
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void carregar(loteria)}>
        <Icone nome="atualizar" tamanho={1.125} />
        {textos.historico.tentarDeNovo}
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

    <Cartao variante="contorno" titulo={textos.historico.busca.titulo}>
      <form class="busca" onsubmit={buscar} novalidate>
        <div class="controle controle-estreito">
          <Segmentado
            opcoes={opcoesModo}
            valor={modoBusca}
            rotuloGrupo={textos.historico.busca.rotuloModo}
            aoMudar={trocarModo}
          />
        </div>
        <label class="rotulo-campo" for={idCampo}>{rotuloCampo}</label>
        <div class="linha-campo">
          <input
            id={idCampo}
            class="campo"
            type="text"
            inputmode="numeric"
            enterkeyhint="search"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            placeholder={exemploCampo}
            value={textoBusca}
            oninput={aoDigitar}
            aria-invalid={motivoInvalido !== null}
            aria-describedby={motivoInvalido !== null ? idErro : undefined}
          />
          <button type="submit" class="botao botao-cheio">{textos.historico.busca.buscar}</button>
        </div>
        {#if textoInvalido !== null}
          <div id={idErro}><Aviso tipo="erro">{textoInvalido}</Aviso></div>
        {/if}
      </form>
      {#if textoBuscaAtiva !== null}
        <p class="ativa">
          <span>{textoBuscaAtiva}</span>
          <button type="button" class="botao" onclick={limparBusca}>
            {textos.historico.busca.limpar}
          </button>
        </p>
      {/if}
    </Cartao>

    <details class="filtro">
      <summary>
        <span class="filtro-titulo">{textos.historico.filtro.titulo}</span>
        <span class="filtro-quantas"
          >({textos.historico.filtro.escolhidas(dezenasFiltro.length)})</span
        >
      </summary>
      <div class="filtro-corpo">
        <p class="discreto">{textos.historico.filtro.intro}</p>
        <Volante
          {loteria}
          modo="selecao"
          selecionadas={dezenasFiltro}
          aoAlternar={alternarDezena}
          bloquearNovas={dezenasFiltro.length >= maximoDezenas}
        />
        {#if dezenasFiltro.length >= maximoDezenas}
          <p class="discreto">{textos.historico.filtro.limite(maximoDezenas)}</p>
        {/if}
        {#if filtroAtivo}
          <div class="acoes">
            <button
              type="button"
              class="botao"
              onclick={() => {
                dezenasFiltro = [];
                void voltarAoTopoDaLista();
              }}
            >
              {textos.historico.filtro.limpar}
            </button>
          </div>
        {/if}
      </div>
    </details>

    <div class="resultados" bind:this={secaoResultados}>
      {#if filtroAtivo}
        <p class="discreto">{textos.historico.filtro.ativas(listarDezenas(filtroOrdenado))}</p>
      {/if}
      <p class="contagem" role="status">{contagem}</p>

      {#if resultado && resultado.aviso}
        {@const aviso = resultado.aviso}
        <Aviso>
          {#if aviso.tipo === 'concurso_inexistente'}
            {textos.historico.busca.concursoInexistente(aviso.numero, aviso.primeiro, aviso.ultimo)}
          {:else if aviso.tipo === 'data_sem_sorteio'}
            {textos.historico.busca.dataSemSorteio(formatarDataSorteio(aviso.data))}
            {#if aviso.anterior}
              {textos.historico.busca.vizinhoAnterior(
                aviso.anterior.concurso,
                formatarDataSorteio(aviso.anterior.data),
              )}
            {/if}
            {#if aviso.seguinte}
              {textos.historico.busca.vizinhoSeguinte(
                aviso.seguinte.concurso,
                formatarDataSorteio(aviso.seguinte.data),
              )}
            {/if}
          {:else}
            {textos.historico.contagem.nenhum(buscaAplicada !== null, filtroAtivo)}
          {/if}
        </Aviso>
      {/if}

      {#if total > 0}
        <ul
          class="lista"
          bind:this={lista}
          aria-label={textos.historico.lista(visao.nomeLoteria, total)}
          style:padding-top="{inicio * alturaRem}rem"
          style:padding-bottom="{(total - fim) * alturaRem}rem"
        >
          {#each visiveis as { posicao, concurso } (concurso[0])}
            <LinhaConcurso
              {loteria}
              concurso={concurso[0]}
              data={concurso[1]}
              dezenas={concurso[2]}
              filtro={dezenasFiltro}
              posicao={posicao + 1}
              {total}
            />
          {/each}
        </ul>
      {/if}
    </div>
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
  .controle-estreito {
    max-width: 20rem;
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

  /* Busca */
  .busca {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--esp-3);
  }
  .rotulo-campo {
    font: var(--fonte-rotulo-g);
  }
  .linha-campo {
    display: flex;
    gap: var(--esp-2);
  }
  .campo {
    flex: 1 1 0;
    min-width: 0;
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-p);
    background: var(--cor-superficie);
    color: var(--cor-texto);
    font: var(--fonte-corpo-g);
    font-variant-numeric: tabular-nums;
  }
  .campo[aria-invalid='true'] {
    border-color: var(--cor-erro);
    border-width: 2px;
  }
  .ativa {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--esp-2) var(--esp-3);
    margin-top: var(--esp-3);
    font: var(--fonte-corpo-m);
  }

  /* Filtro por dezenas */
  .filtro {
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-m);
    background: var(--cor-superficie);
  }
  .filtro summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--esp-2);
    min-height: var(--alvo-minimo);
    padding: var(--esp-2) var(--esp-4);
    font: var(--fonte-titulo-m);
    cursor: pointer;
  }
  .filtro-quantas {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .filtro-corpo {
    display: grid;
    gap: var(--esp-3);
    padding: 0 var(--esp-2) var(--esp-4);
  }
  .filtro-corpo > .discreto {
    padding: 0 var(--esp-2);
  }

  /* Resultados */
  .resultados {
    display: grid;
    gap: var(--esp-3);
  }
  .contagem {
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-g);
  }
  .contagem:empty {
    display: none;
  }
  .lista {
    margin: 0;
    padding-inline: 0;
    border-radius: var(--raio-m);
    background: var(--cor-cont-baixo);
    box-shadow: var(--elev-1);
    list-style: none;
    /* O navegador não deve "compensar" a troca de linhas na rolagem. */
    overflow-anchor: none;
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
  .botao-cheio {
    flex: none;
    border-color: transparent;
    background: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
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
