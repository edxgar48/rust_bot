use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use rusqlite::{params, Connection};
use std::path::Path;

pub fn abrir(caminho: &Path) -> Result<Connection> {
    let conn = Connection::open(caminho)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS resultados (
            loteria      TEXT NOT NULL,
            concurso     INTEGER NOT NULL,
            data_sorteio TEXT NOT NULL,
            dezenas      TEXT NOT NULL,
            PRIMARY KEY (loteria, concurso)
        );",
    )?;
    Ok(conn)
}

/// Insere resultados novos e atualiza os que mudaram. Retorna quantos
/// registros foram efetivamente gravados (novos + alterados), para dar
/// feedback útil em execuções repetidas (ex.: agendadas via cron/Task Scheduler).
pub fn salvar_resultados(conn: &mut Connection, resultados: &[Resultado]) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut gravados = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO resultados (loteria, concurso, data_sorteio, dezenas)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(loteria, concurso) DO UPDATE SET
                data_sorteio = excluded.data_sorteio,
                dezenas = excluded.dezenas
             WHERE data_sorteio != excluded.data_sorteio OR dezenas != excluded.dezenas",
        )?;
        for r in resultados {
            let dezenas_json =
                serde_json::to_string(&r.dezenas).map_err(|e| SpiderError::LinhaInvalida(e.to_string()))?;
            gravados += stmt.execute(params![r.loteria, r.concurso, r.data_sorteio, dezenas_json])?;
        }
    }
    tx.commit()?;
    Ok(gravados)
}

/// Maior concurso já salvo para a loteria, ou `None` se o banco ainda está vazio.
pub fn ultimo_concurso(conn: &Connection, loteria: &str) -> Result<Option<u32>> {
    conn.query_row(
        "SELECT MAX(concurso) FROM resultados WHERE loteria = ?1",
        params![loteria],
        |row| row.get::<_, Option<u32>>(0),
    )
    .map_err(SpiderError::from)
}

pub fn listar(conn: &Connection, loteria: &str) -> Result<Vec<Resultado>> {
    let mut stmt = conn.prepare(
        "SELECT loteria, concurso, data_sorteio, dezenas FROM resultados
         WHERE loteria = ?1 ORDER BY concurso DESC",
    )?;
    let linhas = stmt.query_map(params![loteria], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut resultados = Vec::new();
    for linha in linhas {
        let (loteria, concurso, data_sorteio, dezenas_json) = linha?;
        let dezenas: Vec<u32> =
            serde_json::from_str(&dezenas_json).map_err(|e| SpiderError::LinhaInvalida(e.to_string()))?;
        resultados.push(Resultado {
            loteria,
            concurso,
            data_sorteio,
            dezenas,
        });
    }
    Ok(resultados)
}
