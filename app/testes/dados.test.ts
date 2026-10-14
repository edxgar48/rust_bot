// @vitest-environment node
import { describe, expect, it } from 'vitest';
import { ArmazenamentoEmMemoria } from '../src/dados/armazenamento';
import { criarCliente, type ResultadoArquivo } from '../src/dados/cliente';
import { URL_BASE_PADRAO } from '../src/dados/config';
import { ErroDados, ErroSchemaNaoSuportado } from '../src/dados/erros';
import { sha256Hex } from '../src/dados/hash';
import { PARAMETROS_LOTERIA, SLUGS, type ChaveArquivo, type Slug } from '../src/dados/tipos';
import {
  validarHistorico,
  validarIndice,
  validarTabelaGeral,
  validarUltimo,
} from '../src/dados/validacao';
import {
  CAMINHOS,
  ServidorFalso,
  URL_BASE_TESTE,
  lerFixture,
  lerFixtureTexto,
} from './servidor-falso';

function novoCliente(servidor: ServidorFalso, armazenamento = new ArmazenamentoEmMemoria()) {
  const cliente = criarCliente({ urlBase: URL_BASE_TESTE, fetch: servidor.fetch, armazenamento });
  return { cliente, armazenamento };
}

function falhou<K extends ChaveArquivo>(r: ResultadoArquivo<K> | undefined): ErroDados {
  expect(r?.estado).toBe('falhou');
  if (r?.estado !== 'falhou') throw new Error('esperava falha');
  return r.erro;
}

const json = (caminho: string) => JSON.parse(lerFixtureTexto(caminho));

describe('configuração', () => {
  it('URL base padrão é a do contrato', () => {
    expect(URL_BASE_PADRAO).toBe(
      'https://raw.githubusercontent.com/edxgar48/rust_spider_dados/main/v1/',
    );
  });

  it('usa a URL base configurada, com ou sem barra final', async () => {
    const servidor = new ServidorFalso();
    const cliente = criarCliente({
      urlBase: 'https://dados.exemplo.test/v1',
      fetch: servidor.fetch,
      armazenamento: new ArmazenamentoEmMemoria(),
    });
    await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(servidor.pedidos).toEqual(['index.json', 'quina/ultimo.json']);
  });
});

describe('fixtures reais passam na validação', () => {
  it('o hash e o tamanho do índice batem com os arquivos copiados', async () => {
    const indice = validarIndice(json('index.json'));
    for (const slug of SLUGS) {
      const l = indice.loterias[slug];
      expect(l, slug).not.toBeNull();
      for (const d of Object.values(l!.arquivos)) {
        const bytes = lerFixture(d.caminho);
        expect(bytes.byteLength, d.caminho).toBe(d.bytes);
        // O hash do índice foi calculado pelo Rust, independente do Web Crypto do app.
        expect(await sha256Hex(bytes), d.caminho).toBe(d.sha256);
      }
    }
  });

  it.each(SLUGS)('%s: ultimo, historico e tabela_geral', (slug: Slug) => {
    const p = PARAMETROS_LOTERIA[slug];
    const ultimo = validarUltimo(json(`${slug}/ultimo.json`), slug);
    const historico = validarHistorico(json(`${slug}/historico.json`), slug);
    const tabela = validarTabelaGeral(json(`${slug}/tabela_geral.json`), slug);

    expect(ultimo.dezenas).toHaveLength(p.dezenasSorteadas);
    const ultimoDoHistorico = historico.concursos.at(-1)!;
    expect(ultimoDoHistorico[0]).toBe(ultimo.concurso);
    expect(ultimoDoHistorico[1]).toBe(ultimo.data);
    expect(ultimoDoHistorico[2]).toEqual(ultimo.dezenas);
    expect(historico.concursos[0]![0]).toBe(1);
    expect(historico.concursos).toHaveLength(ultimo.concurso);

    expect(tabela.dezenas).toHaveLength(p.totalDezenas);
    expect(tabela.dezenas.map((d) => d.posicao)).toEqual(
      Array.from({ length: p.totalDezenas }, (_, i) => i + 1),
    );
    expect(new Set(tabela.dezenas.map((d) => d.dezena)).size).toBe(p.totalDezenas);
    expect(tabela.ultimoSorteado).toBe(ultimo.concurso);
    expect(tabela.concurso).toBe(ultimo.concurso + 1);
  });

  it('o índice declara o mesmo último concurso de ultimo.json', () => {
    const indice = validarIndice(json('index.json'));
    for (const slug of SLUGS) {
      const ultimo = validarUltimo(json(`${slug}/ultimo.json`), slug);
      expect(indice.loterias[slug]?.ultimoConcurso).toBe(ultimo.concurso);
      expect(indice.loterias[slug]?.data).toBe(ultimo.data);
    }
  });

  it('componentes null do arquivo real continuam null (nunca viram zero)', () => {
    const bruto = json('quina/tabela_geral.json');
    const tabela = validarTabelaGeral(bruto, 'quina');
    let nulos = 0;
    for (const [i, d] of tabela.dezenas.entries()) {
      const b = bruto.dezenas[i];
      expect(d.maiorEspaco).toBe(b.maior_espaco);
      expect(d.somaVariacoes).toBe(b.soma_variacoes);
      expect(d.espacoFinal).toBe(b.espaco_final);
      if (b.maior_espaco === null) nulos += 1;
    }
    expect(nulos).toBeGreaterThan(0);
  });
});

