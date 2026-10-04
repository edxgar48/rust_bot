use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::motor::FeaturesDezena;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
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
            let dezenas_json = serde_json::to_string(&r.dezenas)
                .map_err(|e| SpiderError::LinhaInvalida(e.to_string()))?;
            gravados +=
                stmt.execute(params![r.loteria, r.concurso, r.data_sorteio, dezenas_json])?;
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

/// Resultados da loteria do mais recente para o mais antigo (`listar` da CLI).
pub fn listar(conn: &Connection, loteria: &str) -> Result<Vec<Resultado>> {
    listar_ordenado(
        conn,
        loteria,
        "SELECT loteria, concurso, data_sorteio, dezenas FROM resultados
         WHERE loteria = ?1 ORDER BY concurso DESC",
    )
}

/// Resultados da loteria em ordem crescente de concurso, a ordem em que o
/// motor os incorpora.
pub fn listar_crescente(conn: &Connection, loteria: &str) -> Result<Vec<Resultado>> {
    listar_ordenado(
        conn,
        loteria,
        "SELECT loteria, concurso, data_sorteio, dezenas FROM resultados
         WHERE loteria = ?1 ORDER BY concurso ASC",
    )
}

fn listar_ordenado(conn: &Connection, loteria: &str, sql: &str) -> Result<Vec<Resultado>> {
    let mut stmt = conn.prepare(sql)?;
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
        let dezenas: Vec<u32> = serde_json::from_str(&dezenas_json)
            .map_err(|e| SpiderError::LinhaInvalida(e.to_string()))?;
        resultados.push(Resultado {
            loteria,
            concurso,
            data_sorteio,
            dezenas,
        });
    }
    Ok(resultados)
}

// ---------------------------------------------------------------------------
// Tabelas derivadas do motor (`features_dezena`, `motor_estado`).
//
// Podem ser apagadas e regeneradas a partir de `resultados` a qualquer
// momento; nada aqui altera `resultados`.
// ---------------------------------------------------------------------------

/// Começa uma transação de escrita que já reserva o banco (`BEGIN IMMEDIATE`):
/// quem lê o estado do motor e grava as features dentro dela não corre o
/// risco de outro processo gravar no meio. Sem `commit`, nada é gravado.
pub fn iniciar_escrita(conn: &mut Connection) -> Result<Transaction<'_>> {
    Ok(conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
}

