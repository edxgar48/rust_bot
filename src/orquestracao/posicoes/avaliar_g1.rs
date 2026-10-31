//! `sync posicoes --avaliar` (Fase 1g, tarefa 1g.4): liga o banco às funções
//! puras e avalia a ordem G-1 (o índice de força das posições, traduzido para
//! dezenas) contra o acaso, com a referência da nota A da 1f no mesmo recorte,
//! nas dobras de teste 2 a 5 do walk-forward da busca. Nenhum SQL aqui;
//! nenhuma matemática nova.
//!
//! **Só lê o banco.** Recalcula as marcas, as features e o índice das posições
//! em memória (`motor::calcular_posicoes`) e as taxas da 1f
//! ([`avaliacao::calcular_taxas`] e [`avaliacao::ranquear`]), sem escrever: não
//! lê as tabelas `posicao_*` nem `ranking_taxa`, então não há tabela
//! desatualizada a conferir e a avaliação não depende de o `sync posicoes` já
//! ter rodado. É descrição do passado: sorteios são independentes e o esperado
//! é não superar o acaso.
//!
//! # Conjunto trancado
//!
//! Fechado. A leitura do banco é a do `sync avaliar` e traz o histórico inteiro
//! (as conferências do índice e das features precisam dele), mas resultados,
//! índice e observações são cortados em `LoteriaConfig::ultimo_concurso_busca`
//! **antes** de qualquer cálculo de marcas ou taxas, e [`avaliacao::avaliar_g1`]
//! corta de novo. Não há `--trancado` nem registro de uso.
//!
//! # Ponto no tempo
//!
//! A marca de `t` usa `indice(t)` e o resultado de `t`; o `g_1` de `t` vem de
//! features que olham só as marcas até `t−1`; as taxas de A veem só `… t−1`.
//! O aquecimento (concursos `1 ..= AQUECIMENTO`) sai da avaliação inteira.

use super::super::ranking_taxa::montar_observacoes;
use super::perfil_posicoes_alterado;
use crate::avaliacao::{self, AvaliacaoG1, LinhaG1};
use crate::config::LoteriaConfig;
use crate::db;
use crate::db_posicoes as dbp;
use crate::error::{Result, SpiderError};
use crate::motor;
use crate::orquestracao::{conferir_loteria_do_perfil, ler_indice_conferido, IndiceConferido};
use crate::perfil::PerfilCarregado;
use rusqlite::Connection;
use std::collections::HashMap;

/// A avaliação do G-1 e o que identifica os perfis e o trecho lido.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioAvaliacaoG1 {
    pub loteria: String,
    /// Perfil de força que deu as posições.
    pub perfil: String,
    pub perfil_posicoes: String,
    pub versao: u32,
    pub janela: u32,
    pub normalizacao: String,
    /// Hash do `.toml` do perfil das posições.
    pub hash_toml: String,
    /// O perfil das posições já está em `posicao_perfil` com o mesmo hash. Falso
    /// quando o `sync posicoes` ainda não rodou com ele (a avaliação não grava
    /// nem exige o registro).
    pub registrado: bool,
    /// Último concurso sorteado em `resultados` (o trancado, acima do corte,
    /// foi lido para as conferências mas não entra na avaliação).
    pub ultimo_concurso: u32,
    pub avaliacao: AvaliacaoG1,
}

