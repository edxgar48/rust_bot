<script lang="ts">
  import { montarReguaSoma } from '../estatisticas/grafico-soma';
  import { larguraDoGrafico } from '../estatisticas/grafico';
  import { textos } from '../lib/textos';
  import type { BlocoSoma } from './estatisticas';

  interface Props {
    bloco: BlocoSoma;
  }

  let { bloco }: Props = $props();

  const t = textos.estatisticas.soma.acaso;
  // Id próprio por instância: a hachura é referenciada por url(#id).
  const idInstancia = $props.id();
  const idHachura = `hachura-${idInstancia}`;

  // Mesma medição dos outros gráficos: viewBox em px reais, texto sem escalar.
  let caixa: HTMLDivElement | undefined = $state();
  let larguraMedida = $state(0);
  const largura = $derived(larguraDoGrafico(larguraMedida));

  $effect(() => {
    const elemento = caixa;
    if (!elemento) return;
    const medir = (): void => {
      larguraMedida = Math.round(elemento.clientWidth);
    };
    medir();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', medir);
      return () => window.removeEventListener('resize', medir);
    }
    const observador = new ResizeObserver(medir);
    observador.observe(elemento);
    return () => observador.disconnect();
  });

  const dados = $derived(
    montarReguaSoma(
      bloco.acaso,
      { media: bloco.media, minimo: bloco.minimo, maximo: bloco.maximo },
      largura,
    ),
  );
  const raio = 5;
</script>

