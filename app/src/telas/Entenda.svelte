<script lang="ts">
  import BolaDezena from '../componentes/BolaDezena.svelte';
  import Cartao from '../componentes/Cartao.svelte';
  import InstalarApp from '../componentes/InstalarApp.svelte';
  import type { SituacaoDados } from '../dados/abertura';
  import type { OpcoesSincronizacao, ResultadoCache } from '../dados/cliente';
  import {
    DIRECOES_EVIDENCIA,
    GRAUS_EVIDENCIA,
    SLUGS,
    CHAVES_COMPONENTES,
    type Slug,
    type TabelaGeralV1,
  } from '../dados/tipos';
  import {
    frasesEvidencia,
    rotuloEvidenciaDirecao,
    rotuloEvidenciaGrau,
    textos,
    textosComponentes,
  } from '../lib/textos';
  import { EXEMPLO_LINHA, montarVisaoEntenda, textoDoExemplo } from './entenda';

  interface Props {
    /** Só leitura do cache: abrir a Entenda nunca exige rede. */
    fonte: { lerCache: (opcoes: OpcoesSincronizacao) => Promise<ResultadoCache | null> };
    /** Situação da abertura (data da última atualização, último concurso). */
    situacao: SituacaoDados | null;
    versaoApp: string;
    /** Já existe uma versão nova do app esperando (aviso do service worker). */
    atualizacaoPronta: boolean;
  }

  let { fonte, situacao, versaoApp, atualizacaoPronta }: Props = $props();

  let tabelas = $state<Partial<Record<Slug, TabelaGeralV1 | null>>>({});
  let sequencia = 0;

  // Lê a tabela_geral guardada de cada loteria. Refaz quando a situação muda
  // (a Início baixa as tabelas em segundo plano depois de atualizar).
  $effect(() => {
    void situacao;
    const minha = ++sequencia;
    void fonte
      .lerCache({ arquivos: ['tabela_geral'], loterias: [...SLUGS] })
      .then((cache) => {
        if (minha !== sequencia) return;
        tabelas = Object.fromEntries(
          SLUGS.map((slug) => [slug, cache?.loterias[slug]?.dados.tabela_geral ?? null]),
        );
      })
      .catch(() => undefined);
  });

  const visao = $derived(
    montarVisaoEntenda({ situacao, tabelas, versaoApp, atualizacaoDoApp: atualizacaoPronta }),
  );
  const t = textos.entenda;
</script>

<h2 id="titulo-aba">{t.titulo}</h2>