describe('campos opcionais e desconhecidos', () => {
  it('ignora campo desconhecido em qualquer objeto e não o repassa', () => {
    const indice = json('index.json');
    indice.campo_novo = 1;
    indice.loterias.quina.campo_novo = { a: 1 };
    indice.loterias.quina.arquivos.ultimo.campo_novo = 'x';
    const ok = validarIndice(indice);
    expect(ok).not.toHaveProperty('campo_novo');

    const tabela = json('quina/tabela_geral.json');
    tabela.evidencia_do_indice = { grau: 'forte' };
    tabela.dezenas[0].campo_novo = 7;
    const t = validarTabelaGeral(tabela, 'quina');
    expect(t).not.toHaveProperty('evidencia_do_indice');
    expect(t.dezenas[0]).not.toHaveProperty('campo_novo');
    expect(t.dezenas[0]!.dezena).toBe(tabela.dezenas[0].dezena);

    const ultimo = json('quina/ultimo.json');
    ultimo.novo = true;
    expect(validarUltimo(ultimo, 'quina')).not.toHaveProperty('novo');

    const hist = json('quina/historico.json');
    hist.novo = true;
    expect(validarHistorico(hist, 'quina').concursos.length).toBeGreaterThan(0);
  });

  it('ausência de campo opcional equivale a null', () => {
    const tabela = json('quina/tabela_geral.json');
    const d = tabela.dezenas[0];
    delete d.variacao_posicao;
    delete d.maior_espaco;
    delete d.maior_espaco_ocorrencias;
    delete d.espaco_final;
    delete d.evidencia_grau;
    delete d.evidencia_direcao;
    const t = validarTabelaGeral(tabela, 'quina');
    expect(t.dezenas[0]).toMatchObject({
      variacaoPosicao: null,
      maiorEspaco: null,
      maiorEspacoOcorrencias: null,
      espacoFinal: null,
      evidenciaGrau: null,
      evidenciaDirecao: null,
    });
  });

  it('loteria null (ou ausente) no índice é tolerada', () => {
    const indice = json('index.json');
    indice.loterias.megasena = null;
    delete indice.loterias.quina;
    const ok = validarIndice(indice);
    expect(ok.loterias.megasena).toBeNull();
    expect(ok.loterias.quina).toBeNull();
    expect(ok.loterias.lotofacil).not.toBeNull();
  });

  it('slug desconhecido no índice é ignorado', () => {
    const indice = json('index.json');
    indice.loterias.timemania = indice.loterias.quina;
    expect(Object.keys(validarIndice(indice).loterias)).toEqual([...SLUGS]);
  });
});

