use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::motor::{FeaturesDezena, IndiceDezena};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
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

/// Copia o banco `origem` inteiro (todas as tabelas) para `destino` com
/// `VACUUM INTO`, que lê numa transação: a cópia é consistente mesmo que outro
/// processo esteja gravando, e também valeria se o banco passasse a usar WAL
/// (copiar o `.db` com `std::fs::copy` perderia o que ainda está no `-wal`).
///
/// `origem` é aberta só para leitura: não é criada se não existir nem ganha a
/// tabela `resultados` (ao contrário de [`abrir`]). O SQLite recusa `destino`
/// que já exista com conteúdo, então um backup nunca é sobrescrito.
pub fn copiar_banco(origem: &Path, destino: &Path) -> Result<()> {
    let destino_txt = destino.to_str().ok_or_else(|| SpiderError::Backup {
        caminho: destino.to_path_buf(),
        motivo: "o caminho não é UTF-8 válido".to_string(),
    })?;
    let conn = Connection::open_with_flags(
        origem,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute("VACUUM INTO ?1", [destino_txt])?;
    Ok(())
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

/// Começa uma transação só para ler (`BEGIN DEFERRED`): as consultas feitas
/// dentro dela veem o banco num único estado, mesmo que outro processo grave
/// no meio. Quem chama não faz `commit`: ao sair, ela é desfeita.
pub fn iniciar_leitura(conn: &mut Connection) -> Result<Transaction<'_>> {
    Ok(conn.transaction_with_behavior(TransactionBehavior::Deferred)?)
}

/// Abre um banco existente **só para leitura** (comandos que não gravam
/// nada, como `sync avaliar` e `sync aleatoriedade`): qualquer escrita
/// falharia no SQLite. Ao contrário de [`abrir`], não cria o arquivo nem a
/// tabela `resultados`.
pub fn abrir_somente_leitura(caminho: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        caminho,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
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

/// Lê as linhas de `features_dezena` de uma janela (a do perfil do índice),
/// ordenadas por (concurso, dezena).
pub fn ler_features_da_janela(conn: &Connection, janela: u32) -> Result<Vec<LinhaFeatures>> {
    ler_features_onde(conn, "janela = ?1", [janela])
}

/// Indica se a tabela (ou view) `nome` existe no banco. Serve para consultar
/// uma tabela derivada sem criá-la (e sem cair em "no such table").
pub fn tabela_existe(conn: &Connection, nome: &str) -> Result<bool> {
    let quantas: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1",
        params![nome],
        |row| row.get(0),
    )?;
    Ok(quantas > 0)
}

/// Maior concurso de `features_dezena` (o snapshot mais recente), ou `None`
/// se a tabela está vazia. Usa a chave primária: não varre a tabela.
pub fn maior_concurso_features(conn: &Connection) -> Result<Option<u32>> {
    Ok(
        conn.query_row("SELECT MAX(concurso) FROM features_dezena", [], |row| {
            row.get(0)
        })?,
    )
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

// ---------------------------------------------------------------------------
// Tabela `perfis`: perfis de pesos usados para gerar o índice.
//
// Derivada como as outras (pode ser apagada junto com `indice`), mas nunca
// sobrescrita: a regra "mesmo nome, outro conteúdo → erro" está em
// `orquestracao::registrar_perfil`; aqui só há `INSERT` simples, então nem
// uma corrida entre dois processos troca um perfil já gravado (o segundo
// falha na chave primária).
// ---------------------------------------------------------------------------

/// Cria `perfis` se ainda não existe. Idempotente.
pub fn criar_tabela_perfis(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS perfis (
            nome          TEXT NOT NULL PRIMARY KEY,
            versao        INTEGER NOT NULL,
            loteria       TEXT NOT NULL,
            janela        INTEGER NOT NULL,
            normalizacao  TEXT NOT NULL,
            hash_toml     TEXT NOT NULL,
            toml          TEXT NOT NULL,
            criado_em     TEXT NOT NULL
        );",
    )?;
    Ok(())
}

/// Uma linha de `perfis`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerfilRegistrado {
    pub nome: String,
    pub versao: u32,
    pub loteria: String,
    pub janela: u32,
    /// `zscore` ou `minmax` (texto de `perfil::Normalizacao::como_str`).
    pub normalizacao: String,
    /// SHA-256 (hex minúsculo) de `toml`.
    pub hash_toml: String,
    /// Texto do `.toml` com fim de linha em LF.
    pub toml: String,
    /// Momento do primeiro registro, fornecido por quem registra.
    pub criado_em: String,
}

