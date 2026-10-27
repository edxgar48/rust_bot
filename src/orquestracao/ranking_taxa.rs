//! `sync ranking` (Fase 1f, tarefa 1f.3): liga o banco (`db`) às funções puras
//! de `avaliacao` (taxas por posição e faixa, notas A e B, evidência e
//! esperado sob o acaso) e grava as tabelas derivadas `ranking_taxa`,
//! `taxa_posicao` e `taxa_faixa`. Nenhum SQL aqui; nenhuma matemática nova.
//!
//! É descrição do passado: sorteios são independentes e o esperado de
//! qualquer ranking feito com estas taxas é **não superar o acaso**.
//!
//! # Ponto no tempo
//!
//! A entrada de cada concurso `t` junta `indice(t)` (posição medida antes do
//! sorteio) e `features_dezena(t)` (componentes olhando `t−J … t−1`, janela do
//! perfil), então a taxa de `t` vê só concursos até `t−1`
//! ([`avaliacao::calcular_taxas`]). `saiu` de `t` é o alvo e nunca entra na
//! taxa de `t`. O snapshot do próximo concurso usa o histórico inteiro.
//!
//! A evidência (grau e direção da 1d) e o esperado sob o acaso descrevem o
//! histórico **inteiro**, inclusive o conjunto trancado, como a 1d-A: aqui
//! nada é avaliado nem escolhido. Por isso grau e direção só vão para o
//! relatório e para `taxa_posicao`/`taxa_faixa`; nenhuma nota, ordem ou
//! escolha lê esses campos.

use super::{
    conferir_loteria_do_perfil, juntar_indice_e_features, ler_indice_conferido, IndiceConferido,
};
use crate::avaliacao::{
    self, EsperadoTaxas, EvidenciaTaxas, LinhaRanking, ObservacaoPosicoes, TaxasAcumuladas,
    NOMES_COMPONENTES, REPLICAS_ESPERADO, SEMENTE_ESPERADO,
};
use crate::config::LoteriaConfig;
use crate::db;
use crate::error::{Result, SpiderError};
use crate::motor::{FeaturesDezena, IndiceDezena};
use crate::perfil::PerfilCarregado;
use rusqlite::Connection;

/// Tudo o que o `sync ranking` imprime e grava, depois de gravar.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioRanking {
    pub loteria: String,
    /// Nome do perfil (coluna `perfil` das três tabelas).
    pub perfil: String,
    pub versao: u32,
    pub janela: u32,
    pub normalizacao: String,
    /// Hash do `.toml`: o mesmo registrado em `perfis` (conferido).
    pub hash_toml: String,
    /// Primeiro concurso da série (o 1).
    pub primeiro_concurso: u32,
    /// Último concurso sorteado em `resultados`.
    pub ultimo_concurso: u32,
    /// Snapshot do próximo concurso (último + 1), ainda não sorteado.
    pub proximo_concurso: u32,
    /// Concursos sorteados incorporados nas taxas agregadas.
    pub concursos_sorteados: u64,
    /// Linhas de `ranking_taxa` do perfil que foram substituídas.
    pub linhas_apagadas: usize,
    /// Linhas gravadas em `ranking_taxa` (concursos × dezenas, com o próximo).
    pub linhas_gravadas: usize,
    /// Linhas gravadas em `taxa_posicao` (uma por posição).
    pub posicoes_gravadas: usize,
    /// Linhas gravadas em `taxa_faixa` (9 componentes × 4 faixas).
    pub faixas_gravadas: usize,
    /// O ranking do próximo concurso, ordenado por dezena.
    pub proximo: Vec<LinhaRanking>,
    /// As linhas gravadas em `taxa_posicao`, por posição crescente.
    pub taxas_posicao: Vec<db::LinhaTaxaPosicao>,
    /// As linhas gravadas em `taxa_faixa`, por componente e faixa (a ordem de
    /// `evidencia.faixas`).
    pub taxas_faixa: Vec<db::LinhaTaxaFaixa>,
    /// Grau e direção da 1d por posição e faixa, histórico inteiro.
    pub evidencia: EvidenciaTaxas,
    /// Contagem de graus esperada sob o acaso (simulação com semente fixa).
    pub esperado: EsperadoTaxas,
}

/// Calcula e grava o ranking por taxa histórica do perfil `carregado` no banco
/// `conn` da loteria `cfg` (comando `sync ranking`), com as réplicas e a
/// semente padrão do esperado sob o acaso. Ver [`sincronizar_ranking_com`].
pub fn sincronizar_ranking(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    carregado: &PerfilCarregado,
) -> Result<RelatorioRanking> {
    sincronizar_ranking_com(conn, cfg, carregado, REPLICAS_ESPERADO, SEMENTE_ESPERADO)
}

