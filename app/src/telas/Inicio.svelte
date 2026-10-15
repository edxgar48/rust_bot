<script lang="ts">
  import Aviso from '../componentes/Aviso.svelte';
  import BolaDezena from '../componentes/BolaDezena.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import Icone from '../componentes/Icone.svelte';
  import type { NomeIcone } from '../componentes/tipos';
  import type { SituacaoDados } from '../dados/abertura';
  import type { TipoEstadoSincronizacao } from '../dados/estado-sincronizacao';
  import type { NovidadeLoteria } from '../jogos/conferencia-automatica';
  import { textos } from '../lib/textos';
  import { montarAvisosNovidade, montarVisaoInicio } from './inicio';

  interface Props {
    situacao: SituacaoDados | null;
    /** Há uma sincronização em andamento (desativa o botão Atualizar). */
    ocupada: boolean;
    aoAtualizar: () => void;
    /** Conferência automática dos jogos salvos (6.4); vazio = nada a mostrar. */
    novidades?: readonly NovidadeLoteria[];
  }

  let { situacao, ocupada, aoAtualizar, novidades = [] }: Props = $props();

  const visao = $derived(montarVisaoInicio(situacao));
  const avisosNovidade = $derived(montarAvisosNovidade(novidades));

  // O ícone acompanha o texto do estado; a cor nunca é a única informação.
  const iconesDoEstado: Record<TipoEstadoSincronizacao, NomeIcone> = {
    sincronizando: 'atualizar',
    atualizado: 'conferir',
    sem_rede: 'sem-rede',
    recusado: 'erro',
    sem_dados: 'sem-rede',
  };
</script>

