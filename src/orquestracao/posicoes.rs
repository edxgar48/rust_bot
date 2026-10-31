//! `sync posicoes` (Fase 1g, tarefa 1g.3): liga o banco (`db_posicoes`) às
//! funções puras de `motor::posicoes` e grava as tabelas derivadas
//! `posicao_marcas`, `posicao_features`, `posicao_indice`, `posicao_perfil` e
//! `posicao_motor_estado`. Nenhum SQL aqui; nenhuma matemática nova.
//!
//! É independente do que já existe: só **lê** `resultados`, `features_dezena`,
//! `indice` e `perfis` (para conferir que o índice de força está em dia, como o
//! `sync ranking`); só **grava** nas tabelas `posicao_*`. O perfil das posições
//! é registrado em `posicao_perfil`, nunca em `perfis`.
//!
//! # Ponto no tempo
//!
//! A marca de `t` usa `indice(t)` e o resultado de `t`; as features da posição
//! em `t` usam só as marcas até `t−1` (`motor::posicoes`). Retomar do estado
//! salvo é o mesmo caminho do cálculo completo, concurso a concurso.
//!
//! # Incremental
//!
//! Marcas, features e estado do motor retomam do último concurso incorporado
//! (`posicao_motor_estado`, um por perfil de força). O índice das posições é
//! sempre refeito inteiro a partir das features gravadas: depende só delas e
//! é barato. Recalcula tudo do zero se não houver estado, se ele for ilegível
//! ou de outra versão, se as marcas já incorporadas mudaram (a impressão cobre
//! os resultados **e** as posições do índice de força) ou se
//! `posicao_features` não bater com o estado.

use super::{
    conferir_loteria_do_perfil, impressao_resultados, ler_indice_conferido, IndiceConferido,
    RegistroPerfil,
};
use crate::config::LoteriaConfig;
use crate::db::{self, PerfilRegistrado};
use crate::db_posicoes::{self as dbp, EstadoSalvo, LinhaMarca};
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::motor::{self, EstadoMotor, IndiceDezena, FEATURE_VERSION, JANELAS};
use crate::perfil::PerfilCarregado;
use rusqlite::Connection;
use std::collections::HashMap;

/// Nome do perfil das posições que o `sync exportar` publica
/// (`ranking_posicoes.json`, tarefa 2.6): o mesmo padrão do `--perfil-posicoes`
/// do `sync posicoes`. Não entra em `LoteriaConfig` (decisão de 2026-10-31).
pub const PERFIL_POSICOES: &str = "posicoes_v1";

/// Como o `sync posicoes` calculou as marcas e as features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModoPosicoes {
    /// Retomou do estado salvo e calculou só os concursos novos.
    Incremental,
    /// Recalculou tudo do zero; o motivo diz por quê.
    Completo(String),
}

/// Uma posição no próximo concurso, da mais forte para a mais fraca.
#[derive(Debug, Clone, PartialEq)]
pub struct LinhaProximoPosicoes {
    /// Posição da posição no índice das posições (1 = a de maior total).
    pub g_1: u32,
    /// A posição na tabela de força (1…N) que esta linha descreve.
    pub posicao: u32,
    /// Dezena que ocupa essa posição na tabela de força do próximo concurso.
    pub dezena: u32,
    pub total: f64,
    /// `g_1` do concurso anterior − `g_1` atual (positivo = subiu).
    pub variacao_g_1: Option<i64>,
}

/// Tudo o que o `sync posicoes` imprime e grava, depois de gravar.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioPosicoes {
    pub loteria: String,
    /// Perfil de força que deu as posições (`indice.perfil`).
    pub perfil: String,
    /// Perfil das posições (`posicao_indice.perfil_posicoes`).
    pub perfil_posicoes: String,
    pub versao: u32,
    pub janela: u32,
    pub normalizacao: String,
    /// Hash do `.toml` do perfil das posições, registrado em `posicao_perfil`.
    pub hash_toml: String,
    pub registro: RegistroPerfil,
    pub modo: ModoPosicoes,
    /// Último concurso sorteado em `resultados`.
    pub ultimo_concurso: u32,
    /// Snapshot do próximo concurso (último + 1), ainda não sorteado.
    pub proximo_concurso: u32,
    /// Concursos sorteados incorporados nesta execução.
    pub concursos_novos: usize,
    pub marcas_gravadas: usize,
    pub features_gravadas: usize,
    pub indice_apagado: usize,
    pub indice_gravado: usize,
    /// O índice das posições no próximo concurso, de `g_1` 1 a N.
    pub proximo: Vec<LinhaProximoPosicoes>,
}

