//! Cópias de segurança do banco de uma loteria antes de operações que gravam
//! muito de uma vez (hoje, o `sync atualizar` com salto, tarefa 0.8 de
//! `docs/PLANEJAMENTO.md`).
//!
//! O nome é `resultados_<loteria>_<AAAAMMDD-HHMMSS>.db`, com o horário em UTC
//! (ver [`crate::tempo::carimbo_utc`]). Dois backups da mesma loteria no
//! mesmo segundo não se sobrescrevem: o segundo ganha o sufixo `_2`, o
//! terceiro `_3` e assim por diante. A cópia em si (consistente, sem
//! sobrescrever) é [`crate::db::copiar_banco`]; aqui só há a escolha do nome
//! e a pasta. Quem lê o relógio é a CLI.

use crate::db;
use crate::error::{Result, SpiderError};
use crate::tempo;
use std::path::{Path, PathBuf};

/// Pasta padrão dos backups, relativa ao diretório de trabalho (como os
/// bancos padrão `resultados_<loteria>.db`).
pub const PASTA_PADRAO: &str = "backups";

/// Quantos nomes tentar no mesmo segundo antes de desistir. Na prática só o
/// primeiro é usado; o limite evita laço sem fim se a pasta estiver estranha.
const LIMITE_SEQUENCIA: u32 = 100;

/// Nome do arquivo de backup. `sequencia` 1 é o nome sem sufixo; a partir
/// de 2, `_<sequencia>` antes do `.db`.
pub fn nome(slug: &str, segundos: u64, sequencia: u32) -> String {
    let carimbo = tempo::carimbo_utc(segundos);
    if sequencia <= 1 {
        format!("resultados_{slug}_{carimbo}.db")
    } else {
        format!("resultados_{slug}_{carimbo}_{sequencia}.db")
    }
}

