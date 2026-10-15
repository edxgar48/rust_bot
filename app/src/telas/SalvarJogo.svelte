<script lang="ts">
  import { untrack } from 'svelte';
  import type { Slug } from '../dados/tipos';
  import { TAMANHO_MAXIMO_NOME } from '../jogos/jogo';
  import type { RepositorioJogos } from '../jogos/repositorio';
  import { pedirArmazenamentoPersistente, salvarJogo } from '../jogos/servico';
  import { textos } from '../lib/textos';

  interface Props {
    loteria: Slug;
    dezenas: readonly number[];
    /** Quantidade de dezenas aceita pela loteria: só então o botão fica habilitado. */
    valido: boolean;
    jogos: RepositorioJogos;
    /** Último concurso do histórico mostrado: o jogo novo começa conferido até ele (6.4). */
    ultimoConcurso?: number | null;
    /** Leva para a lista Meus jogos. */
    aoVerJogos: () => void;
  }

  let { loteria, dezenas, valido, jogos, ultimoConcurso = null, aoVerJogos }: Props = $props();

  const idCampo = $props.id();
  let nome = $state('');
  let ocupado = $state(false);
  let mensagem = $state<{ tipo: 'ok' | 'info' | 'erro'; texto: string } | null>(null);
  let salvouAgora = $state(false);

  // Mexer nas dezenas ou na loteria torna a mensagem anterior obsoleta.
  const chave = $derived(`${loteria}:${dezenas.join(',')}`);
  $effect(() => {
    void chave;
    untrack(() => {
      mensagem = null;
      salvouAgora = false;
    });
  });

  async function aoEnviar(evento: SubmitEvent): Promise<void> {
    evento.preventDefault();
    if (!valido || ocupado) return;
    ocupado = true;
    mensagem = null;
    salvouAgora = false;
    try {
      const r = await salvarJogo(jogos, { loteria, dezenas, nome, conferidoAte: ultimoConcurso });
      if (r.tipo === 'salvo') {
        salvouAgora = true;
        nome = '';
        mensagem = { tipo: 'ok', texto: textos.conferir.salvar.salvo };
        // Best-effort: pede ao navegador para não apagar os jogos sem avisar. Não bloqueia nada.
        if (r.primeiro) void pedirArmazenamentoPersistente();
        if (jogos.aviso) mensagem = { tipo: 'erro', texto: textos.conferir.salvar.volatil };
      } else if (r.tipo === 'duplicado') {
        mensagem = { tipo: 'info', texto: textos.conferir.salvar.duplicado(r.existente.nome) };
      } else {
        mensagem = { tipo: 'erro', texto: textos.conferir.salvar.invalido };
      }
    } catch {
      mensagem = { tipo: 'erro', texto: textos.conferir.salvar.falhou };
    } finally {
      ocupado = false;
    }
  }
</script>

<form class="salvar" onsubmit={aoEnviar}>
  <div class="campo">
    <label for="{idCampo}-nome">{textos.conferir.salvar.rotuloNome}</label>
    <input
      id="{idCampo}-nome"
      type="text"
      autocomplete="off"
      maxlength={TAMANHO_MAXIMO_NOME}
      aria-describedby="{idCampo}-ajuda"
      bind:value={nome}
    />
    <p id="{idCampo}-ajuda" class="ajuda">
      {textos.conferir.salvar.ajudaNome(TAMANHO_MAXIMO_NOME)}
    </p>
  </div>
  <div class="acoes">
    <button type="submit" class="botao primario" disabled={!valido || ocupado}>
      {ocupado ? textos.conferir.salvar.salvando : textos.conferir.salvar.botao}
    </button>
    {#if salvouAgora}
      <button type="button" class="botao" onclick={aoVerJogos}>
        {textos.conferir.salvar.verMeusJogos}
      </button>
    {/if}
  </div>
  <p class="mensagem" role="status" data-tipo={mensagem?.tipo ?? 'nenhum'}>
    {mensagem?.texto ?? ''}
  </p>
</form>

<style>
  .salvar {
    display: grid;
    gap: var(--esp-3);
    margin-top: var(--esp-4);
    padding-top: var(--esp-4);
    border-top: 1px solid var(--cor-contorno-var);
  }
  .campo {
    display: grid;
    gap: var(--esp-1);
    max-width: 26rem;
  }
  label {
    font: var(--fonte-rotulo-g);
  }
  input {
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-3);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-p);
    background: var(--cor-superficie);
    color: var(--cor-texto);
    font: var(--fonte-corpo-g);
  }
  .ajuda {
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
  }
  .acoes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--esp-3);
  }
  .botao {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: var(--alvo-minimo);
    padding: 0 var(--esp-4);
    border: 1px solid var(--cor-contorno);
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  .botao.primario {
    border-color: var(--cor-primaria);
    background: var(--cor-primaria);
    color: var(--cor-sobre-primaria);
  }
  .botao:disabled {
    border-color: var(--cor-contorno-var);
    background: transparent;
    color: var(--cor-texto-var);
    cursor: not-allowed;
    opacity: 0.6;
  }
  .mensagem {
    min-height: 1.5rem;
    font: var(--fonte-corpo-m);
    font-weight: 600;
  }
  .mensagem[data-tipo='erro'] {
    color: var(--cor-erro);
  }
  .mensagem[data-tipo='nenhum'] {
    min-height: 0;
  }
</style>
