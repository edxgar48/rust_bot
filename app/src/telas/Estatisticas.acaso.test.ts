// Tela Estatísticas, camada "comparação com o acaso" (7.3): faixa no gráfico de frequência,
// coluna "Dentro da faixa?", texto-resumo e mini-barras observado × esperado em Pares e
// Repetidas. Os números esperados vêm de fixtures/acaso-esperado.json (Python exato; a
// hipergeométrica conferida contra o back-end Rust) e de estatisticas-esperado.json (SQL).
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import { SLUGS, type Slug } from '../dados/tipos';
import { formatarDecimal, formatarPercentual } from '../lib/numeros';
import { textos } from '../lib/textos';
import Estatisticas from './Estatisticas.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(servidor: ServidorFalso, arm: ArmazenamentoEmMemoria): Fonte {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: async (url, init) => servidor.fetch(url, init),
    armazenamento: arm,
    agora: () => T1,
  });
  return {
    lerCache: (o) => cliente.lerCache(o),
    garantir: (o) => garantirArquivos(cliente, o),
  };
}

let montado: Record<string, unknown> | null = null;
function montar(fonte: Fonte): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Estatisticas as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte },
  });
  flushSync();
  return alvo;
}

function desmontar(): void {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
}

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/estatisticas';
});

afterEach(() => {
  desmontar();
  vi.restoreAllMocks();
  location.hash = '';
});

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();

async function montarEm(slug: Slug, janela: string): Promise<HTMLElement> {
  location.hash = `#/estatisticas?loteria=${slug}&janela=${janela}`;
  const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
  await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
  return alvo;
}

interface ForaRef {
  n: number;
  inferior: number;
  superior: number;
  abaixo: number[];
  acima: number[];
}
interface RefAcaso {
  soma: Record<Slug, { inferior: number; superior: number; cobertura_real: number; media: number }>;
  hipergeometrica: Record<string, { pmf: number[]; escolhidas: number }>;
  binomial: Record<
    Slug,
    {
      total_dezenas: number;
      janelas: { n: number; fora_esperado_por_dezena: number }[];
      fora: Record<string, ForaRef>;
    }
  >;
}
const ref = JSON.parse(lerFixtureTexto('acaso-esperado.json')) as RefAcaso;
const sql = JSON.parse(lerFixtureTexto('estatisticas-esperado.json')) as {
  loterias: Record<
    Slug,
    Record<
      string,
      {
        tamanho: number;
        distribuicao_pares: number[];
        distribuicao_repetidas: number[];
        soma_total: number;
        soma_min: number;
        soma_max: number;
        atraso: (number | null)[];
      }
    >
  >;
};
const historicos = Object.fromEntries(
  SLUGS.map((slug) => [
    slug,
    (
      JSON.parse(lerFixtureTexto(`${slug}/historico.json`)) as {
        concursos: [number, string, number[]][];
      }
    ).concursos,
  ]),
) as Record<Slug, [number, string, number[]][]>;
const PARAMS = {
  lotofacil: { total: 25, k: 15 },
  megasena: { total: 60, k: 6 },
  quina: { total: 80, k: 5 },
} as const;

const JANELAS = ['10', '50', '100', 'total'] as const;
const dois = (d: number) => String(d).padStart(2, '0');
const t = textos.estatisticas;

