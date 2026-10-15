<script lang="ts">
  import { montarMiniBarras, ALTURA_BARRA } from '../estatisticas/grafico-comparacao';
  import { larguraDoGrafico } from '../estatisticas/grafico';
  import { formatarDecimal, formatarPercentual } from '../lib/numeros';
  import { textos } from '../lib/textos';
  import type { BlocoComparacao } from './estatisticas';

  interface Props {
    bloco: BlocoComparacao;
  }

  let { bloco }: Props = $props();

  const t = textos.estatisticas.comparacao;
  // Id próprio por instância: a hachura é referenciada por url(#id) e há dois destes na tela.
  const idInstancia = $props.id();
  const idHachura = `hachura-${idInstancia}`;

  // Mesma medição do gráfico de frequência: viewBox em px reais, texto sem escalar.
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

  const dados = $derived(montarMiniBarras(bloco.visiveis, largura));
</script>

<div class="comparacao" data-comparacao={bloco.tipo}>
  <p class="eixo-texto">{t.eixo(bloco.nome)}</p>
  <div class="caixa" bind:this={caixa}>
    <svg viewBox="0 0 {dados.largura} {dados.altura}" role="img" aria-label={bloco.rotulo}>
      <defs>
        <pattern id={idHachura} width="5" height="5" patternUnits="userSpaceOnUse">
          <path d="M -1 6 L 6 -1" class="hachura" />
        </pattern>
      </defs>
      <g aria-hidden="true">
        <line class="base" x1={dados.xBarras} x2={dados.xBarras} y1="0" y2={dados.altura} />
        {#each dados.linhas as linha (linha.valor)}
          <text
            class="rotulo"
            x={dados.xBarras - 8}
            y={linha.yObservada + ALTURA_BARRA + 1}
            text-anchor="end">{linha.rotulo}</text
          >
          {#if linha.larguraObservada > 0}
            <rect
              class="observada"
              x={dados.xBarras}
              y={linha.yObservada}
              width={linha.larguraObservada}
              height={ALTURA_BARRA}
            />
          {/if}
          <text class="valor" x={linha.xTextoObservado} y={linha.yObservada + ALTURA_BARRA - 1}
            >{linha.textoObservado}</text
          >
          {#if linha.larguraEsperada > 0}
            <rect
              class="esperada"
              x={dados.xBarras + 0.75}
              y={linha.yEsperada + 0.75}
              width={Math.max(linha.larguraEsperada - 1.5, 0.5)}
              height={ALTURA_BARRA - 1.5}
              fill="url(#{idHachura})"
            />
          {/if}
          <text class="valor" x={linha.xTextoEsperado} y={linha.yEsperada + ALTURA_BARRA - 1}
            >{linha.textoEsperado}</text
          >
        {/each}
      </g>
    </svg>
  </div>

  <ul class="legenda">
    <li>
      <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
        <rect class="observada" x="1" y="3" width="14" height="10" />
      </svg>
      <span>{t.legendaObservado(bloco.comparacao.sorteios)}</span>
    </li>
    <li>
      <svg class="amostra" viewBox="0 0 16 16" aria-hidden="true">
        <rect
          class="esperada"
          x="1.75"
          y="3.75"
          width="12.5"
          height="8.5"
          fill="url(#{idHachura})"
        />
      </svg>
      <span>{t.legendaEsperado}</span>
    </li>
  </ul>
  <p class="nota">{t.explicacao}</p>
  {#if bloco.temRaros}<p class="nota" data-raros>{t.valoresRaros}</p>{/if}

  <details class="tabela-comparacao">
    <summary>{t.tabelaResumo}</summary>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="rolagem" role="region" aria-label={t.tabelaRegiao(bloco.nome)} tabindex="0">
      <table>
        <caption class="so-leitor">{t.tabelaLegenda(bloco.nome, bloco.comparacao.sorteios)}</caption
        >
        <thead>
          <tr>
            <th scope="col">{t.colValor(bloco.nome)}</th>
            <th scope="col">{t.colObservado}</th>
            <th scope="col">{t.colObservadoPct}</th>
            <th scope="col">{t.colEsperado}</th>
            <th scope="col">{t.colEsperadoPct}</th>
          </tr>
        </thead>
        <tbody>
          {#each bloco.comparacao.linhas as linha (linha.valor)}
            <tr>
              <th scope="row">{linha.valor}</th>
              <td>{linha.observado.toLocaleString('pt-BR')}</td>
              <td>{formatarPercentual(linha.fracaoObservada)}</td>
              <td>{formatarDecimal(linha.esperado)}</td>
              <td>{formatarPercentual(linha.fracaoEsperada)}</td>
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
  .base {
    stroke: var(--cor-contorno);
    stroke-width: 1;
  }
  .rotulo {
    fill: var(--cor-texto);
    font: var(--fonte-rotulo-m);
    font-variant-numeric: tabular-nums;
  }
  .valor {
    fill: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
    font-variant-numeric: tabular-nums;
  }
  .observada {
    fill: var(--cor-primaria);
  }
  .esperada {
    stroke: var(--cor-texto);
    stroke-width: 1.5;
  }
  .hachura {
    stroke: var(--cor-texto-var);
    stroke-width: 1;
    opacity: 0.55;
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
