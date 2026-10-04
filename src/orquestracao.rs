//! Orquestração do `sync features`: liga os resultados do banco (`db`), a
//! validação (`validacao`) e o motor puro (`motor`), e decide entre retomar
//! do estado salvo ou regenerar tudo do zero.
//!
//! Fica na biblioteca, e não na CLI, para ser testável com um banco em
//! memória; a CLI só abre o banco, chama [`sincronizar_features`] e imprime.
//! Nenhum SQL aqui: tudo passa por `db`.
//!
//! Regras (`docs/PLANEJAMENTO.md`, 1a; `docs/PROGRESSO.md`, nota da 1a.2):
//!
//! - só roda sobre um histórico que passa no `validar` (buraco é erro);
//! - o snapshot de `t = último + 1` (`saiu = NULL`) é gravado em
//!   `features_dezena` e substituído quando `t` passa a existir;
//! - sem estado salvo, versão das fórmulas diferente, estado ilegível ou
//!   incompatível, resultados já incorporados alterados (impressão diferente)
//!   ou tabela de features incoerente com o estado → regenera do zero;
//! - features, troca do snapshot do próximo concurso e `motor_estado` são
//!   gravados numa única transação: tudo ou nada.

use crate::config::LoteriaConfig;
use crate::db::{self, MotorEstadoSalvo};
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::motor::{EstadoMotor, FEATURE_VERSION, JANELAS};
use crate::validacao::{self, Problema, Severidade};
use rusqlite::Connection;
use std::fmt;

/// Por que as features foram recalculadas do zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MotivoRegeneracao {
    /// Pedido explícito (`--completo`).
    Forcada,
    /// Nenhum estado salvo (primeira execução).
    SemEstado,
    /// Estado salvo com outra versão das fórmulas.
    VersaoDiferente { salva: u32, atual: u32 },
    /// O JSON do estado salvo não pôde ser lido.
    EstadoIlegivel(String),
    /// O estado salvo não serve para esta loteria/configuração.
    EstadoIncompativel(String),
    /// Algum resultado já incorporado mudou (corrigido, removido) desde que o
    /// estado foi salvo.
    ResultadosAlterados,
    /// `features_dezena` não tem as linhas que o estado salvo pressupõe.
    FeaturesIncoerentes {
        linhas: u64,
        esperadas: u64,
        maior_concurso: Option<u32>,
    },
}

impl fmt::Display for MotivoRegeneracao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MotivoRegeneracao::Forcada => write!(f, "pedido com --completo"),
            MotivoRegeneracao::SemEstado => write!(f, "nenhum estado salvo"),
            MotivoRegeneracao::VersaoDiferente { salva, atual } => write!(
                f,
                "estado salvo na versão {salva} das features, código na versão {atual}"
            ),
            MotivoRegeneracao::EstadoIlegivel(e) => write!(f, "estado salvo ilegível: {e}"),
            MotivoRegeneracao::EstadoIncompativel(e) => {
                write!(f, "estado salvo incompatível: {e}")
            }
            MotivoRegeneracao::ResultadosAlterados => write!(
                f,
                "resultados já incorporados mudaram desde o último cálculo"
            ),
            MotivoRegeneracao::FeaturesIncoerentes {
                linhas,
                esperadas,
                maior_concurso,
            } => {
                let maior = maior_concurso.map_or_else(|| "nenhum".to_string(), |c| c.to_string());
                write!(
                    f,
                    "features_dezena com {linhas} linha(s) (maior concurso: {maior}), \
                     o estado salvo pressupõe {esperadas}"
                )
            }
        }
    }
}

/// Como as features foram calculadas nesta execução.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModoFeatures {
    /// Tudo apagado e recalculado desde o concurso 1.
    Completo(MotivoRegeneracao),
    /// Retomado do estado salvo, só com os concursos novos.
    Incremental,
}

impl fmt::Display for ModoFeatures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModoFeatures::Completo(motivo) => write!(f, "completo ({motivo})"),
            ModoFeatures::Incremental => write!(f, "incremental"),
        }
    }
}

/// O que uma execução de [`sincronizar_features`] fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoFeatures {
    pub modo: ModoFeatures,
    /// Concursos incorporados ao estado nesta execução.
    pub concursos_incorporados: usize,
    /// Linhas inseridas em `features_dezena` (inclui o snapshot do próximo
    /// concurso). Zero quando não havia nada novo.
    pub linhas_gravadas: usize,
    /// Último concurso incorporado (o maior do banco).
    pub ultimo_concurso: u32,
    /// Concurso do snapshot com `saiu = NULL` gravado em `features_dezena`.
    pub proximo_concurso: u32,
    /// Avisos do `validar` (ex.: data fora de ordem), que não impedem o cálculo.
    pub avisos: Vec<Problema>,
}