describe('faixa do acaso no gráfico de frequência', () => {
  for (const slug of SLUGS) {
    for (const janela of JANELAS) {
      it(`${slug}, janela ${janela}: faixa, legenda, tabela e resumo batem com a referência`, async () => {
        const alvo = await montarEm(slug, janela);
        const fora = ref.binomial[slug].fora[janela]!;
        const bin = ref.binomial[slug].janelas.find((j) => j.n === fora.n)!;
        const total = ref.binomial[slug].total_dezenas;

        // faixa desenhada (forma, não só cor): hachura + duas bordas pontilhadas
        const faixa = alvo.querySelector('svg[role="img"] [data-faixa]')!;
        expect(faixa).not.toBeNull();
        expect(faixa.querySelectorAll('rect')).toHaveLength(1);
        expect(faixa.querySelectorAll('line')).toHaveLength(2);
        const idPattern = alvo.querySelector('svg[role="img"] pattern')!.id;
        expect(idPattern).toMatch(/^hachura-faixa-/);
        expect(faixa.querySelector('rect')!.getAttribute('fill')).toBe(`url(#${idPattern})`);

        // legenda em texto com os limites da referência
        const legenda = texto(alvo.querySelector('[data-legenda="faixa"]'));
        expect(legenda).toBe(t.frequencia.legendaFaixa(fora.inferior, fora.superior));
        expect(legenda).toContain('cada dezena tem pelo menos 95% de chance');
        expect(legenda).not.toContain('das dezenas');

        // tabela alternativa: coluna "Dentro da faixa?" igual à referência
        const linhas = Array.from(alvo.querySelectorAll('details.tabela-alt tbody tr')).map((tr) =>
          Array.from(tr.querySelectorAll('th, td')).map(texto),
        );
        expect(linhas).toHaveLength(total);
        const abaixo = linhas.filter((l) => l[3] === 'Não, abaixo').map((l) => Number(l[0]));
        const acima = linhas.filter((l) => l[3] === 'Não, acima').map((l) => Number(l[0]));
        expect(abaixo).toEqual(fora.abaixo);
        expect(acima).toEqual(fora.acima);
        expect(linhas.filter((l) => l[3] === 'Sim')).toHaveLength(
          total - fora.abaixo.length - fora.acima.length,
        );
        expect(texto(alvo.querySelector('details.tabela-alt caption'))).toContain(
          t.frequencia.tabelaLegendaFaixa(fora.inferior, fora.superior),
        );

        // traço no topo das barras fora da faixa: um por dezena fora
        const tracos = Array.from(alvo.querySelectorAll('svg[role="img"] line[data-fora]')).map(
          (l) => Number(l.getAttribute('data-fora')),
        );
        expect(tracos).toEqual([...fora.abaixo, ...fora.acima].sort((a, b) => a - b));
        expect(!!alvo.querySelector('[data-legenda="fora"]')).toBe(tracos.length > 0);

        // texto-resumo honesto, com a contagem e o esperado da referência
        const n = fora.abaixo.length + fora.acima.length;
        const esperado = formatarDecimal(total * bin.fora_esperado_por_dezena);
        const resumo = texto(alvo.querySelector('[data-bloco="resumo-acaso"] .resumo-texto'));
        expect(resumo).toBe(t.frequencia.resumoAcaso(n, esperado));
        expect(resumo).toContain(`o esperado é cerca de ${esperado} fora`);
        expect(resumo).toContain('não quer dizer dezena especial e não ajuda a prever');
        expect(resumo).not.toMatch(/normal/i);
        const listaAbaixo = alvo.querySelector('[data-fora-lista="abaixo"]');
        const listaAcima = alvo.querySelector('[data-fora-lista="acima"]');
        expect(listaAbaixo ? texto(listaAbaixo) : null).toBe(
          fora.abaixo.length ? `Abaixo da faixa: ${fora.abaixo.map(dois).join(', ')}.` : null,
        );
        expect(listaAcima ? texto(listaAcima) : null).toBe(
          fora.acima.length ? `Acima da faixa: ${fora.acima.map(dois).join(', ')}.` : null,
        );
      });
    }
  }

  it('janela de 10: faixa larga e aviso, sem esconder a faixa', async () => {
    const alvo = await montarEm('quina', '10');
    expect(alvo.querySelector('[data-faixa]')).not.toBeNull();
    expect(texto(alvo.querySelector('[data-aviso-faixa]'))).toBe(t.frequencia.faixaLarga(10));
    expect(texto(alvo.querySelector('[data-aviso-faixa]'))).toContain('a faixa é larga');
  });

  it('janela de 50 não mostra o aviso de faixa larga', async () => {
    const alvo = await montarEm('lotofacil', '50');
    expect(alvo.querySelector('[data-aviso-faixa]')).toBeNull();
  });

  it('o rótulo do SVG continua curto e cita a faixa', async () => {
    const alvo = await montarEm('megasena', '100');
    const svg = alvo.querySelector('svg[role="img"]')!;
    const rotulo = svg.getAttribute('aria-label')!;
    const f = ref.binomial.megasena.fora['100']!;
    expect(rotulo).toContain(`faixa do acaso de ${f.inferior} a ${f.superior} vezes`);
    expect(rotulo.length).toBeLessThan(400);
  });
});

