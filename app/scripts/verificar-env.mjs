// Barra segredos em variáveis VITE_*: tudo com esse prefixo vai para o bundle do navegador
// (o equivalente do NEXT_PUBLIC_). Confere os arquivos .env* de app/ e o uso de
// import.meta.env.VITE_* no código. Uso (a partir de app/): node scripts/verificar-env.mjs

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const raiz = fileURLToPath(new URL('..', import.meta.url));

const NOME_SUSPEITO = /(KEY|SECRET|TOKEN|PASS(WORD|WD)?|PRIVATE|CREDENTIAL|AUTH|BEARER|DSN)/i;
const VALOR_SUSPEITO = [
  /AKIA[0-9A-Z]{16}/,
  /sk-[A-Za-z0-9_-]{20,}/,
  /gh[pousr]_[A-Za-z0-9]{30,}/,
  /AIza[0-9A-Za-z_-]{30,}/,
  /xox[abprs]-[A-Za-z0-9-]{10,}/,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
  /eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\./,
  /^[A-Za-z0-9+/_=-]{32,}$/,
];
// Variáveis públicas de propósito, sem segredo.
const PERMITIDAS = new Set(['VITE_CATALOGO']);

const problemas = [];

function arquivos(pasta, aceita) {
  const achados = [];
  for (const nome of readdirSync(pasta)) {
    if (['node_modules', 'dist', 'dist-catalogo', '.git'].includes(nome)) continue;
    const caminho = join(pasta, nome);
    if (statSync(caminho).isDirectory()) achados.push(...arquivos(caminho, aceita));
    else if (aceita(nome)) achados.push(caminho);
  }
  return achados;
}

function nomeCurto(caminho) {
  return relative(raiz, caminho).replaceAll('\\', '/');
}

for (const caminho of arquivos(raiz, (n) => n === '.env' || n.startsWith('.env.'))) {
  readFileSync(caminho, 'utf8')
    .split(/\r?\n/)
    .forEach((linha, i) => {
      const m = linha.match(/^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/);
      if (!m) return;
      const [, nome, bruto] = m;
      const valor = bruto.trim().replace(/^(['"])(.*)\1$/, '$2');
      const onde = `${nomeCurto(caminho)}:${i + 1}`;
      if (!nome.startsWith('VITE_')) {
        if (valor && NOME_SUSPEITO.test(nome))
          problemas.push(`${onde}: ${nome} guarda segredo em arquivo .env que pode ser commitado`);
        return;
      }
      if (PERMITIDAS.has(nome)) return;
      if (NOME_SUSPEITO.test(nome))
        problemas.push(
          `${onde}: ${nome} — nome de segredo com prefixo VITE_ (vai para o navegador)`,
        );
      else if (VALOR_SUSPEITO.some((r) => r.test(valor)))
        problemas.push(`${onde}: ${nome} — valor parece uma chave (vai para o navegador)`);
    });
}

const fontes = arquivos(join(raiz, 'src'), (n) => /\.(ts|js|svelte)$/.test(n)).concat(
  arquivos(raiz, (n) => /^(vite\.config|manifesto)\.ts$/.test(n)).filter(
    (c) => !c.includes('node_modules'),
  ),
);
for (const caminho of fontes) {
  const texto = readFileSync(caminho, 'utf8');
  for (const m of texto.matchAll(/import\.meta\.env\.(VITE_[A-Z0-9_]+)/g)) {
    if (!PERMITIDAS.has(m[1]) && NOME_SUSPEITO.test(m[1]))
      problemas.push(`${nomeCurto(caminho)}: lê ${m[1]}, nome de segredo exposto ao navegador`);
  }
}

if (problemas.length) {
  console.error('verificar-env: possível vazamento de segredo no bundle\n');
  for (const p of problemas) console.error(`  - ${p}`);
  console.error(
    '\nSegredos não podem usar VITE_*. Se a variável é pública de verdade, inclua-a em PERMITIDAS.',
  );
  process.exit(1);
}
console.log('verificar-env: ok');
