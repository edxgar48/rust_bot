import { mount } from 'svelte';
import './estilos/tokens.css';
import './estilos/base.css';
import App from './App.svelte';

const alvo = document.getElementById('app');
if (!alvo) throw new Error('Elemento #app não encontrado');

export default mount(App, { target: alvo });
