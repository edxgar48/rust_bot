<script lang="ts">
  import type { Slug } from '../dados/tipos';
  import { formatarDataSorteio } from '../lib/datas';
  import { textos } from '../lib/textos';
  import { ALTURA_LINHA_REM, dezenasDoFiltroNoConcurso, listarDezenas } from './historico';

  interface Props {
    loteria: Slug;
    concurso: number;
    /** AAAA-MM-DD */
    data: string;
    dezenas: readonly number[];
    /** Dezenas do filtro: as que estão no concurso ganham destaque (anel e negrito). */
    filtro: readonly number[];
    /** Posição (a partir de 1) na lista e tamanho da lista, para leitor de tela. */
    posicao: number;
    total: number;
  }

  let { loteria, concurso, data, dezenas, filtro, posicao, total }: Props = $props();

  /** Disco simples (sem montar o componente BolaDezena): centenas de linhas na rolagem. */
  const rotuloDezena = (dezena: number): string => String(dezena).padStart(2, '0');

  const marcadas = $derived(dezenasDoFiltroNoConcurso(dezenas, filtro));
  const leitura = $derived(
    textos.historico.dezenasDoConcurso(
      listarDezenas(dezenas),
      filtro.length === 0 ? null : listarDezenas(marcadas),
    ),
  );
</script>

<!-- Altura fixa (rem) por loteria: a lista virtual calcula a janela sem medir cada linha. -->
<li
  class="linha"
  data-loteria={loteria}
  style:height="{ALTURA_LINHA_REM[loteria]}rem"
  aria-posinset={posicao}
  aria-setsize={total}
>
  <p class="cabeca">
    <strong>{textos.historico.concurso(concurso)}</strong>
    <span>{formatarDataSorteio(data)}</span>
  </p>
  <!-- As bolas são só visuais; o leitor de tela lê a frase abaixo. -->
  <div class="bolas" aria-hidden="true">
    {#each dezenas as dezena (dezena)}
      <span class="disco" data-destaque={marcadas.includes(dezena)}>{rotuloDezena(dezena)}</span>
    {/each}
  </div>
  <span class="so-leitor">{leitura}</span>
</li>

<style>
  .linha {
    position: relative;
    display: grid;
    align-content: center;
    gap: var(--esp-2);
    padding: 0 var(--esp-3);
    border-bottom: 1px solid var(--cor-contorno-var);
    overflow: hidden;
    /* Altura fixa vem do style (rem); a largura é a da lista, então size não a afeta. */
    contain: layout paint size;
  }
  .cabeca {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 0 var(--esp-3);
    height: 1.5rem;
    overflow: hidden;
    font: var(--fonte-corpo-g);
  }
  .cabeca strong {
    font-weight: 700;
  }
  .cabeca span {
    color: var(--cor-texto-var);
    font-variant-numeric: tabular-nums;
  }
  .bolas {
    display: grid;
    justify-content: space-between;
    row-gap: 0.375rem;
    max-width: 20rem;
  }
  /* Mesmo visual do BolaDezena (neutra e destaque). */
  .disco {
    --lado: 2.5rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    width: var(--lado);
    aspect-ratio: 1;
    border: 1px solid var(--cor-contorno-var);
    border-radius: var(--raio-cheio);
    background: var(--cor-cont-altissimo);
    color: var(--cor-texto);
    font: var(--fonte-rotulo-g);
    font-variant-numeric: tabular-nums;
  }
  .linha[data-loteria='lotofacil'] .disco {
    --lado: 1.75rem;
    font: var(--fonte-rotulo-m);
  }
  /* Destaque: anel duplo e negrito, além da cor. */
  .disco[data-destaque='true'] {
    background: var(--cor-primaria-cont);
    border-color: var(--cor-primaria);
    color: var(--cor-sobre-primaria-cont);
    box-shadow:
      0 0 0 2px var(--cor-superficie),
      0 0 0 4px var(--cor-primaria);
    font-weight: 700;
  }
  .linha[data-loteria='lotofacil'] .bolas {
    grid-template-columns: repeat(8, 1.75rem);
  }
  .linha[data-loteria='megasena'] .bolas {
    grid-template-columns: repeat(6, 2.5rem);
  }
  .linha[data-loteria='quina'] .bolas {
    grid-template-columns: repeat(5, 2.5rem);
  }
</style>
