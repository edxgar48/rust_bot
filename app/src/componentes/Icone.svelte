<script lang="ts">
  import type { NomeIcone } from './tipos';

  // Ícones decorativos (Material Symbols, 24x24): sempre acompanhados de texto
  // ou de aria-label no elemento que os contém.
  const caminhos: Readonly<Record<NomeIcone, string>> = {
    inicio: 'M10 20v-6h4v6h5v-8h3L12 3 2 12h3v8z',
    forca: 'M5 9.2h3V19H5zM10.6 5h2.8v14h-2.8zm5.6 8H19v6h-2.8z',
    conferir:
      'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 15l-5-5 1.41-1.41L10 14.17l7.59-7.59L19 8l-9 9z',
    estatisticas: 'M3.5 18.49l6-6.01 4 4L22 6.92l-1.41-1.41-7.09 7.97-4-4L2 16.99z',
    entenda:
      'M11 18h2v-2h-2v2zm1-16C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8zm0-14c-2.21 0-4 1.79-4 4h2c0-1.1.9-2 2-2s2 .9 2 2c0 2-3 1.75-3 5h2c0-2.25 3-2.5 3-5 0-2.21-1.79-4-4-4z',
    info: 'M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z',
    baixar: 'M5 20h14v-2H5v2zM19 9h-4V3H9v6H5l7 7 7-7z',
    erro: 'M12 2L1 21h22L12 2zm1 15h-2v-2h2v2zm0-4h-2v-4h2v4z',
    atualizar:
      'M17.65 6.35A7.958 7.958 0 0012 4c-4.42 0-7.99 3.58-7.99 8s3.57 8 7.99 8c3.73 0 6.84-2.55 7.73-6h-2.08A5.99 5.99 0 0112 18c-3.31 0-6-2.69-6-6s2.69-6 6-6c1.66 0 3.14.69 4.22 1.78L13 11h7V4l-2.35 2.35z',
    'sem-rede':
      'M19.35 10.04A7.49 7.49 0 0012 4c-1.48 0-2.85.43-4.01 1.17l1.46 1.46a5.497 5.497 0 018.05 4.87v.5h1.5c1.38 0 2.5 1.12 2.5 2.5 0 .73-.32 1.38-.82 1.84l1.43 1.43A4.49 4.49 0 0024 15.5c0-2.34-1.79-4.22-4.07-4.43zM3 5.27l2.75 2.74A5.986 5.986 0 002 13.5C2 16.54 4.46 19 7.5 19h10.73l1.5 1.5L21 19.23 4.27 3.5 3 5.27zM7.5 17c-1.93 0-3.5-1.57-3.5-3.5S5.57 10 7.5 10h.73l7 7H7.5z',
    'tema-automatico': 'M12 2a10 10 0 100 20 10 10 0 000-20zm0 18V4a8 8 0 010 16z',
    'tema-claro':
      'M12 7a5 5 0 100 10 5 5 0 000-10zM2 13h2v-2H2v2zm18 0h2v-2h-2v2zM11 2v2h2V2h-2zm0 18v2h2v-2h-2zM5.99 4.58L4.58 5.99l1.41 1.42 1.42-1.42-1.42-1.41zm12.02 12.01l-1.42 1.42 1.41 1.41 1.42-1.41-1.41-1.42zM18.01 7.41l1.41-1.42-1.41-1.41-1.42 1.41 1.42 1.42zM5.99 19.42l1.42-1.41-1.42-1.42-1.41 1.42 1.41 1.41z',
    'tema-escuro':
      'M12 3a9 9 0 109 9c0-.46-.04-.92-.1-1.36a5.389 5.389 0 01-4.4 2.26 5.403 5.403 0 01-3.14-9.8c-.44-.06-.9-.1-1.36-.1z',
  };

  interface Props {
    nome: NomeIcone;
    /** Lado em rem. */
    tamanho?: number;
  }
  let { nome, tamanho = 1.5 }: Props = $props();
</script>

<svg
  viewBox="0 0 24 24"
  width={`${tamanho}rem`}
  height={`${tamanho}rem`}
  fill="currentColor"
  aria-hidden="true"
  focusable="false"
>
  <path d={caminhos[nome]} />
</svg>
