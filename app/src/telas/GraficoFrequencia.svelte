<script lang="ts">
  import { PARAMETROS_LOTERIA } from '../dados/tipos';
  import type { EstatisticasJanela } from '../estatisticas/estatisticas';
  import {
    extremosFrequencia,
    linhasTabelaFrequencia,
    montarGrafico,
    larguraDoGrafico,
    MAXIMO_EMPATADOS_MARCADOS,
  } from '../estatisticas/grafico';
  import { doisDigitos, formatarDecimal, formatarDiferenca } from '../lib/numeros';
  import { textos } from '../lib/textos';
  import { linhasDoResumoFrequencia, rotuloDoGrafico } from './estatisticas';

  interface Props {
    estatisticas: EstatisticasJanela;
  }

  let { estatisticas }: Props = $props();

  const t = textos.estatisticas.frequencia;

  // O viewBox tem o tamanho real do contêiner: o texto do SVG fica em px de verdade.
  // Antes da medição (e fora de um navegador) vale um tamanho de celular.
  let caixa: HTMLDivElement | undefined = $state();
  let larguraMedida = $state(0);
  const largura = $derived(larguraDoGrafico(larguraMedida));

  // Mede a caixa e acompanha mudanças (girar a tela, redimensionar a janela).
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

  const extremos = $derived(extremosFrequencia(estatisticas.frequencia));
  // Só recalcula quando muda a janela/loteria (estatisticas) ou a largura medida.
  const dados = $derived(
    montarGrafico({
      frequencia: estatisticas.frequencia,
      esperado: estatisticas.frequenciaEsperada,
      largura,
    }),
  );
  const esperadoTexto = $derived(formatarDecimal(estatisticas.frequenciaEsperada));
  const resumo = $derived(linhasDoResumoFrequencia(extremos));
  const rotulo = $derived(rotuloDoGrafico(estatisticas));
  const nomeLoteria = $derived(textos.loterias[estatisticas.slug]);
  const total = $derived(PARAMETROS_LOTERIA[estatisticas.slug].totalDezenas);
  const linhas = $derived(
    linhasTabelaFrequencia(estatisticas.frequencia, estatisticas.frequenciaEsperada),
  );
  const marcaMaior = $derived(
    !extremos.todasIguais && extremos.dezenasMaximo.length <= MAXIMO_EMPATADOS_MARCADOS,
  );
  const marcaMenor = $derived(
    !extremos.todasIguais && extremos.dezenasMinimo.length <= MAXIMO_EMPATADOS_MARCADOS,
  );
</script>

