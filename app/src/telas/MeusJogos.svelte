<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import Aviso from '../componentes/Aviso.svelte';
  import FolhaModal from '../componentes/FolhaModal.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import type { OpcaoSegmentada } from '../componentes/tipos';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import { formatarDataHora } from '../dados/estado-sincronizacao';
  import { SLUGS, type ConcursoHistorico, type Slug } from '../dados/tipos';
  import { TAMANHO_MAXIMO_ARQUIVO } from '../jogos/arquivo';
  import { TAMANHO_MAXIMO_NOME, validarNome, type JogoSalvo } from '../jogos/jogo';
  import type { RepositorioJogos } from '../jogos/repositorio';
  import { exportarJogos, importarJogos } from '../jogos/servico';
  import { baixarTexto, lerTextoDoArquivo } from '../lib/download';
  import { gravarLoteriaGuardada, lerLoteriaGuardada } from '../lib/loteria-guardada';
  import {
    LOTERIA_PADRAO,
    hashDoJogo,
    hashDosJogos,
    loteriaDoHash,
    rotaDoHash,
    secaoConferirDoHash,
  } from '../lib/rotas';
  import { textos } from '../lib/textos';
  import { conferirJogo } from './conferir';

  interface Props {
    jogos: RepositorioJogos;
    /** Só leitura do cache: o melhor resultado histórico não baixa nada. */
    fonte: {
      lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null>;
    };
    /** Carrega o jogo no volante de Conferir jogo. */
    aoConferir: (jogo: JogoSalvo) => void;
  }

  let { jogos, fonte, aoConferir }: Props = $props();

  type Mensagem = { tipo: 'ok' | 'erro'; texto: string };

  let loteria = $state<Slug>(
    loteriaDoHash(location.hash) ?? lerLoteriaGuardada() ?? LOTERIA_PADRAO,
  );
  // null = ainda carregando ou leitura falhou (ver `falhaAoListar`); nunca "vazio" por engano.
  let todos = $state.raw<JogoSalvo[] | null>(null);
  let falhaAoListar = $state(false);
  let volatil = $state(false);
  let historico = $state.raw<readonly ConcursoHistorico[] | null>(null);
  let mensagem = $state<Mensagem | null>(null);
  let desfazer = $state.raw<JogoSalvo | null>(null);
  let ocupado = $state(false);
  let sequenciaHistorico = 0;

  let editandoId = $state<string | null>(null);
  let textoEdicao = $state('');
  let erroEdicao = $state<string | null>(null);
  let paraApagar = $state.raw<JogoSalvo | null>(null);
  // Elemento que abriu a confirmação: recebe o foco de volta se a pessoa desistir.
  let gatilhoApagar: HTMLElement | null = null;

  let tituloLista: HTMLElement | undefined = $state();
  let campoEdicao: HTMLInputElement | undefined = $state();
  let campoArquivo: HTMLInputElement | undefined = $state();

  const idBase = $props.id();

  const opcoesLoteria: readonly OpcaoSegmentada<Slug>[] = SLUGS.map((slug) => ({
    valor: slug,
    rotulo: textos.loterias[slug],
  }));

  const daLoteria = $derived((todos ?? []).filter((j) => j.loteria === loteria));
  const nomeLoteria = $derived(textos.loterias[loteria]);

  // Melhor resultado de cada jogo no histórico em cache. Calculado aos poucos (um lote
  // por vez, cedendo o fio entre lotes) para a tela não travar com muitos jogos; cada
  // jogo é calculado uma só vez por histórico (o id e as dezenas de um jogo não mudam).
  type Melhor = { acertos: number; concurso: number };
  const JOGOS_POR_LOTE = 20;
  let melhores = $state.raw<Record<string, Melhor>>({});
  // Sem protótipo: um id como "constructor" não pode colidir com Object.prototype.
  let cacheMelhores: Record<string, Melhor | null> = Object.create(null);
  let historicoDoCache: readonly ConcursoHistorico[] | null = null;

  $effect(() => {
    const h = historico;
    const lista = daLoteria;
    return untrack(() => {
      if (h !== historicoDoCache) {
        cacheMelhores = Object.create(null);
        historicoDoCache = h;
      }
      if (h === null || h.length === 0) {
        melhores = {};
        return undefined;
      }
      let proximo = 0;
      let cancelado = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const passo = (): void => {
        if (cancelado) return;
        let calculados = 0;
        while (proximo < lista.length && calculados < JOGOS_POR_LOTE) {
          const j = lista[proximo++] as JogoSalvo;
          if (j.id in cacheMelhores) continue;
          const melhor = conferirJogo(j.loteria, j.dezenas, h).melhor;
          const primeiro = melhor?.concursos[0];
          cacheMelhores[j.id] =
            melhor && primeiro !== undefined
              ? { acertos: melhor.acertos, concurso: primeiro }
              : null;
          calculados++;
        }
        const mapa: Record<string, Melhor> = Object.create(null);
        for (const j of lista) {
          const m = cacheMelhores[j.id];
          if (m) mapa[j.id] = m;
        }
        melhores = mapa;
        if (proximo < lista.length) timer = setTimeout(passo, 0);
      };
      passo();
      return () => {
        cancelado = true;
        if (timer !== undefined) clearTimeout(timer);
      };
    });
  });

  const dezena2 = (d: number): string => String(d).padStart(2, '0');
  const descricao = (j: JogoSalvo): string =>
    j.nome ?? `${textos.jogos.semNome} (${j.dezenas.map(dezena2).join(' ')})`;

  async function recarregar(): Promise<void> {
    try {
      todos = await jogos.listar();
      falhaAoListar = false;
    } catch {
      // Não finge lista vazia: sem a leitura, a tela mostra o erro e bloqueia Exportar.
      todos = null;
      falhaAoListar = true;
    }
    volatil = Boolean(jogos.aviso);
  }

  /** Só lê o cache; sem histórico guardado, o jogo simplesmente não mostra o melhor resultado. */
  async function carregarHistorico(slug: Slug): Promise<void> {
    const minha = ++sequenciaHistorico;
    historico = null;
    try {
      const cache = await fonte.lerCache({ arquivos: ['historico'], loterias: [slug] });
      if (minha !== sequenciaHistorico) return;
      historico = cache?.loterias[slug]?.dados.historico?.concursos ?? null;
    } catch {
      if (minha === sequenciaHistorico) historico = null;
    }
  }

  function trocarLoteria(slug: Slug): void {
    if (slug === loteria) return;
    loteria = slug;
    editandoId = null;
    mensagem = null;
    desfazer = null;
    gravarLoteriaGuardada(slug);
    history.replaceState(history.state, '', hashDosJogos(slug));
    void carregarHistorico(slug);
  }

  function aoMudarHash(): void {
    if (
      rotaDoHash(location.hash) !== 'conferir' ||
      secaoConferirDoHash(location.hash) !== 'jogos'
    ) {
      return;
    }
    const daUrl = loteriaDoHash(location.hash);
    if (daUrl !== null && daUrl !== loteria) {
      loteria = daUrl;
      editandoId = null;
      void carregarHistorico(daUrl);
    }
  }

  // ---------------------------------------------------------------- renomear

  const idBotaoRenomear = (id: string): string => `${idBase}-renomear-${id}`;

  /** O botão Renomear some enquanto o campo está aberto: ao voltar, o foco vai para o novo botão. */
  async function devolverFocoRenomear(id: string): Promise<void> {
    await tick();
    const botao = document.getElementById(idBotaoRenomear(id));
    (botao ?? tituloLista)?.focus();
  }

  async function comecarEdicao(jogo: JogoSalvo): Promise<void> {
    editandoId = jogo.id;
    textoEdicao = jogo.nome ?? '';
    erroEdicao = null;
    await tick();
    campoEdicao?.focus();
  }

  async function cancelarEdicao(): Promise<void> {
    const id = editandoId;
    editandoId = null;
    erroEdicao = null;
    if (id !== null) await devolverFocoRenomear(id);
  }

  async function salvarEdicao(evento: SubmitEvent, jogo: JogoSalvo): Promise<void> {
    evento.preventDefault();
    const nome = validarNome(textoEdicao);
    if (!nome.ok) {
      erroEdicao = textos.jogos.nomeInvalido(TAMANHO_MAXIMO_NOME);
      campoEdicao?.focus();
      return;
    }
    try {
      const novo = await jogos.renomear(jogo.id, nome.valor);
      mensagem = novo
        ? { tipo: 'ok', texto: textos.jogos.nomeAtualizado }
        : { tipo: 'erro', texto: textos.jogos.renomearFalhou };
    } catch {
      mensagem = { tipo: 'erro', texto: textos.jogos.renomearFalhou };
    }
    desfazer = null;
    editandoId = null;
    erroEdicao = null;
    await recarregar();
    await devolverFocoRenomear(jogo.id);
  }

  // ---------------------------------------------------------------- apagar e desfazer

  function pedirApagar(jogo: JogoSalvo, evento: Event): void {
    gatilhoApagar = evento.currentTarget as HTMLElement;
    paraApagar = jogo;
  }

  async function fecharConfirmacao(): Promise<void> {
    paraApagar = null;
    await tick();
    gatilhoApagar?.focus();
  }

  async function confirmarApagar(): Promise<void> {
    const jogo = paraApagar;
    if (!jogo) return;
    paraApagar = null;
    try {
      await jogos.apagar(jogo.id);
      desfazer = jogo;
      mensagem = { tipo: 'ok', texto: textos.jogos.apagado };
    } catch {
      mensagem = { tipo: 'erro', texto: textos.jogos.apagarFalhou };
    }
    await recarregar();
    await tick();
    // O botão que abriu a confirmação some junto com o jogo: o foco vai para a lista.
    tituloLista?.focus();
  }

  async function desfazerApagar(): Promise<void> {
    const jogo = desfazer;
    if (!jogo) return;
    desfazer = null;
    try {
      await jogos.gravar([jogo]);
      mensagem = { tipo: 'ok', texto: textos.jogos.desfeito };
    } catch {
      mensagem = { tipo: 'erro', texto: textos.jogos.desfazerFalhou };
    }
    await recarregar();
  }

  // ---------------------------------------------------------------- exportar e importar

  async function exportar(): Promise<void> {
    if (ocupado) return;
    ocupado = true;
    desfazer = null;
    try {
      const arquivo = await exportarJogos(jogos);
      if (arquivo.quantidade === 0) {
        mensagem = { tipo: 'erro', texto: textos.jogos.nadaParaExportar };
      } else if (baixarTexto(arquivo.nome, arquivo.conteudo)) {
        mensagem = {
          tipo: 'ok',
          texto: textos.jogos.exportado(arquivo.quantidade, arquivo.nome),
        };
      } else {
        mensagem = { tipo: 'erro', texto: textos.jogos.exportarFalhou };
      }
    } catch {
      mensagem = { tipo: 'erro', texto: textos.jogos.exportarFalhou };
    } finally {
      ocupado = false;
    }
  }

  async function aoEscolherArquivo(): Promise<void> {
    const arquivo = campoArquivo?.files?.[0];
    if (!arquivo || ocupado) return;
    ocupado = true;
    desfazer = null;
    try {
      if (arquivo.size > TAMANHO_MAXIMO_ARQUIVO) {
        mensagem = { tipo: 'erro', texto: textos.jogos.recusa.grande };
        return;
      }
      const texto = await lerTextoDoArquivo(arquivo);
      const r = await importarJogos(jogos, texto);
      if (r.tipo === 'recusado') {
        mensagem = { tipo: 'erro', texto: textos.jogos.recusa[r.motivo] };
      } else {
        mensagem = {
          tipo: 'ok',
          texto: textos.jogos.resultadoImportacao(r.entraram, r.ignorados, r.invalidos),
        };
        await recarregar();
      }
    } catch {
      mensagem = { tipo: 'erro', texto: textos.jogos.recusa.leitura };
    } finally {
      // Permite escolher o mesmo arquivo de novo.
      if (campoArquivo) campoArquivo.value = '';
      ocupado = false;
    }
  }

  onMount(() => {
    void recarregar();
    void carregarHistorico(loteria);
    return () => {
      sequenciaHistorico++;
    };
  });