/// Avalia a ordem G-1 do perfil de força `forca` com o perfil das posições
/// `posicoes` no banco `conn` da loteria `cfg`, com k = K (as dezenas sorteadas
/// por concurso).
///
/// Recusa, sem escrever nada, com as mesmas conferências do `sync ranking` e do
/// `sync avaliar` (`ler_indice_conferido`): perfil de outra loteria, banco sem
/// resultados ou com erro do `validar`, perfil de força fora de `perfis` ou com
/// hash diferente do `.toml`, features ou índice fora de dia; perfil das
/// posições com o mesmo nome e outro conteúdo em `posicao_perfil`
/// ([`SpiderError::PerfilAlterado`]); e menos concursos avaliáveis na busca que
/// dobras do walk-forward ([`SpiderError::AvaliacaoSemConcursos`]).
pub fn avaliar_g1_do_banco(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    forca: &PerfilCarregado,
    posicoes: &PerfilCarregado,
) -> Result<RelatorioAvaliacaoG1> {
    conferir_loteria_do_perfil(cfg, &forca.perfil)?;
    conferir_loteria_do_perfil(cfg, &posicoes.perfil)?;
    let pp = &posicoes.perfil;
    let k = u32::try_from(cfg.qtd_dezenas).map_err(|_| {
        SpiderError::AvaliacaoOpcaoInvalida(format!(
            "a {} sorteia {} dezenas",
            cfg.nome_exibicao, cfg.qtd_dezenas
        ))
    })?;

    // Só leitura: a transação nunca recebe `commit` e é desfeita ao sair.
    let tx = db::iniciar_leitura(conn)?;
    let IndiceConferido {
        mut resultados,
        ultimo,
        mut indice,
        features,
    } = ler_indice_conferido(&tx, cfg, forca)?;
    let registrado = if db::tabela_existe(&tx, "posicao_perfil")? {
        match dbp::buscar_perfil(&tx, &pp.nome)? {
            Some(r) if r.hash_toml == posicoes.hash_toml => true,
            Some(r) => return Err(perfil_posicoes_alterado(&tx, posicoes, r.hash_toml)),
            None => false,
        }
    } else {
        false
    };
    drop(tx);

    // Ordem de A (a referência): as taxas da 1f, só com a busca.
    let mut observacoes = montar_observacoes(cfg, &forca.perfil.nome, &indice, &features)?;
    drop(features);
    observacoes.retain(|o| o.concurso <= cfg.ultimo_concurso_busca);
    let serie = avaliacao::calcular_taxas(cfg, &observacoes)?;
    drop(observacoes);
    let ranking = avaliacao::ranquear(cfg, &serie.linhas)?;
    drop(serie);

    // Ordem G-1: o índice das posições, só com a busca.
    resultados.retain(|r| r.concurso <= cfg.ultimo_concurso_busca);
    indice.retain(|l| l.concurso <= cfg.ultimo_concurso_busca);
    let (_, _, indice_pos) = motor::calcular_posicoes(
        cfg,
        &resultados,
        &indice,
        pp.janela,
        pp.normalizacao,
        &pp.pesos,
    )?;
    drop((resultados, indice));
    let g_1_da_posicao: HashMap<(u32, u32), u32> = indice_pos
        .iter()
        .map(|l| ((l.concurso, l.dezena), l.posicao))
        .collect();
    drop(indice_pos);

    let linhas = juntar_ordens(&ranking, &g_1_da_posicao)?;
    drop(ranking);
    let avaliacao = avaliacao::avaliar_g1(cfg, &linhas, k)?;

    Ok(RelatorioAvaliacaoG1 {
        loteria: cfg.slug.to_string(),
        perfil: forca.perfil.nome.clone(),
        perfil_posicoes: pp.nome.clone(),
        versao: pp.versao,
        janela: pp.janela,
        normalizacao: pp.normalizacao.como_str().to_string(),
        hash_toml: posicoes.hash_toml.clone(),
        registrado,
        ultimo_concurso: ultimo,
        avaliacao,
    })
}

/// Junta, por (concurso, dezena), a posição pela nota A, o `g_1` da posição que
/// a dezena ocupava na tabela de força e o `saiu`. O snapshot do próximo
/// concurso (sem `saiu`) fica fora.
fn juntar_ordens(
    ranking: &[avaliacao::LinhaRanking],
    g_1_da_posicao: &HashMap<(u32, u32), u32>,
) -> Result<Vec<LinhaG1>> {
    ranking
        .iter()
        .filter_map(|l| l.saiu.map(|saiu| (l, saiu)))
        .map(|(l, saiu)| {
            let g_1 = g_1_da_posicao
                .get(&(l.concurso, l.posicao_indice))
                .copied()
                .ok_or_else(|| {
                    SpiderError::MotorIndiceEntradaInvalida(format!(
                        "concurso {}: posição {} sem g_1 no índice das posições",
                        l.concurso, l.posicao_indice
                    ))
                })?;
            Ok(LinhaG1 {
                concurso: l.concurso,
                dezena: l.dezena,
                posicao_ranking_a: l.posicao_ranking_a,
                g_1,
                saiu,
            })
        })
        .collect()
}
