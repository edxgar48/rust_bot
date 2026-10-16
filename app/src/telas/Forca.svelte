<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import FolhaModal from '../componentes/FolhaModal.svelte';
  import Icone from '../componentes/Icone.svelte';
  import LinhaRanking from '../componentes/LinhaRanking.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import Volante from '../componentes/Volante.svelte';
  import type { NomeIcone, OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import type { ResultadoGarantia } from '../dados/sob-demanda';
  import { SLUGS, type Slug } from '../dados/tipos';
  import {
    gravarExibicaoGuardada,
    gravarLoteriaGuardada,
    lerExibicaoGuardada,
    lerLoteriaGuardada,
  } from '../lib/loteria-guardada';
  import {
    EXIBICAO_PADRAO,
    LOTERIA_PADRAO,
    dezenaDoHash,
    exibicaoDoHash,
    hashDaForca,
    hashDoJogo,
    loteriaDoHash,
    rotaDoHash,
    type ExibicaoForca,
  } from '../lib/rotas';
  import { doisDigitos } from '../lib/numeros';
  import { textos } from '../lib/textos';
  import DetalheDezena from './DetalheDezena.svelte';
  import { montarDetalheDaDezena } from './detalhe-dezena';
  import type { EstadoTela, SituacaoTela } from './estado-tela';
  import { montarVisaoForca } from './forca';
  import { alternarNoRascunho, dezenasDoRascunho, pedirAbertura } from './rascunho-jogo';

  interface Props {
    /** O que a tela precisa da sessão de dados (injetável nos testes). */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
      garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
    };
  }

  let { fonte }: Props = $props();

  // Precedência: URL (#/forca?loteria=quina), depois a última escolha guardada, depois Lotofácil.
  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  // Mesma precedência para a forma de exibição: URL (&visao=), aparelho, lista.
  let visaoEscolhida = $state<ExibicaoForca>(
    exibicaoDoHash(location.hash) ?? lerExibicaoGuardada() ?? EXIBICAO_PADRAO,
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
  const opcoesVisao: readonly OpcaoSegmentada<ExibicaoForca>[] = [
    { valor: 'lista', rotulo: textos.forca.visaoLista },
    { valor: 'volante', rotulo: textos.forca.visaoVolante },
  ];

  const visao = $derived(montarVisaoForca(loteria, situacao));
  const detalhe = $derived(
    visao.tipo === 'conteudo' && dezenaAberta !== null
      ? montarDetalheDaDezena(visao.tabela, dezenaAberta)
      : null,
  );

  // Jogo em montagem (botão Inserir): o rascunho vive em memória de módulo; aqui fica o espelho reativo.
  let inseridas = $state<readonly number[]>(dezenasDoRascunho(untrack(() => loteria)));
  let avisoRascunho = $state('');

  async function inserirNoJogo(dezena: number): Promise<void> {
    const resultado = alternarNoRascunho(loteria, dezena);
    inseridas = dezenasDoRascunho(loteria);
    const rotulo = doisDigitos(dezena);
    const mensagem =
      resultado.tipo === 'cheio'
        ? textos.forca.rascunho.cheio(rotulo, resultado.maximo)
        : resultado.tipo === 'inserida'
          ? textos.forca.rascunho.inserida(rotulo)
          : textos.forca.rascunho.removida(rotulo);
    // Limpa e regrava depois do tick: o leitor de tela reanuncia mesmo que o texto se repita.
    avisoRascunho = '';
    await tick();
    avisoRascunho = mensagem;
  }

  function abrirEmConferir(): void {
    pedirAbertura();
    gravarLoteriaGuardada(loteria);
    location.hash = hashDoJogo(loteria);
  }

  const urlAtual = (dezena: number | null = null): string =>
    hashDaForca(loteria, { visao: visaoEscolhida, dezena });

  const iconesDoEstado: Record<EstadoTela, NomeIcone> = {
    sincronizando: 'atualizar',
    atualizado: 'conferir',
    sem_rede: 'sem-rede',
    recusado: 'erro',
  };

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
    inseridas = dezenasDoRascunho(slug);
    avisoRascunho = '';
    dezenaAberta = null;
    // replaceState: não polui o histórico nem dispara hashchange.
    history.replaceState(history.state, '', urlAtual());
    void carregar(slug);
  }

  function trocarVisao(nova: ExibicaoForca): void {
    visaoEscolhida = nova;
    gravarExibicaoGuardada(nova);
    history.replaceState(history.state, '', urlAtual());
  }

  // Abrir o detalhe cria uma entrada no histórico: o botão voltar fecha.
  function abrirDetalhe(dezena: number): void {
    dezenaAberta = dezena;
    history.pushState({ detalheForca: true }, '', urlAtual(dezena));
  }

  function fecharDetalhe(): void {
    if (history.state?.detalheForca) {
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

  // Ao fechar o detalhe, o foco volta à linha/célula de origem.
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

  // Hash colado ou botão voltar com outro ?loteria=: segue a URL.
  function aoMudarHash(): void {
    if (rotaDoHash(location.hash) !== 'forca') return;
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      inseridas = dezenasDoRascunho(daUrl);
      avisoRascunho = '';
      void carregar(daUrl);
    }
    const exibicao = exibicaoDoHash(location.hash);
    if (exibicao !== null) visaoEscolhida = exibicao;
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

<h2 id="titulo-aba">{textos.forca.titulo}</h2>
{#if visao.tipo === 'conteudo'}
  <p class="subtitulo">{visao.subtitulo}</p>
  <p class="meta">{visao.perfilJanela}</p>
{/if}

<div class="pilha" bind:this={raiz}>
  <!-- Em tela larga os seletores não esticam pela página inteira. -->
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={textos.forca.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>
  <div class="controle controle-estreito">
    <Segmentado
      opcoes={opcoesVisao}
      valor={visaoEscolhida}
      rotuloGrupo={textos.forca.rotuloVisao}
      aoMudar={trocarVisao}
    />
  </div>

  <Aviso>{textos.avisos.naoPreveForca}</Aviso>

  {#if visao.tipo === 'carregando'}
    <div class="carregando" aria-busy="true">
      <p class="discreto" role="status">{textos.forca.carregando}</p>
      {#each [0, 1, 2, 3, 4] as i (i)}
        <span class="barra" aria-hidden="true"></span>
      {/each}
    </div>
  {:else if visao.tipo === 'sem_dados'}
    <Aviso tipo={visao.erro ? 'erro' : 'informativo'}>{visao.texto}</Aviso>
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void carregar(loteria)}>
        <Icone nome="atualizar" tamanho={1.125} />
        {textos.forca.tentarDeNovo}
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

    {#if visaoEscolhida === 'volante'}
      <Volante {loteria} modo="leitura" posicoes={visao.posicoes} aoEscolher={abrirDetalhe} />
    {:else}
      <div class="tabela">
        <div class="cabecalho" aria-hidden="true">
          <span>{textos.forca.colunas.posicao}</span>
          <span>{textos.forca.colunas.dezena}</span>
          <span>{textos.forca.colunas.variacao}</span>
          <span class="centro">{textos.forca.colunas.jogo}</span>
          <span class="direita">{textos.forca.colunas.indice}</span>
          <span class="evidencia">{textos.forca.colunas.evidencia}</span>
        </div>
        <div class="rascunho">
          <p class="aviso-rascunho" role="status">{avisoRascunho}</p>
          {#if inseridas.length > 0}
            <p class="atalho">
              <span>{textos.forca.rascunho.contador(inseridas.length)}</span>
              <span aria-hidden="true">·</span>
              <button type="button" class="link" onclick={abrirEmConferir}>
                {textos.forca.rascunho.abrir}
              </button>
            </p>
          {/if}
        </div>
        <ul class="lista" aria-label={textos.forca.lista(visao.nomeLoteria, visao.linhas.length)}>
          {#each visao.linhas as linha (linha.dezena)}
            <LinhaRanking
              posicao={linha.posicao}
              numero={linha.dezena}
              variacao={linha.variacao}
              indice={linha.indice}
              grau={linha.grau}
              direcao={linha.direcao}
              aoEscolher={abrirDetalhe}
              aoInserir={(n) => void inserirNoJogo(n)}
              inserida={inseridas.includes(linha.dezena)}
              rotuloEvidenciaVisivel={false}
            />
          {/each}
        </ul>
      </div>
    {/if}
  {/if}
</div>

{#if detalhe}
  <FolhaModal titulo={detalhe.titulo} aoFechar={fecharDetalhe}>
    <DetalheDezena {detalhe} />
  </FolhaModal>
{/if}

<style>
  h2 {
    font: var(--fonte-titulo-g);
  }
  .subtitulo {
    margin-top: var(--esp-1);
    font: var(--fonte-corpo-m);
  }
  .meta {
    margin-top: var(--esp-1);
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .pilha {
    display: grid;
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
  .tabela {
    border-radius: var(--raio-m);
    background: var(--cor-cont-baixo);
    box-shadow: var(--elev-1);
    overflow: hidden;
  }
  /* Mesmas colunas de LinhaRanking (posição, dezena, variação, Inserir, índice). */
  .cabecalho {
    display: grid;
    grid-template-columns: 2.5rem 2.5rem 1fr 5.5rem auto;
    column-gap: var(--esp-3);
    padding: var(--esp-2) var(--esp-4);
    border-bottom: 1px solid var(--cor-contorno);
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .direita {
    text-align: right;
  }
  .centro {
    text-align: center;
  }
  /* Feedback e atalho do jogo em montagem, logo abaixo do cabeçalho. */
  .rascunho {
    padding: 0 var(--esp-4);
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .aviso-rascunho:not(:empty) {
    padding-top: var(--esp-2);
  }
  .atalho {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    column-gap: var(--esp-2);
  }
  .link {
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-1);
    border: 0;
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    text-decoration: underline;
    cursor: pointer;
  }
  .evidencia {
    grid-column: 2 / -1;
  }
  .lista {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .lista :global(li:last-child) {
    border-bottom: 0;
  }
  .discreto {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
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
  .acoes {
    display: flex;
  }
  .botao {
    display: inline-flex;
    align-items: center;
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
</style>
