<script lang="ts">
  import { onMount } from 'svelte';
  import {
    controleGlobal,
    decidirOferta,
    ehIosSafari,
    estaInstalado,
    gravarDispensa,
    lerDispensa,
    type ControleInstalacao,
  } from '../lib/instalacao';
  import { textos } from '../lib/textos';
  import Cartao from './Cartao.svelte';
  import Icone from './Icone.svelte';

  interface Props {
    /** `cartao`: aviso discreto e dispensável (Início). `secao`: dentro da Entenda; sempre visível. */
    modo?: 'cartao' | 'secao';
    /** Injetável nos testes; o padrão é o controle da página. */
    controle?: ControleInstalacao | null;
  }

  let { modo = 'cartao', controle = controleGlobal() }: Props = $props();

  const t = textos.instalar;
  const cartao = $derived(modo === 'cartao');

  let instalado = $state(false);
  let dispensado = $state(false);
  let eventoDisponivel = $state(false);
  let ios = $state(false);
  let mensagem = $state<string | null>(null);

  const oferta = $derived(
    decidirOferta({ instalado, dispensado: cartao && dispensado, eventoDisponivel, ios }),
  );

  onMount(() => {
    instalado = estaInstalado({ matchMedia: globalThis.matchMedia?.bind(globalThis), navigator });
    dispensado = lerDispensa();
    ios = ehIosSafari(navigator);
    const atualizar = (): void => {
      eventoDisponivel = controle?.eventoDisponivel ?? false;
      if (controle?.instaladoAgora) {
        instalado = true;
        if (!jaInstaladoAntes) mensagem = t.instaladoAgora;
      }
    };
    // Mensagem só se a instalação acontecer com esta instância montada; remontar na
    // Início depois do evento não repete "App instalado".
    const jaInstaladoAntes = controle?.instaladoAgora ?? false;
    atualizar();
    return controle?.assinar(atualizar);
  });

  async function instalar(): Promise<void> {
    const resultado = (await controle?.instalar()) ?? 'indisponivel';
    // Aceito: o evento `appinstalled` confirma depois; recusado: o evento só vale uma vez.
    if (resultado === 'recusado') mensagem = t.recusado;
  }

  function dispensar(): void {
    dispensado = true; // some já; se não der para lembrar, volta na próxima abertura
    gravarDispensa();
  }
</script>

{#snippet conteudo()}
  {#if oferta !== 'nenhuma'}
    <p>{t.descricao}</p>
  {/if}
  {#if oferta === 'botao'}
    <div class="acoes">
      <button type="button" class="botao" onclick={() => void instalar()}>
        <Icone nome="baixar" tamanho={1.125} />
        {t.botao}
      </button>
      {#if cartao}
        <button
          type="button"
          class="botao secundario"
          aria-label={t.dispensarRotulo}
          onclick={dispensar}
        >
          {t.dispensar}
        </button>
      {/if}
    </div>
  {:else if oferta === 'ios'}
    <p class="ios">{t.ios}</p>
    {#if cartao}
      <div class="acoes">
        <button
          type="button"
          class="botao secundario"
          aria-label={t.dispensarRotulo}
          onclick={dispensar}
        >
          {t.dispensar}
        </button>
      </div>
    {/if}
  {:else if !cartao}
    <p>{instalado ? t.instalado : t.semOferta}</p>
  {/if}
{/snippet}

<div class="instalar">
  {#if cartao}
    {#if oferta !== 'nenhuma'}
      <Cartao variante="contorno" titulo={t.titulo} nivel={3}>
        <div class="corpo">{@render conteudo()}</div>
      </Cartao>
    {/if}
  {:else}
    <div class="corpo">{@render conteudo()}</div>
  {/if}
  <!-- Região sempre montada: leitores de tela só anunciam mudanças em uma região já existente. -->
  <p class="nota" role="status">{mensagem ?? ''}</p>
</div>

<style>
  .corpo {
    display: grid;
    gap: var(--esp-2);
    font: var(--fonte-corpo-m);
  }
  .nota:not(:empty) {
    margin-top: var(--esp-3);
  }
  .nota {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .acoes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-2);
  }
  .botao {
    display: inline-flex;
    align-items: center;
    gap: var(--esp-2);
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-primaria);
    border-radius: var(--raio-cheio);
    background: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .botao.secundario {
    border-color: var(--cor-contorno);
    background: transparent;
    color: var(--cor-primaria);
  }
</style>