<div class="pilha">
  <Cartao variante="contorno" titulo={t.destaque.titulo}>
    <div class="destaque">
      <ul class="lista">
        {#each t.destaque.itens as item (item)}
          <li>{item}</li>
        {/each}
      </ul>
      <p>{t.destaque.fecho}</p>
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={t.indice.titulo}>
    <div class="texto">
      {#each t.indice.paragrafos as paragrafo (paragrafo)}
        <p>{paragrafo}</p>
      {/each}
      <details>
        <summary>{t.componentes.resumo}</summary>
        <p class="nota">{t.componentes.intro}</p>
        <dl class="definicoes">
          {#each CHAVES_COMPONENTES as chave (chave)}
            <div>
              <dt>{textosComponentes[chave].rotulo}</dt>
              <dd>{textosComponentes[chave].explicacao}</dd>
            </div>
          {/each}
        </dl>
      </details>
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={t.g1.titulo}>
    <div class="texto">
      {#each t.g1.paragrafos as paragrafo (paragrafo)}
        <p>{paragrafo}</p>
      {/each}
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={t.evidencia.titulo}>
    <div class="texto">
      {#each t.evidencia.paragrafos as paragrafo (paragrafo)}
        <p>{paragrafo}</p>
      {/each}
      <h4>{t.evidencia.direcoesTitulo}</h4>
      <dl class="definicoes">
        {#each DIRECOES_EVIDENCIA as direcao (direcao)}
          <div>
            <dt>{rotuloEvidenciaDirecao(direcao)}</dt>
            <dd>{t.evidencia.direcoes[direcao]}</dd>
          </div>
        {/each}
      </dl>
      <p>{t.evidencia.direcoesNota}</p>
      <p>{t.evidencia.fecho}</p>
      <details>
        <summary>{t.evidencia.grausResumo}</summary>
        <p class="nota">{t.evidencia.grausIntro}</p>
        <ol class="graus">
          {#each GRAUS_EVIDENCIA as grau, i (grau)}
            <li>
              <p class="grau-nome">
                <span class="segmentos" aria-hidden="true">
                  {#each [1, 2, 3, 4, 5] as n (n)}
                    <span class="segmento" data-aceso={n <= i + 1}></span>
                  {/each}
                </span>
                <strong>{rotuloEvidenciaGrau(grau)}</strong>
              </p>
              <dl class="definicoes">
                {#each DIRECOES_EVIDENCIA as direcao (direcao)}
                  <div>
                    <dt>{t.evidencia.direcaoRotulo(rotuloEvidenciaDirecao(direcao))}</dt>
                    <dd>{frasesEvidencia[grau][direcao]}</dd>
                  </div>
                {/each}
              </dl>
            </li>
          {/each}
        </ol>
      </details>
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={t.leitura.titulo}>
    <div class="texto">
      <dl class="definicoes">
        {#each Object.values(t.leitura.itens) as item (item.rotulo)}
          <div>
            <dt>{item.rotulo}</dt>
            <dd>{item.texto}</dd>
          </div>
        {/each}
      </dl>
      <div class="exemplo">
        <p class="exemplo-rotulo">{t.leitura.exemploRotulo}</p>
        <p class="exemplo-linha">
          <BolaDezena numero={EXEMPLO_LINHA.dezena} tamanho="m" />
          <span>{textoDoExemplo()}</span>
        </p>
        <p class="nota">{t.leitura.exemploLeitura}</p>
      </div>
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={t.dados.titulo}>
    <div class="texto">
      <dl class="definicoes linhas">
        <div>
          <dt>{t.dados.resultadosRotulo}</dt>
          <dd>{t.dados.resultadosValor}</dd>
        </div>
        <div>
          <dt>{t.dados.atualizadoRotulo}</dt>
          <dd data-testid="atualizado-em">{visao.atualizadoEm ?? t.dados.semAtualizacao}</dd>
        </div>
        <div>
          <dt>{t.dados.formatoRotulo}</dt>
          <dd>{t.dados.formatoValor(visao.formatoDados)}</dd>
        </div>
        <div>
          <dt>{t.dados.appRotulo}</dt>
          <dd>{visao.versaoApp}</dd>
        </div>
        <div>
          <dt>{t.dados.appAtualizacaoRotulo}</dt>
          <dd>
            {visao.atualizacaoDoApp
              ? textos.sincronizacao.atualizacaoDisponivel
              : t.dados.appSemAtualizacao}
          </dd>
        </div>
      </dl>

      <p class="nota">{t.dados.perfilIntro}</p>

      {#each visao.loterias as loteria (loteria.slug)}
        <section class="loteria" aria-labelledby={`entenda-${loteria.slug}`}>
          <h4 id={`entenda-${loteria.slug}`}>{loteria.nome}</h4>
          <dl class="definicoes linhas">
            <div>
              <dt>{t.dados.ultimoRotulo}</dt>
              <dd>{loteria.ultimoConcurso ?? t.dados.semResultado}</dd>
            </div>
            {#if loteria.perfil}
              <div>
                <dt>{t.dados.perfilRotulo}</dt>
                <dd>{loteria.perfil.perfil}</dd>
              </div>
              <div>
                <dt>{t.dados.janelaRotulo}</dt>
                <dd>{loteria.perfil.janela}</dd>
              </div>
              <div>
                <dt>{t.dados.normalizacaoRotulo}</dt>
                <dd>{loteria.perfil.normalizacao}</dd>
              </div>
              <div>
                <dt>{t.dados.featureRotulo}</dt>
                <dd>{loteria.perfil.featureVersion}</dd>
              </div>
              <div>
                <dt>{t.dados.codigoRotulo}</dt>
                <dd>
                  <details class="codigo">
                    <summary title={loteria.perfil.hashCompleto}>
                      <code>{loteria.perfil.hashAbreviado}</code>
                      <span aria-hidden="true">…</span>
                    </summary>
                    <p class="rotulo-pequeno">{t.dados.codigoCompleto}</p>
                    <code class="completo">{loteria.perfil.hashCompleto}</code>
                  </details>
                </dd>
              </div>
            {:else}
              <div>
                <dt>{t.dados.perfilRotulo}</dt>
                <dd>{t.dados.tabelaAusente}</dd>
              </div>
            {/if}
          </dl>
        </section>
      {/each}
    </div>
  </Cartao>

  <Cartao variante="elevado" titulo={textos.instalar.titulo}>
    <InstalarApp modo="secao" />
  </Cartao>

  <p class="rodape">{textos.avisos.jogoResponsavel}</p>
</div>

<style>
  h2 {
    margin-bottom: var(--esp-4);
    font: var(--fonte-titulo-g);
  }
  h4 {
    margin: var(--esp-2) 0 0;
    font: var(--fonte-titulo-p);
  }
  .pilha {
    display: grid;
    gap: var(--esp-3);
  }
  .destaque,
  .texto {
    display: grid;
    gap: var(--esp-3);
    font: var(--fonte-corpo-m);
    overflow-wrap: anywhere;
  }
  .lista {
    display: grid;
    gap: var(--esp-2);
    margin: 0;
    padding-left: var(--esp-5, 1.25rem);
  }
  .nota {
    color: var(--cor-texto-var);
  }
  .definicoes {
    display: grid;
    gap: var(--esp-3);
    margin: 0;
  }
  dt {
    font: var(--fonte-rotulo-g);
  }
  dd {
    margin: 0;
  }
  /* Rótulo e valor lado a lado quando cabe; empilhados em tela estreita. */
  .linhas > div {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0 var(--esp-2);
  }
  .linhas dt::after {
    content: ':';
  }
  details {
    border-top: 1px solid var(--cor-contorno-var);
    padding-top: var(--esp-1);
  }
  summary {
    display: flex;
    align-items: center;
    gap: var(--esp-1);
    min-height: var(--alvo-minimo);
    color: var(--cor-primaria);
    font: var(--fonte-rotulo-g);
    cursor: pointer;
  }
  details > :not(summary) {
    margin-top: var(--esp-3);
  }
  .graus {
    display: grid;
    gap: var(--esp-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .grau-nome {
    display: flex;
    align-items: center;
    gap: var(--esp-2);
    margin-bottom: var(--esp-2);
  }
  .segmentos {
    display: inline-flex;
    gap: 0.125rem;
  }
  .segmento {
    width: 0.75rem;
    height: 0.5rem;
    border: 1px solid var(--cor-primaria);
    border-radius: 0.125rem;
  }
  .segmento[data-aceso='true'] {
    background: var(--cor-primaria);
  }
  .exemplo {
    display: grid;
    gap: var(--esp-2);
    padding: var(--esp-3);
    border: 1px dashed var(--cor-contorno);
    border-radius: var(--raio-m);
  }
  .exemplo-rotulo {
    font: var(--fonte-rotulo-g);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .exemplo-linha {
    display: flex;
    align-items: center;
    gap: var(--esp-3);
  }
  .loteria {
    display: grid;
    gap: var(--esp-2);
    padding-top: var(--esp-3);
    border-top: 1px solid var(--cor-contorno-var);
  }
  code {
    font-family: ui-monospace, 'Cascadia Mono', Consolas, monospace;
  }
  .codigo summary {
    display: inline-flex;
    padding-right: var(--esp-2);
  }
  .codigo {
    border-top: 0;
    padding-top: 0;
  }
  .codigo .completo {
    display: block;
    user-select: all;
    overflow-wrap: anywhere;
  }
  .rotulo-pequeno {
    color: var(--cor-texto-var);
    font: var(--fonte-rotulo-m);
  }
  .rodape {
    padding: var(--esp-2) var(--esp-1) var(--esp-4);
    color: var(--cor-texto-var);
    font: var(--fonte-corpo-m);
    text-align: center;
  }
</style>
