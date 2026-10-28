// Procura senhas e chaves de API no repositório inteiro (não só em app/).
//
// Uso (a partir da raiz):
//   node scripts/verificar-segredos.mjs            arquivos rastreados + novos não ignorados
//   node scripts/verificar-segredos.mjs --staged   só o que está no índice (hook de pre-commit)
//   node scripts/verificar-segredos.mjs --pasta app/dist   também varre uma pasta (ex.: o bundle)
//
// Linha com falso positivo conhecido: acrescente o comentário `segredo-ok` na própria linha.
// Nunca imprime o valor encontrado, só arquivo, linha e tipo.

import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const raiz = fileURLToPath(new URL('..', import.meta.url));
const staged = process.argv.includes('--staged');
const pastas = process.argv.flatMap((a, i, v) => (a === '--pasta' && v[i + 1] ? [v[i + 1]] : []));

const PADROES = [
  ['chave AWS', /AKIA[0-9A-Z]{16}/],
  ['chave estilo OpenAI/Anthropic', /sk-[A-Za-z0-9_-]{20,}/],
  ['token GitHub', /gh[pousr]_[A-Za-z0-9]{30,}/],
  ['chave Google', /AIza[0-9A-Za-z_-]{30,}/],
  ['token Slack', /xox[abprs]-[A-Za-z0-9-]{10,}/],
  ['chave privada', /-----BEGIN [A-Z ]*PRIVATE KEY-----/],
  ['JWT', /eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/],
  ['URL com senha embutida', /[a-z][a-z0-9+.-]*:\/\/[^\s:/@'"]+:[^\s:/@'"]{3,}@[^\s'"]+/i],
  [
    'atribuição de segredo literal',
    /(api[_-]?key|secret|token|passw(or)?d|senha)[A-Za-z0-9_]*["']?\s*[=:]\s*["'][^"'\s${}]{12,}["']/i,
  ],
];

// Arquivos de ambiente: só o catálogo (flag pública) pode estar no repositório.
const ENV_PERMITIDO = new Set(['app/.env.catalogo']);
const NOME_SENSIVEL =
  /(^|\/)(\.env(\..+)?|id_rsa|id_ed25519|credentials(\.json)?|.*\.(pem|key|p12|pfx|jks))$/i;
const IGNORAR = /(^|\/)(package-lock\.json|Cargo\.lock|node_modules|target|\.venv)\//;
const BINARIO = /\.(db|png|jpg|jpeg|gif|ico|webp|woff2?|ttf|parquet|zip|gz|pdf|xlsx?|wasm)$/i;

const problemas = [];

function git(...args) {
  return execFileSync('git', args, {
    cwd: raiz,
    encoding: 'utf8',
    maxBuffer: 1 << 28,
  });
}

function varrer(nome, texto) {
  texto.split(/\r?\n/).forEach((linha, i) => {
    if (linha.length > 4000 || linha.includes('segredo-ok')) return;
    for (const [tipo, regex] of PADROES) {
      if (regex.test(linha)) {
        problemas.push(`${nome}:${i + 1}: ${tipo}`);
        break;
      }
    }
  });
}

function checarNome(nome) {
  if (NOME_SENSIVEL.test(nome) && !ENV_PERMITIDO.has(nome))
    problemas.push(`${nome}: arquivo de segredo/ambiente não deve ir para o repositório`);
}

const lista = staged
  ? git('diff', '--cached', '--name-only', '--diff-filter=ACMR', '-z').split('\0')
  : git('ls-files', '-co', '--exclude-standard', '-z').split('\0');

for (const nome of lista.filter(Boolean)) {
  checarNome(nome);
  if (IGNORAR.test(nome) || BINARIO.test(nome)) continue;
  try {
    const texto = staged ? git('show', `:${nome}`) : readFileSync(join(raiz, nome), 'utf8');
    if (!texto.includes('\0')) varrer(nome, texto);
  } catch {
    // arquivo removido ou ilegível: nada a varrer
  }
}

function percorrer(pasta, base) {
  for (const nome of readdirSync(pasta)) {
    const caminho = join(pasta, nome);
    const rel = `${base}/${nome}`;
    if (statSync(caminho).isDirectory()) percorrer(caminho, rel);
    else if (!BINARIO.test(nome)) varrer(rel, readFileSync(caminho, 'utf8'));
  }
}
for (const p of pastas) percorrer(join(raiz, p), p.replace(/\/$/, ''));

if (problemas.length) {
  console.error('verificar-segredos: possível senha/chave no repositório\n');
  for (const p of problemas) console.error(`  - ${p}`);
  console.error(
    '\nTire o segredo do código (variável de ambiente fora do git) e, se já foi commitado, revogue-o.',
  );
  process.exit(1);
}
console.log(
  `verificar-segredos: ok (${lista.filter(Boolean).length} arquivos${pastas.length ? ' + ' + pastas.join(', ') : ''})`,
);