describe('observado × esperado ao acaso: Pares e Repetidas', () => {
  for (const slug of SLUGS) {
    for (const janela of ['50', 'total'] as const) {
      it(`${slug}, janela ${janela}: tabelas com observado do SQL e esperado da hipergeométrica`, async () => {
        const alvo = await montarEm(slug, janela);
        for (const [tipo, chave, obsSql] of [
          ['pares', `${slug}_pares`, sql.loterias[slug][janela]!.distribuicao_pares],
          ['repetidas', `${slug}_repetidas`, sql.loterias[slug][janela]!.distribuicao_repetidas],
        ] as const) {
          const bloco = alvo.querySelector(`[data-comparacao="${tipo}"]`)!;
          expect(bloco).not.toBeNull();
          const hip = ref.hipergeometrica[chave]!;
          const sorteios = obsSql.reduce((a, b) => a + b, 0);
          const linhas = Array.from(bloco.querySelectorAll('tbody tr')).map((tr) =>
            Array.from(tr.querySelectorAll('th, td')).map(texto),
          );
          // todos os valores do suporte (pmf > 0) aparecem na tabela, em ordem
          const suporte = hip.pmf.map((p, v) => [p, v] as const).filter(([p]) => p > 0);
          expect(linhas.map((l) => Number(l[0]))).toEqual(suporte.map(([, v]) => v));
          for (const [i, [p, v]] of suporte.entries()) {
            const l = linhas[i]!;
            expect(Number(l[1]!.replace(/\./g, ''))).toBe(obsSql[v]);
            expect(l[2]).toBe(formatarPercentual(obsSql[v]! / sorteios));
            expect(l[3]).toBe(formatarDecimal(sorteios * p));
            expect(l[4]).toBe(formatarPercentual(p));
          }
          // mini-barras: barra cheia só onde há observação, dentro dos valores visíveis
          const visiveis = suporte.filter(([p, v]) => p >= 0.0005 || obsSql[v]! > 0);
          const primeiro = visiveis[0]![1];
          const ultimo = visiveis.at(-1)![1];
          const svg = bloco.querySelector('svg[role="img"]')!;
          const comBarra = suporte.filter(
            ([, v]) => v >= primeiro && v <= ultimo && obsSql[v]! > 0,
          ).length;
          expect(svg.querySelectorAll('.observada')).toHaveLength(comBarra);
          expect(svg.querySelectorAll('text.rotulo')).toHaveLength(
            suporte.filter(([, v]) => v >= primeiro && v <= ultimo).length,
          );
          expect(svg.getAttribute('aria-label')).toBe(t.comparacao.rotulo(tipo, sorteios));
          expect((svg.getAttribute('aria-label') ?? '').length).toBeLessThan(220);
          expect(texto(bloco.querySelector('.legenda'))).toContain('Barra cheia: observado em');
          expect(texto(bloco.querySelector('.legenda'))).toContain('Barra vazada com hachura');
        }
      });
    }
  }

  it('Mega-Sena, repetidas: valores raros ficam só na tabela e o texto avisa', async () => {
    const alvo = await montarEm('megasena', '50');
    const bloco = alvo.querySelector('[data-comparacao="repetidas"]')!;
    expect(bloco.querySelector('[data-raros]')).not.toBeNull();
    expect(texto(bloco.querySelector('[data-raros]'))).toBe(t.comparacao.valoresRaros);
    const naTabela = bloco.querySelectorAll('tbody tr').length;
    const nasBarras = bloco.querySelectorAll('svg[role="img"] text.rotulo').length;
    expect(naTabela).toBe(7);
    expect(nasBarras).toBeLessThan(naTabela);
  });

  it('tabela em <details> fechado, região rolável focável, cabeçalhos com scope', async () => {
    const alvo = await montarEm('quina', '50');
    for (const tipo of ['pares', 'repetidas']) {
      const detalhes = alvo.querySelector<HTMLDetailsElement>(
        `[data-comparacao="${tipo}"] details`,
      )!;
      expect(detalhes.open).toBe(false);
      expect(texto(detalhes.querySelector('summary'))).toBe('Ver como tabela');
      expect(detalhes.querySelector('[role="region"]')!.getAttribute('tabindex')).toBe('0');
      expect(detalhes.querySelectorAll('thead th[scope="col"]')).toHaveLength(5);
      expect(detalhes.querySelector('caption')).not.toBeNull();
    }
  });

  it('as hachuras têm ids diferentes por instância e toda referência url(#id) existe', async () => {
    const alvo = await montarEm('lotofacil', '50');
    const ids = Array.from(alvo.querySelectorAll('pattern')).map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const el of alvo.querySelectorAll('[fill^="url(#"]')) {
      const id = el.getAttribute('fill')!.slice(5, -1);
      expect(ids).toContain(id);
    }
  });
});