/// Calcula e grava as posições do perfil de força `forca` com o perfil das
/// posições `posicoes` (comando `sync posicoes`).
///
/// Recusa sem gravar nada (nem criar tabelas), com as conferências do
/// `sync ranking` (`ler_indice_conferido`): perfil de outra loteria, banco sem
/// resultados ou com erro do `validar`, perfil de força fora de `perfis` ou
/// com hash diferente do `.toml`, features ou índice fora de dia. Perfil das
/// posições com o mesmo nome e outro conteúdo em `posicao_perfil` também é
/// recusado ([`SpiderError::PerfilAlterado`]).
///
/// Tudo numa única transação (`BEGIN IMMEDIATE`, leitura e escrita juntas):
/// um erro antes do `commit` desfaz tudo. `criado_em` só é gravado no primeiro
/// registro do perfil das posições.
pub fn sincronizar_posicoes(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    forca: &PerfilCarregado,
    posicoes: &PerfilCarregado,
    criado_em: &str,
) -> Result<RelatorioPosicoes> {
    conferir_loteria_do_perfil(cfg, &forca.perfil)?;
    conferir_loteria_do_perfil(cfg, &posicoes.perfil)?;
    let perfil = forca.perfil.nome.as_str();
    let pp = &posicoes.perfil;

    let tx = db::iniciar_escrita(conn)?;
    let IndiceConferido {
        resultados,
        ultimo,
        indice,
        ..
    } = ler_indice_conferido(&tx, cfg, forca)?;
    let proximo = ultimo + 1;
    let marcas = motor::marcar_historico(cfg, &resultados, &indice)?;
    drop(resultados);
    let cfg_pos = motor::config_posicoes(cfg);

    dbp::criar_tabelas(&tx)?;
    let registro = registrar_perfil_posicoes(&tx, posicoes, criado_em)?;

    let (modo, mut estado, ja_incorporados) = match decidir(&tx, &cfg_pos, perfil, &marcas)? {
        Decisao::Completo(motivo) => {
            dbp::apagar_do_perfil(&tx, perfil)?;
            (
                ModoPosicoes::Completo(motivo),
                EstadoMotor::novo(&cfg_pos),
                0,
            )
        }
        Decisao::Incremental(estado) => {
            let ja = estado.ultimo_concurso().unwrap_or(0) as usize;
            // Sai o snapshot antigo do próximo concurso (`saiu` nulo): agora
            // que o concurso existe, ele é recalculado com o alvo preenchido.
            dbp::apagar_features_desde(&tx, perfil, estado.proximo_concurso())?;
            (ModoPosicoes::Incremental, estado, ja)
        }
    };

    let novos = &marcas[ja_incorporados..];
    let mut marcas_gravadas = 0;
    let mut features_gravadas = 0;
    // Um concurso por vez: a memória fica em um concurso de linhas.
    for marca in novos {
        let linhas = estado.adicionar(&cfg_pos, marca)?;
        features_gravadas += dbp::inserir_features(&tx, perfil, &linhas, FEATURE_VERSION)?;
        let linhas_marca: Vec<LinhaMarca> = cfg_pos
            .dezenas()
            .map(|posicao| LinhaMarca {
                concurso: marca.concurso,
                posicao,
                saiu: marca.dezenas.contains(&posicao),
            })
            .collect();
        marcas_gravadas += dbp::inserir_marcas(&tx, perfil, &linhas_marca)?;
    }
    features_gravadas += dbp::inserir_features(&tx, perfil, &estado.snapshot(), FEATURE_VERSION)?;

    let estado_json = serde_json::to_string(&estado)
        .map_err(|e| SpiderError::LinhaInvalida(format!("estado do motor das posições: {e}")))?;
    dbp::gravar_estado(
        &tx,
        perfil,
        &EstadoSalvo {
            loteria: cfg.slug.to_string(),
            feature_version: FEATURE_VERSION,
            ultimo_concurso: ultimo,
            impressao: impressao_resultados(&marcas),
            estado_json,
        },
    )?;

    // O índice só depende das features da janela do perfil: refeito inteiro.
    let features = dbp::ler_features_da_janela(&tx, perfil, pp.janela)?;
    let indice_pos = motor::calcular_indice(&features, pp.janela, pp.normalizacao, &pp.pesos)?;
    drop(features);
    let mut indice_apagado = dbp::apagar_indice(&tx, perfil, &pp.nome)?;
    if !novos.is_empty() {
        // Com concursos novos, o índice de outro perfil das posições ficaria
        // velho (terminando no próximo antigo): sai, como no recálculo do zero,
        // e volta quando aquele perfil rodar de novo.
        indice_apagado += dbp::apagar_indice_dos_outros(&tx, perfil, &pp.nome)?;
    }
    let indice_gravado = dbp::inserir_indice(&tx, perfil, &pp.nome, &indice_pos)?;

    let proximo_linhas = montar_proximo(cfg, proximo, &indice, &indice_pos)?;
    tx.commit()?;

    Ok(RelatorioPosicoes {
        loteria: cfg.slug.to_string(),
        perfil: perfil.to_string(),
        perfil_posicoes: pp.nome.clone(),
        versao: pp.versao,
        janela: pp.janela,
        normalizacao: pp.normalizacao.como_str().to_string(),
        hash_toml: posicoes.hash_toml.clone(),
        registro,
        modo,
        ultimo_concurso: ultimo,
        proximo_concurso: proximo,
        concursos_novos: novos.len(),
        marcas_gravadas,
        features_gravadas,
        indice_apagado,
        indice_gravado,
        proximo: proximo_linhas,
    })
}