<div class="grafico" bind:this={caixa}>
  <svg
    viewBox="0 0 {dados.largura} {dados.altura}"
    role="img"
    aria-label={rotulo}
    data-total={total}
  >
    <g class="grade" aria-hidden="true">
      {#each dados.ticksY as tick (tick.valor)}
        <line x1={dados.esquerda} x2={dados.direita} y1={tick.y} y2={tick.y} />
        <text class="rotulo" x={dados.esquerda - 6} y={tick.y} text-anchor="end" dy="0.35em"
          >{tick.rotulo}</text
        >
      {/each}
    </g>

    <g class="barras" aria-hidden="true">
      {#each dados.barras as barra (barra.dezena)}
        {#if barra.altura > 0}
          <rect x={barra.x} y={barra.y} width={barra.largura} height={barra.altura} rx="0.75" />
        {/if}
      {/each}
    </g>

    <line
      class="eixo"
      x1={dados.esquerda}
      x2={dados.direita}
      y1={dados.base}
      y2={dados.base}
      aria-hidden="true"
    />

    <g class="eixo-x" aria-hidden="true">
      {#each dados.ticksX as tick (tick.dezena)}
        <line x1={tick.x} x2={tick.x} y1={dados.base} y2={dados.base + 4} />
        <text class="rotulo" x={tick.x} y={dados.base + 16} text-anchor="middle">{tick.rotulo}</text
        >
      {/each}
    </g>

    <!-- Linha do esperado: contorno claro por baixo para ler sobre as barras nos dois temas. -->
    <g aria-hidden="true">
      <line
        class="esperado-halo"
        x1={dados.esquerda}
        x2={dados.direita}
        y1={dados.esperadoY}
        y2={dados.esperadoY}
      />
      <line
        class="esperado"
        x1={dados.esquerda}
        x2={dados.direita}
        y1={dados.esperadoY}
        y2={dados.esperadoY}
      />
    </g>

    <g aria-hidden="true">
      {#each dados.barras as barra (barra.dezena)}
        {#if barra.marca !== null}
          {@const cx = barra.x + barra.largura / 2}
          {@const r = dados.raioMarca}
          {@const cy = barra.y - r - 3}
          {#if barra.marca === 'maior'}
            <path
              class="marca"
              d="M {cx} {cy - r} L {cx + r} {cy} L {cx} {cy + r} L {cx - r} {cy} Z"
            />
          {:else}
            <circle class="marca marca-vazada" {cx} {cy} r={r * 0.8} />
          {/if}
        {/if}
      {/each}
    </g>
  </svg>
</div>

<ul class="legenda">
  {#if resumo.iguais !== null}
    <li><span class="amostra" aria-hidden="true"></span><span>{resumo.iguais}</span></li>
  {/if}
  {#if resumo.maior !== null}
    <li>
      {#if marcaMaior}
        <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
          <path class="marca" d="M 8 2.5 L 13.5 8 L 8 13.5 L 2.5 8 Z" />
        </svg>
      {:else}
        <span class="amostra" aria-hidden="true"></span>
      {/if}
      <span>{resumo.maior}</span>
    </li>
  {/if}
  {#if resumo.menor !== null}
    <li>
      {#if marcaMenor}
        <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
          <circle class="marca marca-vazada" cx="8" cy="8" r="4.5" />
        </svg>
      {:else}
        <span class="amostra" aria-hidden="true"></span>
      {/if}
      <span>{resumo.menor}</span>
    </li>
  {/if}
  <li>
    <svg class="amostra amostra-linha" viewBox="0 0 16 16" aria-hidden="true">
      <line class="esperado-halo" x1="0" x2="16" y1="8" y2="8" />
      <line class="esperado" x1="0" x2="16" y1="8" y2="8" />
    </svg>
    <span>{t.legendaEsperado(esperadoTexto)}</span>
  </li>
</ul>

<p class="explicacao">{t.explicacao}</p>

<details class="tabela-alt">
  <summary>{t.tabelaResumo}</summary>
  <!-- Região rolável: precisa de foco para quem usa só o teclado. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="rolagem" role="region" aria-label={t.tabelaRegiao} tabindex="0">
    <table>
      <caption class="so-leitor">{t.tabelaLegenda(nomeLoteria, esperadoTexto)}</caption>
      <thead>
        <tr>
          <th scope="col">{t.colDezena}</th>
          <th scope="col">{t.colVezes}</th>
          <th scope="col">{t.colDiferenca}</th>
        </tr>
      </thead>
      <tbody>
        {#each linhas as linha (linha.dezena)}
          <tr>
            <th scope="row">{doisDigitos(linha.dezena)}</th>
            <td>{linha.vezes.toLocaleString('pt-BR')}</td>
            <td>{formatarDiferenca(linha.diferenca)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</details>

<style>
  .grafico {
    width: 100%;
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  .grade line {
    stroke: var(--cor-contorno-var);
    stroke-width: 1;
  }
  .rotulo {
    fill: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
    font-variant-numeric: tabular-nums;
  }
  .barras rect {
    fill: var(--cor-primaria);
  }
  .eixo {
    stroke: var(--cor-contorno);
    stroke-width: 1;
  }
  .eixo-x line {
    stroke: var(--cor-contorno);
    stroke-width: 1;
  }
  .esperado-halo {
    stroke: var(--cor-superficie);
    stroke-width: 4;
  }
  .esperado {
    stroke: var(--cor-texto);
    stroke-width: 1.75;
    stroke-dasharray: 6 4;
  }
  .marca {
    fill: var(--cor-texto);
    stroke: var(--cor-superficie);
    stroke-width: 1;
  }
  .marca-vazada {
    fill: var(--cor-superficie);
    stroke: var(--cor-texto);
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
    width: 1rem;
    height: 1rem;
  }
  .amostra-linha {
    width: 1.5rem;
  }
  .legenda .esperado-halo {
    stroke-width: 4;
  }
  .explicacao {
    margin: var(--esp-3) 0 0;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }

  .tabela-alt {
    margin-top: var(--esp-3);
    border-top: 1px solid var(--cor-contorno-var);
  }
  .tabela-alt summary {
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