/// Cria `features_dezena` e `motor_estado` se ainda não existem. Idempotente.
///
/// `features_dezena`: uma linha por (concurso, dezena, janela), colunas do
/// plano; as de feature e `saiu` aceitam `NULL`. `WITHOUT ROWID` porque a
/// chave é composta e a tabela é grande (Quina ≈ 2,3 milhões de linhas): a
/// linha mora no próprio índice da chave, sem um segundo B-tree.
///
/// `motor_estado`: uma única linha (`id = 1`) com o estado incremental do
/// motor em JSON, a versão das fórmulas, o último concurso incorporado e a
/// impressão dos resultados incorporados (para detectar resultado corrigido).
pub fn criar_tabelas_features(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS features_dezena (
            concurso                 INTEGER NOT NULL,
            dezena                   INTEGER NOT NULL,
            janela                   INTEGER NOT NULL,
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
            PRIMARY KEY (concurso, dezena, janela)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS motor_estado (
            id               INTEGER PRIMARY KEY CHECK (id = 1),
            loteria          TEXT NOT NULL,
            feature_version  INTEGER NOT NULL,
            ultimo_concurso  INTEGER NOT NULL,
            impressao        TEXT NOT NULL,
            estado           TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Conteúdo da linha de `motor_estado`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotorEstadoSalvo {
    pub loteria: String,
    pub feature_version: u32,
    pub ultimo_concurso: u32,
    /// Impressão determinística dos resultados `1 … ultimo_concurso` usados
    /// para construir o estado.
    pub impressao: String,
    /// `EstadoMotor` serializado em JSON. Fica como texto: quem lê decide o
    /// que fazer se não for possível desserializar (regenerar do zero).
    pub estado_json: String,
}

/// Lê o estado salvo do motor, ou `None` se nunca foi gravado.
pub fn ler_motor_estado(conn: &Connection) -> Result<Option<MotorEstadoSalvo>> {
    conn.query_row(
        "SELECT loteria, feature_version, ultimo_concurso, impressao, estado
         FROM motor_estado WHERE id = 1",
        [],
        |row| {
            Ok(MotorEstadoSalvo {
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

/// Grava (ou substitui) a única linha de `motor_estado`.
pub fn gravar_motor_estado(conn: &Connection, salvo: &MotorEstadoSalvo) -> Result<()> {
    conn.execute(
        "INSERT INTO motor_estado
            (id, loteria, feature_version, ultimo_concurso, impressao, estado)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
            loteria = excluded.loteria,
            feature_version = excluded.feature_version,
            ultimo_concurso = excluded.ultimo_concurso,
            impressao = excluded.impressao,
            estado = excluded.estado",
        params![
            salvo.loteria,
            salvo.feature_version,
            salvo.ultimo_concurso,
            salvo.impressao,
            salvo.estado_json
        ],
    )?;
    Ok(())
}

/// Apaga todas as linhas de `features_dezena` (regeneração do zero).
pub fn apagar_features(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM features_dezena", [])?)
}

/// Apaga as linhas de `features_dezena` a partir de `concurso` (inclusive).
/// Na retomada incremental, remove o snapshot do próximo concurso (o que
/// tinha `saiu = NULL`), que é recalculado agora que o concurso existe.
pub fn apagar_features_desde(conn: &Connection, concurso: u32) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM features_dezena WHERE concurso >= ?1",
        params![concurso],
    )?)
}

/// Linhas por `INSERT` no caminho rápido de [`inserir_features`]: 64 × 14
/// parâmetros fica bem abaixo do limite do SQLite (32766).
const LINHAS_POR_INSERT: usize = 64;
const COLUNAS_FEATURES: usize = 14;

/// Insere linhas em `features_dezena` com a `feature_version` dada. Usa
/// statements preparados e reaproveitados (cache da conexão), com várias
/// linhas por `INSERT`: chamada dentro de uma transação, é o caminho rápido
/// para milhões de linhas. Linha com chave já existente é erro (quem chama
/// apaga antes o que vai substituir).
pub fn inserir_features(
    conn: &Connection,
    linhas: &[FeaturesDezena],
    feature_version: u32,
) -> Result<usize> {
    let (blocos, resto) = linhas.as_chunks::<LINHAS_POR_INSERT>();
    if !blocos.is_empty() {
        let mut stmt = conn.prepare_cached(&sql_inserir_features(LINHAS_POR_INSERT))?;
        let mut valores = Vec::with_capacity(LINHAS_POR_INSERT * COLUNAS_FEATURES);
        for bloco in blocos {
            valores.clear();
            for f in bloco {
                valores.extend(valores_features(f, feature_version));
            }
            stmt.execute(rusqlite::params_from_iter(&valores))?;
        }
    }
    if !resto.is_empty() {
        let mut stmt = conn.prepare_cached(&sql_inserir_features(1))?;
        for f in resto {
            stmt.execute(rusqlite::params_from_iter(valores_features(
                f,
                feature_version,
            )))?;
        }
    }
    Ok(linhas.len())
}

fn sql_inserir_features(linhas: usize) -> String {
    let marcadores = vec!["(?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"; linhas].join(", ");
    format!(
        "INSERT INTO features_dezena (
            concurso, dezena, janela,
            frequencia, maior_espaco, maior_espaco_ocorrencias,
            menor_espaco, menor_espaco_ocorrencias, soma_variacoes,
            espaco_final, repeticoes_total, repeticao_atual,
            saiu, feature_version
         ) VALUES {marcadores}"
    )
}

/// Valores de uma linha, na ordem das colunas de `sql_inserir_features`.
fn valores_features(
    f: &FeaturesDezena,
    feature_version: u32,
) -> [rusqlite::types::Value; COLUNAS_FEATURES] {
    use rusqlite::types::Value;
    let inteiro = |v: u32| Value::Integer(i64::from(v));
    let opcional = |v: Option<u32>| v.map_or(Value::Null, inteiro);
    [
        inteiro(f.concurso),
        inteiro(f.dezena),
        inteiro(f.janela),
        inteiro(f.frequencia),
        opcional(f.maior_espaco),
        opcional(f.maior_espaco_ocorrencias),
        opcional(f.menor_espaco),
        opcional(f.menor_espaco_ocorrencias),
        opcional(f.soma_variacoes),
        opcional(f.espaco_final),
        inteiro(f.repeticoes_total),
        inteiro(f.repeticao_atual),
        f.saiu.map_or(Value::Null, |s| Value::Integer(i64::from(s))),
        inteiro(feature_version),
    ]
}

/// Uma linha lida de `features_dezena`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaFeatures {
    pub features: FeaturesDezena,
    pub feature_version: u32,
}

/// Lê `features_dezena` inteira, ordenada pela chave (concurso, dezena,
/// janela). `NULL` volta como `None`.
pub fn ler_features(conn: &Connection) -> Result<Vec<LinhaFeatures>> {
    ler_features_onde(conn, "1 = 1", params![])
}

/// Lê as linhas de `features_dezena` de um concurso, ordenadas por (dezena,
/// janela).
pub fn ler_features_do_concurso(conn: &Connection, concurso: u32) -> Result<Vec<LinhaFeatures>> {
    ler_features_onde(conn, "concurso = ?1", [concurso])
}

fn ler_features_onde<P: rusqlite::Params>(
    conn: &Connection,
    filtro: &str,
    parametros: P,
) -> Result<Vec<LinhaFeatures>> {
    let sql = format!(
        "SELECT concurso, dezena, janela,
                frequencia, maior_espaco, maior_espaco_ocorrencias,
                menor_espaco, menor_espaco_ocorrencias, soma_variacoes,
                espaco_final, repeticoes_total, repeticao_atual,
                saiu, feature_version
         FROM features_dezena WHERE {filtro}
         ORDER BY concurso, dezena, janela"
    );
    let mut stmt = conn.prepare(&sql)?;
    let linhas = stmt.query_map(parametros, |row| {
        Ok(LinhaFeatures {
            features: FeaturesDezena {
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
            },
            feature_version: row.get(13)?,
        })
    })?;
    Ok(linhas.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Quantas linhas `features_dezena` tem e o maior concurso presente
/// (`None` se vazia). Usado para conferir se a tabela bate com o estado salvo.
pub fn resumo_features(conn: &Connection) -> Result<(u64, Option<u32>)> {
    let (linhas, maior): (i64, Option<u32>) = conn.query_row(
        "SELECT COUNT(*), MAX(concurso) FROM features_dezena",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    // COUNT(*) nunca é negativo.
    Ok((linhas.unsigned_abs(), maior))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_com_tabelas() -> Connection {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabelas_features(&conn).expect("tabelas criadas");
        conn
    }

    fn linha(concurso: u32, dezena: u32, janela: u32) -> FeaturesDezena {
        FeaturesDezena {
            concurso,
            dezena,
            janela,
            frequencia: 6,
            maior_espaco: Some(4),
            maior_espaco_ocorrencias: Some(1),
            menor_espaco: Some(0),
            menor_espaco_ocorrencias: Some(3),
            soma_variacoes: Some(10),
            espaco_final: Some(4),
            repeticoes_total: 3,
            repeticao_atual: 0,
            saiu: Some(true),
        }
    }

    fn sem_historico(concurso: u32, dezena: u32, janela: u32) -> FeaturesDezena {
        FeaturesDezena {
            concurso,
            dezena,
            janela,
            frequencia: 0,
            maior_espaco: None,
            maior_espaco_ocorrencias: None,
            menor_espaco: None,
            menor_espaco_ocorrencias: None,
            soma_variacoes: None,
            espaco_final: None,
            repeticoes_total: 0,
            repeticao_atual: 0,
            saiu: None,
        }
    }

    fn estado_salvo(ultimo: u32) -> MotorEstadoSalvo {
        MotorEstadoSalvo {
            loteria: "quina".to_string(),
            feature_version: 1,
            ultimo_concurso: ultimo,
            impressao: format!("impressao-{ultimo}"),
            estado_json: format!("{{\"ultimo\":{ultimo}}}"),
        }
    }

    #[test]
    fn criar_tabelas_features_e_idempotente() {
        let conn = conn_com_tabelas();
        inserir_features(&conn, &[linha(1, 1, 0)], 1).expect("insere");
        gravar_motor_estado(&conn, &estado_salvo(1)).expect("grava estado");
        // Segunda e terceira chamadas não falham nem apagam nada.
        criar_tabelas_features(&conn).expect("de novo");
        criar_tabelas_features(&conn).expect("e de novo");
        assert_eq!(ler_features(&conn).expect("lê").len(), 1);
        assert_eq!(
            ler_motor_estado(&conn).expect("lê estado"),
            Some(estado_salvo(1))
        );
    }

    #[test]
    fn features_round_trip_preserva_nulls() {
        let conn = conn_com_tabelas();
        let mut saiu_falso = linha(2, 3, 10);
        saiu_falso.saiu = Some(false);
        let mut so_espacos_nulos = linha(2, 3, 50);
        so_espacos_nulos.soma_variacoes = None;
        so_espacos_nulos.saiu = None;
        // Fora de ordem de propósito: a leitura devolve pela chave.
        let gravadas = vec![
            sem_historico(1, 2, 0),
            so_espacos_nulos.clone(),
            saiu_falso.clone(),
            linha(1, 1, 100),
        ];
        assert_eq!(inserir_features(&conn, &gravadas, 7).expect("insere"), 4);

        let lidas = ler_features(&conn).expect("lê");
        let esperadas = vec![
            linha(1, 1, 100),
            sem_historico(1, 2, 0),
            saiu_falso,
            so_espacos_nulos,
        ];
        assert_eq!(
            lidas.iter().map(|l| l.features.clone()).collect::<Vec<_>>(),
            esperadas
        );
        assert!(lidas.iter().all(|l| l.feature_version == 7));

        // NULL de verdade no banco, não 0 nem -1.
        let nulos: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM features_dezena
                 WHERE concurso = 1 AND dezena = 2 AND maior_espaco IS NULL
                   AND espaco_final IS NULL AND soma_variacoes IS NULL AND saiu IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("consulta");
        assert_eq!(nulos, 1);
        assert_eq!(resumo_features(&conn).expect("resumo"), (4, Some(2)));
        assert_eq!(
            ler_features_do_concurso(&conn, 2)
                .expect("lê concurso")
                .len(),
            2
        );
    }

    #[test]
    fn round_trip_em_blocos_e_no_resto() {
        // 2 blocos de LINHAS_POR_INSERT + resto: passa pelos dois statements.
        let conn = conn_com_tabelas();
        let total = 2 * LINHAS_POR_INSERT + 5;
        let gravadas: Vec<FeaturesDezena> = (0..total as u32)
            .map(|i| {
                let mut f = if i % 3 == 0 {
                    sem_historico(1 + i / 10, 1 + i % 10, 0)
                } else {
                    linha(1 + i / 10, 1 + i % 10, 0)
                };
                f.frequencia = i;
                f.espaco_final = (i % 2 == 0).then_some(i);
                f.saiu = [None, Some(true), Some(false)][(i % 3) as usize];
                f
            })
            .collect();
        assert_eq!(
            inserir_features(&conn, &gravadas, 3).expect("insere"),
            total
        );
        let lidas: Vec<FeaturesDezena> = ler_features(&conn)
            .expect("lê")
            .into_iter()
            .map(|l| l.features)
            .collect();
        assert_eq!(lidas, gravadas);
    }

    #[test]
    fn tabela_vazia_e_estado_ausente() {
        let conn = conn_com_tabelas();
        assert_eq!(resumo_features(&conn).expect("resumo"), (0, None));
        assert!(ler_features(&conn).expect("lê").is_empty());
        assert_eq!(ler_motor_estado(&conn).expect("lê estado"), None);
    }

    #[test]
    fn motor_estado_tem_uma_linha_so() {
        let conn = conn_com_tabelas();
        gravar_motor_estado(&conn, &estado_salvo(5)).expect("grava");
        gravar_motor_estado(&conn, &estado_salvo(9)).expect("substitui");
        assert_eq!(ler_motor_estado(&conn).expect("lê"), Some(estado_salvo(9)));
        let linhas: i64 = conn
            .query_row("SELECT COUNT(*) FROM motor_estado", [], |r| r.get(0))
            .expect("conta");
        assert_eq!(linhas, 1);
    }

    #[test]
    fn apagar_features_desde_remove_so_os_concursos_seguintes() {
        let conn = conn_com_tabelas();
        let linhas: Vec<_> = (1..=4).map(|c| linha(c, 1, 0)).collect();
        inserir_features(&conn, &linhas, 1).expect("insere");
        assert_eq!(apagar_features_desde(&conn, 3).expect("apaga"), 2);
        assert_eq!(resumo_features(&conn).expect("resumo"), (2, Some(2)));
        assert_eq!(apagar_features(&conn).expect("apaga tudo"), 2);
        assert_eq!(resumo_features(&conn).expect("resumo"), (0, None));
    }

    #[test]
    fn chave_repetida_e_erro() {
        let conn = conn_com_tabelas();
        inserir_features(&conn, &[linha(1, 1, 0)], 1).expect("insere");
        assert!(inserir_features(&conn, &[linha(1, 1, 0)], 1).is_err());
    }

    #[test]
    fn transacao_sem_commit_nao_deixa_estado_parcial() {
        let mut conn = conn_com_tabelas();
        inserir_features(&conn, &[linha(1, 1, 0)], 1).expect("insere");
        gravar_motor_estado(&conn, &estado_salvo(1)).expect("grava");

        // Escreve parte, falha no meio (chave repetida) e desiste.
        {
            let tx = iniciar_escrita(&mut conn).expect("transação");
            apagar_features(&tx).expect("apaga");
            inserir_features(&tx, &[linha(2, 1, 0), linha(3, 1, 0)], 1).expect("insere");
            gravar_motor_estado(&tx, &estado_salvo(3)).expect("grava");
            assert!(inserir_features(&tx, &[linha(4, 1, 0), linha(4, 1, 0)], 1).is_err());
            // `tx` sai de escopo sem commit: rollback.
        }

        let lidas: Vec<_> = ler_features(&conn)
            .expect("lê")
            .into_iter()
            .map(|l| l.features)
            .collect();
        assert_eq!(lidas, vec![linha(1, 1, 0)]);
        assert_eq!(ler_motor_estado(&conn).expect("lê"), Some(estado_salvo(1)));
    }

    #[test]
    fn listar_crescente_e_listar_decrescente() {
        let mut conn = abrir(Path::new(":memory:")).expect("banco em memória");
        let r = |concurso: u32| Resultado {
            loteria: "quina".to_string(),
            concurso,
            data_sorteio: "2020-01-01".to_string(),
            dezenas: vec![1, 2, 3, 4, 5],
        };
        salvar_resultados(&mut conn, &[r(2), r(3), r(1)]).expect("salva");
        let concursos = |v: Vec<Resultado>| v.iter().map(|r| r.concurso).collect::<Vec<_>>();
        assert_eq!(concursos(listar(&conn, "quina").expect("lista")), [3, 2, 1]);
        assert_eq!(
            concursos(listar_crescente(&conn, "quina").expect("lista")),
            [1, 2, 3]
        );
    }
}