enum Decisao {
    Incremental(EstadoMotor),
    Completo(String),
}

/// Decide entre retomar do estado salvo e recalcular tudo, sem gravar nada.
/// `marcas` são todas as marcas `1 … último` do perfil de força.
fn decidir(
    conn: &Connection,
    cfg_pos: &LoteriaConfig,
    perfil: &str,
    marcas: &[Resultado],
) -> Result<Decisao> {
    let completo = |motivo: &str| Ok(Decisao::Completo(motivo.to_string()));
    let Some(salvo) = dbp::ler_estado(conn, perfil)? else {
        return completo("sem estado salvo das posições");
    };
    let estado: EstadoMotor = match serde_json::from_str(&salvo.estado_json) {
        Ok(estado) => estado,
        Err(e) => return completo(&format!("estado das posições ilegível: {e}")),
    };
    if let Err(e) = estado.verificar_compatibilidade(cfg_pos) {
        return completo(&e.to_string());
    }
    let ultimo = estado.ultimo_concurso().unwrap_or(0);
    if salvo.ultimo_concurso != ultimo || salvo.feature_version != estado.feature_version() {
        return completo("estado salvo incoerente com a linha de posicao_motor_estado");
    }
    if ultimo as usize > marcas.len() {
        return completo("há menos concursos que os já incorporados");
    }
    if impressao_resultados(&marcas[..ultimo as usize]) != salvo.impressao {
        return completo("as marcas já incorporadas mudaram (resultado ou posição do índice)");
    }
    // `posicao_marcas` tem de ter os concursos 1 … último, e nada além.
    let (marcas_gravadas, maior_marca) = dbp::resumo_marcas(conn, perfil)?;
    let esperado_marcas = u64::from(ultimo) * u64::from(cfg_pos.qtd_dezenas_possiveis());
    if marcas_gravadas != esperado_marcas || maior_marca.unwrap_or(0) != ultimo {
        return completo("posicao_marcas não bate com o estado salvo");
    }
    // `posicao_features` tem de ter exatamente o que o estado diz: todos os
    // concursos até o último e o snapshot do próximo, em todas as janelas.
    let (linhas, maior) = dbp::resumo_features(conn, perfil)?;
    let esperado =
        (u64::from(ultimo) + 1) * u64::from(cfg_pos.qtd_dezenas_possiveis()) * JANELAS.len() as u64;
    if linhas != esperado || maior != Some(ultimo + 1) {
        return completo("posicao_features não bate com o estado salvo");
    }
    Ok(Decisao::Incremental(estado))
}