{#snippet botao(rotulo: string)}
  <button type="button" class="botao" disabled={ocupada} onclick={aoAtualizar}>
    <Icone nome="atualizar" tamanho={1.125} />
    {ocupada ? textos.inicio.atualizando : rotulo}
  </button>
{/snippet}

<h2 id="titulo-aba">{textos.inicio.titulo}</h2>

<div class="pilha">
  {#if visao.tipo === 'carregando'}
    <div class="carregando" aria-busy="true">
      <p class="discreto" role="status">{textos.inicio.carregando}</p>
      {#each [0, 1, 2] as i (i)}
        <Cartao variante="elevado">
          <div class="esqueleto" aria-hidden="true">
            <span class="barra largo"></span>
            <span class="barra medio"></span>
            <span class="barra bolas-falsas"></span>
          </div>
        </Cartao>
      {/each}
    </div>
  {:else if visao.tipo === 'sem_dados'}
    <Aviso tipo="erro">{visao.texto}</Aviso>
    <div class="acoes">{@render botao(textos.inicio.tentarDeNovo)}</div>
  {:else}
    <div class="estado" data-estado={visao.estado}>
      {#if visao.erro}
        <div class="aviso-cheio"><Aviso tipo="erro">{visao.texto}</Aviso></div>
        {@render botao(textos.inicio.atualizar)}
      {:else}
        <p class="linha" role="status">
          <Icone nome={iconesDoEstado[visao.estado]} tamanho={1.25} />
          <span>{visao.texto}</span>
        </p>
        {@render botao(textos.inicio.atualizar)}
      {/if}
    </div>

    <!-- Região estável (existe antes do aviso): o cartão chega depois, de forma assíncrona,
         e o leitor de tela anuncia o título "Concurso N saiu…" quando ele aparece. -->
    <div class="novidades" role="status" aria-atomic="false">
      {#if avisosNovidade.length > 0}
        {#each avisosNovidade as aviso (aviso.slug)}
          <Cartao variante="contorno" titulo={aviso.titulo}>
            <div class="novidade" data-loteria={aviso.slug}>
              {#if aviso.resumo}<p class="resumo">{aviso.resumo}</p>{/if}
              {#if aviso.concursos}<p class="detalhe">{aviso.concursos}</p>{/if}
              {#if aviso.faixa}<p class="faixa">{aviso.faixa}</p>{/if}
              {#if aviso.pendente}<p class="detalhe">{aviso.pendente}</p>{/if}
              <p class="detalhe">{textos.novidade.premioNota}</p>
              <a class="link" href={aviso.href}>{textos.novidade.verJogos}</a>
            </div>
          </Cartao>
        {/each}
        <Aviso>{textos.avisos.naoPreveConferir} {textos.avisos.jogoResponsavel}</Aviso>
      {/if}
    </div>

    {#each visao.cartoes as cartao (cartao.slug)}
      <Cartao variante="elevado" titulo={cartao.nome}>
        {#if cartao.tipo === 'sem_loteria'}
          <p class="detalhe">{textos.inicio.semLoteria}</p>
        {:else}
          <p class="detalhe">
            {textos.inicio.concursoData(cartao.concurso ?? 0, cartao.data ?? '')}
          </p>
          {#if cartao.tipo === 'resultado'}
            <ul
              class="bolas"
              data-disposicao={cartao.disposicao}
              aria-label={textos.inicio.dezenasSorteadas(cartao.nome)}
            >
              {#each cartao.dezenas as dezena (dezena)}
                <li><BolaDezena numero={dezena} estado="sorteada" /></li>
              {/each}
            </ul>
          {:else}
            <p class="detalhe">{textos.inicio.semResultado}</p>
          {/if}
        {/if}
      </Cartao>
    {/each}
  {/if}

  <Aviso>{textos.avisos.naoPreveInicio}</Aviso>
</div>

<style>
  h2 {
    margin-bottom: var(--esp-4);
    font: var(--fonte-titulo-g);
  }
  .pilha {
    display: grid;
    gap: var(--esp-3);
  }
  .estado {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--esp-2) var(--esp-3);
  }
  .aviso-cheio {
    flex: 1 1 100%;
  }
  .linha {
    display: flex;
    flex: 1 1 14rem;
    align-items: flex-start;
    gap: var(--esp-2);
    min-width: 0;
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .linha :global(svg) {
    flex: none;
  }
  .linha span {
    min-width: 0;
    padding-top: 0.125rem;
    overflow-wrap: anywhere;
  }
  .acoes {
    display: flex;
  }
  .botao {
    display: inline-flex;
    align-items: center;
    gap: var(--esp-2);
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .botao:disabled {
    color: var(--cor-texto-var);
    cursor: progress;
  }
  .novidades,
  .novidade {
    display: grid;
    gap: var(--esp-3);
  }
  .novidade {
    gap: var(--esp-2);
  }
  /* Região vazia: não deixa um espaço a mais na pilha. */
  .novidades:empty {
    margin-top: calc(-1 * var(--esp-3));
  }
  .resumo,
  .faixa {
    font: var(--fonte-corpo-g);
    font-weight: 600;
  }
  .link {
    display: inline-flex;
    align-items: center;
    justify-self: start;
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    text-decoration: none;
  }
  .detalhe {
    margin-bottom: var(--esp-3);
    color: var(--cor-texto-var);
  }
  .novidade .detalhe {
    margin-bottom: 0;
  }
  .detalhe:last-child {
    margin-bottom: 0;
  }
  .bolas {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .bolas li {
    display: grid;
    place-items: center;
  }
  /* Mega-Sena e Quina: uma linha (quebra só se a tela for muito estreita). */
  .bolas[data-disposicao='linha'] {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-2);
  }
  /* Lotofácil: 15 dezenas em grade de 5 colunas. */
  .bolas[data-disposicao='grade'] {
    display: grid;
    grid-template-columns: repeat(5, 2.5rem);
    justify-content: space-between;
    gap: var(--esp-2);
    max-width: 20rem;
  }
  .discreto {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .carregando {
    display: grid;
    gap: var(--esp-3);
  }
  .esqueleto {
    display: grid;
    gap: var(--esp-3);
  }
  .barra {
    display: block;
    height: 1rem;
    border-radius: var(--raio-p);
    background: var(--cor-cont-altissimo);
    animation: pulso 1.6s ease-in-out infinite;
  }
  .barra.largo {
    width: 40%;
  }
  .barra.medio {
    width: 60%;
    height: 0.75rem;
  }
  .barra.bolas-falsas {
    height: 2.5rem;
    border-radius: var(--raio-cheio);
  }
  @keyframes pulso {
    50% {
      opacity: 0.45;
    }
  }
</style>
