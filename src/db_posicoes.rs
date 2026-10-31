//! Tabelas derivadas do índice de força das posições (Fase 1g, tarefa 1g.3).
//!
//! Ficam num módulo à parte para o `db` existente não mudar: nada aqui lê nem
//! grava `resultados`, `features_dezena`, `indice`, `perfis` ou qualquer outra
//! tabela que já existia. Todas as linhas são amarradas pelo `perfil` (o
//! perfil de **força** que deu as posições) e podem ser apagadas e
//! regeneradas a partir de `resultados`, `features_dezena` e `indice`.
//!
//! - `posicao_marcas`: por (perfil, concurso, posição), se a posição ocupada
//!   por uma dezena sorteada naquele concurso (`saiu` 1) ou não (0).
//! - `posicao_features`: as features do motor sobre as marcas; o campo
//!   `posicao` faz o papel da dezena. Traz o snapshot do próximo concurso
//!   (`saiu` nulo).
//! - `posicao_indice`: o índice das posições; `g_1` é a posição 1…N da
//!   posição no índice (1 = a de maior total).
//! - `posicao_perfil`: registro próprio do perfil das posições (a tabela
//!   `perfis` não é tocada), com a mesma regra de imutabilidade.
//! - `posicao_motor_estado`: o estado incremental do motor das posições, uma
//!   linha por perfil de força.
//!
//! A chave primária começa por `perfil` (e `janela`) para apagar e ler um
//! perfil sem varrer os outros; é a única diferença de ordem em relação às
//! tabelas que já existem.

use crate::db::PerfilRegistrado;
use crate::error::{Result, SpiderError};
use crate::motor::{FeaturesDezena, IndiceDezena};
use rusqlite::{params, Connection, OptionalExtension};

