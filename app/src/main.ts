import { mount } from 'svelte';
import './estilos/tokens.css';
import './estilos/base.css';
import App from './App.svelte';
import { controleGlobal } from './lib/instalacao';

const alvo = document.getElementById('app');
if (!alvo) throw new Error('Elemento #app não encontrado');

// O navegador dispara beforeinstallprompt cedo: captura antes de montar as telas.
controleGlobal();

export default mount(App, { target: alvo });