describe('acessibilidade e honestidade (7.3)', () => {
  it('a região viva continua a mesma: curta, role status, só loteria e janela', async () => {
    const alvo = await montarEm('quina', '50');
    const viva = alvo.querySelector('[data-anuncio]')!;
    expect(viva.getAttribute('role')).toBe('status');
    expect(texto(viva)).toBe('Mostrando Quina, últimos 50 concursos.');
    // Os blocos novos não criam região viva nova (não anunciam nada sozinhos).
    expect(
      alvo.querySelectorAll(
        '[data-bloco="resumo-acaso"] [aria-live], [data-comparacao] [aria-live], [data-bloco="resumo-acaso"] [role="status"], [data-comparacao] [role="status"]',
      ),
    ).toHaveLength(0);
  });

  it('todo SVG com informação tem role="img" e rótulo', async () => {
    const alvo = await montarEm('lotofacil', '100');
    const imagens = alvo.querySelectorAll('svg[role="img"]');
    expect(imagens).toHaveLength(4); // frequência, pares, repetidas, soma
    for (const svg of imagens) expect(svg.getAttribute('aria-label')!.length).toBeGreaterThan(20);
  });

  it('os blocos novos não têm linguagem de previsão, quente/frio ou "devida"', async () => {
    for (const slug of SLUGS) {
      const alvo = await montarEm(slug, '10');
      const blocos = [
        ...alvo.querySelectorAll('[data-bloco="resumo-acaso"], [data-comparacao], [data-legenda]'),
      ]
        .map(texto)
        .join(' ')
        .toLowerCase();
      expect(blocos).not.toMatch(/quente|frio|frias|devid|vai sair|vão sair|palpite|significativ/);
      expect(blocos).toContain('acaso');
      desmontar();
    }
  });

  it('o aviso de que as estatísticas não preveem o sorteio e o de jogo responsável seguem na tela', async () => {
    const alvo = await montarEm('megasena', '50');
    expect(texto(alvo)).toContain(textos.avisos.naoPreveEstatisticas);
    expect(texto(alvo)).toContain(textos.avisos.jogoResponsavel);
  });
});

