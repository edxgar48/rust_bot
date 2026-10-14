<script lang="ts">
  import { textos } from '../lib/textos';
  import type { EstadoBola, TamanhoBola } from './tipos';

  interface Props {
    numero: number;
    tamanho?: TamanhoBola;
    estado?: EstadoBola;
    /** Faixa de posição 1..5 (colore a bola pelo ranking); `null` = sem faixa. */
    faixa?: number | null;
    /** Substitui o texto lido por leitor de tela. */
    rotulo?: string;
    /** Se informado, a bola vira botão (aria-pressed reflete `estado === 'selecionada'`). */
    aoAlternar?: (numero: number) => void;
    desabilitada?: boolean;
  }

  let {
    numero,
    tamanho = 'm',
    estado = 'neutra',
    faixa = null,
    rotulo,
    aoAlternar,
    desabilitada = false,
  }: Props = $props();

  const texto = $derived(String(numero).padStart(2, '0'));
  const sufixo = $derived(textos.bola.estados[estado]);
  // Botão: o estado vai em aria-pressed, então o rótulo é só a dezena.
  const rotuloBotao = $derived(rotulo ?? textos.bola.dezena(numero));
  const rotuloImagem = $derived(
    rotulo ?? [textos.bola.dezena(numero), sufixo].filter(Boolean).join(', '),
  );
</script>

{#snippet disco()}
  <span class="disco" data-estado={estado} data-faixa={faixa ?? undefined} aria-hidden="true">
    {texto}
    {#if estado === 'selecionada'}<span class="marca">✓</span>{/if}
  </span>
{/snippet}

{#if aoAlternar}
  <button
    type="button"
    class="bola"
    data-tamanho={tamanho}
    aria-label={rotuloBotao}
    aria-pressed={estado === 'selecionada'}
    disabled={desabilitada}
    onclick={() => aoAlternar(numero)}
  >
    {@render disco()}
  </button>
{:else}
  <span class="bola" data-tamanho={tamanho} role="img" aria-label={rotuloImagem}>
    {@render disco()}
  </span>
{/if}

<style>
  .bola {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    --lado: 2.5rem;
  }
  .bola[data-tamanho='p'] {
    --lado: 1.75rem;
  }
  .bola[data-tamanho='g'] {
    --lado: 3.5rem;
  }
  .bola[data-tamanho='celula'] {
    width: 100%;
    --lado: min(calc(100% - 0.25rem), 3rem);
  }
  button.bola {
    min-width: var(--alvo-minimo);
    min-height: var(--alvo-minimo);
    cursor: pointer;
  }
  button.bola[data-tamanho='celula'] {
    min-width: 0;
  }
  button.bola:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .disco {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--lado);
    aspect-ratio: 1;
    border-radius: var(--raio-cheio);
    border: 1px solid var(--cor-contorno-var);
    background: var(--cor-cont-altissimo);
    color: var(--cor-texto);
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
  }
  .bola[data-tamanho='p'] .disco {
    font: var(--fonte-rotulo-m);
  }
  .bola[data-tamanho='g'] .disco {
    font: var(--fonte-titulo-m);
  }
  .bola[data-tamanho='celula'] .disco {
    font-size: 0.8125rem;
  }
  button.bola .disco[data-estado='neutra'] {
    border-color: var(--cor-contorno);
  }

  .disco[data-estado='sorteada'] {
    background: var(--cor-primaria);
    border-color: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
  }
  /* Destaque: anel duplo, além da cor. */
  .disco[data-estado='destaque'] {
    background: var(--cor-primaria-cont);
    border-color: var(--cor-primaria);
    color: var(--cor-sobre-primaria-cont);
    box-shadow:
      0 0 0 2px var(--cor-superficie),
      0 0 0 4px var(--cor-primaria);
    font-weight: 700;
  }
  /* Selecionada: borda grossa e marca de verificação, além da cor. */
  .disco[data-estado='selecionada'] {
    background: var(--cor-secundaria-cont);
    border: 3px solid var(--cor-primaria);
    color: var(--cor-sobre-secundaria-cont);
    font-weight: 700;
  }
  .marca {
    position: absolute;
    top: -0.4rem;
    right: -0.3rem;
    display: grid;
    place-items: center;
    width: 0.875rem;
    height: 0.875rem;
    border-radius: var(--raio-cheio);
    background: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
    font-size: 0.625rem;
    line-height: 1;
  }

  /* Faixas do volante: cor e padrão de borda (1 forte ... 5 pontilhada). */
  .disco[data-faixa] {
    border-width: 2px;
    border-color: var(--cor-primaria);
  }
  .disco[data-faixa='1'] {
    background: var(--cor-faixa-1);
    color: var(--cor-sobre-faixa-1);
    border-width: 3px;
    font-weight: 700;
  }
  .disco[data-faixa='2'] {
    background: var(--cor-faixa-2);
    color: var(--cor-sobre-faixa-2);
  }
  .disco[data-faixa='3'] {
    background: var(--cor-faixa-3);
    color: var(--cor-sobre-faixa-3);
    border-width: 1px;
  }
  .disco[data-faixa='4'] {
    background: var(--cor-faixa-4);
    color: var(--cor-sobre-faixa-4);
    border-width: 1px;
    border-style: dashed;
  }
  .disco[data-faixa='5'] {
    background: var(--cor-faixa-5);
    color: var(--cor-sobre-faixa-5);
    border-width: 1px;
    border-style: dotted;
    border-color: var(--cor-contorno);
  }
</style>
