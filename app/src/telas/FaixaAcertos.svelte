<script lang="ts">
  import type { ConcursoHistorico, Slug } from '../dados/tipos';
  import { textos } from '../lib/textos';
  import { concursosComAcertos, type FaixaDeAcertos } from './conferir';

  interface Props {
    faixa: FaixaDeAcertos;
    /** Esta é a faixa do melhor resultado (marcada com texto, não só com cor). */
    melhor: boolean;
    loteria: Slug;
    jogo: readonly number[];
    concursos: readonly ConcursoHistorico[];
    /** Quantos concursos a lista mostra antes de "e mais N". */
    limite: number;
  }

  let { faixa, melhor, loteria, jogo, concursos, limite }: Props = $props();

  let aberta = $state(false);
  // A lista só é calculada com a faixa aberta (uma passada extra sobre o histórico).
  // O total da faixa já é conhecido (faixa.concursos); o excedente é total − mostrados.
  const lista = $derived(
    aberta && faixa.concursos > 0
      ? concursosComAcertos(loteria, jogo, concursos, faixa.acertos, limite)
      : [],
  );
  const restantes = $derived(Math.max(0, faixa.concursos - lista.length));
  const rotulo = $derived(
    `${textos.conferir.resultado.acertos(faixa.acertos)}: ${textos.conferir.resultado.concursos(faixa.concursos)}` +
      (melhor ? ` (${textos.conferir.resultado.etiquetaMelhor})` : ''),
  );
</script>

<details
  class="faixa"
  data-melhor={melhor}
  ontoggle={(e) => (aberta = (e.currentTarget as HTMLDetailsElement).open)}
>
  <summary aria-label={rotulo}>
    <span class="faixa-rotulo">
      {textos.conferir.resultado.acertos(faixa.acertos)}
      {#if melhor}
        <span class="etiqueta">{textos.conferir.resultado.etiquetaMelhor}</span>
      {/if}
    </span>
    <span class="trilho" aria-hidden="true">
      <span class="preenchimento" style:width="{faixa.proporcao * 100}%"></span>
    </span>
    <span class="faixa-valor">{textos.conferir.resultado.concursos(faixa.concursos)}</span>
    <span class="seta" aria-hidden="true"></span>
  </summary>
  {#if aberta}
    {#if faixa.concursos === 0}
      <p class="discreto">{textos.conferir.resultado.faixaSemConcursos}</p>
    {:else}
      <ul class="lista-faixa">
        {#each lista as concurso (concurso)}
          <li>{textos.conferir.resultado.concursoItem(concurso)}</li>
        {/each}
        {#if restantes > 0}
          <li class="mais">{textos.conferir.resultado.faixaMais(restantes)}</li>
        {/if}
      </ul>
    {/if}
  {/if}
</details>

<style>
  .faixa {
    font: var(--fonte-corpo-m);
    font-variant-numeric: tabular-nums;
  }
  summary {
    display: grid;
    grid-template-columns: minmax(6.5rem, auto) minmax(0, 1fr) minmax(5.5rem, auto) 1rem;
    align-items: center;
    gap: var(--esp-2);
    min-height: var(--alvo-minimo);
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .faixa-rotulo {
    display: flex;
    flex-direction: column;
  }
  .faixa-valor {
    text-align: end;
  }
  .faixa[data-melhor='true'] .faixa-rotulo,
  .faixa[data-melhor='true'] .faixa-valor {
    font-weight: 700;
  }
  .etiqueta {
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
    font-weight: 400;
  }
  .trilho {
    display: block;
    height: 0.75rem;
    border-radius: var(--raio-cheio);
    background: var(--cor-cont-altissimo);
    overflow: hidden;
  }
  .preenchimento {
    display: block;
    height: 100%;
    border-radius: var(--raio-cheio);
    background: var(--cor-primaria);
  }
  .faixa[data-melhor='true'] .trilho {
    outline: 2px solid var(--cor-primaria);
    outline-offset: 1px;
  }
  .seta {
    width: 0.5rem;
    height: 0.5rem;
    border-right: 2px solid var(--cor-texto-var);
    border-bottom: 2px solid var(--cor-texto-var);
    transform: rotate(45deg);
  }
  .faixa[open] .seta {
    transform: rotate(-135deg);
  }
  summary:focus-visible {
    outline: 2px solid var(--cor-primaria);
    outline-offset: 2px;
    border-radius: var(--raio-m);
  }
  .lista-faixa {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-1) var(--esp-3);
    margin: 0 0 var(--esp-2);
    padding: 0;
    list-style: none;
  }
  .mais,
  .discreto {
    color: var(--cor-texto-var);
  }
  .discreto {
    margin: 0 0 var(--esp-2);
  }
</style>