/// Calcula e grava as features da loteria `cfg` no banco `conn`.
///
/// Recusa sem gravar nada se os resultados reprovarem no `validar`
/// ([`SpiderError::ResultadosReprovados`]) ou se não houver nenhum
/// ([`SpiderError::SemResultados`]). Com `forcar_completo`, ignora o estado
/// salvo e regenera do zero.
pub fn sincronizar_features(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    forcar_completo: bool,
) -> Result<ResumoFeatures> {
    // A leitura dos resultados e do estado fica dentro da mesma transação de
    // escrita: o que é gravado corresponde exatamente ao que foi lido. Saindo
    // por erro antes do `commit`, a transação é desfeita e nada fica gravado
    // (nem as tabelas derivadas criadas).
    let tx = db::iniciar_escrita(conn)?;
    let resultados = db::listar_crescente(&tx, cfg.slug)?;

    let (erros, avisos): (Vec<Problema>, Vec<Problema>) = validacao::validar(cfg, &resultados)
        .into_iter()
        .partition(|p| p.severidade() == Severidade::Erro);
    if !erros.is_empty() {
        return Err(SpiderError::ResultadosReprovados {
            loteria: cfg.slug.to_string(),
            erros,
            avisos,
        });
    }
    let Some(ultimo_banco) = resultados.last().map(|r| r.concurso) else {
        return Err(SpiderError::SemResultados(cfg.slug.to_string()));
    };

    db::criar_tabelas_features(&tx)?;
    let decisao = if forcar_completo {
        Decisao::Completo(MotivoRegeneracao::Forcada)
    } else {
        decidir(&tx, cfg, &resultados)?
    };

    let (modo, mut estado, novos) = match decisao {
        Decisao::Completo(motivo) => {
            db::apagar_features(&tx)?;
            (
                ModoFeatures::Completo(motivo),
                EstadoMotor::novo(cfg),
                &resultados[..],
            )
        }
        Decisao::Incremental(estado) => {
            let ultimo = estado.ultimo_concurso().unwrap_or(0);
            let inicio = resultados.partition_point(|r| r.concurso <= ultimo);
            let novos = &resultados[inicio..];
            if novos.is_empty() {
                // Nada novo: o banco já está em dia. Nada é gravado.
                return Ok(ResumoFeatures {
                    modo: ModoFeatures::Incremental,
                    concursos_incorporados: 0,
                    linhas_gravadas: 0,
                    ultimo_concurso: ultimo_banco,
                    proximo_concurso: estado.proximo_concurso(),
                    avisos,
                });
            }
            // Sai o snapshot antigo do próximo concurso (`saiu = NULL`): agora
            // que o concurso existe, ele é recalculado com o alvo preenchido.
            db::apagar_features_desde(&tx, estado.proximo_concurso())?;
            (ModoFeatures::Incremental, estado, novos)
        }
    };

    let feature_version = estado.feature_version();
    let mut linhas_gravadas = 0;
    // Um concurso por vez: a memória fica em um concurso de linhas em vez dos
    // milhões do histórico inteiro. Um erro no meio desfaz a transação toda.
    for r in novos {
        let linhas = estado.adicionar(cfg, r)?;
        linhas_gravadas += db::inserir_features(&tx, &linhas, feature_version)?;
    }
    linhas_gravadas += db::inserir_features(&tx, &estado.snapshot(), feature_version)?;

    let estado_json = serde_json::to_string(&estado)
        .map_err(|e| SpiderError::LinhaInvalida(format!("estado do motor: {e}")))?;
    db::gravar_motor_estado(
        &tx,
        &MotorEstadoSalvo {
            loteria: cfg.slug.to_string(),
            feature_version,
            ultimo_concurso: ultimo_banco,
            impressao: impressao_resultados(&resultados),
            estado_json,
        },
    )?;
    tx.commit()?;

    Ok(ResumoFeatures {
        modo,
        concursos_incorporados: novos.len(),
        linhas_gravadas,
        ultimo_concurso: ultimo_banco,
        proximo_concurso: estado.proximo_concurso(),
        avisos,
    })
}

enum Decisao {
    Completo(MotivoRegeneracao),
    Incremental(EstadoMotor),
}

