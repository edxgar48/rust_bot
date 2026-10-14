<script lang="ts">
  // Catálogo de componentes (ferramenta de desenvolvimento, acessível por #catalogo).
  // Não é tela do app: os textos daqui ficam locais, fora de src/lib/textos.ts, e o
  // módulo só entra no build com VITE_CATALOGO=1 (ver App.svelte).
  import Aviso from '../componentes/Aviso.svelte';
  import BarraNavegacao from '../componentes/BarraNavegacao.svelte';
  import BolaDezena from '../componentes/BolaDezena.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Chip from '../componentes/Chip.svelte';
  import LinhaRanking from '../componentes/LinhaRanking.svelte';
  import Segmentado from '../componentes/Segmentado.svelte';
  import SeletorTema from '../componentes/SeletorTema.svelte';
  import Volante from '../componentes/Volante.svelte';
  import { dezenasDoVolante } from '../componentes/volante';
  import type { AbaNavegacao, EstadoBola, OpcaoSegmentada } from '../componentes/tipos';
  import { SLUGS, type Slug } from '../dados/tipos';
  import { idsAbas, textos, type IdAba } from '../lib/textos';

  const estados: EstadoBola[] = ['neutra', 'sorteada', 'destaque', 'selecionada'];
  const tamanhos = ['p', 'm', 'g'] as const;

  let loteria = $state<Slug>('lotofacil');
  const opcoesLoteria: OpcaoSegmentada<Slug>[] = SLUGS.map((s) => ({
    valor: s,
    rotulo: textos.loterias[s],
  }));

  // Posições de exemplo: permutação fixa de 1..N (7 é coprimo com 25, 60 e 80).
  const posicoes = $derived.by(() => {
    const dezenas = dezenasDoVolante(loteria);
    const total = dezenas.length;
    return Object.fromEntries(dezenas.map((d) => [d, (((d - 1) * 7) % total) + 1]));
  });
  let selecionadas = $state<number[]>([3, 12, 25]);
  function alternar(n: number): void {
    selecionadas = selecionadas.includes(n)
      ? selecionadas.filter((x) => x !== n)
      : [...selecionadas, n];
  }

  let chipA = $state(true);
  let chipB = $state(false);
  let janela = $state<'10' | '50' | '100'>('50');
  const opcoesJanela: OpcaoSegmentada<'10' | '50' | '100'>[] = [
    { valor: '10', rotulo: '10' },
    { valor: '50', rotulo: '50' },
    { valor: '100', rotulo: '100' },
  ];
  let aba = $state<IdAba>('forca');
  const icones = {
    inicio: 'inicio',
    forca: 'forca',
    conferir: 'conferir',
    estatisticas: 'estatisticas',
    entenda: 'entenda',
  } as const;
  const abas: AbaNavegacao<IdAba>[] = idsAbas.map((id) => ({
    id,
    rotulo: textos.abas[id],
    icone: icones[id],
  }));
  const temas = ['claro', 'escuro'] as const;
</script>

