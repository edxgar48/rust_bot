//! `sync ranking --avaliar` (Fase 1f, tarefa 1f.4): liga o banco às funções
//! puras de `avaliacao` e avalia o ranking por taxa histórica (ordens de A e
//! de B) contra o acaso, nas dobras de teste 2 a 5 do walk-forward da busca.
//! Nenhum SQL aqui; nenhuma matemática nova.
//!
//! **Só lê o banco.** Recalcula as taxas e os rankings em memória, pelo mesmo
//! caminho do `sync ranking` (montagem da entrada, [`avaliacao::calcular_taxas`]
//! e [`avaliacao::ranquear`]), sem escrever: não lê as tabelas `ranking_taxa`,
//! `taxa_posicao` e `taxa_faixa`, então não há tabela desatualizada a conferir,
//! e a avaliação não depende de o `sync ranking` já ter rodado. É descrição do
//! passado: sorteios são independentes e o esperado é não superar o acaso.
//!
//! # Conjunto trancado
//!
//! Fechado. A leitura do banco é a mesma do `sync avaliar` e traz o histórico
//! inteiro (as conferências do índice e das features precisam dele), mas as
//! observações são cortadas em `LoteriaConfig::ultimo_concurso_busca` **antes**
//! de calcular as taxas, e [`avaliacao::avaliar_ranking`] corta de novo: nada
//! acima do corte entra em taxa, ordem, calibração ou métrica. Não há
//! `--trancado` nem registro de uso.
//!
//! # Ponto no tempo
//!
//! A entrada de `t` junta `indice(t)` e `features_dezena(t)`; as taxas de `t`
//! veem só `… t−1`. A evidência e o esperado sob o acaso do `sync ranking`
//! (histórico inteiro) não são calculados aqui.

use super::montar_observacoes;
use crate::avaliacao::{self, AvaliacaoRanking};
use crate::config::LoteriaConfig;
use crate::db;
use crate::error::{Result, SpiderError};
use crate::orquestracao::{conferir_loteria_do_perfil, ler_indice_conferido, IndiceConferido};
use crate::perfil::PerfilCarregado;
use rusqlite::Connection;

/// A avaliação do ranking e o que identifica o perfil e o trecho lido.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioAvaliacaoRanking {
    pub loteria: String,
    pub perfil: String,
    pub versao: u32,
    pub janela: u32,
    pub normalizacao: String,
    /// Hash do `.toml`: o mesmo registrado em `perfis` (conferido).
    pub hash_toml: String,
    /// Primeiro concurso da série do índice (o 1); a calibração parte dele e
    /// ele não é avaliado.
    pub primeiro_concurso: u32,
    /// Último concurso sorteado em `resultados` (o trancado, acima do corte,
    /// foi lido para as conferências mas não entra na avaliação).
    pub ultimo_concurso: u32,
    /// `cfg.ultimo_concurso_busca`.
    pub corte: u32,
    pub avaliacao: AvaliacaoRanking,
}

/// Avalia o ranking por taxa histórica do perfil `carregado` no banco `conn`
/// da loteria `cfg`, com k = K (as dezenas sorteadas por concurso).
///
/// Recusa, sem escrever nada, com as mesmas conferências do `sync ranking` e do
/// `sync avaliar` (`ler_indice_conferido`):
/// - perfil de outra loteria ([`SpiderError::PerfilDeOutraLoteria`]);
/// - banco sem resultados ou com erro do `validar`
///   ([`SpiderError::SemResultados`], [`SpiderError::ResultadosReprovados`]);
/// - perfil fora de `perfis` ([`SpiderError::PerfilNaoRegistrado`]) ou com hash
///   diferente do `.toml` em disco ([`SpiderError::PerfilAlterado`]);
/// - features fora de dia, índice que não vai até o último + 1 ou que difere do
///   recalculado ([`SpiderError::FeaturesDesatualizadas`],
///   [`SpiderError::IndiceDesatualizado`], [`SpiderError::IndiceIncoerente`]…);
/// - menos concursos avaliáveis na busca que dobras do walk-forward
///   ([`SpiderError::AvaliacaoSemConcursos`]).
pub fn avaliar_ranking_do_banco(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    carregado: &PerfilCarregado,
) -> Result<RelatorioAvaliacaoRanking> {
    let perfil = &carregado.perfil;
    conferir_loteria_do_perfil(cfg, perfil)?;
    let k = u32::try_from(cfg.qtd_dezenas).map_err(|_| {
        SpiderError::AvaliacaoOpcaoInvalida(format!(
            "a {} sorteia {} dezenas",
            cfg.nome_exibicao, cfg.qtd_dezenas
        ))
    })?;

    // Só leitura: a transação nunca recebe `commit` e é desfeita ao sair
    // (leitura consistente de resultados, features, perfis e índice).
    let tx = db::iniciar_leitura(conn)?;
    let IndiceConferido {
        ultimo,
        indice,
        features,
        ..
    } = ler_indice_conferido(&tx, cfg, carregado)?;
    drop(tx);

    let mut observacoes = montar_observacoes(cfg, &perfil.nome, &indice, &features)?;
    drop((indice, features));
    let primeiro = observacoes.first().map_or(ultimo + 1, |o| o.concurso);
    // O trancado sai antes de qualquer taxa: as taxas de `t ≤ corte` veem só
    // `… t−1`, então cortar aqui não muda nenhuma linha que fica.
    observacoes.retain(|o| o.concurso <= cfg.ultimo_concurso_busca);

    let serie = avaliacao::calcular_taxas(cfg, &observacoes)?;
    drop(observacoes);
    let ranking = avaliacao::ranquear(cfg, &serie.linhas)?;
    drop(serie);
    let avaliacao = avaliacao::avaliar_ranking(cfg, &ranking, k)?;

    Ok(RelatorioAvaliacaoRanking {
        loteria: cfg.slug.to_string(),
        perfil: perfil.nome.clone(),
        versao: perfil.versao,
        janela: perfil.janela,
        normalizacao: perfil.normalizacao.como_str().to_string(),
        hash_toml: carregado.hash_toml.clone(),
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        corte: cfg.ultimo_concurso_busca,
        avaliacao,
    })
}