/// Cria as cinco tabelas se ainda não existem. Idempotente.
pub fn criar_tabelas(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS posicao_marcas (
            perfil    TEXT NOT NULL,
            concurso  INTEGER NOT NULL,
            posicao   INTEGER NOT NULL,
            saiu      INTEGER NOT NULL,
            PRIMARY KEY (perfil, concurso, posicao)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS posicao_features (
            perfil                   TEXT NOT NULL,
            janela                   INTEGER NOT NULL,
            concurso                 INTEGER NOT NULL,
            posicao                  INTEGER NOT NULL,
            frequencia               INTEGER NOT NULL,
            maior_espaco             INTEGER,
            maior_espaco_ocorrencias INTEGER,
            menor_espaco             INTEGER,
            menor_espaco_ocorrencias INTEGER,
            soma_variacoes           INTEGER,
            espaco_final             INTEGER,
            repeticoes_total         INTEGER NOT NULL,
            repeticao_atual          INTEGER NOT NULL,
            saiu                     INTEGER,
            feature_version          INTEGER NOT NULL,
            PRIMARY KEY (perfil, janela, concurso, posicao)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS posicao_indice (
            perfil           TEXT NOT NULL,
            perfil_posicoes  TEXT NOT NULL,
            concurso         INTEGER NOT NULL,
            posicao          INTEGER NOT NULL,
            total            REAL NOT NULL,
            g_1              INTEGER NOT NULL,
            variacao_g_1     INTEGER,
            PRIMARY KEY (perfil, perfil_posicoes, concurso, posicao)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS posicao_perfil (
            nome          TEXT NOT NULL PRIMARY KEY,
            versao        INTEGER NOT NULL,
            loteria       TEXT NOT NULL,
            janela        INTEGER NOT NULL,
            normalizacao  TEXT NOT NULL,
            hash_toml     TEXT NOT NULL,
            toml          TEXT NOT NULL,
            criado_em     TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS posicao_motor_estado (
            perfil           TEXT NOT NULL PRIMARY KEY,
            loteria          TEXT NOT NULL,
            feature_version  INTEGER NOT NULL,
            ultimo_concurso  INTEGER NOT NULL,
            impressao        TEXT NOT NULL,
            estado           TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Uma linha de `posicao_marcas` (sem o `perfil`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinhaMarca {
    pub concurso: u32,
    pub posicao: u32,
    pub saiu: bool,
}

/// Conteúdo da linha de `posicao_motor_estado`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoSalvo {
    pub loteria: String,
    pub feature_version: u32,
    pub ultimo_concurso: u32,
    /// Impressão das marcas `1 … ultimo_concurso` incorporadas (que cobre os
    /// resultados e as posições do índice de força).
    pub impressao: String,
    /// `EstadoMotor` em JSON; quem lê decide o que fazer se não desserializar.
    pub estado_json: String,
}

/// Lê o perfil das posições registrado com esse nome, ou `None`. Exige a
/// tabela criada.
pub fn buscar_perfil(conn: &Connection, nome: &str) -> Result<Option<PerfilRegistrado>> {
    conn.query_row(
        "SELECT nome, versao, loteria, janela, normalizacao, hash_toml, toml, criado_em
         FROM posicao_perfil WHERE nome = ?1",
        params![nome],
        |row| {
            Ok(PerfilRegistrado {
                nome: row.get(0)?,
                versao: row.get(1)?,
                loteria: row.get(2)?,
                janela: row.get(3)?,
                normalizacao: row.get(4)?,
                hash_toml: row.get(5)?,
                toml: row.get(6)?,
                criado_em: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(SpiderError::from)
}

/// Insere um perfil novo. Nome já existente é erro (chave primária): nunca
/// substitui uma linha.
pub fn inserir_perfil(conn: &Connection, perfil: &PerfilRegistrado) -> Result<()> {
    conn.execute(
        "INSERT INTO posicao_perfil
            (nome, versao, loteria, janela, normalizacao, hash_toml, toml, criado_em)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            perfil.nome,
            perfil.versao,
            perfil.loteria,
            perfil.janela,
            perfil.normalizacao,
            perfil.hash_toml,
            perfil.toml,
            perfil.criado_em
        ],
    )?;
    Ok(())
}

/// Lê o estado salvo do motor das posições do perfil de força, ou `None`.
pub fn ler_estado(conn: &Connection, perfil: &str) -> Result<Option<EstadoSalvo>> {
    conn.query_row(
        "SELECT loteria, feature_version, ultimo_concurso, impressao, estado
         FROM posicao_motor_estado WHERE perfil = ?1",
        params![perfil],
        |row| {
            Ok(EstadoSalvo {
                loteria: row.get(0)?,
                feature_version: row.get(1)?,
                ultimo_concurso: row.get(2)?,
                impressao: row.get(3)?,
                estado_json: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(SpiderError::from)
}

/// Grava (ou substitui) o estado do perfil de força.
pub fn gravar_estado(conn: &Connection, perfil: &str, salvo: &EstadoSalvo) -> Result<()> {
    conn.execute(
        "INSERT INTO posicao_motor_estado
            (perfil, loteria, feature_version, ultimo_concurso, impressao, estado)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(perfil) DO UPDATE SET
            loteria = excluded.loteria,
            feature_version = excluded.feature_version,
            ultimo_concurso = excluded.ultimo_concurso,
            impressao = excluded.impressao,
            estado = excluded.estado",
        params![
            perfil,
            salvo.loteria,
            salvo.feature_version,
            salvo.ultimo_concurso,
            salvo.impressao,
            salvo.estado_json
        ],
    )?;
    Ok(())
}

/// Apaga marcas, features, índice (de todos os perfis de posições) e estado do
/// perfil de força: regeneração do zero. Não toca em `posicao_perfil`.
pub fn apagar_do_perfil(conn: &Connection, perfil: &str) -> Result<()> {
    for tabela in [
        "posicao_marcas",
        "posicao_features",
        "posicao_indice",
        "posicao_motor_estado",
    ] {
        conn.execute(
            &format!("DELETE FROM {tabela} WHERE perfil = ?1"),
            params![perfil],
        )?;
    }
    Ok(())
}

/// Apaga as features do perfil de força dos concursos `>= desde` (o snapshot
/// antigo do próximo concurso, que o retomar do estado recalcula).
pub fn apagar_features_desde(conn: &Connection, perfil: &str, desde: u32) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM posicao_features WHERE perfil = ?1 AND concurso >= ?2",
        params![perfil, desde],
    )?)
}

/// Apaga o índice de um par (perfil de força, perfil das posições). Devolve
/// quantas linhas apagou.
pub fn apagar_indice(conn: &Connection, perfil: &str, perfil_posicoes: &str) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM posicao_indice WHERE perfil = ?1 AND perfil_posicoes = ?2",
        params![perfil, perfil_posicoes],
    )?)
}

/// Apaga o índice das posições do perfil de força de todos os perfis das
/// posições **menos** `manter`. Devolve quantas linhas apagou.
pub fn apagar_indice_dos_outros(conn: &Connection, perfil: &str, manter: &str) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM posicao_indice WHERE perfil = ?1 AND perfil_posicoes != ?2",
        params![perfil, manter],
    )?)
}

/// Quantas linhas de `posicao_marcas` o perfil de força tem e o maior
/// concurso presente (para conferir a coerência com o estado salvo).
pub fn resumo_marcas(conn: &Connection, perfil: &str) -> Result<(u64, Option<u32>)> {
    let (linhas, maior) = conn.query_row(
        "SELECT COUNT(*), MAX(concurso) FROM posicao_marcas WHERE perfil = ?1",
        params![perfil],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<u32>>(1)?)),
    )?;
    Ok((linhas.unsigned_abs(), maior))
}

/// Quantas linhas de `posicao_features` o perfil de força tem e o maior
/// concurso presente (para conferir a coerência com o estado salvo).
pub fn resumo_features(conn: &Connection, perfil: &str) -> Result<(u64, Option<u32>)> {
    let (linhas, maior) = conn.query_row(
        "SELECT COUNT(*), MAX(concurso) FROM posicao_features WHERE perfil = ?1",
        params![perfil],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<u32>>(1)?)),
    )?;
    Ok((linhas.unsigned_abs(), maior))
}