{#snippet exemplos()}
  <div class="bloco">
    <h3>BolaDezena: estados</h3>
    <div class="linha">
      {#each estados as estado (estado)}
        <div class="rotulado">
          <BolaDezena numero={7} {estado} />
          <span>{estado}</span>
        </div>
      {/each}
    </div>
    <h3>BolaDezena: tamanhos (p, m, g)</h3>
    <div class="linha">
      {#each tamanhos as tamanho (tamanho)}
        <BolaDezena numero={42} {tamanho} estado="sorteada" />
      {/each}
    </div>
    <h3>BolaDezena: botão (aria-pressed)</h3>
    <div class="linha">
      <BolaDezena
        numero={9}
        estado={selecionadas.includes(9) ? 'selecionada' : 'neutra'}
        aoAlternar={alternar}
      />
      <BolaDezena numero={10} desabilitada aoAlternar={alternar} />
    </div>
  </div>

  <div class="bloco">
    <h3>Volante: leitura por faixa ({textos.loterias[loteria]})</h3>
    <Volante {loteria} modo="leitura" {posicoes} destacadas={[1, 2]} />
  </div>

  <div class="bloco">
    <h3>Volante: seleção</h3>
    <Volante {loteria} modo="selecao" {selecionadas} aoAlternar={alternar} />
    <p class="nota">{selecionadas.length} marcadas</p>
  </div>

  <div class="bloco">
    <h3>Cartão</h3>
    <div class="coluna">
      <Cartao variante="elevado" titulo="Elevado">Conteúdo do cartão elevado.</Cartao>
      <Cartao variante="preenchido" titulo="Preenchido">Conteúdo do cartão preenchido.</Cartao>
      <Cartao variante="contorno" titulo="Contorno">Conteúdo do cartão com contorno.</Cartao>
    </div>
  </div>

  <div class="bloco">
    <h3>Aviso</h3>
    <div class="coluna">
      <Aviso>Atualizado em 14/10/2026 09:30.</Aviso>
      <Aviso tipo="erro">Dado recusado (falha de conferência), mantendo os anteriores.</Aviso>
    </div>
  </div>

  <div class="bloco">
    <h3>Chip (alternância) e Segmentado (seleção única)</h3>
    <div class="linha">
      <Chip rotulo="Com viés" selecionado={chipA} aoAlternar={() => (chipA = !chipA)} />
      <Chip rotulo="Ao acaso" selecionado={chipB} aoAlternar={() => (chipB = !chipB)} />
      <Chip rotulo="Desabilitado" desabilitado />
    </div>
    <Segmentado opcoes={opcoesJanela} bind:valor={janela} rotuloGrupo="Janela de concursos" />
    <p class="nota">Janela: {janela}</p>
  </div>

  <div class="bloco">
    <h3>LinhaRanking</h3>
    <ol class="lista">
      <LinhaRanking
        posicao={1}
        numero={43}
        variacao={0}
        indice={4.063717154}
        grau="muito_fraca"
        direcao="acaso"
      />
      <LinhaRanking
        posicao={2}
        numero={48}
        variacao={3}
        indice={2.865651768}
        grau="forte"
        direcao="vies"
      />
      <LinhaRanking
        posicao={3}
        numero={7}
        variacao={-2}
        indice={-0.4}
        grau="muito_forte"
        direcao="vies"
      />
      <LinhaRanking posicao={4} numero={19} variacao={null} indice={0} grau={null} direcao={null} />
    </ol>
  </div>

  <div class="bloco">
    <h3>BarraNavegacao (aba: {textos.abas[aba]})</h3>
    <BarraNavegacao
      {abas}
      atual={aba}
      rotulo={textos.navegacao}
      aoSelecionar={(id) => (aba = id)}
    />
  </div>
{/snippet}

<header class="topo">
  <h1>Catálogo de componentes</h1>
  <SeletorTema />
</header>
<div class="controle">
  <Segmentado opcoes={opcoesLoteria} bind:valor={loteria} rotuloGrupo="Loteria do volante" />
</div>

<div class="paineis">
  {#each temas as tema (tema)}
    <section class="painel" data-tema={tema} aria-label={`Tema ${tema}`}>
      <h2>Tema {tema}</h2>
      {@render exemplos()}
    </section>
  {/each}
</div>

<style>
  .topo {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--esp-2) var(--esp-2) var(--esp-2) var(--esp-4);
    background: var(--cor-cont);
    color: var(--cor-texto);
  }
  h1 {
    font: var(--fonte-titulo-g);
  }
  .controle {
    padding: var(--esp-3) var(--esp-4);
  }
  .paineis {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  @media (min-width: 64rem) {
    .paineis {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .painel {
    padding: var(--esp-4);
    background: var(--cor-superficie);
    color: var(--cor-texto);
    min-width: 0;
  }
  h2 {
    margin-bottom: var(--esp-3);
    font: var(--fonte-titulo-g);
  }
  h3 {
    margin: var(--esp-4) 0 var(--esp-2);
    font: var(--fonte-titulo-p);
    color: var(--cor-texto-var);
  }
  .bloco {
    margin-bottom: var(--esp-6);
  }
  .linha {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--esp-3);
  }
  .coluna {
    display: grid;
    gap: var(--esp-3);
  }
  .rotulado {
    display: grid;
    justify-items: center;
    gap: var(--esp-1);
    font: var(--fonte-rotulo-m);
    color: var(--cor-texto-var);
  }
  .nota {
    margin-top: var(--esp-2);
    font: var(--fonte-rotulo-m);
    color: var(--cor-texto-var);
  }
  .lista {
    margin: 0;
    padding: 0;
  }
</style>
