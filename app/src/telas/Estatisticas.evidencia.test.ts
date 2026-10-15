// Aba Estatísticas, seção "Por evidência" (7.4): seletor de seção e URL, grupos por grau x direção
// conferidos contra leitura independente das fixtures reais, nulos, estados e honestidade dos
// textos. Servidor falso; nada de rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente, type OpcoesSincronizacao } from '../dados/cliente';
import { garantirArquivos, type ResultadoGarantia } from '../dados/sob-demanda';
import type { DirecaoEvidencia, GrauEvidencia, Slug } from '../dados/tipos';
import { CHAVE_LOTERIA_GUARDADA } from '../lib/loteria-guardada';
import { fraseEvidencia, textos } from '../lib/textos';
import Estatisticas from './Estatisticas.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

interface Fonte {
  lerCache: ReturnType<typeof criarCliente>['lerCache'];
  garantir: (opcoes: OpcoesSincronizacao) => Promise<ResultadoGarantia>;
}

function criarFonte(
  servidor: ServidorFalso,
  arm: ArmazenamentoEmMemoria,
  rede = () => true,
): Fonte {
  const cliente = criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: async (url, init) => {
      if (!rede()) throw new TypeError('Failed to fetch');
      return servidor.fetch(url, init);
    },
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

beforeEach(() => {
  localStorage.clear();
  location.hash = '#/estatisticas';
});

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
  document.body.style.overflow = '';
  vi.restoreAllMocks();
  location.hash = '';
});

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const pausa = (ms = 20) => new Promise((ok) => setTimeout(ok, ms));
const GRUPO_SECAO = textos.estatisticas.rotuloSecao;
const GRUPO_LOTERIA = textos.estatisticas.rotuloLoteria;
const GRUPO_JANELA = textos.estatisticas.rotuloJanela;
const radio = (alvo: HTMLElement, grupo: string, rotulo: string) =>
  Array.from(
    alvo.querySelectorAll<HTMLButtonElement>(
      `[role="radiogroup"][aria-label="${grupo}"] [role="radio"]`,
    ),
  ).find((b) => b.textContent?.includes(rotulo));
const marcada = (alvo: HTMLElement, grupo: string) =>
  Array.from(
    alvo.querySelectorAll(
      `[role="radiogroup"][aria-label="${grupo}"] [role="radio"][aria-checked="true"]`,
    ),
  ).map((b) => texto(b));
const botao = (alvo: HTMLElement, rotulo: string) =>
  Array.from(alvo.querySelectorAll('button')).find((b) => b.textContent?.includes(rotulo));
const dialogo = () => document.querySelector<HTMLElement>('[role="dialog"]');
const teclar = (alvo: Element, key: string) =>
  alvo.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
const gruposNaTela = (alvo: HTMLElement) =>
  Array.from(alvo.querySelectorAll<HTMLElement>('[data-grupo]'));
const esperarGrupos = (alvo: HTMLElement) =>
  vi.waitFor(() => expect(gruposNaTela(alvo).length).toBeGreaterThan(0));
const dezenasDoGrupo = (grupo: Element) =>
  Array.from(grupo.querySelectorAll<HTMLElement>('ul.dezenas button')).map((b) =>
    Number(b.dataset.dezena),
  );

async function montarEvidencia(slug: Slug, servidor = new ServidorFalso()): Promise<HTMLElement> {
  location.hash = `#/estatisticas?loteria=${slug}&secao=evidencia`;
  const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
  await esperarGrupos(alvo);
  return alvo;
}

interface DezenaCrua {
  dezena: number;
  posicao: number;
  variacao_posicao: number | null;
  evidencia_grau: string | null;
  evidencia_direcao: string | null;
}