<div class="comparacao" data-comparacao-soma>
  <p class="eixo-texto">{t.eixo}</p>
  <div class="caixa" bind:this={caixa}>
    <svg viewBox="0 0 {dados.largura} {dados.altura}" role="img" aria-label={bloco.rotulo}>
      <defs>
        <pattern id={idHachura} width="6" height="6" patternUnits="userSpaceOnUse">
          <path d="M -1 7 L 7 -1" class="hachura" />
        </pattern>
      </defs>
      <g aria-hidden="true">
        <line class="trilho" x1="0" x2={dados.largura} y1={dados.yMarcas} y2={dados.yMarcas} />
        <g class="faixa" data-faixa-soma>
          <rect
            x={dados.faixaX}
            y={dados.yFaixaTopo}
            width={dados.faixaLargura}
            height={dados.yFaixaBase - dados.yFaixaTopo}
            fill="url(#{idHachura})"
          />
          <line x1={dados.faixaX} x2={dados.faixaX} y1={dados.yFaixaTopo} y2={dados.yFaixaBase} />
          <line
            x1={dados.faixaX + dados.faixaLargura}
            x2={dados.faixaX + dados.faixaLargura}
            y1={dados.yFaixaTopo}
            y2={dados.yFaixaBase}
          />
        </g>
        <line
          class="halo-esperado"
          x1={dados.esperadoX}
          x2={dados.esperadoX}
          y1={dados.yFaixaTopo - 4}
          y2={dados.yFaixaBase + 4}
        />
        <line
          class="linha-esperada"
          x1={dados.esperadoX}
          x2={dados.esperadoX}
          y1={dados.yFaixaTopo - 4}
          y2={dados.yFaixaBase + 4}
        />
        <circle class="extremo" cx={dados.minimoX} cy={dados.yMarcas} r={raio - 0.5} />
        <circle class="extremo" cx={dados.maximoX} cy={dados.yMarcas} r={raio - 0.5} />
        <path
          class="media"
          d="M {dados.mediaX} {dados.yMarcas - raio - 1} L {dados.mediaX +
            raio +
            1} {dados.yMarcas} L {dados.mediaX} {dados.yMarcas + raio + 1} L {dados.mediaX -
            raio -
            1} {dados.yMarcas} Z"
        />
        {#each dados.ticks as tick (tick.x)}
          <text class="rotulo" x={tick.x} y={dados.yRotulos} text-anchor={tick.ancora}
            >{tick.rotulo}</text
          >
        {/each}
      </g>
    </svg>
  </div>

  <ul class="legenda">
    <li>
      <svg class="amostra" viewBox="0 0 24 16" aria-hidden="true">
        <g class="faixa">
          <rect x="0" y="3" width="24" height="10" fill="url(#{idHachura})" />
          <line x1="0" x2="0" y1="3" y2="13" />
          <line x1="24" x2="24" y1="3" y2="13" />
        </g>
      </svg>
      <span>{bloco.legendaFaixa}</span>
    </li>
    <li>
      <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
        <line class="halo-esperado" x1="8" x2="8" y1="0" y2="16" />
        <line class="linha-esperada" x1="8" x2="8" y1="0" y2="16" />
      </svg>
      <span>{bloco.legendaEsperado}</span>
    </li>
    <li>
      <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
        <path class="media" d="M 8 2 L 14 8 L 8 14 L 2 8 Z" />
      </svg>
      <span>{bloco.legendaMedia}</span>
    </li>
    <li>
      <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
        <circle class="extremo" cx="8" cy="8" r="4.5" />
      </svg>
      <span>{bloco.legendaExtremos}</span>
    </li>
  </ul>
  <p class="nota" data-soma-dentro>{bloco.textoDentro}</p>
  <p class="nota">{bloco.explicacao}</p>

  <details class="tabela-comparacao">
    <summary>{t.tabelaResumo}</summary>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="rolagem" role="region" aria-label={t.tabelaRegiao} tabindex="0">
      <table>
        <caption class="so-leitor">{bloco.legendaTabela}</caption>
        <thead>
          <tr>
            <th scope="col">{t.colMedida}</th>
            <th scope="col">{t.colValor}</th>
            <th scope="col">{t.colSituacao}</th>
          </tr>
        </thead>
        <tbody>
          {#each bloco.tabela as linha (linha.medida)}
            <tr>
              <th scope="row">{linha.medida}</th>
              <td>{linha.valor}</td>
              <td>{linha.situacao}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </details>
</div>

<style>
  .comparacao {
    margin-top: var(--esp-3);
    padding-top: var(--esp-3);
    border-top: 1px solid var(--cor-contorno-var);
  }
  .eixo-texto {
    margin: 0 0 var(--esp-1);
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .caixa {
    width: 100%;
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  .trilho {
    stroke: var(--cor-contorno);
    stroke-width: 1;
  }
  .rotulo {
    fill: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
    font-variant-numeric: tabular-nums;
  }
  .hachura {
    stroke: var(--cor-texto-var);
    stroke-width: 1;
    opacity: 0.4;
  }
  .faixa line {
    stroke: var(--cor-texto-var);
    stroke-width: 1.25;
    stroke-dasharray: 2 2;
  }
  .halo-esperado {
    stroke: var(--cor-superficie);
    stroke-width: 4;
  }
  .linha-esperada {
    stroke: var(--cor-texto);
    stroke-width: 1.75;
    stroke-dasharray: 5 3;
  }
  .media {
    fill: var(--cor-primaria);
    stroke: var(--cor-superficie);
    stroke-width: 1.5;
  }
  .extremo {
    fill: var(--cor-superficie);
    stroke: var(--cor-primaria);
    stroke-width: 2;
  }

  .legenda {
    display: grid;
    gap: var(--esp-2);
    margin: var(--esp-3) 0 0;
    padding: 0;
    list-style: none;
    font: var(--fonte-corpo-m);
  }
  .legenda li {
    display: flex;
    align-items: center;
    gap: var(--esp-2);
  }
  .amostra {
    flex: none;
    width: 1.5rem;
    height: 1rem;
  }
  .nota {
    margin: var(--esp-3) 0 0;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }

  .tabela-comparacao {
    margin-top: var(--esp-3);
    border-top: 1px solid var(--cor-contorno-var);
  }
  .tabela-comparacao summary {
    display: flex;
    align-items: center;
    min-height: var(--alvo-minimo);
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .rolagem {
    max-height: 22rem;
    overflow: auto;
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-p);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font: var(--fonte-corpo-m);
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    padding: var(--esp-2) var(--esp-3);
    text-align: right;
    border-bottom: 1px solid var(--cor-contorno-var);
  }
  th:first-child {
    text-align: left;
  }
  thead th {
    position: sticky;
    top: 0;
    background: var(--cor-cont);
    font: var(--fonte-rotulo-g);
  }
  tbody th {
    font: var(--fonte-rotulo-g);
  }
</style>
