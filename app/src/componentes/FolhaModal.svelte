<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { textos } from '../lib/textos';
  import { SELETOR_FOCAVEIS, destinoDoTab } from './folha';

  interface Props {
    titulo: string;
    /** Chamado por Esc, pelo botão Fechar e por toque no fundo. */
    aoFechar: () => void;
    children: Snippet;
  }

  let { titulo, aoFechar, children }: Props = $props();
  const idTitulo = $props.id();

  let folha: HTMLElement | undefined = $state();
  let botaoFechar: HTMLButtonElement | undefined = $state();

  onMount(() => {
    // O foco entra na folha; a página de trás não rola enquanto ela está aberta.
    botaoFechar?.focus();
    const anterior = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = anterior;
    };
  });

  function aoTeclar(evento: KeyboardEvent): void {
    if (evento.key === 'Escape') {
      evento.preventDefault();
      aoFechar();
      return;
    }
    if (evento.key !== 'Tab' || !folha) return;
    const focaveis = Array.from(folha.querySelectorAll<HTMLElement>(SELETOR_FOCAVEIS));
    const destino = destinoDoTab(
      focaveis,
      document.activeElement as HTMLElement | null,
      evento.shiftKey,
    );
    if (destino) {
      evento.preventDefault();
      destino.focus();
    }
  }
</script>

<!-- Fundo: toque fora fecha (Esc e o botão Fechar são os caminhos acessíveis). -->
<div
  class="fundo"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) aoFechar();
  }}
>
  <div
    bind:this={folha}
    class="folha"
    role="dialog"
    aria-modal="true"
    aria-labelledby={idTitulo}
    tabindex="-1"
    onkeydown={aoTeclar}
  >
    <header class="cabeca">
      <h2 id={idTitulo}>{titulo}</h2>
      <button bind:this={botaoFechar} type="button" class="fechar" onclick={aoFechar}>
        {textos.detalhe.fechar}
      </button>
    </header>
    <div class="corpo">
      {@render children()}
    </div>
  </div>
</div>

<style>
  .fundo {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    background: rgb(0 0 0 / 0.5);
  }
  .folha {
    display: flex;
    flex-direction: column;
    width: 100%;
    max-width: 36rem;
    max-height: 88dvh;
    border-radius: var(--raio-gg) var(--raio-gg) 0 0;
    background: var(--cor-cont-baixo);
    color: var(--cor-texto);
    box-shadow: var(--elev-3);
  }
  .folha:focus {
    outline: none;
  }
  .cabeca {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--esp-3);
    padding: var(--esp-3) var(--esp-3) var(--esp-3) var(--esp-4);
    border-bottom: 1px solid var(--cor-contorno-var);
  }
  h2 {
    font: var(--fonte-titulo-m);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .fechar {
    flex: none;
    min-width: var(--alvo-minimo);
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .corpo {
    padding: var(--esp-4);
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  /* Tela larga: janela centralizada em vez de folha colada embaixo. */
  @media (min-width: 40rem) {
    .fundo {
      align-items: center;
    }
    .folha {
      border-radius: var(--raio-gg);
      max-height: 90dvh;
    }
  }
</style>
