// Teste de componente da Entenda: cada estado, com fixtures reais e sem rede.
import { flushSync, mount, unmount, type Component } from 'svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ServidorFalso, URL_BASE_TESTE, lerFixtureTexto } from '../../testes/servidor-falso';
import { abrirComCache, type SituacaoDados } from '../dados/abertura';
import { ArmazenamentoEmMemoria } from '../dados/armazenamento';
import { criarCliente } from '../dados/cliente';
import { OPCOES_DA_ABERTURA } from '../dados/sob-demanda';
import { DIRECOES_EVIDENCIA, GRAUS_EVIDENCIA, SLUGS } from '../dados/tipos';
import { validarTabelaGeral } from '../dados/validacao';
import { frasesEvidencia, textos } from '../lib/textos';
import Entenda from './Entenda.svelte';
import { propsReativas } from './props-reativas.svelte';

const T1 = new Date('2026-10-14T12:30:00Z');

function criar(servidor = new ServidorFalso()) {
  return criarCliente({
    urlBase: URL_BASE_TESTE,
    fetch: servidor.fetch,
    armazenamento: new ArmazenamentoEmMemoria(),
    agora: () => T1,
  });
}
type Cliente = ReturnType<typeof criar>;

const abrir = (c: Cliente): Promise<SituacaoDados> =>
  abrirComCache(
    c,
    () => undefined,
    () => T1,
    OPCOES_DA_ABERTURA,
  );

let montado: Record<string, unknown> | null = null;
function montar(
  fonte: Pick<Cliente, 'lerCache'>,
  situacao: SituacaoDados | null,
  extra: { atualizacaoPronta?: boolean } = {},
): HTMLElement {
  const alvo = document.createElement('div');
  document.body.appendChild(alvo);
  montado = mount(Entenda as unknown as Component<Record<string, unknown>>, {
    target: alvo,
    props: { fonte, situacao, versaoApp: '9.8.7', atualizacaoPronta: false, ...extra },
  });
  flushSync();
  return alvo;
}

afterEach(() => {
  if (montado) void unmount(montado);
  montado = null;
  document.body.innerHTML = '';
});

