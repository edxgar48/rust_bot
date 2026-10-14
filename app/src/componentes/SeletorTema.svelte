<script lang="ts">
  import { textos } from '../lib/textos';
  import {
    aplicarPreferencia,
    gravarPreferencia,
    lerPreferencia,
    proximaPreferencia,
    type PreferenciaTema,
  } from '../lib/tema';
  import Icone from './Icone.svelte';

  let preferencia = $state<PreferenciaTema>(lerPreferencia());
  const rotulo = $derived(textos.tema[preferencia]);
  const icone = $derived(
    preferencia === 'claro'
      ? 'tema-claro'
      : preferencia === 'escuro'
        ? 'tema-escuro'
        : 'tema-automatico',
  );

  function alternar(): void {
    preferencia = proximaPreferencia(preferencia);
    aplicarPreferencia(preferencia, document.documentElement);
    gravarPreferencia(preferencia);
  }
</script>

<button type="button" class="tema" aria-label={textos.tema.alternar(rotulo)} onclick={alternar}>
  <Icone nome={icone} />
</button>

<style>
  .tema {
    display: inline-grid;
    place-items: center;
    width: var(--alvo-minimo);
    height: var(--alvo-minimo);
    padding: 0;
    border: 0;
    border-radius: var(--raio-cheio);
    background: transparent;
    color: var(--cor-texto-var);
    cursor: pointer;
  }
</style>