describe('formas inválidas', () => {
  it('schema_version 2 gera erro específico, em qualquer arquivo', () => {
    for (const [caminho, validar] of [
      ['index.json', (v: unknown) => validarIndice(v)],
      ['quina/ultimo.json', (v: unknown) => validarUltimo(v)],
      ['quina/historico.json', (v: unknown) => validarHistorico(v)],
      ['quina/tabela_geral.json', (v: unknown) => validarTabelaGeral(v)],
    ] as const) {
      const valor = json(caminho);
      valor.schema_version = 2;
      // Mesmo que a forma tenha mudado na v2, o erro é o de versão.
      valor.loterias = 'outra forma';
      valor.concursos = 'outra forma';
      expect(() => validar(valor), caminho).toThrow(ErroSchemaNaoSuportado);
      try {
        validar(valor);
      } catch (e) {
        expect((e as ErroSchemaNaoSuportado).codigo).toBe('schema_version');
        expect((e as ErroSchemaNaoSuportado).versaoEncontrada).toBe(2);
        expect((e as Error).message).toContain('schema_version 2');
      }
    }
  });

  it('schema_version ausente ou texto também é recusado como versão', () => {
    const v = json('quina/ultimo.json');
    delete v.schema_version;
    expect(() => validarUltimo(v)).toThrow(ErroSchemaNaoSuportado);
    v.schema_version = '1';
    expect(() => validarUltimo(v)).toThrow(ErroSchemaNaoSuportado);
  });

  it('dezena duplicada', () => {
    const v = json('quina/ultimo.json');
    v.dezenas = [2, 7, 7, 61, 79];
    expect(() => validarUltimo(v, 'quina')).toThrow(/dezena 7 repetida/);
  });

  it('dezenas fora de ordem', () => {
    const v = json('quina/ultimo.json');
    v.dezenas = [7, 2, 50, 61, 79];
    expect(() => validarUltimo(v, 'quina')).toThrow(/fora de ordem crescente/);
  });

  it('dezena fora do intervalo, não inteira ou em quantidade errada', () => {
    const v = json('quina/ultimo.json');
    v.dezenas = [2, 7, 50, 61, 81];
    expect(() => validarUltimo(v, 'quina')).toThrow(/fora do intervalo 1 a 80/);
    v.dezenas = [2, 7, 50, 61, '79'];
    expect(() => validarUltimo(v, 'quina')).toThrow(/número inteiro/);
    v.dezenas = [2, 7, 50, 61];
    expect(() => validarUltimo(v, 'quina')).toThrow(/sorteia 5 dezenas/);
  });

  it('historico com dezena fora de ordem, buraco ou repetição de concurso', () => {
    const h = json('quina/historico.json');
    h.concursos[10][2] = [...h.concursos[10][2]].reverse();
    expect(() => validarHistorico(h, 'quina')).toThrow(/concursos\[10\]\[2\]/);

    const buraco = json('quina/historico.json');
    buraco.concursos.splice(5, 1);
    expect(() => validarHistorico(buraco, 'quina')).toThrow(/sem buraco nem repetição/);

    const rep = json('quina/historico.json');
    rep.concursos[3][0] = rep.concursos[2][0];
    expect(() => validarHistorico(rep, 'quina')).toThrow(/sem buraco nem repetição/);
  });

  it('data inválida', () => {
    const v = json('quina/ultimo.json');
    v.data = '2026-02-30';
    expect(() => validarUltimo(v, 'quina')).toThrow(/data válida AAAA-MM-DD/);
    v.data = '07/10/2026';
    expect(() => validarUltimo(v, 'quina')).toThrow(/data válida AAAA-MM-DD/);
  });

  it('loteria diferente da esperada', () => {
    expect(() => validarUltimo(json('quina/ultimo.json'), 'megasena')).toThrow(
      /esperada megasena, veio quina/,
    );
  });

  it('tabela com quantidade de dezenas errada', () => {
    const t = json('lotofacil/tabela_geral.json');
    t.dezenas.pop();
    expect(() => validarTabelaGeral(t, 'lotofacil')).toThrow(/tem 25 dezenas, vieram 24/);
  });

  it('tabela com posição repetida, dezena repetida ou fora de ordem de total', () => {
    const pos = json('lotofacil/tabela_geral.json');
    pos.dezenas[3].posicao = 2;
    expect(() => validarTabelaGeral(pos, 'lotofacil')).toThrow(/esperada a posição 4/);

    const rep = json('lotofacil/tabela_geral.json');
    rep.dezenas[1].dezena = rep.dezenas[0].dezena;
    expect(() => validarTabelaGeral(rep, 'lotofacil')).toThrow(/repetida/);

    const ord = json('lotofacil/tabela_geral.json');
    ord.dezenas[5].total = ord.dezenas[0].total + 1;
    expect(() => validarTabelaGeral(ord, 'lotofacil')).toThrow(/decrescente/);
  });

  it('tabela com tipos errados', () => {
    const t = json('quina/tabela_geral.json');
    t.dezenas[0].frequencia = '7';
    expect(() => validarTabelaGeral(t, 'quina')).toThrow(/dezenas\[0\]\.frequencia.*inteiro/);

    const u = json('quina/tabela_geral.json');
    u.dezenas[0].total = null;
    expect(() => validarTabelaGeral(u, 'quina')).toThrow(/total.*número finito/);

    const w = json('quina/tabela_geral.json');
    w.hash_toml = 'ABC';
    expect(() => validarTabelaGeral(w, 'quina')).toThrow(/hash_toml/);
  });

  it('valores enumerados desconhecidos são recusados, não exibidos', () => {
    const t = json('quina/tabela_geral.json');
    t.dezenas[0].evidencia_grau = 'extrema';
    expect(() => validarTabelaGeral(t, 'quina')).toThrow(/evidencia_grau.*desconhecido/);

    const d = json('quina/tabela_geral.json');
    d.dezenas[0].evidencia_direcao = 'talvez';
    expect(() => validarTabelaGeral(d, 'quina')).toThrow(/evidencia_direcao.*desconhecido/);

    const n = json('quina/tabela_geral.json');
    n.normalizacao = 'robusta';
    expect(() => validarTabelaGeral(n, 'quina')).toThrow(/normalizacao.*desconhecido/);
  });

  it('caminho do índice que escapa da base é recusado', () => {
    for (const ruim of ['../segredo.json', 'https://x.test/a.json', '/abs.json', 'a\\b.json']) {
      const indice = json('index.json');
      indice.loterias.quina.arquivos.ultimo.caminho = ruim;
      expect(() => validarIndice(indice), ruim).toThrow(/caminho relativo inválido/);
    }
  });

  it('mensagens de erro são ErroDados com codigo forma', () => {
    const v = json('quina/ultimo.json');
    v.concurso = 'x';
    try {
      validarUltimo(v, 'quina');
      expect.unreachable();
    } catch (e) {
      expect(e).toBeInstanceOf(ErroDados);
      expect((e as ErroDados).codigo).toBe('forma');
      expect((e as Error).message).toMatch(/^ultimo\.json\.concurso:/);
    }
  });
});