const texto = (e: Element | null | undefined) => (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
const secao = (alvo: HTMLElement, titulo: string) =>
  Array.from(alvo.querySelectorAll('section')).find((s) => texto(s.querySelector('h3')) === titulo);

/** Cliente com ultimo e tabela_geral das 3 loterias guardados. */
async function clienteComTabelas(servidor = new ServidorFalso()) {
  const c = criar(servidor);
  await c.sincronizar({ arquivos: ['ultimo', 'tabela_geral'] });
  return { c, situacao: await abrir(c) };
}

describe('Entenda: estrutura e acessibilidade', () => {
  it('h2 da tela, h3 nos cartões (section com aria-labelledby), sem h1 e sem salto de nível', async () => {
    const { c, situacao } = await clienteComTabelas();
    const alvo = montar(c, situacao);
    await vi.waitFor(() => expect(alvo.querySelector('.loteria code')).not.toBeNull());
    expect(alvo.querySelector('h1')).toBeNull();
    expect(texto(alvo.querySelector('h2'))).toBe('Entenda');
    expect(alvo.querySelector('h2')?.id).toBe('titulo-aba');
    expect(Array.from(alvo.querySelectorAll('section > h3')).map(texto)).toEqual([
      'Este app não prevê sorteios',
      'O que é o índice de força',
      'O que é o G-1',
      'O que é a evidência',
      'Como ler a tabela de força',
      'De onde vêm os dados',
      'Instalar o app',
    ]);
    const niveis = Array.from(alvo.querySelectorAll('h2,h3,h4')).map((h) => Number(h.tagName[1]));
    niveis.forEach((n, i) => {
      if (i > 0) expect(n - (niveis[i - 1] ?? 0)).toBeLessThanOrEqual(1);
    });
    for (const s of Array.from(alvo.querySelectorAll('section'))) {
      const id = s.getAttribute('aria-labelledby');
      expect(id).toBeTruthy();
      expect(s.querySelector(`[id="${id}"]`)).not.toBeNull();
    }
  });

  it('destaque, jogo responsável e exemplo marcado como exemplo', async () => {
    const { c, situacao } = await clienteComTabelas();
    const alvo = montar(c, situacao);
    const destaque = secao(alvo, textos.entenda.destaque.titulo);
    for (const item of textos.entenda.destaque.itens) expect(texto(destaque)).toContain(item);
    expect(texto(alvo)).toContain(
      'Jogue com responsabilidade. Apostas são para maiores de 18 anos.',
    );
    expect(texto(secao(alvo, 'Como ler a tabela de força'))).toContain(
      textos.entenda.leitura.exemploRotulo,
    );
  });

  it('explica os 5 graus nas 2 direções (frases importadas) e os 9 componentes', async () => {
    const { c, situacao } = await clienteComTabelas();
    const alvo = montar(c, situacao);
    const t = texto(alvo);
    for (const g of GRAUS_EVIDENCIA)
      for (const d of DIRECOES_EVIDENCIA) expect(t).toContain(frasesEvidencia[g][d]);
    const indice = secao(alvo, textos.entenda.indice.titulo);
    expect(indice?.querySelectorAll('dl dt')).toHaveLength(9);
    expect(t).toContain('sem dados na janela');
  });
});

describe('Entenda: De onde vêm os dados', () => {
  it('com tabelas em cache: valores reais, hash abreviado com o completo à mão', async () => {
    const { c, situacao } = await clienteComTabelas();
    const alvo = montar(c, situacao);
    await vi.waitFor(() => expect(alvo.querySelectorAll('.loteria summary code')).toHaveLength(3));
    const dados = secao(alvo, 'De onde vêm os dados');
    const t = texto(dados);
    expect(t).toContain('Resultados planilhas oficiais da Caixa');
    expect(texto(alvo.querySelector('[data-testid="atualizado-em"]'))).toBe('14/10/2026 09:30');
    expect(t).toContain('Formato dos dados versão 1');
    expect(t).toContain('Versão do app 9.8.7');
    expect(t).toContain('Atualização do app nenhuma esperando');
    for (const slug of SLUGS) {
      const tabela = validarTabelaGeral(
        JSON.parse(lerFixtureTexto(`${slug}/tabela_geral.json`)),
        slug,
      );
      const bloco = dados?.querySelector(`[id="entenda-${slug}"]`)?.closest('section');
      const tb = texto(bloco);
      expect(tb).toContain(textos.loterias[slug]);
      expect(tb).toContain(`Perfil de pesos ${tabela.perfil}`);
      expect(tb).toContain('Janela últimos 50 concursos');
      expect(tb).toContain('Normalização Padronização (z-score)');
      expect(tb).toContain(`Versão das medidas ${tabela.featureVersion}`);
      expect(tb).toContain(`Último concurso ${tabela.ultimoSorteado} (`);
      const resumo = bloco?.querySelector('summary');
      expect(texto(resumo?.querySelector('code'))).toBe(tabela.hashToml.slice(0, 8));
      expect(resumo?.getAttribute('title')).toBe(tabela.hashToml);
      expect(texto(bloco?.querySelector('code.completo'))).toBe(tabela.hashToml);
    }
  });

  it('sem tabela em cache: "ainda não baixada" e o último concurso continua aparecendo', async () => {
    const c = criar();
    const situacao = await abrir(c); // só ultimo.json
    const alvo = montar(c, situacao);
    await vi.waitFor(() => expect(texto(alvo)).toContain('tabela de força ainda não baixada'));
    expect(alvo.querySelectorAll('.loteria code')).toHaveLength(0);
    const dados = texto(secao(alvo, 'De onde vêm os dados'));
    expect(dados.match(/tabela de força ainda não baixada/g)).toHaveLength(3);
    expect(dados.match(/Último concurso \d+ \(/g)).toHaveLength(3);
  });

  it('sem nada: "ainda não atualizado" e "ainda sem dados", sem quebrar e sem usar a rede', async () => {
    const servidor = new ServidorFalso();
    const c = criar(servidor);
    const alvo = montar(c, null);
    await vi.waitFor(() =>
      expect(texto(alvo.querySelector('[data-testid="atualizado-em"]'))).toBe(
        'ainda não atualizado',
      ),
    );
    expect(texto(alvo)).toContain('Último concurso ainda sem dados');
    expect(texto(alvo)).toContain('tabela de força ainda não baixada');
    expect(texto(alvo)).toContain('Formato dos dados versão 1');
    expect(servidor.pedidos).toEqual([]);
  });

  it('abrir a Entenda só lê o cache: nenhum pedido de rede', async () => {
    const servidor = new ServidorFalso();
    const { c, situacao } = await clienteComTabelas(servidor);
    const antes = servidor.pedidos.length;
    const alvo = montar(c, situacao);
    await vi.waitFor(() => expect(alvo.querySelectorAll('.loteria summary code')).toHaveLength(3));
    expect(servidor.pedidos.length).toBe(antes);
  });

  it('atualização do app esperando é dita na seção de dados', async () => {
    const { c, situacao } = await clienteComTabelas();
    const alvo = montar(c, situacao, { atualizacaoPronta: true });
    expect(texto(secao(alvo, 'De onde vêm os dados'))).toContain(
      textos.sincronizacao.atualizacaoDisponivel,
    );
  });

  it('quando a situação muda (tabelas baixadas em segundo plano), a tela relê o cache', async () => {
    const c = criar();
    const situacao = await abrir(c);
    const lerCache = vi.fn((o: Parameters<Cliente['lerCache']>[0]) => c.lerCache(o));
    const alvo = document.createElement('div');
    document.body.appendChild(alvo);
    const props = propsReativas({
      fonte: { lerCache },
      situacao,
      versaoApp: '1',
      atualizacaoPronta: false,
    });
    montado = mount(Entenda as unknown as Component<Record<string, unknown>>, {
      target: alvo,
      props,
    });
    flushSync();
    await vi.waitFor(() => expect(texto(alvo)).toContain('tabela de força ainda não baixada'));
    expect(lerCache).toHaveBeenCalledTimes(1);

    await c.sincronizar({ arquivos: ['tabela_geral'] });
    props.situacao = await abrir(c);
    flushSync();
    await vi.waitFor(() => expect(alvo.querySelectorAll('.loteria summary code')).toHaveLength(3));
    expect(lerCache).toHaveBeenCalledTimes(2);
  });
});
