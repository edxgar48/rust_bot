import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';

const alvo = document.getElementById('app');
if (!alvo) throw new Error('Elemento #app não encontrado');

export default mount(App, { target: alvo });