/// Erro de perfil das posições editado sob o mesmo nome (`hash_registrado` em
/// `posicao_perfil` diferente do `.toml` em disco), sugerindo a próxima versão
/// livre: não sugere versão que já tem arquivo ao lado ou já está registrada.
fn perfil_posicoes_alterado(
    conn: &Connection,
    carregado: &PerfilCarregado,
    hash_registrado: String,
) -> SpiderError {
    let perfil = &carregado.perfil;
    let (proximo_nome, proxima_versao) = perfil.proxima_versao(|nome| {
        carregado
            .origem
            .with_file_name(format!("{nome}.toml"))
            .exists()
            || matches!(dbp::buscar_perfil(conn, nome), Ok(Some(_)))
    });
    SpiderError::PerfilAlterado {
        nome: perfil.nome.clone(),
        hash_registrado,
        hash_arquivo: carregado.hash_toml.clone(),
        proximo_nome,
        proxima_versao,
    }
}

/// Registra o perfil das posições em `posicao_perfil`, com a imutabilidade do
/// plano: mesmo nome e mesmo hash → nada muda; mesmo nome com hash diferente
/// → erro pedindo para subir a versão. Não toca em `perfis`.
fn registrar_perfil_posicoes(
    conn: &Connection,
    carregado: &PerfilCarregado,
    criado_em: &str,
) -> Result<RegistroPerfil> {
    let perfil = &carregado.perfil;
    match dbp::buscar_perfil(conn, &perfil.nome)? {
        Some(r) if r.hash_toml == carregado.hash_toml => Ok(RegistroPerfil::JaRegistrado),
        Some(r) => Err(perfil_posicoes_alterado(conn, carregado, r.hash_toml)),
        None => {
            dbp::inserir_perfil(
                conn,
                &PerfilRegistrado {
                    nome: perfil.nome.clone(),
                    versao: perfil.versao,
                    loteria: perfil.loteria.clone(),
                    janela: perfil.janela,
                    normalizacao: perfil.normalizacao.como_str().to_string(),
                    hash_toml: carregado.hash_toml.clone(),
                    toml: carregado.toml.clone(),
                    criado_em: criado_em.to_string(),
                },
            )?;
            Ok(RegistroPerfil::Novo)
        }
    }
}

/// As posições do próximo concurso por `g_1`, cada uma com a dezena que a
/// ocupa na tabela de força.
fn montar_proximo(
    cfg: &LoteriaConfig,
    proximo: u32,
    indice: &[IndiceDezena],
    indice_pos: &[IndiceDezena],
) -> Result<Vec<LinhaProximoPosicoes>> {
    let via = motor::dezenas_via_posicao(cfg, proximo, indice, indice_pos)?;
    let por_posicao: HashMap<u32, &IndiceDezena> = indice_pos
        .iter()
        .filter(|l| l.concurso == proximo)
        .map(|l| (l.dezena, l))
        .collect();
    via.into_iter()
        .map(|v| {
            let linha = por_posicao.get(&v.posicao).ok_or_else(|| {
                SpiderError::MotorIndiceEntradaInvalida(format!(
                    "posição {} sem linha no índice das posições do concurso {proximo}",
                    v.posicao
                ))
            })?;
            Ok(LinhaProximoPosicoes {
                g_1: v.g_1,
                posicao: v.posicao,
                dezena: v.dezena,
                total: linha.total,
                variacao_g_1: linha.variacao_posicao,
            })
        })
        .collect()
}

// `sync posicoes --avaliar` (1g.4): avaliação do G-1 contra o acaso, só leitura.
// Fica num módulo filho para usar os auxiliares privados deste (o erro de perfil
// alterado) sem alterar a visibilidade deles.
mod avaliar_g1;
pub use avaliar_g1::*;
