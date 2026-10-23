<script lang="ts">
  import Aviso from '../componentes/Aviso.svelte';
  import { textos } from '../lib/textos';
  import type { DetalheDezena } from './detalhe-dezena';

  interface Props {
    detalhe: DetalheDezena;
  }

  let { detalhe }: Props = $props();
</script>

<div class="detalhe">
  <section class="destaques">
    <div class="indice">
      <p class="rotulo">{textos.detalhe.indice}</p>
      <p class="numero">{detalhe.indice}</p>
    </div>
    <div class="variacao" data-sentido={detalhe.variacao.sentido}>
      <p class="rotulo">{textos.detalhe.variacao}</p>
      <p class="numero">
        <span aria-hidden="true">{detalhe.variacao.sinal}</span>
        <span class="texto">{detalhe.variacao.texto}</span>
      </p>
    </div>
  </section>

  <section class="evidencia" aria-labelledby="det-evidencia">
    <h3 id="det-evidencia">{textos.detalhe.evidencia}</h3>
    {#if detalhe.evidencia}
      <p class="grau">
        <span class="segmentos" aria-hidden="true">
          {#each [1, 2, 3, 4, 5] as i (i)}
            <span class="segmento" data-aceso={i <= detalhe.evidencia.segmentos}></span>
          {/each}
        </span>
        <strong>{detalhe.evidencia.rotulo}</strong>
      </p>
      <p>{detalhe.evidencia.frase}</p>
      <p class="nota">{textos.detalhe.evidenciaNota}</p>
    {:else}
      <p>{textos.detalhe.semEvidencia}</p>
    {/if}
  </section>

  <section class="componentes" aria-labelledby="det-componentes">
    <h3 id="det-componentes">{textos.detalhe.componentes}</h3>
    <p class="nota">{textos.detalhe.componentesIntro}</p>
    <p class="janela">{detalhe.janela}</p>
    <dl>
      {#each detalhe.componentes as item (item.chave)}
        <div class="item" data-componente={item.chave} data-sem-dados={item.semDados}>
          <dt>{item.rotulo}</dt>
          <dd class="valor">{item.texto}</dd>
          <dd class="explicacao">
            {item.explicacao}
            {#if item.motivo}<span class="motivo">{item.motivo}</span>{/if}
          </dd>
        </div>
      {/each}
    </dl>
  </section>

  <Aviso>{textos.avisos.naoPreveDetalhe}</Aviso>
</div>

<style>
  .detalhe {
    display: grid;
    gap: var(--esp-4);
  }
  h3 {
    margin-bottom: var(--esp-2);
    font: var(--fonte-titulo-m);
  }
  p {
    font: var(--fonte-corpo-m);
  }
  .rotulo,
  .nota,
  .janela {
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .destaques {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: var(--esp-3);
  }
  .indice,
  .variacao {
    padding: var(--esp-3);
    border-radius: var(--raio-m);
    background: var(--cor-cont-altissimo);
  }
  .numero {
    margin-top: var(--esp-1);
    font: var(--fonte-titulo-g);
    font-variant-numeric: tabular-nums;
  }
  .variacao .texto {
    display: block;
    font: var(--fonte-corpo-m);
  }
  .variacao[data-sentido='subiu'] .numero {
    color: var(--cor-primaria);
  }
  .variacao[data-sentido='caiu'] .numero {
    color: var(--cor-erro);
  }
  .evidencia {
    display: grid;
    gap: var(--esp-2);
  }
  .evidencia h3 {
    margin-bottom: 0;
  }
  .grau {
    display: flex;
    align-items: center;
    gap: var(--esp-2);
  }
  .segmentos {
    display: inline-flex;
    gap: 2px;
    flex: none;
  }
  .segmento {
    width: 0.5rem;
    height: 1rem;
    border: 1px solid var(--cor-contorno);
    border-radius: 1px;
  }
  .segmento[data-aceso='true'] {
    background: var(--cor-texto-var);
    border-color: var(--cor-texto-var);
  }
  dl {
    margin: var(--esp-3) 0 0;
  }
  .item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    column-gap: var(--esp-3);
    row-gap: var(--esp-1);
    padding: var(--esp-3) 0;
    border-top: 1px solid var(--cor-contorno-var);
  }
  dt {
    font: var(--fonte-rotulo-g);
  }
  dd {
    margin: 0;
  }
  .valor {
    text-align: right;
    font: var(--fonte-titulo-m);
    font-variant-numeric: tabular-nums;
  }
  /* Sem dados: texto por extenso e itálico (nunca um zero). */
  .item[data-sem-dados='true'] .valor {
    font: var(--fonte-corpo-m);
    font-style: italic;
    color: var(--cor-texto-var);
  }
  .explicacao {
    grid-column: 1 / -1;
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .motivo {
    display: block;
  }
</style>