/// [`sincronizar_ranking`] com as réplicas e a semente do esperado escolhidas
/// (os testes usam poucas réplicas; o esperado só é impresso, não é gravado).
///
/// Recusa sem gravar nada (nem criar tabelas), com as mesmas conferências do
/// `sync avaliar` e do `sync tabela` (`ler_indice_conferido`):
/// - perfil de outra loteria ([`SpiderError::PerfilDeOutraLoteria`]);
/// - banco sem resultados ou com erro do `validar`
///   ([`SpiderError::SemResultados`], [`SpiderError::ResultadosReprovados`]);
/// - perfil fora de `perfis` ([`SpiderError::PerfilNaoRegistrado`]) ou com
///   hash diferente do `.toml` em disco ([`SpiderError::PerfilAlterado`]): o
///   ranking fica amarrado ao perfil e ao hash registrados pelo `sync indice`;
/// - features fora de dia (`SemFeatures`, `FeaturesDesatualizadas`,
///   `FeaturesDeResultadosAlterados`, `FeaturesDeOutraVersao`,
///   `FeaturesIncoerentes`);
/// - índice que não vai até o último + 1 ([`SpiderError::IndiceDesatualizado`])
///   ou que difere do recalculado com as features atuais
///   ([`SpiderError::IndiceIncoerente`]);
/// - menos de 2 réplicas ([`SpiderError::EvidenciaEntradaInvalida`]).
///
/// Senão, numa única transação (`BEGIN IMMEDIATE`, leitura e escrita juntas):
/// calcula as taxas, as duas notas e as duas ordens, a evidência e o esperado,
/// apaga as linhas do perfil nas três tabelas e insere tudo de novo. Outros
/// perfis não são tocados. Regenerar tudo (e não só os concursos novos) é
/// simples e barato, e mantém o resultado idêntico ao de uma execução do zero.
pub fn sincronizar_ranking_com(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    carregado: &PerfilCarregado,
    replicas: usize,
    semente: u64,
) -> Result<RelatorioRanking> {
    let perfil = &carregado.perfil;
    conferir_loteria_do_perfil(cfg, perfil)?;

    // Leitura e escrita na mesma transação: o ranking gravado corresponde
    // exatamente ao índice e às features lidos. Qualquer erro antes do
    // `commit` desfaz tudo. As conferências vêm antes de qualquer `CREATE`.
    let tx = db::iniciar_escrita(conn)?;
    let IndiceConferido {
        ultimo,
        indice,
        features,
        ..
    } = ler_indice_conferido(&tx, cfg, carregado)?;
    let proximo = ultimo + 1;

    let observacoes = montar_observacoes(cfg, &perfil.nome, &indice, &features)?;
    drop((indice, features));
    let primeiro = observacoes.first().map_or(proximo, |o| o.concurso);

    let serie = avaliacao::calcular_taxas(cfg, &observacoes)?;
    drop(observacoes);
    let ranking = avaliacao::ranquear(cfg, &serie.linhas)?;
    let evidencia = avaliacao::evidencia_das_taxas(cfg, &serie.acumuladas)?;
    let esperado = avaliacao::esperado_das_taxas(cfg, &serie.linhas, replicas, semente)?;

    let linhas_ranking: Vec<db::LinhaRankingTaxa> = ranking.iter().map(linha_do_ranking).collect();
    let linhas_posicao = linhas_taxa_posicao(&evidencia, &serie.acumuladas)?;
    let linhas_faixa = linhas_taxa_faixa(&evidencia, &serie.acumuladas)?;

    db::criar_tabelas_ranking(&tx)?;
    let linhas_apagadas = db::apagar_ranking_do_perfil(&tx, &perfil.nome)?;
    let linhas_gravadas = db::inserir_ranking_taxa(&tx, &perfil.nome, &linhas_ranking)?;
    let posicoes_gravadas = db::inserir_taxa_posicao(&tx, &perfil.nome, &linhas_posicao)?;
    let faixas_gravadas = db::inserir_taxa_faixa(&tx, &perfil.nome, &linhas_faixa)?;
    tx.commit()?;

    Ok(RelatorioRanking {
        loteria: cfg.slug.to_string(),
        perfil: perfil.nome.clone(),
        versao: perfil.versao,
        janela: perfil.janela,
        normalizacao: perfil.normalizacao.como_str().to_string(),
        hash_toml: carregado.hash_toml.clone(),
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        proximo_concurso: proximo,
        concursos_sorteados: serie.acumuladas.concursos_incorporados(),
        linhas_apagadas,
        linhas_gravadas,
        posicoes_gravadas,
        faixas_gravadas,
        proximo: ranking
            .into_iter()
            .filter(|l| l.concurso == proximo)
            .collect(),
        taxas_posicao: linhas_posicao,
        taxas_faixa: linhas_faixa,
        evidencia,
        esperado,
    })
}