/// Insere as marcas do perfil. Chave repetida é erro.
pub fn inserir_marcas(conn: &Connection, perfil: &str, linhas: &[LinhaMarca]) -> Result<usize> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO posicao_marcas (perfil, concurso, posicao, saiu) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for l in linhas {
        stmt.execute(params![perfil, l.concurso, l.posicao, l.saiu])?;
    }
    Ok(linhas.len())
}

/// Lê as marcas do perfil, ordenadas por (concurso, posição).
pub fn ler_marcas(conn: &Connection, perfil: &str) -> Result<Vec<LinhaMarca>> {
    let mut stmt = conn.prepare(
        "SELECT concurso, posicao, saiu FROM posicao_marcas
         WHERE perfil = ?1 ORDER BY concurso, posicao",
    )?;
    let linhas = stmt.query_map(params![perfil], |row| {
        Ok(LinhaMarca {
            concurso: row.get(0)?,
            posicao: row.get(1)?,
            saiu: row.get(2)?,
        })
    })?;
    Ok(linhas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Insere as features das posições (o campo `dezena` é a posição) com a versão
/// das fórmulas. Chave repetida é erro.
pub fn inserir_features(
    conn: &Connection,
    perfil: &str,
    linhas: &[FeaturesDezena],
    feature_version: u32,
) -> Result<usize> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO posicao_features (
            perfil, janela, concurso, posicao, frequencia, maior_espaco,
            maior_espaco_ocorrencias, menor_espaco, menor_espaco_ocorrencias,
            soma_variacoes, espaco_final, repeticoes_total, repeticao_atual,
            saiu, feature_version
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;
    for l in linhas {
        stmt.execute(params![
            perfil,
            l.janela,
            l.concurso,
            l.dezena,
            l.frequencia,
            l.maior_espaco,
            l.maior_espaco_ocorrencias,
            l.menor_espaco,
            l.menor_espaco_ocorrencias,
            l.soma_variacoes,
            l.espaco_final,
            l.repeticoes_total,
            l.repeticao_atual,
            l.saiu,
            feature_version
        ])?;
    }
    Ok(linhas.len())
}

/// Lê as features das posições de uma janela, ordenadas por (concurso,
/// posição). O campo `dezena` do resultado é a posição.
pub fn ler_features_da_janela(
    conn: &Connection,
    perfil: &str,
    janela: u32,
) -> Result<Vec<FeaturesDezena>> {
    let mut stmt = conn.prepare(
        "SELECT concurso, posicao, janela, frequencia, maior_espaco,
                maior_espaco_ocorrencias, menor_espaco, menor_espaco_ocorrencias,
                soma_variacoes, espaco_final, repeticoes_total, repeticao_atual, saiu
         FROM posicao_features WHERE perfil = ?1 AND janela = ?2
         ORDER BY concurso, posicao",
    )?;
    let linhas = stmt.query_map(params![perfil, janela], |row| {
        Ok(FeaturesDezena {
            concurso: row.get(0)?,
            dezena: row.get(1)?,
            janela: row.get(2)?,
            frequencia: row.get(3)?,
            maior_espaco: row.get(4)?,
            maior_espaco_ocorrencias: row.get(5)?,
            menor_espaco: row.get(6)?,
            menor_espaco_ocorrencias: row.get(7)?,
            soma_variacoes: row.get(8)?,
            espaco_final: row.get(9)?,
            repeticoes_total: row.get(10)?,
            repeticao_atual: row.get(11)?,
            saiu: row.get(12)?,
        })
    })?;
    Ok(linhas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Insere o índice das posições (`dezena` = posição, `posicao` = `g_1`).
pub fn inserir_indice(
    conn: &Connection,
    perfil: &str,
    perfil_posicoes: &str,
    linhas: &[IndiceDezena],
) -> Result<usize> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO posicao_indice
            (perfil, perfil_posicoes, concurso, posicao, total, g_1, variacao_g_1)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for l in linhas {
        stmt.execute(params![
            perfil,
            perfil_posicoes,
            l.concurso,
            l.dezena,
            l.total,
            l.posicao,
            l.variacao_posicao
        ])?;
    }
    Ok(linhas.len())
}

/// Lê o índice das posições, ordenado por (concurso, posição).
pub fn ler_indice(
    conn: &Connection,
    perfil: &str,
    perfil_posicoes: &str,
) -> Result<Vec<IndiceDezena>> {
    let mut stmt = conn.prepare(
        "SELECT concurso, posicao, total, g_1, variacao_g_1
         FROM posicao_indice WHERE perfil = ?1 AND perfil_posicoes = ?2
         ORDER BY concurso, posicao",
    )?;
    let linhas = stmt.query_map(params![perfil, perfil_posicoes], |row| {
        Ok(IndiceDezena {
            concurso: row.get(0)?,
            dezena: row.get(1)?,
            total: row.get(2)?,
            posicao: row.get(3)?,
            variacao_posicao: row.get(4)?,
        })
    })?;
    Ok(linhas.collect::<rusqlite::Result<Vec<_>>>()?)
}
