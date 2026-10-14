# App Força das Dezenas

Front-end estático (Vite + Svelte 5 + TypeScript estrito). Lê apenas os JSON do
contrato `v1` (`docs/contrato-v1.md`); sem servidor, sem analytics. Plano em
`docs/PLANEJAMENTO-FRONTEND.md`. Fica fora do `cargo build`.

Requisitos: Node 22+ e npm 11+.

## Comandos (a partir de `app/`)

```bash
npm install        # dependências (use npm ci em máquina limpa)
npm run dev        # servidor de desenvolvimento
npm run lint       # ESLint + checagem do Prettier
npm run formatar   # aplica o Prettier
npm run check      # tipos (svelte-check + tsc estrito)
npm test           # Vitest (sem rede)
npm run build      # gera o site estático em dist/
npm run preview    # serve dist/ localmente
```

## Estrutura

- `src/lib/textos.ts`: todos os textos de interface (um só módulo).
- `src/dados/`: camada de dados do contrato v1 (tipos, validação de forma,
  cliente do `index.json` com conferência de SHA-256/bytes, interface de
  armazenamento). URL base em `src/dados/config.ts`.
- `src/app.css`: tokens de cor (Material 3, semente verde-azulada), claro/escuro.
- `testes/`: fixtures (cópias de `publico/v1/`) e testes de dados.