/** P = ((total − K)/total)^a por BigInt (sem Math.pow), escalada por 10^30. */
function chanceExata(slug: Slug, a: number): number {
  const { total, k } = PARAMS[slug];
  const escala = 10n ** 30n;
  return Number((BigInt(total - k) ** BigInt(a) * escala) / BigInt(total) ** BigInt(a)) / 1e30;
}

describe('soma das dezenas × acaso (7.3)', () => {
  for (const slug of SLUGS) {
    for (const janela of ['10', '50', 'total'] as const) {
      it(`${slug}, janela ${janela}: faixa, contagem dentro dela e tabela batem com a referência`, async () => {
        const alvo = await montarEm(slug, janela);
        const bloco = alvo.querySelector('[data-comparacao-soma]')!;
        expect(bloco).not.toBeNull();
        const r = ref.soma[slug];
        const j = sql.loterias[slug][janela]!;
        const sorteios = historicos[slug].slice(-j.tamanho);
        const somas = sorteios.map(([, , d]) => d.reduce((a, b) => a + b, 0));
        const dentro = somas.filter((v) => v >= r.inferior && v <= r.superior).length;

        // legenda e frase com os números da referência
        const legenda = texto(bloco.querySelector('.legenda'));
        expect(legenda).toContain(`somas de ${r.inferior} a ${r.superior}`);
        expect(legenda).toContain('cada sorteio tem pelo menos 95% de chance');
        expect(texto(bloco.querySelector('[data-soma-dentro]'))).toBe(
          t.soma.acaso.dentro(
            dentro,
            j.tamanho,
            formatarPercentual(dentro / j.tamanho),
            formatarPercentual(r.cobertura_real),
          ),
        );

        // tabela alternativa: linhas, valores e situação (texto, não só cor)
        const linhas = Array.from(bloco.querySelectorAll('tbody tr')).map((tr) =>
          Array.from(tr.querySelectorAll('th, td')).map(texto),
        );
        expect(linhas).toHaveLength(6);
        expect(linhas[0]).toEqual([t.soma.acaso.linhaFaixa, `${r.inferior} a ${r.superior}`, '-']);
        expect(linhas[1]![1]).toBe(formatarDecimal(r.media));
        const posicao = (v: number) =>
          v < r.inferior ? 'Abaixo' : v > r.superior ? 'Acima' : 'Dentro';
        expect(linhas[2]).toEqual([
          t.soma.acaso.linhaMedia,
          formatarDecimal(j.soma_total / j.tamanho),
          posicao(j.soma_total / j.tamanho),
        ]);
        expect(linhas[3]![1]).toBe(j.soma_min.toLocaleString('pt-BR'));
        expect(linhas[3]![2]).toBe(posicao(j.soma_min));
        expect(linhas[4]![1]).toBe(j.soma_max.toLocaleString('pt-BR'));
        expect(linhas[4]![2]).toBe(posicao(j.soma_max));
        expect(linhas[5]![1]).toContain(`${dentro.toLocaleString('pt-BR')} de `);

        // régua: role img com rótulo curto, hachura própria e três rótulos de escala
        const svg = bloco.querySelector('svg[role="img"]')!;
        expect(svg.getAttribute('aria-label')!.length).toBeLessThan(260);
        expect(svg.querySelectorAll('text.rotulo')).toHaveLength(3);
        const id = svg.querySelector('pattern')!.id;
        expect(svg.querySelector('[data-faixa-soma] rect')!.getAttribute('fill')).toBe(
          `url(#${id})`,
        );
        expect(bloco.querySelector('details')!.open).toBe(false);
        expect(bloco.querySelector('caption')).not.toBeNull();
      });
    }
  }
});

