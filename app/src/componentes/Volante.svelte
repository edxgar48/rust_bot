<script lang="ts">
  import type { Slug } from '../dados/tipos';
  import { textos } from '../lib/textos';
  import BolaDezena from './BolaDezena.svelte';
  import {
    NUMERO_FAIXAS,
    dezenasDoVolante,
    faixaPorPosicao,
    layoutVolante,
    limitesDaFaixa,
  } from './volante';

  interface Props {
    loteria: Slug;
    modo: 'leitura' | 'selecao';
    /** Leitura: posição no ranking de cada dezena (vem de tabela_geral.json). */
    posicoes?: Readonly<Record<number, number>>;
    /** Leitura: dezenas com anel de destaque. */
    destacadas?: readonly number[];
    /** Seleção: dezenas marcadas. */
    selecionadas?: readonly number[];
    /** Seleção: chamado ao tocar numa dezena. */
    aoAlternar?: (numero: number) => void;
    /** Seleção: bloqueia dezenas ainda não marcadas (limite atingido). */
    bloquearNovas?: boolean;
  }

  let {
    loteria,
    modo,
    posicoes,
    destacadas = [],
    selecionadas = [],
    aoAlternar,
    bloquearNovas = false,
  }: Props = $props();

  const layout = $derived(layoutVolante(loteria));
  const dezenas = $derived(dezenasDoVolante(loteria));
  const nome = $derived(textos.loterias[loteria]);
  const marcadas = $derived(new Set(selecionadas));
  const emDestaque = $derived(new Set(destacadas));
  const faixas = $derived(Array.from({ length: NUMERO_FAIXAS }, (_, i) => i + 1));

  function faixaDe(numero: number): number | null {
    const posicao = posicoes?.[numero];
    return posicao === undefined ? null : faixaPorPosicao(posicao, layout.total);
  }

  function rotuloLeitura(numero: number): string {
    const posicao = posicoes?.[numero];
    const faixa = faixaDe(numero);
    const base =
      posicao === undefined || faixa === null
        ? textos.volante.dezenaSemPosicao(numero)
        : textos.volante.dezenaNaFaixa(numero, posicao, faixa, NUMERO_FAIXAS);
    return emDestaque.has(numero) ? `${base}, ${textos.volante.emDestaque}` : base;
  }
</script>

<div class="volante">
  {#if modo === 'selecao'}
    <div
      class="grade"
      role="group"
      aria-label={textos.volante.grupo(nome)}
      style:--colunas={layout.colunas}
    >
      {#each dezenas as numero (numero)}
        {@const marcada = marcadas.has(numero)}
        <BolaDezena
          {numero}
          tamanho="celula"
          estado={marcada ? 'selecionada' : 'neutra'}
          desabilitada={bloquearNovas && !marcada}
          {aoAlternar}
        />
      {/each}
    </div>
  {:else}
    <div
      class="grade"
      role="list"
      aria-label={textos.volante.grupo(nome)}
      style:--colunas={layout.colunas}
    >
      {#each dezenas as numero (numero)}
        <div role="listitem" class="item">
          <BolaDezena
            {numero}
            tamanho="celula"
            estado={emDestaque.has(numero) ? 'destaque' : 'neutra'}
            faixa={faixaDe(numero)}
            rotulo={rotuloLeitura(numero)}
          />
        </div>
      {/each}
    </div>
    {#if posicoes}
      <ul class="legenda" aria-label={textos.volante.legenda}>
        {#each faixas as faixa (faixa)}
          {@const limites = limitesDaFaixa(faixa, layout.total)}
          <li>
            <span class="amostra" data-faixa={faixa} aria-hidden="true">{faixa}</span>
            <span>{textos.volante.faixa(faixa, limites.de, limites.ate)}</span>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .grade {
    display: grid;
    grid-template-columns: repeat(var(--colunas), minmax(0, 1fr));
    /* Sem vão entre colunas: com 10 colunas em 360 px cada alvo ganha ~33 px de largura. */
    column-gap: 0;
    row-gap: var(--esp-1);
    justify-items: center;
  }
  .item {
    display: flex;
    justify-content: center;
    width: 100%;
    /* Respiro para o anel de destaque não encostar na vizinha. */
    padding: 0.125rem 0;
  }
  .legenda {
    display: grid;
    gap: var(--esp-1);
    margin: var(--esp-3) 0 0;
    padding: 0;
    list-style: none;
    font: var(--fonte-rotulo-m);
    color: var(--cor-texto-var);
  }
  .legenda li {
    display: flex;
    align-items: center;
    gap: var(--esp-2);
  }
  .amostra {
    display: inline-grid;
    place-items: center;
    width: 1.5rem;
    aspect-ratio: 1;
    border-radius: var(--raio-cheio);
    border: 2px solid var(--cor-primaria);
    font: var(--fonte-rotulo-m);
  }
  .amostra[data-faixa='1'] {
    background: var(--cor-faixa-1);
    color: var(--cor-sobre-faixa-1);
    border-width: 3px;
  }
  .amostra[data-faixa='2'] {
    background: var(--cor-faixa-2);
    color: var(--cor-sobre-faixa-2);
  }
  .amostra[data-faixa='3'] {
    background: var(--cor-faixa-3);
    color: var(--cor-sobre-faixa-3);
    border-width: 1px;
  }
  .amostra[data-faixa='4'] {
    background: var(--cor-faixa-4);
    color: var(--cor-sobre-faixa-4);
    border-width: 1px;
    border-style: dashed;
  }
  .amostra[data-faixa='5'] {
    background: var(--cor-faixa-5);
    color: var(--cor-sobre-faixa-5);
    border-width: 1px;
    border-style: dotted;
    border-color: var(--cor-contorno);
  }
</style>