describe('fluxo do cliente', () => {
  it('primeira sincronização baixa e valida os 9 arquivos reais', async () => {
    const servidor = new ServidorFalso();
    const { cliente, armazenamento } = novoCliente(servidor);
    const r = await cliente.sincronizar();
    expect(servidor.pedidosDeDados().sort()).toEqual([...CAMINHOS].sort());
    for (const slug of SLUGS) {
      const l = r.loterias[slug]!;
      for (const chave of ['ultimo', 'tabela_geral', 'historico'] as const) {
        expect(l.arquivos[chave]?.estado, `${slug}/${chave}`).toBe('baixado');
      }
    }
    expect(armazenamento.tamanho).toBe(9);
  });

  it('segunda sincronização sem mudanças só baixa o índice', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar();
    servidor.pedidos.length = 0;
    const r = await cliente.sincronizar();
    expect(servidor.pedidos).toEqual(['index.json']);
    expect(r.loterias.quina!.arquivos.historico?.estado).toBe('em_cache');
    expect(r.loterias.quina!.arquivos.historico?.dados).not.toBeNull();
  });

  it('só o arquivo com sha256 novo é rebaixado', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar();

    // Novo concurso na Quina: ultimo.json muda (concurso 7142 fictício) e o índice acompanha.
    await servidor.alterarJson('quina/ultimo.json', (j) => {
      j.concurso += 1;
      j.data = '2026-10-14';
    });
    const indice = servidor.indice();
    indice.loterias.quina.ultimo_concurso += 1;
    indice.loterias.quina.data = '2026-10-14';
    servidor.gravarIndice(indice);

    servidor.pedidos.length = 0;
    const r = await cliente.sincronizar();
    expect(servidor.pedidos).toEqual(['index.json', 'quina/ultimo.json']);
    expect(r.loterias.quina!.arquivos.ultimo?.estado).toBe('baixado');
    expect(r.loterias.quina!.arquivos.tabela_geral?.estado).toBe('em_cache');
    expect(r.loterias.lotofacil!.arquivos.ultimo?.estado).toBe('em_cache');
  });

  it('gerado_em novo no índice, sem mudança de hash, não rebaixa nada', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar();
    const indice = servidor.indice();
    indice.gerado_em = '2030-01-01T00:00:00Z';
    servidor.gravarIndice(indice);
    servidor.pedidos.length = 0;
    await cliente.sincronizar();
    expect(servidor.pedidos).toEqual(['index.json']);
  });

  it('respeita o recorte de loterias e arquivos pedidos', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['megasena'], arquivos: ['tabela_geral'] });
    expect(servidor.pedidos).toEqual(['index.json', 'megasena/tabela_geral.json']);
    expect(Object.keys(r.loterias)).toEqual(['megasena']);
  });

  it('loteria null no índice não baixa nada e vira null no resultado', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    indice.loterias.megasena = null;
    servidor.gravarIndice(indice);
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar();
    expect(r.loterias.megasena).toBeNull();
    expect(r.loterias.quina).not.toBeNull();
    expect(servidor.pedidos.some((p) => p.startsWith('megasena/'))).toBe(false);
  });

  it('índice com schema_version 2 interrompe com erro específico e não baixa dados', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    indice.schema_version = 2;
    servidor.gravarIndice(indice);
    const { cliente, armazenamento } = novoCliente(servidor);
    await expect(cliente.sincronizar()).rejects.toBeInstanceOf(ErroSchemaNaoSuportado);
    expect(servidor.pedidosDeDados()).toEqual([]);
    expect(armazenamento.tamanho).toBe(0);
  });

  it('índice que não é JSON, ou HTTP 500 no índice, lança ErroDados', async () => {
    const servidor = new ServidorFalso();
    servidor.arquivos.set('index.json', new TextEncoder().encode('{ não é json'));
    const { cliente } = novoCliente(servidor);
    await expect(cliente.sincronizar()).rejects.toMatchObject({ codigo: 'json_invalido' });

    const s2 = new ServidorFalso();
    s2.com500.add('index.json');
    await expect(novoCliente(s2).cliente.sincronizar()).rejects.toMatchObject({ codigo: 'http' });
  });

  it('falha de rede no fetch vira erro de rede', async () => {
    const cliente = criarCliente({
      urlBase: URL_BASE_TESTE,
      fetch: async () => {
        throw new TypeError('Failed to fetch');
      },
      armazenamento: new ArmazenamentoEmMemoria(),
    });
    await expect(cliente.sincronizar()).rejects.toMatchObject({ codigo: 'rede' });
  });
});