/** Leitura independente do arquivo: JSON cru, sem passar pelo validador nem pelo agrupador. */
function crua(slug: Slug) {
  const j = JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`)) as {
    concurso: number;
    ultimo_sorteado: number;
    dezenas: DezenaCrua[];
  };
  return j;
}

const ROTULO_GRAU: Record<string, string> = {
  muito_forte: 'Muito forte',
  forte: 'Forte',
  moderada: 'Moderada',
  fraca: 'Fraca',
  muito_fraca: 'Muito fraca',
};
const ROTULO_DIRECAO: Record<string, string> = { vies: 'viés', acaso: 'acaso' };
const ORDEM = ['muito_forte', 'forte', 'moderada', 'fraca', 'muito_fraca'].flatMap((g) => [
  `${g}|vies`,
  `${g}|acaso`,
]);

function gruposEsperados(slug: Slug) {
  const { dezenas } = crua(slug);
  return ORDEM.map((chave) => {
    const [g, d] = chave.split('|') as [string, string];
    const membros = dezenas
      .filter((x) => x.evidencia_grau === g && x.evidencia_direcao === d)
      .toSorted((a, b) => a.posicao - b.posicao);
    return { chave, rotulo: `${ROTULO_GRAU[g]} · ${ROTULO_DIRECAO[d]}`, membros };
  }).filter((x) => x.membros.length > 0);
}

describe('Estatísticas: seletor de seção e URL', () => {
  it('URL antiga (sem secao) continua em Frequência e atraso, com janela e gráfico', async () => {
    location.hash = '#/estatisticas?loteria=quina&janela=10';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    expect(alvo.querySelector('h2')?.textContent).toBe('Estatísticas');
    expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Frequência e atraso']);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 10']);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    expect(gruposNaTela(alvo)).toHaveLength(0);
  });

  it('o seletor tem as duas seções, em radiogroup nomeado e com ✓ em texto', async () => {
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    const g = alvo.querySelector(`[role="radiogroup"][aria-label="${GRUPO_SECAO}"]`)!;
    expect(
      Array.from(g.querySelectorAll('[role="radio"]')).map((r) => texto(r).replace('✓ ', '')),
    ).toEqual(['Frequência e atraso', 'Por evidência']);
    expect(g.querySelectorAll('[role="radio"][aria-checked="true"]')).toHaveLength(1);
    expect(g.querySelectorAll('[role="radio"][tabindex="0"]')).toHaveLength(1);
  });

  it('trocar para "Por evidência" muda a URL, carrega a janela (ignorada) e mostra os grupos', async () => {
    location.hash = '#/estatisticas?loteria=megasena&janela=100';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    radio(alvo, GRUPO_SECAO, 'Por evidência')!.click();
    await esperarGrupos(alvo);
    expect(location.hash).toBe('#/estatisticas?loteria=megasena&secao=evidencia&janela=100');
    expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Por evidência']);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Mega-Sena']);
    // A janela não se aplica: sem chips de janela e sem gráfico de frequência.
    expect(alvo.querySelector(`[role="radiogroup"][aria-label="${GRUPO_JANELA}"]`)).toBeNull();
    expect(alvo.querySelector('svg[role="img"]')).toBeNull();
    expect(alvo.querySelector('h2')?.textContent).toBe('Estatísticas');
  });

  it('a janela escolhida em Frequência sobrevive à ida e volta pela evidência', async () => {
    location.hash = '#/estatisticas?loteria=quina&janela=100';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    radio(alvo, GRUPO_SECAO, 'Por evidência')!.click();
    await esperarGrupos(alvo);
    // Trocar de loteria e abrir o detalhe dentro da evidência mantêm a janela na URL.
    radio(alvo, GRUPO_LOTERIA, 'Lotofácil')!.click();
    await vi.waitFor(() => expect(texto(alvo)).toContain('Próximo concurso 3804'));
    expect(location.hash).toBe('#/estatisticas?loteria=lotofacil&secao=evidencia&janela=100');
    alvo.querySelector<HTMLButtonElement>('button[data-dezena]')!.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(location.hash).toMatch(/&secao=evidencia&janela=100&dezena=\d+$/);
    teclar(dialogo()!, 'Escape');
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    radio(alvo, GRUPO_SECAO, 'Frequência e atraso')!.click();
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 100']);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']);
  });

  it('voltar para "Frequência e atraso" restaura a URL da seção padrão', async () => {
    const alvo = await montarEvidencia('quina');
    radio(alvo, GRUPO_SECAO, 'Frequência e atraso')!.click();
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    expect(location.hash).toBe('#/estatisticas?loteria=quina&janela=50');
    expect(gruposNaTela(alvo)).toHaveLength(0);
    expect(marcada(alvo, GRUPO_JANELA)).toEqual(['✓ 50']);
  });

  it('o hash (botão voltar, link colado) troca a seção', async () => {
    const alvo = await montarEvidencia('lotofacil');
    location.hash = '#/estatisticas?loteria=lotofacil&janela=10';
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Frequência e atraso']));
    await vi.waitFor(() => expect(alvo.querySelector('svg[role="img"]')).not.toBeNull());
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Por evidência']));
    await esperarGrupos(alvo);
  });

  it('secao inválida cai em Frequência e atraso', () => {
    location.hash = '#/estatisticas?loteria=quina&secao=qualquer';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Frequência e atraso']);
  });

  it('loteria: a URL vale mais que o aparelho; sem parâmetro usa o guardado; inválida cai no padrão', async () => {
    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'quina');
    location.hash = '#/estatisticas?loteria=megasena&secao=evidencia';
    let alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrupos(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Mega-Sena']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    location.hash = '#/estatisticas?secao=evidencia';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrupos(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    localStorage.setItem(CHAVE_LOTERIA_GUARDADA, 'loto');
    location.hash = '#/estatisticas?loteria=timemania&secao=evidencia';
    alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrupos(alvo);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']);
  });

  it('trocar de loteria carrega a outra tabela, guarda a escolha e mantém a seção na URL (sem nova entrada)', async () => {
    const alvo = await montarEvidencia('quina');
    const entradas = history.length;
    radio(alvo, GRUPO_LOTERIA, 'Lotofácil')!.click();
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Lotofácil']));
    await vi.waitFor(() => expect(texto(alvo)).toContain('Próximo concurso 3804'));
    expect(location.hash).toBe('#/estatisticas?loteria=lotofacil&secao=evidencia');
    expect(localStorage.getItem(CHAVE_LOTERIA_GUARDADA)).toBe('lotofacil');
    expect(history.length).toBe(entradas);
    expect(marcada(alvo, GRUPO_SECAO)).toEqual(['✓ Por evidência']);
  });

  it('hashchange com outra loteria segue a URL', async () => {
    const alvo = await montarEvidencia('quina');
    location.hash = '#/estatisticas?loteria=megasena&secao=evidencia';
    await vi.waitFor(() => expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Mega-Sena']));
    await vi.waitFor(() => expect(texto(alvo)).toContain('Próximo concurso 3070'));
  });
});

describe('Estatísticas: Por evidência, cabeçalho e textos', () => {
  it('cabeçalho "Próximo concurso N, com dados até o M" e perfil/janela do motor', async () => {
    const alvo = await montarEvidencia('lotofacil');
    const { concurso, ultimo_sorteado } = crua('lotofacil');
    expect(texto(alvo.querySelector('.subtitulo'))).toBe(
      `Próximo concurso ${concurso}, com dados até o ${ultimo_sorteado}`,
    );
    expect(texto(alvo.querySelector('[data-perfil-janela]'))).toContain(
      'Perfil padrao_v1 · janela de 50 concursos',
    );
    expect(texto(alvo.querySelector('[data-perfil-janela]'))).toContain('o do motor');
  });

  it('textos honestos: não prevê, nenhum grupo tem mais chance, jogo responsável, sem linguagem de aposta', async () => {
    const alvo = await montarEvidencia('quina');
    const t = texto(alvo);
    expect(t).toContain(textos.avisos.naoPreveEvidencia);
    expect(t).toContain('não prevê o sorteio');
    expect(t).toContain('nenhum grupo tem mais chance de sair');
    expect(t).toContain(textos.avisos.jogoResponsavel);
    // Grau = o quanto os dados convencem da hipótese favorecida, nunca o tamanho do desvio.
    const explicacao = texto(alvo.querySelector('[data-explicacao]'));
    expect(explicacao).toContain('e não o tamanho do desvio');
    expect(explicacao).toContain('"Viés" não diz se a dezena saiu mais ou menos que o esperado');
    expect(explicacao).toContain('"muito fraca" (Mega-Sena e Quina) e "fraca" (Lotofácil)');
    expect(explicacao).toContain('o máximo que os dados podem dar a favor dele');
    expect(t).not.toContain('foge do que o acaso explicaria');
    expect(t).not.toContain('o acaso explica quase tudo');
    expect(t.toLowerCase()).not.toMatch(
      /quente|frio|frias|melhores|piores|recomendad|palpite|vai sair|vão sair/,
    );
  });

  it('resumo no topo: uma entrada por grupo presente, com a contagem, e quantas estão em graus fracos', async () => {
    const alvo = await montarEvidencia('lotofacil');
    const resumo = Array.from(alvo.querySelectorAll('[data-resumo] li')).map((li) => texto(li));
    expect(resumo).toEqual(
      gruposEsperados('lotofacil').map((g) => `${g.membros.length} ${g.rotulo}`),
    );
    const fracos = crua('lotofacil').dezenas.filter(
      (d) => d.evidencia_grau === 'fraca' || d.evidencia_grau === 'muito_fraca',
    ).length;
    expect(texto(alvo.querySelector('[data-fracos]'))).toBe(
      `${fracos} de 25 dezenas estão nos graus "muito fraca" ou "fraca".`,
    );
    const lista = alvo.querySelector('[data-resumo]')!;
    expect(lista.tagName).toBe('UL');
    expect(lista.getAttribute('aria-label')).toBe('Dezenas por grupo de evidência, de 25 no total');
  });
});

describe('Estatísticas: Por evidência, grupos', () => {
  for (const slug of ['lotofacil', 'megasena', 'quina'] as const) {
    it(`${slug}: um cartão por grupo presente, na ordem, com rótulo, contagem e dezenas em ordem de posição`, async () => {
      const alvo = await montarEvidencia(slug);
      const esperados = gruposEsperados(slug);
      const grupos = gruposNaTela(alvo);
      expect(grupos.map((g) => g.dataset.grupo)).toEqual(esperados.map((g) => g.chave));
      for (const [i, esperado] of esperados.entries()) {
        const grupo = grupos[i]!;
        const secao = grupo.querySelector('section')!;
        // Região com título (h3) e nome acessível.
        expect(texto(secao.querySelector('h3'))).toBe(esperado.rotulo);
        expect(secao.getAttribute('aria-labelledby')).toBe(secao.querySelector('h3')!.id);
        const n = esperado.membros.length;
        expect(texto(secao.querySelector('.contagem'))).toBe(
          `${n} ${n === 1 ? 'dezena' : 'dezenas'}`,
        );
        expect(dezenasDoGrupo(grupo)).toEqual(esperado.membros.map((m) => m.dezena));
        // Lista semântica com nome.
        const ul = secao.querySelector('ul.dezenas')!;
        expect(ul.getAttribute('aria-label')).toBe(
          `${n} ${n === 1 ? 'dezena' : 'dezenas'} do grupo ${esperado.rotulo}, em ordem de posição`,
        );
        expect(ul.querySelectorAll(':scope > li')).toHaveLength(n);
      }
      // Todas as dezenas aparecem, uma vez cada.
      const todas = grupos.flatMap(dezenasDoGrupo);
      expect(todas).toHaveLength(crua(slug).dezenas.length);
      expect(new Set(todas).size).toBe(todas.length);
    });
  }

  it('cada dezena mostra posição e variação com sinal e seta; o leitor de tela ouve o texto por extenso', async () => {
    const alvo = await montarEvidencia('lotofacil');
    const cru = crua('lotofacil').dezenas;
    for (const b of Array.from(alvo.querySelectorAll<HTMLButtonElement>('ul.dezenas button'))) {
      const d = cru.find((x) => x.dezena === Number(b.dataset.dezena))!;
      const v = d.variacao_posicao;
      expect(texto(b.querySelector('.posicao'))).toBe(`${d.posicao}º`);
      const sinal = v === null ? '' : v > 0 ? `▲ +${v}` : v < 0 ? `▼ −${Math.abs(v)}` : '■ 0';
      expect(texto(b.querySelector('.variacao'))).toBe(v === null ? '—' : sinal);
      const falado =
        v === null
          ? 'sem concurso anterior para comparar'
          : v > 0
            ? `subiu ${v} ${v === 1 ? 'posição' : 'posições'}`
            : v < 0
              ? `caiu ${Math.abs(v)} ${Math.abs(v) === 1 ? 'posição' : 'posições'}`
              : 'manteve a posição';
      expect(b.getAttribute('aria-label')).toBe(
        `Dezena ${d.dezena}, ${d.posicao}º lugar, ${falado}`,
      );
      expect(b.querySelector('.posicao')!.getAttribute('aria-hidden')).toBe('true');
      expect(b.getAttribute('aria-haspopup')).toBe('dialog');
    }
  });

  it('variação nula vira travessão (não zero) e o texto diz que não há concurso anterior', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('lotofacil/tabela_geral.json', (j) => {
      j.dezenas[0].variacao_posicao = null;
    });
    const alvo = await montarEvidencia('lotofacil', servidor);
    const dezena = crua('lotofacil').dezenas[0]!.dezena;
    const b = alvo.querySelector<HTMLButtonElement>(`ul.dezenas button[data-dezena="${dezena}"]`)!;
    expect(texto(b.querySelector('.variacao'))).toBe('—');
    expect(b.getAttribute('aria-label')).toContain('sem concurso anterior para comparar');
  });

  it('grau e direção nulos: grupo "Sem evidência calculada" por último; os demais ficam intactos', async () => {
    const servidor = new ServidorFalso();
    let alterada = 0;
    await servidor.alterarJson('megasena/tabela_geral.json', (j) => {
      alterada = j.dezenas[3].dezena;
      j.dezenas[3].evidencia_grau = null;
      j.dezenas[3].evidencia_direcao = null;
    });
    const alvo = await montarEvidencia('megasena', servidor);
    const grupos = gruposNaTela(alvo);
    const ultimo = grupos[grupos.length - 1]!;
    expect(ultimo.dataset.grupo).toBe('sem');
    expect(texto(ultimo.querySelector('h3'))).toBe('Sem evidência calculada');
    expect(dezenasDoGrupo(ultimo)).toEqual([alterada]);
    // Nenhuma outra dezena foi para lá, e a dezena alterada não está em outro grupo.
    const outras = grupos.slice(0, -1).flatMap(dezenasDoGrupo);
    expect(outras).not.toContain(alterada);
    expect(outras).toHaveLength(59);
  });

  for (const slug of ['lotofacil', 'quina'] as const) {
    it(`${slug}: cada cartão traz a frase do grau e da direção logo abaixo do título; a ordem é explicada no topo`, async () => {
      const alvo = await montarEvidencia(slug);
      const ordem = alvo.querySelector('[data-ordem]')!;
      expect(texto(ordem)).toBe(textos.evidencia.ordem);
      expect(texto(ordem)).toContain('não a chance');
      // A frase de ordem vem antes do primeiro cartão de grupo.
      const primeiro = gruposNaTela(alvo)[0]!;
      expect(
        ordem.compareDocumentPosition(primeiro) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      for (const g of gruposNaTela(alvo)) {
        const [grau, direcao] = g.dataset.grupo!.split('|') as [GrauEvidencia, DirecaoEvidencia];
        const secao = g.querySelector('section')!;
        const nota = secao.querySelector('[data-nota-grupo]')!;
        expect(texto(nota)).toBe(fraseEvidencia(grau, direcao));
        // Logo abaixo do h3: nenhum outro conteúdo entre o título e a nota.
        const h3 = secao.querySelector('h3')!;
        const filhos = Array.from(secao.querySelectorAll('h3, [data-nota-grupo], .contagem'));
        expect(filhos).toEqual([h3, nota, secao.querySelector('.contagem')]);
      }
    });
  }

  it('"Sem evidência calculada" tem texto próprio, que não finge ser grau fraco', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('megasena/tabela_geral.json', (j) => {
      j.dezenas[3].evidencia_grau = null;
      j.dezenas[3].evidencia_direcao = null;
    });
    const alvo = await montarEvidencia('megasena', servidor);
    const sem = gruposNaTela(alvo).find((g) => g.dataset.grupo === 'sem')!;
    expect(texto(sem.querySelector('[data-nota-grupo]'))).toBe(
      textos.evidencia.grupo.semEvidenciaNota,
    );
    expect(textos.evidencia.grupo.semEvidenciaNota).toContain('sem cálculo');
  });

  it('grupos vazios não aparecem: a Quina real só tem três cartões', async () => {
    const alvo = await montarEvidencia('quina');
    expect(gruposNaTela(alvo).map((g) => texto(g.querySelector('h3')))).toEqual([
      'Fraca · viés',
      'Muito fraca · viés',
      'Muito fraca · acaso',
    ]);
  });
});

describe('Estatísticas: Por evidência, detalhe da dezena', () => {
  it('tocar numa dezena abre o mesmo detalhe da Força, põe &dezena= na URL, e fechar devolve o foco', async () => {
    const alvo = await montarEvidencia('lotofacil');
    const primeira = crua('lotofacil').dezenas.toSorted((a, b) => a.posicao - b.posicao)[0]!;
    const b = alvo.querySelector<HTMLButtonElement>(`button[data-dezena="${primeira.dezena}"]`)!;
    b.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(location.hash).toBe(
      `#/estatisticas?loteria=lotofacil&secao=evidencia&dezena=${primeira.dezena}`,
    );
    expect(texto(dialogo())).toContain(`Dezena ${primeira.dezena} · ${primeira.posicao}º lugar`);
    const fechar = Array.from(dialogo()!.querySelectorAll('button')).find(
      (x) => x.textContent?.trim() === textos.detalhe.fechar,
    )!;
    fechar.click();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    await vi.waitFor(() => expect(document.activeElement).toBe(b));
  });

  it('o botão voltar do navegador fecha o detalhe e o foco volta à dezena', async () => {
    const alvo = await montarEvidencia('lotofacil');
    const b = alvo.querySelector<HTMLButtonElement>('button[data-dezena]')!;
    b.focus();
    b.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(location.hash).toContain('&dezena=');
    history.back();
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    expect(location.hash).toBe('#/estatisticas?loteria=lotofacil&secao=evidencia');
    await vi.waitFor(() => expect(document.activeElement).toBe(b));
  });

  it('Esc fecha o detalhe, limpa a URL e o foco volta à dezena', async () => {
    const alvo = await montarEvidencia('quina');
    const b = alvo.querySelector<HTMLButtonElement>('button[data-dezena]')!;
    b.click();
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    teclar(dialogo()!, 'Escape');
    await vi.waitFor(() => expect(dialogo()).toBeNull());
    await vi.waitFor(() =>
      expect(location.hash).toBe('#/estatisticas?loteria=quina&secao=evidencia'),
    );
    await vi.waitFor(() => expect(document.activeElement).toBe(b));
  });

  it('abrir direto pela URL com &dezena= mostra o detalhe; dezena inexistente é ignorada e limpa', async () => {
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia&dezena=7';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(dialogo()).not.toBeNull());
    expect(texto(dialogo())).toContain('Dezena 7');
    void unmount(montado!);
    montado = null;
    document.body.innerHTML = '';

    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia&dezena=99';
    const outro = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    await esperarGrupos(outro);
    await vi.waitFor(() =>
      expect(location.hash).toBe('#/estatisticas?loteria=lotofacil&secao=evidencia'),
    );
    expect(dialogo()).toBeNull();
    void alvo;
  });
});