/// Junta, por (concurso, dezena), a posição do índice de `t`, o `saiu` de `t`
/// (o alvo) e os 9 componentes crus de `features_dezena(t)` na janela do
/// perfil: a entrada de [`avaliacao::calcular_taxas`]. Índice e features têm
/// de ter exatamente as mesmas chaves, na mesma ordem
/// ([`SpiderError::IndiceIncoerente`] se não).
fn montar_observacoes(
    cfg: &LoteriaConfig,
    perfil: &str,
    indice: &[IndiceDezena],
    features: &[FeaturesDezena],
) -> Result<Vec<ObservacaoPosicoes>> {
    // Confere as chaves e traz posição e `saiu`; o `zip` abaixo só vale por
    // causa dessa conferência.
    let base = juntar_indice_e_features(cfg, perfil, indice, features)?;
    Ok(base
        .iter()
        .zip(features)
        .map(|(o, f)| ObservacaoPosicoes {
            concurso: o.concurso,
            dezena: o.dezena,
            posicao: o.posicao,
            saiu: o.saiu,
            componentes: ObservacaoPosicoes::componentes_de(f),
        })
        .collect())
}

fn linha_do_ranking(l: &LinhaRanking) -> db::LinhaRankingTaxa {
    db::LinhaRankingTaxa {
        concurso: l.concurso,
        dezena: l.dezena,
        posicao_indice: l.posicao_indice,
        taxa_posicao: l.nota_a,
        nota_b: l.nota_b,
        posicao_ranking_a: l.posicao_ranking_a,
        posicao_ranking_b: l.posicao_ranking_b,
    }
}

/// Grau e direção (priori principal) em texto, ou `None, None` sem exposição.
fn grau_e_direcao(avaliacao: Option<&avaliacao::AvaliacaoFb>) -> (Option<String>, Option<String>) {
    match avaliacao {
        Some(a) => {
            let c = a.classificacao[0];
            (
                Some(c.grau.nome().to_string()),
                Some(c.direcao.nome().to_string()),
            )
        }
        None => (None, None),
    }
}

/// As linhas de `taxa_posicao`: contagem, taxa e evidência de cada posição,
/// em ordem crescente de posição.
fn linhas_taxa_posicao(
    evidencia: &EvidenciaTaxas,
    acumuladas: &TaxasAcumuladas,
) -> Result<Vec<db::LinhaTaxaPosicao>> {
    evidencia
        .posicoes
        .iter()
        .map(|p| {
            let taxa = acumuladas.taxa_posicao(p.posicao).ok_or_else(|| {
                SpiderError::AvaliacaoEntradaInvalida(format!(
                    "posição {} sem taxa nas contagens",
                    p.posicao
                ))
            })?;
            let (grau, direcao) = grau_e_direcao(p.avaliacao.as_ref());
            Ok(db::LinhaTaxaPosicao {
                posicao: p.posicao,
                saidas: p.saidas,
                exposicoes: p.total,
                taxa,
                grau,
                direcao,
            })
        })
        .collect()
}

/// As linhas de `taxa_faixa`: contagem, taxa e evidência de cada (componente,
/// faixa), por componente e, dentro dele, na ordem de `Faixa::TODAS`. A faixa
/// sem exposição fica na tabela, com a taxa do prior e `grau`/`direcao` nulos.
fn linhas_taxa_faixa(
    evidencia: &EvidenciaTaxas,
    acumuladas: &TaxasAcumuladas,
) -> Result<Vec<db::LinhaTaxaFaixa>> {
    evidencia
        .faixas
        .iter()
        .map(|f| {
            let taxa = acumuladas
                .taxa_faixa(f.componente, f.faixa)
                .ok_or_else(|| {
                    SpiderError::AvaliacaoEntradaInvalida(format!(
                        "componente {} sem taxa nas contagens",
                        f.componente
                    ))
                })?;
            let (grau, direcao) = grau_e_direcao(f.avaliacao.as_ref());
            Ok(db::LinhaTaxaFaixa {
                componente: NOMES_COMPONENTES[f.componente].to_string(),
                faixa: f.faixa.nome().to_string(),
                saidas: f.saidas,
                exposicoes: f.total,
                taxa,
                grau,
                direcao,
            })
        })
        .collect()
}

// `sync ranking --avaliar` (1f.4): avaliação do ranking, só leitura. Fica num
// módulo filho para usar a montagem da entrada (`montar_observacoes`) sem
// alterar a visibilidade dela.
mod avaliar_ranking;
pub use avaliar_ranking::*;