describe('dado inválido nunca é aceito nem substitui o cache válido', () => {
  it('hash errado: recusa, não guarda e não interpreta', async () => {
    const servidor = new ServidorFalso();
    const bytes = lerFixture('quina/ultimo.json').slice();
    // Mesmo tamanho, outro conteúdo: troca um dígito da data.
    const texto = new TextDecoder().decode(bytes).replace('2026-10-13', '2026-10-12');
    expect(texto.length).toBe(bytes.length);
    await servidor.trocar('quina/ultimo.json', texto, false);
    const { cliente, armazenamento } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'] });
    const erro = falhou(r.loterias.quina!.arquivos.ultimo);
    expect(erro.codigo).toBe('hash');
    expect(erro.message).toContain('SHA-256');
    expect(await armazenamento.ler('quina/ultimo.json')).toBeNull();
    // Os outros arquivos da mesma loteria seguem normalmente.
    expect(r.loterias.quina!.arquivos.tabela_geral?.estado).toBe('baixado');
  });

  it('bytes diferente do declarado: recusa (antes mesmo do hash)', async () => {
    const servidor = new ServidorFalso();
    const indice = servidor.indice();
    indice.loterias.quina.arquivos.tabela_geral.bytes += 1;
    servidor.gravarIndice(indice);
    const { cliente, armazenamento } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'] });
    const erro = falhou(r.loterias.quina!.arquivos.tabela_geral);
    expect(erro.codigo).toBe('bytes');
    expect(await armazenamento.ler('quina/tabela_geral.json')).toBeNull();
  });

  it('arquivo truncado (gravação interrompida) é recusado pela conferência', async () => {
    const servidor = new ServidorFalso();
    const inteiro = lerFixture('quina/historico.json');
    await servidor.trocar('quina/historico.json', inteiro.slice(0, inteiro.length - 40), false);
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['historico'] });
    expect(falhou(r.loterias.quina!.arquivos.historico).codigo).toBe('bytes');
  });

  it('JSON inválido com hash correto: recusa como json_invalido e não guarda', async () => {
    const servidor = new ServidorFalso();
    await servidor.trocar('quina/ultimo.json', '{ "schema_version": 1, ');
    const { cliente, armazenamento } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(falhou(r.loterias.quina!.arquivos.ultimo).codigo).toBe('json_invalido');
    expect(armazenamento.tamanho).toBe(0);
  });

  it('bytes que não são UTF-8 são recusados como json_invalido', async () => {
    const servidor = new ServidorFalso();
    await servidor.trocar('quina/ultimo.json', new Uint8Array([0x7b, 0xff, 0xfe, 0x7d]));
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(falhou(r.loterias.quina!.arquivos.ultimo).codigo).toBe('json_invalido');
  });

  it('dezena duplicada com hash correto: recusa como forma', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('quina/ultimo.json', (j) => {
      j.dezenas = [j.dezenas[0], j.dezenas[0], ...j.dezenas.slice(2)];
    });
    const { cliente, armazenamento } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    const erro = falhou(r.loterias.quina!.arquivos.ultimo);
    expect(erro.codigo).toBe('forma');
    expect(erro.message).toContain('repetida');
    expect(armazenamento.tamanho).toBe(0);
  });

  it('dezenas fora de ordem no histórico com hash correto: recusa como forma', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('megasena/historico.json', (j) => {
      j.concursos[100][2].reverse();
    });
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['megasena'], arquivos: ['historico'] });
    expect(falhou(r.loterias.megasena!.arquivos.historico).codigo).toBe('forma');
  });

  it('schema_version 2 num arquivo de dados (índice v1): erro específico', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('quina/tabela_geral.json', (j) => {
      j.schema_version = 2;
    });
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['tabela_geral'] });
    const erro = falhou(r.loterias.quina!.arquivos.tabela_geral);
    expect(erro).toBeInstanceOf(ErroSchemaNaoSuportado);
    expect(erro.codigo).toBe('schema_version');
  });

  it('arquivo de outra loteria no caminho errado é recusado', async () => {
    const servidor = new ServidorFalso();
    await servidor.trocar('quina/ultimo.json', lerFixture('megasena/ultimo.json'));
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(falhou(r.loterias.quina!.arquivos.ultimo).message).toMatch(/esperada quina/);
  });

  it('ultimo.json que contradiz o índice é recusado', async () => {
    const servidor = new ServidorFalso();
    await servidor.alterarJson('quina/ultimo.json', (j) => {
      j.concurso += 5;
    });
    const { cliente } = novoCliente(servidor);
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(falhou(r.loterias.quina!.arquivos.ultimo).message).toMatch(/não conferem com o índice/);
  });

  it('arquivo novo adulterado não substitui o cache válido, e o cache segue disponível', async () => {
    const servidor = new ServidorFalso();
    const { cliente, armazenamento } = novoCliente(servidor);
    const primeira = await cliente.sincronizar({ loterias: ['lotofacil'], arquivos: ['ultimo'] });
    const antigo = primeira.loterias.lotofacil!.arquivos.ultimo!;
    expect(antigo.estado).toBe('baixado');
    const guardadoAntes = await armazenamento.ler('lotofacil/ultimo.json');

    // O índice passa a anunciar um sha256 novo, mas o servidor entrega bytes adulterados.
    const indice = servidor.indice();
    indice.loterias.lotofacil.arquivos.ultimo.sha256 = 'a'.repeat(64);
    servidor.gravarIndice(indice);

    const segunda = await cliente.sincronizar({ loterias: ['lotofacil'], arquivos: ['ultimo'] });
    const r = segunda.loterias.lotofacil!.arquivos.ultimo!;
    expect(r.estado).toBe('falhou');
    expect(r.dados).toEqual(antigo.dados);
    const guardadoDepois = await armazenamento.ler('lotofacil/ultimo.json');
    expect(guardadoDepois).toEqual(guardadoAntes);
  });

  it('HTTP 500 em arquivo de dados mantém o cache e não afeta os outros', async () => {
    const servidor = new ServidorFalso();
    const { cliente } = novoCliente(servidor);
    await cliente.sincronizar({ loterias: ['quina'] });
    // Força novo download da tabela mudando o hash anunciado.
    await servidor.alterarJson('quina/tabela_geral.json', (j) => {
      j.perfil = 'outro_perfil';
    });
    servidor.com500.add('quina/tabela_geral.json');
    const r = await cliente.sincronizar({ loterias: ['quina'] });
    const t = r.loterias.quina!.arquivos.tabela_geral!;
    expect(t.estado).toBe('falhou');
    expect(t.dados?.perfil).toBe('padrao_v1');
    expect(falhou(t).codigo).toBe('http');
    expect(r.loterias.quina!.arquivos.historico?.estado).toBe('em_cache');
  });

  it('cache corrompido no armazenamento é descartado e o arquivo é rebaixado', async () => {
    const servidor = new ServidorFalso();
    const { cliente, armazenamento } = novoCliente(servidor);
    await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    const g = (await armazenamento.ler('quina/ultimo.json'))!;
    const corrompido = g.bytes.slice();
    corrompido[10] = corrompido[10]! ^ 0xff;
    await armazenamento.guardar({ ...g, bytes: corrompido });

    servidor.pedidos.length = 0;
    const r = await cliente.sincronizar({ loterias: ['quina'], arquivos: ['ultimo'] });
    expect(r.loterias.quina!.arquivos.ultimo?.estado).toBe('baixado');
    expect(servidor.pedidos).toContain('quina/ultimo.json');
  });
});

describe('armazenamento em memória', () => {
  it('guarda e lê por caminho, com cópia dos bytes', async () => {
    const a = new ArmazenamentoEmMemoria();
    expect(await a.ler('x/y.json')).toBeNull();
    const bytes = new Uint8Array([1, 2, 3]);
    await a.guardar({ caminho: 'x/y.json', sha256: 'h', bytes });
    bytes[0] = 9;
    expect((await a.ler('x/y.json'))?.bytes).toEqual(new Uint8Array([1, 2, 3]));
  });
});