describe('atraso × acaso (7.3)', () => {
  for (const slug of SLUGS) {
    it(`${slug}, janela 100: chance por item, resumo e janela inteira batem com a conta exata`, async () => {
      const alvo = await montarEm(slug, '100');
      const j = sql.loterias[slug]['100']!;
      const { total, k } = PARAMS[slug];
      const ranking = j.atraso
        .map((a, i) => ({ dezena: i + 1, a }))
        .filter((x): x is { dezena: number; a: number } => x.a !== null)
        .sort((x, y) => y.a - x.a || x.dezena - y.dezena)
        .slice(0, 5);
      const itens = Array.from(alvo.querySelectorAll('ul.atrasos li'));
      expect(itens).toHaveLength(ranking.length);
      ranking.forEach((r, i) => {
        const chance = itens[i]!.querySelector('[data-chance-atraso]');
        if (r.a === 0) {
          expect(chance).toBeNull();
          return;
        }
        const p = chanceExata(slug, r.a);
        const esperado = p < 0.0005 ? 'menos de 0,1%' : formatarPercentual(p);
        expect(texto(chance)).toBe(t.atraso.acaso.chance(r.a, esperado));
      });
      if (ranking[0]!.a > 0) {
        const y = total * chanceExata(slug, ranking[0]!.a);
        const yTexto = y < 0.05 ? 'menos de 0,1' : formatarDecimal(y);
        expect(texto(alvo.querySelector('[data-resumo-atraso]'))).toBe(
          t.atraso.acaso.resumoMaior(total, ranking[0]!.a, yTexto),
        );
      }
      const pj = chanceExata(slug, j.tamanho);
      const yj = total * pj;
      expect(texto(alvo.querySelector('[data-chance-sem-aparicao]'))).toBe(
        t.atraso.acaso.semAparicao(
          j.tamanho,
          pj < 0.0005 ? 'menos de 0,1%' : formatarPercentual(pj),
          total,
          yj < 0.05 ? 'menos de 0,1' : formatarDecimal(yj),
        ),
      );
      // o bloco da janela inteira fica DENTRO do bloco "sem aparição" (sempre junto do ranking)
      expect(
        alvo.querySelector('[data-bloco="sem-aparicao"] [data-chance-sem-aparicao]'),
      ).not.toBeNull();
      const indep = texto(alvo.querySelector('[data-independente]'));
      expect(indep).toBe(t.atraso.acaso.independente(formatarPercentual(k / total)));
      expect(indep).toContain('Seja qual for o atraso, a chance de a dezena sair segue sendo');
      // a nota "Cada sorteio é independente ... não faz uma dezena ter mais chance" segue ao lado
      expect(texto(alvo)).toContain(t.atraso.nota);
    });
  }

  it('sem "devida", "atrasada para sair" nem previsão nos novos textos de atraso e soma', async () => {
    for (const slug of SLUGS) {
      const alvo = await montarEm(slug, '50');
      const novos = [
        ...alvo.querySelectorAll(
          '[data-chance-atraso], [data-resumo-atraso], [data-chance-sem-aparicao], [data-independente], [data-comparacao-soma]',
        ),
      ]
        .map(texto)
        .join(' ')
        .toLowerCase();
      expect(novos).not.toMatch(/devid|quente|frio|vai sair|vão sair|palpite|significativ|normal/);
      desmontar();
    }
  });

  it('a ordem do resumo do acaso põe o aviso antes das listas de dezenas fora da faixa', async () => {
    const alvo = await montarEm('quina', '10');
    const bloco = alvo.querySelector('[data-bloco="resumo-acaso"]')!;
    const filhos = Array.from(bloco.children);
    const iTexto = filhos.findIndex((c) => c.classList.contains('resumo-texto'));
    const iAviso = filhos.findIndex((c) => c.hasAttribute('data-aviso-faixa'));
    const iListas = filhos
      .map((c, i) => (c.hasAttribute('data-fora-lista') ? i : -1))
      .filter((i) => i >= 0);
    expect(iTexto).toBe(0);
    expect(iAviso).toBe(1);
    for (const i of iListas) expect(i).toBeGreaterThan(iAviso);
  });
});