describe('Estatísticas: Por evidência, estados', () => {
  it('antes do cache: esqueleto discreto e nenhum grupo; a região viva existe e está vazia', () => {
    location.hash = '#/estatisticas?loteria=quina&secao=evidencia';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria()));
    expect(alvo.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(texto(alvo)).toContain(textos.evidencia.carregando);
    expect(gruposNaTela(alvo)).toHaveLength(0);
    expect(texto(alvo.querySelector('[data-anuncio]'))).toBe('');
    expect(texto(alvo)).toContain(textos.avisos.naoPreveEvidencia);
  });

  it('com dados, a região viva anuncia a loteria e muda só o texto ao trocar', async () => {
    const alvo = await montarEvidencia('quina');
    const regiao = alvo.querySelector('[data-anuncio]')!;
    expect(regiao.getAttribute('role')).toBe('status');
    expect(texto(regiao)).toBe('Mostrando Quina, dezenas agrupadas por evidência.');
    radio(alvo, GRUPO_LOTERIA, 'Mega-Sena')!.click();
    await vi.waitFor(() =>
      expect(texto(regiao)).toBe('Mostrando Mega-Sena, dezenas agrupadas por evidência.'),
    );
    expect(alvo.querySelector('[data-anuncio]')).toBe(regiao);
  });

  it('atualizado: linha de estado com a data do arquivo', async () => {
    const alvo = await montarEvidencia('lotofacil');
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-estado]'))).toBe('Atualizado em 14/10/2026 09:30.'),
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('sem cache e sem rede: mensagem clara e "Tentar de novo" busca de novo', async () => {
    let rede = false;
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria(), () => rede));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(textos.sincronizacao.semDados),
    );
    expect(gruposNaTela(alvo)).toHaveLength(0);
    rede = true;
    botao(alvo, textos.evidencia.tentarDeNovo)!.click();
    await esperarGrupos(alvo);
    expect(botao(alvo, textos.evidencia.tentarDeNovo)).toBeUndefined();
  });

  it('loteria nula no índice: aviso informativo, sem erro', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    servidor.gravarIndice({ ...indice, loterias: { ...indice.loterias, lotofacil: null } });
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(texto(alvo)).toContain(textos.evidencia.semLoteria));
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
    expect(gruposNaTela(alvo)).toHaveLength(0);
  });

  it('em cache e sem rede: mostra os grupos com a data do arquivo, sem tratar como erro', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({
      arquivos: ['tabela_geral'],
      loterias: ['lotofacil'],
    });
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(criarFonte(servidor, arm, () => false));
    await esperarGrupos(alvo);
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-estado]'))).toBe(
        'Sem rede, mostrando dados de 14/10/2026 09:30.',
      ),
    );
    expect(alvo.querySelector('[role="alert"]')).toBeNull();
  });

  it('o cache aparece antes de a rede responder', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    const base = criarFonte(servidor, arm);
    await base.garantir({ arquivos: ['tabela_geral'], loterias: ['lotofacil'] });
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar({ lerCache: base.lerCache, garantir: () => new Promise(() => undefined) });
    await esperarGrupos(alvo);
    expect(texto(alvo.querySelector('[data-estado]'))).toContain('Verificando atualizações');
  });

  it('arquivo adulterado depois de ter cache: Aviso de erro e os grupos anteriores continuam', async () => {
    const servidor = new ServidorFalso();
    const arm = new ArmazenamentoEmMemoria();
    await criarFonte(servidor, arm).garantir({ arquivos: ['tabela_geral'], loterias: ['quina'] });
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.tabela_geral.sha256 = 'c'.repeat(64);
    servidor.gravarIndice(indice);
    location.hash = '#/estatisticas?loteria=quina&secao=evidencia';
    const alvo = montar(criarFonte(servidor, arm));
    await esperarGrupos(alvo);
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(texto(alvo.querySelector('[role="alert"]'))).toContain('Dado recusado');
    expect(gruposNaTela(alvo)).toHaveLength(3);
  });

  it('sem cache e arquivo recusado (schema_version 2): recusa clara e nenhum grupo', async () => {
    const servidor = new ServidorFalso();
    const adulterada = lerFixtureTexto('lotofacil/tabela_geral.json').replace(
      '"schema_version": 1',
      '"schema_version": 2',
    );
    await servidor.trocar('lotofacil/tabela_geral.json', adulterada);
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[role="alert"]'))).toContain(
        textos.evidencia.recusadoSemAnterior,
      ),
    );
    expect(gruposNaTela(alvo)).toHaveLength(0);
  });

  it('grau desconhecido no arquivo: a validação recusa o arquivo inteiro (nada de grupo inventado)', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('lotofacil/tabela_geral.json', (j) => {
      j.dezenas[0].evidencia_grau = 'extraordinaria';
    });
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(criarFonte(servidor, new ArmazenamentoEmMemoria()));
    await vi.waitFor(() => expect(alvo.querySelector('[role="alert"]')).not.toBeNull());
    expect(gruposNaTela(alvo)).toHaveLength(0);
  });

  it('resposta atrasada de uma loteria anterior não substitui a escolhida', async () => {
    const base = criarFonte(new ServidorFalso(), new ArmazenamentoEmMemoria());
    let liberar: () => void = () => undefined;
    let pediu = false;
    const portao = new Promise<void>((ok) => (liberar = ok));
    const fonte: Fonte = {
      lerCache: base.lerCache,
      garantir: async (o) => {
        if (o.loterias?.[0] === 'lotofacil') {
          pediu = true;
          await portao;
        }
        return base.garantir(o);
      },
    };
    location.hash = '#/estatisticas?loteria=lotofacil&secao=evidencia';
    const alvo = montar(fonte);
    await vi.waitFor(() => expect(pediu).toBe(true));
    radio(alvo, GRUPO_LOTERIA, 'Quina')!.click();
    await esperarGrupos(alvo);
    liberar();
    await pausa(40);
    expect(marcada(alvo, GRUPO_LOTERIA)).toEqual(['✓ Quina']);
    expect(gruposNaTela(alvo)).toHaveLength(3);
  });
});