/// Lê e confere o estado salvo, uma única vez, antes de qualquer uso
/// (inclusive `snapshot()`). `resultados` em ordem crescente, já validados
/// (contínuos a partir do 1).
fn decidir(conn: &Connection, cfg: &LoteriaConfig, resultados: &[Resultado]) -> Result<Decisao> {
    let completo = |motivo| Ok(Decisao::Completo(motivo));

    let Some(salvo) = db::ler_motor_estado(conn)? else {
        return completo(MotivoRegeneracao::SemEstado);
    };
    if salvo.feature_version != FEATURE_VERSION {
        return completo(MotivoRegeneracao::VersaoDiferente {
            salva: salvo.feature_version,
            atual: FEATURE_VERSION,
        });
    }
    let estado: EstadoMotor = match serde_json::from_str(&salvo.estado_json) {
        Ok(estado) => estado,
        Err(e) => return completo(MotivoRegeneracao::EstadoIlegivel(e.to_string())),
    };
    if let Err(e) = estado.verificar_compatibilidade(cfg) {
        return completo(MotivoRegeneracao::EstadoIncompativel(e.to_string()));
    }
    if salvo.loteria != cfg.slug || estado.ultimo_concurso() != Some(salvo.ultimo_concurso) {
        return completo(MotivoRegeneracao::EstadoIncompativel(format!(
            "linha de motor_estado (loteria `{}`, último concurso {}) não confere com o \
             estado serializado (último concurso {:?}) ou com `{}`",
            salvo.loteria,
            salvo.ultimo_concurso,
            estado.ultimo_concurso(),
            cfg.slug
        )));
    }

    // Resultado corrigido (ou removido) até o último incorporado: o estado
    // foi construído com dados que não existem mais.
    let ultimo = salvo.ultimo_concurso;
    let incorporados = &resultados[..resultados.partition_point(|r| r.concurso <= ultimo)];
    if impressao_resultados(incorporados) != salvo.impressao {
        return completo(MotivoRegeneracao::ResultadosAlterados);
    }

    // A tabela tem de ter os concursos 1 … último + 1 completos; se alguém
    // apagou linhas, retomar deixaria buracos na série.
    let por_concurso = u64::from(cfg.qtd_dezenas_possiveis()) * JANELAS.len() as u64;
    let esperadas = (u64::from(ultimo) + 1) * por_concurso;
    let (linhas, maior_concurso) = db::resumo_features(conn)?;
    if linhas != esperadas || maior_concurso != Some(ultimo + 1) {
        return completo(MotivoRegeneracao::FeaturesIncoerentes {
            linhas,
            esperadas,
            maior_concurso,
        });
    }

    Ok(Decisao::Incremental(estado))
}

/// Impressão determinística de uma sequência de resultados: FNV-1a de 64
/// bits sobre uma linha `loteria|concurso|data|dezenas` por resultado, na
/// ordem recebida, com as dezenas em ordem crescente (o motor as trata como
/// conjunto). Serve para detectar resultado corrigido, não como proteção
/// contra adulteração. Prefixo com o algoritmo e a quantidade: trocar o
/// algoritmo no futuro só força uma regeneração.
///
/// FNV-1a implementado aqui em vez de `DefaultHasher` (cujo algoritmo pode
/// mudar entre versões do Rust) ou de um crate novo de hash.
pub fn impressao_resultados(resultados: &[Resultado]) -> String {
    const BASE: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIMO: u64 = 0x0000_0100_0000_01b3;
    let mut hash = BASE;
    let mut dezenas = Vec::new();
    for r in resultados {
        dezenas.clear();
        dezenas.extend_from_slice(&r.dezenas);
        dezenas.sort_unstable();
        let lista = dezenas
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let linha = format!(
            "{}|{}|{}|{}\n",
            r.loteria, r.concurso, r.data_sorteio, lista
        );
        for &b in linha.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(PRIMO);
        }
    }
    format!("fnv1a64:{}:{hash:016x}", resultados.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(concurso: u32, data: &str, dezenas: &[u32]) -> Resultado {
        Resultado {
            loteria: "quina".to_string(),
            concurso,
            data_sorteio: data.to_string(),
            dezenas: dezenas.to_vec(),
        }
    }

    #[test]
    fn impressao_vazia_e_valor_conhecido_do_fnv() {
        // FNV-1a de 64 bits da entrada vazia é a própria base.
        assert_eq!(impressao_resultados(&[]), "fnv1a64:0:cbf29ce484222325");
    }

    #[test]
    fn impressao_e_estavel() {
        // Valor fixo, conferido com uma implementação independente do FNV-1a
        // sobre a linha "quina|1|2020-01-01|1,2,3,4,5" + quebra de linha. Se
        // mudar, a impressão mudou de formato e todo estado salvo será
        // regenerado — mude de propósito, nunca por acidente.
        let v = [r(1, "2020-01-01", &[5, 4, 3, 2, 1])];
        assert_eq!(impressao_resultados(&v), impressao_resultados(&v));
        assert_eq!(impressao_resultados(&v), "fnv1a64:1:7652b292b4619977");
    }

    #[test]
    fn impressao_ignora_ordem_das_dezenas_mas_nao_o_resto() {
        let base = impressao_resultados(&[r(1, "2020-01-01", &[1, 2, 3, 4, 5])]);
        assert_eq!(
            impressao_resultados(&[r(1, "2020-01-01", &[5, 3, 1, 2, 4])]),
            base
        );
        for outro in [
            r(1, "2020-01-01", &[1, 2, 3, 4, 6]),
            r(1, "2020-01-02", &[1, 2, 3, 4, 5]),
            r(2, "2020-01-01", &[1, 2, 3, 4, 5]),
        ] {
            assert_ne!(impressao_resultados(&[outro]), base);
        }
        let mut outra_loteria = r(1, "2020-01-01", &[1, 2, 3, 4, 5]);
        outra_loteria.loteria = "megasena".to_string();
        assert_ne!(impressao_resultados(&[outra_loteria]), base);
    }

    #[test]
    fn impressao_distingue_fronteiras_entre_campos() {
        // "1|2" + "3" ≠ "12|" + "3": o separador entra no hash.
        let a = impressao_resultados(&[r(1, "2", &[3])]);
        let b = impressao_resultados(&[r(12, "", &[3])]);
        assert_ne!(a, b);
    }
}