</script>

<svelte:window onhashchange={aoMudarHash} />

<h2 id="titulo-aba">{textos.jogos.titulo}</h2>

<div class="pilha">
  <div class="controle">
    <Segmentado
      opcoes={opcoesLoteria}
      valor={loteria}
      rotuloGrupo={textos.jogos.rotuloLoteria}
      aoMudar={trocarLoteria}
    />
  </div>

  <Aviso>{textos.jogos.localAviso}</Aviso>
  {#if volatil}
    <Aviso tipo="erro">{textos.jogos.volatil}</Aviso>
  {/if}
  <Aviso>{textos.avisos.naoPreveConferir} {textos.avisos.jogoResponsavel}</Aviso>

  <div class="acoes" role="group" aria-label={textos.jogos.rotuloBackup}>
    <button
      type="button"
      class="botao"
      disabled={ocupado || falhaAoListar || (todos?.length ?? 0) === 0}
      onclick={() => void exportar()}
    >
      {textos.jogos.exportar}
    </button>
    <button type="button" class="botao" disabled={ocupado} onclick={() => campoArquivo?.click()}>
      {ocupado ? textos.jogos.importando : textos.jogos.importar}
    </button>
    <input
      bind:this={campoArquivo}
      id="{idBase}-arquivo"
      class="oculto"
      type="file"
      accept="application/json,.json"
      tabindex="-1"
      aria-hidden="true"
      onchange={() => void aoEscolherArquivo()}
    />
  </div>

  <div class="viva">
    <p class="mensagem" role="status" data-tipo={mensagem?.tipo ?? 'nenhum'}>
      {mensagem?.texto ?? ''}
    </p>
    {#if desfazer}
      <button type="button" class="botao" onclick={() => void desfazerApagar()}>
        {textos.jogos.desfazer}
      </button>
    {/if}
  </div>

  {#if falhaAoListar}
    <Aviso tipo="erro">{textos.jogos.falhaListar}</Aviso>
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void recarregar()}>
        {textos.jogos.tentarDeNovo}
      </button>
    </div>
  {:else if todos === null}
    <p class="discreto" role="status">{textos.jogos.carregando}</p>
  {:else}
    <h3 bind:this={tituloLista} class="titulo-lista" tabindex="-1">
      {textos.jogos.lista(nomeLoteria, daLoteria.length)}
    </h3>
    {#if daLoteria.length === 0}
      <div class="vazio">
        <p>{textos.jogos.vazio(nomeLoteria)}</p>
        <p class="discreto">{textos.jogos.vazioOrienta}</p>
        <div class="acoes">
          <button
            type="button"
            class="botao"
            onclick={() => {
              location.hash = hashDoJogo(loteria);
            }}
          >
            {textos.jogos.irParaConferir}
          </button>
        </div>
      </div>
    {:else}
      <ul class="lista">
        {#each daLoteria as jogo (jogo.id)}
          {@const melhor = melhores[jogo.id]}
          {@const quando = formatarDataHora(jogo.criadoEm)}
          <li class="jogo">
            {#if editandoId === jogo.id}
              <form class="edicao" onsubmit={(e) => void salvarEdicao(e, jogo)}>
                <label for="{idBase}-nome-{jogo.id}">{textos.jogos.campoNome}</label>
                <input
                  bind:this={campoEdicao}
                  id="{idBase}-nome-{jogo.id}"
                  type="text"
                  autocomplete="off"
                  maxlength={TAMANHO_MAXIMO_NOME}
                  aria-invalid={erroEdicao !== null}
                  aria-describedby={erroEdicao !== null ? `${idBase}-erro-${jogo.id}` : undefined}
                  bind:value={textoEdicao}
                  onkeydown={(e) => {
                    if (e.key === 'Escape') {
                      e.preventDefault();
                      void cancelarEdicao();
                    }
                  }}
                />
                {#if erroEdicao !== null}
                  <p id="{idBase}-erro-{jogo.id}" class="erro-campo" role="alert">{erroEdicao}</p>
                {/if}
                <div class="acoes">
                  <button type="submit" class="botao primario">{textos.jogos.salvarNome}</button>
                  <button type="button" class="botao" onclick={() => void cancelarEdicao()}>
                    {textos.jogos.cancelar}
                  </button>
                </div>
              </form>
            {:else}
              <p class="nome" class:sem-nome={jogo.nome === null}>
                {jogo.nome ?? textos.jogos.semNome}
              </p>
              <p class="resumo">{textos.jogos.resumo(nomeLoteria, jogo.dezenas.length)}</p>
              <p class="dezenas">
                <span class="so-leitor">
                  {textos.jogos.dezenasRotulo}: {jogo.dezenas.join(', ')}.
                </span>
                {#each jogo.dezenas as d (d)}
                  <span class="dezena" aria-hidden="true">{dezena2(d)}</span>
                {/each}
              </p>
              {#if quando}
                <p class="discreto">{textos.jogos.salvoEm(quando)}</p>
              {/if}
              {#if melhor}
                <p class="melhor">{textos.jogos.melhor(melhor.acertos, melhor.concurso)}</p>
              {/if}
              <div class="acoes">
                <button
                  type="button"
                  class="botao primario"
                  aria-label={textos.jogos.acaoDe(textos.jogos.conferir, descricao(jogo))}
                  onclick={() => aoConferir(jogo)}
                >
                  {textos.jogos.conferir}
                </button>
                <button
                  type="button"
                  class="botao"
                  id={idBotaoRenomear(jogo.id)}
                  aria-label={textos.jogos.acaoDe(textos.jogos.renomear, descricao(jogo))}
                  onclick={() => void comecarEdicao(jogo)}
                >
                  {textos.jogos.renomear}
                </button>
                <button
                  type="button"
                  class="botao"
                  aria-label={textos.jogos.acaoDe(textos.jogos.apagar, descricao(jogo))}
                  onclick={(e) => pedirApagar(jogo, e)}
                >
                  {textos.jogos.apagar}
                </button>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

{#if paraApagar}
  <FolhaModal titulo={textos.jogos.confirmaTitulo} aoFechar={() => void fecharConfirmacao()}>
    <p class="confirma">{textos.jogos.confirmaTexto(descricao(paraApagar))}</p>
    <div class="acoes">
      <button type="button" class="botao perigo" onclick={() => void confirmarApagar()}>
        {textos.jogos.confirmaApagar}
      </button>
      <button type="button" class="botao" onclick={() => void fecharConfirmacao()}>
        {textos.jogos.cancelar}
      </button>
    </div>
  </FolhaModal>
{/if}

<style>
  h2 {
    font: var(--fonte-titulo-g);
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
  .discreto {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .acoes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-3);
  }
  .oculto {
    display: none;
  }
  .viva {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--esp-3);
  }
  .mensagem {
    flex: 1 1 14rem;
    min-width: 0;
    font: var(--fonte-corpo-m);
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .mensagem[data-tipo='erro'] {
    color: var(--cor-erro);
  }
  .mensagem[data-tipo='nenhum'] {
    display: none;
  }
  .titulo-lista {
    font: var(--fonte-titulo-m);
  }
  .titulo-lista:focus {
    outline: none;
  }
  .vazio {
    display: grid;
    gap: var(--esp-2);
    padding: var(--esp-4);
    border: 1px dashed var(--cor-contorno);
    border-radius: var(--raio-m);
  }
  .lista {
    display: grid;
    gap: var(--esp-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .jogo {
    display: grid;
    gap: var(--esp-2);
    padding: var(--esp-4);
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-m);
    background: var(--cor-superficie);
  }
  .nome {
    font: var(--fonte-titulo-m);
    overflow-wrap: anywhere;
  }
  .nome.sem-nome {
    color: var(--cor-texto-var);
    font-weight: 400;
  }
  .resumo {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .dezenas {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-1);
    margin: 0;
  }
  .dezena {
    min-width: 2rem;
    padding: 0.125rem var(--esp-1);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .melhor {
    font: var(--fonte-corpo-m);
    font-weight: 600;
  }
  .edicao {
    display: grid;
    gap: var(--esp-2);
  }
  .edicao label {
    font: var(--fonte-rotulo-g);
  }
  .edicao input {
    min-height: var(--alvo-minimo);
    max-width: 26rem;
    padding: 0 var(--esp-3);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-p);
    background: var(--cor-superficie);
    color: var(--cor-texto);
    font: var(--fonte-corpo-g);
  }
  .erro-campo {
    color: var(--cor-erro);
    font: var(--fonte-corpo-m);
    font-weight: 600;
  }
  .confirma {
    margin-bottom: var(--esp-4);
    overflow-wrap: anywhere;
  }
  .botao {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--alvo-minimo);
    min-width: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .botao.primario {
    border-color: var(--cor-primaria);
    background: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
  }
  .botao.perigo {
    border-color: var(--cor-erro);
    background: var(--cor-erro);
    color: var(--cor-sobre-erro);
  }
  .botao:disabled {
    border-color: var(--cor-contorno-var);
    background: transparent;
    color: var(--cor-texto-var);
    cursor: not-allowed;
    opacity: 0.6;
  }
</style>