/// Lê o perfil registrado com esse nome, ou `None`.
pub fn buscar_perfil(conn: &Connection, nome: &str) -> Result<Option<PerfilRegistrado>> {
    conn.query_row(
        "SELECT nome, versao, loteria, janela, normalizacao, hash_toml, toml, criado_em
         FROM perfis WHERE nome = ?1",
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
        "INSERT INTO perfis
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

// ---------------------------------------------------------------------------
// Tabela `indice`: índice de força por (concurso, dezena, perfil).
//
// Derivada: `sync indice` apaga as linhas do perfil e insere todas de novo a
// cada execução, na mesma transação do registro em `perfis`.
// ---------------------------------------------------------------------------

/// Cria `indice` se ainda não existe. Idempotente.
///
/// Esquema do plano: `perfil` é `perfis.nome`, sem chave estrangeira (a
/// imutabilidade do perfil é garantida pelo registro, não pelo SQLite);
/// `variacao_posicao` é `NULL` só no primeiro concurso. `WITHOUT ROWID`
/// pelo mesmo motivo de `features_dezena`.
pub fn criar_tabela_indice(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS indice (
            concurso          INTEGER NOT NULL,
            dezena            INTEGER NOT NULL,
            perfil            TEXT NOT NULL,
            total             REAL NOT NULL,
            posicao           INTEGER NOT NULL,
            variacao_posicao  INTEGER,
            PRIMARY KEY (concurso, dezena, perfil)
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Apaga todas as linhas de `indice` do perfil. Devolve quantas apagou.
pub fn apagar_indice_do_perfil(conn: &Connection, perfil: &str) -> Result<usize> {
    Ok(conn.execute("DELETE FROM indice WHERE perfil = ?1", params![perfil])?)
}

const COLUNAS_INDICE: usize = 6;

/// Insere linhas em `indice` com o `perfil` dado, em blocos de
/// [`LINHAS_POR_INSERT`] linhas por `INSERT` (como [`inserir_features`]).
/// Linha com chave já existente é erro: quem chama apaga antes as do perfil.
pub fn inserir_indice(conn: &Connection, perfil: &str, linhas: &[IndiceDezena]) -> Result<usize> {
    let (blocos, resto) = linhas.as_chunks::<LINHAS_POR_INSERT>();
    if !blocos.is_empty() {
        let mut stmt = conn.prepare_cached(&sql_inserir_indice(LINHAS_POR_INSERT))?;
        let mut valores = Vec::with_capacity(LINHAS_POR_INSERT * COLUNAS_INDICE);
        for bloco in blocos {
            valores.clear();
            for l in bloco {
                valores.extend(valores_indice(l, perfil));
            }
            stmt.execute(rusqlite::params_from_iter(&valores))?;
        }
    }
    if !resto.is_empty() {
        let mut stmt = conn.prepare_cached(&sql_inserir_indice(1))?;
        for l in resto {
            stmt.execute(rusqlite::params_from_iter(valores_indice(l, perfil)))?;
        }
    }
    Ok(linhas.len())
}

fn sql_inserir_indice(linhas: usize) -> String {
    let marcadores = vec!["(?, ?, ?, ?, ?, ?)"; linhas].join(", ");
    format!(
        "INSERT INTO indice (
            concurso, dezena, perfil, total, posicao, variacao_posicao
         ) VALUES {marcadores}"
    )
}

/// Valores de uma linha, na ordem das colunas de `sql_inserir_indice`.
fn valores_indice(l: &IndiceDezena, perfil: &str) -> [rusqlite::types::Value; COLUNAS_INDICE] {
    use rusqlite::types::Value;
    [
        Value::Integer(i64::from(l.concurso)),
        Value::Integer(i64::from(l.dezena)),
        Value::Text(perfil.to_string()),
        // `f64` vai inteiro para o REAL do SQLite (IEEE 754 de 8 bytes): o
        // valor lido de volta tem os mesmos bits.
        Value::Real(l.total),
        Value::Integer(i64::from(l.posicao)),
        l.variacao_posicao.map_or(Value::Null, Value::Integer),
    ]
}

/// Lê as linhas de `indice` do perfil, ordenadas por (concurso, dezena).
pub fn ler_indice(conn: &Connection, perfil: &str) -> Result<Vec<IndiceDezena>> {
    let mut stmt = conn.prepare(
        "SELECT concurso, dezena, total, posicao, variacao_posicao
         FROM indice WHERE perfil = ?1
         ORDER BY concurso, dezena",
    )?;
    let linhas = stmt.query_map(params![perfil], |row| {
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

    fn perfil_registrado(nome: &str, hash: &str) -> PerfilRegistrado {
        PerfilRegistrado {
            nome: nome.to_string(),
            versao: 1,
            loteria: "quina".to_string(),
            janela: 50,
            normalizacao: "zscore".to_string(),
            hash_toml: hash.to_string(),
            toml: "nome = \"padrao_v1\"\n".to_string(),
            criado_em: "2026-10-04T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn perfis_round_trip_e_ausente() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabela_perfis(&conn).expect("cria");
        criar_tabela_perfis(&conn).expect("idempotente");
        assert_eq!(buscar_perfil(&conn, "padrao_v1").expect("busca"), None);
        let p = perfil_registrado("padrao_v1", "abc");
        inserir_perfil(&conn, &p).expect("insere");
        assert_eq!(buscar_perfil(&conn, "padrao_v1").expect("busca"), Some(p));
        assert_eq!(buscar_perfil(&conn, "outro_v1").expect("busca"), None);
    }

    #[test]
    fn inserir_perfil_nunca_substitui() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabela_perfis(&conn).expect("cria");
        let original = perfil_registrado("padrao_v1", "abc");
        inserir_perfil(&conn, &original).expect("insere");
        assert!(inserir_perfil(&conn, &perfil_registrado("padrao_v1", "def")).is_err());
        assert_eq!(
            buscar_perfil(&conn, "padrao_v1").expect("busca"),
            Some(original)
        );
    }

    // --- indice e leituras usadas pelo `sync indice` ------------------------

    fn linha_indice(concurso: u32, dezena: u32, i: u32) -> IndiceDezena {
        IndiceDezena {
            concurso,
            dezena,
            // Valores com bits "difíceis": negativos, frações sem
            // representação exata, subnormal e zero.
            total: match i % 5 {
                0 => -1.234_567_891,
                1 => 0.1 + 0.2,
                2 => f64::MIN_POSITIVE / 4.0,
                3 => 0.0,
                _ => 1e9 + f64::from(i) / 3.0,
            },
            posicao: 1 + i % 80,
            variacao_posicao: (concurso > 1).then_some(i64::from(i % 7) - 3),
        }
    }

    #[test]
    fn indice_round_trip_em_blocos_e_no_resto_preserva_bits_e_nulls() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabela_indice(&conn).expect("cria");
        criar_tabela_indice(&conn).expect("idempotente");
        let total = 2 * LINHAS_POR_INSERT + 7;
        let gravadas: Vec<IndiceDezena> = (0..total as u32)
            .map(|i| linha_indice(1 + i / 10, 1 + i % 10, i))
            .collect();
        assert_eq!(
            inserir_indice(&conn, "p_v1", &gravadas).expect("insere"),
            total
        );
        let lidas = ler_indice(&conn, "p_v1").expect("lê");
        assert_eq!(lidas, gravadas);
        // Igualdade de f64 não distingue 0,0 de -0,0: confere os bits.
        for (l, g) in lidas.iter().zip(&gravadas) {
            assert_eq!(l.total.to_bits(), g.total.to_bits());
        }
        let nulos: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM indice WHERE variacao_posicao IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("conta");
        assert_eq!(nulos, 10, "só as 10 linhas do concurso 1");
        let tipos: String = conn
            .query_row(
                "SELECT group_concat(DISTINCT typeof(total)) FROM indice",
                [],
                |r| r.get(0),
            )
            .expect("tipos");
        assert_eq!(tipos, "real");
    }

    #[test]
    fn apagar_indice_do_perfil_nao_toca_nos_outros() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabela_indice(&conn).expect("cria");
        let a: Vec<_> = (0..5).map(|i| linha_indice(1, i + 1, i)).collect();
        let b: Vec<_> = (0..3).map(|i| linha_indice(1, i + 1, i + 1)).collect();
        inserir_indice(&conn, "a_v1", &a).expect("insere a");
        inserir_indice(&conn, "b_v1", &b).expect("insere b");
        assert_eq!(apagar_indice_do_perfil(&conn, "a_v1").expect("apaga"), 5);
        assert!(ler_indice(&conn, "a_v1").expect("lê a").is_empty());
        assert_eq!(ler_indice(&conn, "b_v1").expect("lê b"), b);
        assert_eq!(
            apagar_indice_do_perfil(&conn, "nenhum_v1").expect("apaga"),
            0
        );
    }

    #[test]
    fn indice_com_chave_repetida_e_erro() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        criar_tabela_indice(&conn).expect("cria");
        inserir_indice(&conn, "a_v1", &[linha_indice(1, 1, 0)]).expect("insere");
        assert!(inserir_indice(&conn, "a_v1", &[linha_indice(1, 1, 1)]).is_err());
        // Mesma (concurso, dezena) em outro perfil é outra chave.
        inserir_indice(&conn, "b_v1", &[linha_indice(1, 1, 1)]).expect("outro perfil");
    }

    #[test]
    fn tabela_existe_e_maior_concurso_sem_criar_nada() {
        let conn = abrir(Path::new(":memory:")).expect("banco em memória");
        assert!(tabela_existe(&conn, "resultados").expect("consulta"));
        assert!(!tabela_existe(&conn, "features_dezena").expect("consulta"));
        assert!(!tabela_existe(&conn, "indice").expect("consulta"));
        // Consultar não criou nada.
        assert!(!tabela_existe(&conn, "features_dezena").expect("consulta"));

        criar_tabelas_features(&conn).expect("cria");
        assert!(tabela_existe(&conn, "features_dezena").expect("consulta"));
        assert_eq!(maior_concurso_features(&conn).expect("maior"), None);
        inserir_features(&conn, &[linha(3, 1, 0), linha(7, 2, 10)], 1).expect("insere");
        assert_eq!(maior_concurso_features(&conn).expect("maior"), Some(7));
    }

    #[test]
    fn ler_features_da_janela_filtra_e_ordena() {
        let conn = conn_com_tabelas();
        let gravadas = vec![
            linha(2, 2, 50),
            linha(1, 1, 10),
            linha(2, 1, 50),
            linha(1, 2, 50),
            linha(1, 1, 50),
        ];
        inserir_features(&conn, &gravadas, 4).expect("insere");
        let lidas = ler_features_da_janela(&conn, 50).expect("lê");
        let chaves: Vec<_> = lidas
            .iter()
            .map(|l| (l.features.concurso, l.features.dezena, l.features.janela))
            .collect();
        assert_eq!(chaves, [(1, 1, 50), (1, 2, 50), (2, 1, 50), (2, 2, 50)]);
        assert!(lidas.iter().all(|l| l.feature_version == 4));
        assert!(ler_features_da_janela(&conn, 100).expect("lê").is_empty());
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