/// Copia o banco `origem` para `pasta/` (criada se não existir) com o
/// primeiro nome livre de [`nome`] e devolve o caminho do backup.
///
/// Um arquivo que apareça entre a escolha do nome e a cópia não é
/// sobrescrito: `VACUUM INTO` recusa destino com conteúdo, e o erro sobe.
pub fn criar(origem: &Path, pasta: &Path, slug: &str, segundos: u64) -> Result<PathBuf> {
    std::fs::create_dir_all(pasta).map_err(|origem_io| SpiderError::Io {
        caminho: pasta.to_path_buf(),
        origem: origem_io,
    })?;
    for sequencia in 1..=LIMITE_SEQUENCIA {
        let destino = pasta.join(nome(slug, segundos, sequencia));
        if destino.exists() {
            continue;
        }
        db::copiar_banco(origem, &destino)?;
        return Ok(destino);
    }
    Err(SpiderError::Backup {
        caminho: pasta.join(nome(slug, segundos, 1)),
        motivo: format!(
            "já existem {LIMITE_SEQUENCIA} backups de `{slug}` neste mesmo segundo; \
             nada foi copiado"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Resultado;

    /// 2026-10-04T18:30:00Z.
    const INSTANTE: u64 = 1_791_138_600;

    /// Pasta temporária única por teste e execução.
    fn dir_temporario(rotulo: &str) -> PathBuf {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "rust_spider_backup_{rotulo}_{}_{unico}",
            std::process::id()
        ))
    }

    fn resultado(concurso: u32, data: &str, dezenas: &[u32]) -> Resultado {
        Resultado {
            loteria: "quina".to_string(),
            concurso,
            data_sorteio: data.to_string(),
            dezenas: dezenas.to_vec(),
        }
    }

    /// Banco em arquivo com três concursos da Quina.
    fn banco_com_resultados(caminho: &Path) -> Vec<Resultado> {
        let lote = vec![
            resultado(1, "1994-03-13", &[25, 45, 60, 76, 79]),
            resultado(2, "1994-03-17", &[13, 30, 58, 63, 64]),
            resultado(3, "1994-03-20", &[9, 13, 21, 22, 33]),
        ];
        let mut conn = db::abrir(caminho).unwrap();
        db::salvar_resultados(&mut conn, &lote).unwrap();
        db::listar(&conn, "quina").unwrap()
    }

    #[test]
    fn nome_tem_slug_carimbo_utc_e_sufixo_so_a_partir_do_segundo() {
        assert_eq!(
            nome("quina", INSTANTE, 1),
            "resultados_quina_20261004-183000.db"
        );
        assert_eq!(
            nome("quina", INSTANTE, 0),
            "resultados_quina_20261004-183000.db"
        );
        assert_eq!(
            nome("megasena", INSTANTE, 2),
            "resultados_megasena_20261004-183000_2.db"
        );
        assert_eq!(
            nome("lotofacil", 0, 10),
            "resultados_lotofacil_19700101-000000_10.db"
        );
    }

    #[test]
    fn criar_copia_as_linhas_cria_a_pasta_e_nao_mexe_no_original() {
        let raiz = dir_temporario("copia");
        std::fs::create_dir_all(&raiz).unwrap();
        let origem = raiz.join("resultados_quina.db");
        let esperado = banco_com_resultados(&origem);
        let bytes_antes = std::fs::read(&origem).unwrap();
        let pasta = raiz.join("backups").join("aninhada");
        assert!(!pasta.exists());

        let destino = criar(&origem, &pasta, "quina", INSTANTE).unwrap();

        assert_eq!(destino, pasta.join("resultados_quina_20261004-183000.db"));
        assert!(pasta.is_dir());
        let copia = db::listar(&db::abrir(&destino).unwrap(), "quina").unwrap();
        assert_eq!(copia, esperado);
        assert_eq!(std::fs::read(&origem).unwrap(), bytes_antes);
        assert_eq!(
            db::listar(&db::abrir(&origem).unwrap(), "quina").unwrap(),
            esperado
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn criar_copia_tambem_as_tabelas_derivadas() {
        let raiz = dir_temporario("derivadas");
        std::fs::create_dir_all(&raiz).unwrap();
        let origem = raiz.join("resultados_quina.db");
        banco_com_resultados(&origem);
        db::criar_tabela_perfis(&db::abrir(&origem).unwrap()).unwrap();

        let destino = criar(&origem, &raiz.join("backups"), "quina", INSTANTE).unwrap();

        let copia = db::abrir(&destino).unwrap();
        assert!(db::tabela_existe(&copia, "perfis").unwrap());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn criar_no_mesmo_segundo_usa_sufixo_sem_sobrescrever() {
        let raiz = dir_temporario("colisao");
        std::fs::create_dir_all(&raiz).unwrap();
        let origem = raiz.join("resultados_quina.db");
        banco_com_resultados(&origem);
        let pasta = raiz.join("backups");

        let primeiro = criar(&origem, &pasta, "quina", INSTANTE).unwrap();
        let bytes_primeiro = std::fs::read(&primeiro).unwrap();
        // O banco muda entre os dois backups: o primeiro tem de ficar como era.
        let mut conn = db::abrir(&origem).unwrap();
        db::salvar_resultados(
            &mut conn,
            &[resultado(4, "1994-03-24", &[7, 13, 30, 39, 41])],
        )
        .unwrap();
        drop(conn);
        let segundo = criar(&origem, &pasta, "quina", INSTANTE).unwrap();
        let terceiro = criar(&origem, &pasta, "quina", INSTANTE).unwrap();

        assert_eq!(segundo, pasta.join("resultados_quina_20261004-183000_2.db"));
        assert_eq!(
            terceiro,
            pasta.join("resultados_quina_20261004-183000_3.db")
        );
        assert_eq!(std::fs::read(&primeiro).unwrap(), bytes_primeiro);
        let n = |p: &Path| db::listar(&db::abrir(p).unwrap(), "quina").unwrap().len();
        assert_eq!(n(&primeiro), 3);
        assert_eq!(n(&segundo), 4);
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn criar_com_origem_inexistente_da_erro_sem_criar_o_banco() {
        let raiz = dir_temporario("inexistente");
        let origem = raiz.join("resultados_quina.db");
        let pasta = raiz.join("backups");

        assert!(criar(&origem, &pasta, "quina", INSTANTE).is_err());
        assert!(!origem.exists(), "não deve criar {}", origem.display());
        assert!(!pasta.join(nome("quina", INSTANTE, 1)).exists());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn copiar_banco_recusa_destino_com_conteudo() {
        let raiz = dir_temporario("destino_ocupado");
        std::fs::create_dir_all(&raiz).unwrap();
        let origem = raiz.join("resultados_quina.db");
        banco_com_resultados(&origem);
        let destino = raiz.join("ocupado.db");
        std::fs::write(&destino, b"conteudo que nao pode sumir").unwrap();

        assert!(db::copiar_banco(&origem, &destino).is_err());
        assert_eq!(
            std::fs::read(&destino).unwrap(),
            b"conteudo que nao pode sumir"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
