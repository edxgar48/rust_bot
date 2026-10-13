//! `sync calendario` (tarefa 3.2): junta os resultados do banco (`db`, só
//! leitura) com o calendário puro (`calendario`) e devolve um relatório.
//! Nenhum SQL aqui e a biblioteca não imprime: a CLI só abre, chama e mostra.
//!
//! O relógio entra por parâmetro. É um comando de leitura: "aguardando" e
//! "atrasado" são informação, não erro (quem conta falha é a 3.6).

use crate::calendario::{self, AvisoCalendario, ConsultaDia, Situacao};
use crate::config::LoteriaConfig;
use crate::db;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::tempo;
use std::path::Path;

/// O que o `sync calendario` mostra de uma loteria.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioCalendario {
    /// O dia consultado (hoje em Brasília, ou o `--data`).
    pub consulta: ConsultaDia,
    /// (concurso, data `AAAA-MM-DD`) do concurso de maior número do banco.
    pub ultimo_concurso: (u32, String),
    /// Estado do banco frente ao relógio (sempre do instante de `agora`, não
    /// do dia consultado).
    pub situacao: Situacao,
    /// Divergências entre o calendário configurado e os últimos concursos.
    pub avisos: Vec<AvisoCalendario>,
}

/// Monta o relatório a partir dos resultados já lidos. `agora` em segundos
/// Unix; `dia_consultado` (dias desde 1970-01-01, relógio de Brasília) é
/// "hoje" se `None`.
///
/// `SemResultados` se não há concurso; `LinhaInvalida` se a data do último
/// concurso não é uma data válida (o `validar` explica melhor).
pub fn montar_relatorio_calendario(
    cfg: &LoteriaConfig,
    resultados: &[Resultado],
    agora: i64,
    dia_consultado: Option<i64>,
) -> Result<RelatorioCalendario> {
    let ultimo = resultados
        .iter()
        .max_by_key(|r| r.concurso)
        .ok_or_else(|| SpiderError::SemResultados(cfg.slug.to_string()))?;
    let dia_ultimo = tempo::dias_da_data_iso(&ultimo.data_sorteio).ok_or_else(|| {
        SpiderError::LinhaInvalida(format!(
            "{}: concurso {} com data inválida `{}` (rode `sync validar {}`)",
            cfg.slug, ultimo.concurso, ultimo.data_sorteio, cfg.slug
        ))
    })?;
    let hoje = dia_consultado.unwrap_or_else(|| calendario::dia_local(agora));
    Ok(RelatorioCalendario {
        consulta: calendario::consultar_dia(cfg, hoje),
        ultimo_concurso: (ultimo.concurso, ultimo.data_sorteio.clone()),
        situacao: calendario::situacao(cfg, dia_ultimo, agora),
        avisos: calendario::conferir(cfg, resultados),
    })
}

/// Lê o banco (somente leitura, não cria arquivo) e monta o relatório.
/// `BancoNaoEncontrado` se o arquivo não existe.
pub fn consultar_calendario(
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    agora: i64,
    dia_consultado: Option<i64>,
) -> Result<RelatorioCalendario> {
    if !db_caminho.exists() {
        return Err(SpiderError::BancoNaoEncontrado(db_caminho.to_path_buf()));
    }
    let conn = db::abrir_somente_leitura(db_caminho)?;
    let resultados = db::listar(&conn, cfg.slug)?;
    montar_relatorio_calendario(cfg, &resultados, agora, dia_consultado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendario::EstadoCalendario;
    use crate::config::buscar;

    fn quina() -> &'static LoteriaConfig {
        buscar("quina").expect("loteria configurada")
    }

    fn res(concurso: u32, data: &str) -> Resultado {
        Resultado {
            loteria: "quina".into(),
            concurso,
            data_sorteio: data.into(),
            dezenas: vec![1, 2, 3, 4, 5],
        }
    }

    /// Segundos Unix de `data` às `hora`h em Brasília.
    fn brasilia(data: &str, hora: i64) -> i64 {
        tempo::dias_da_data_iso(data).expect("data válida") * 86_400 + hora * 3600 + 3 * 3600
    }

    /// Um concurso por dia de sorteio da Quina de 2026-08-01 a `fim`.
    fn historico(fim: &str) -> Vec<Resultado> {
        let (ini, fim) = (
            tempo::dias_da_data_iso("2026-08-01").expect("data"),
            tempo::dias_da_data_iso(fim).expect("data"),
        );
        (ini..=fim)
            .filter(|&d| calendario::eh_dia_de_sorteio(quina(), d))
            .enumerate()
            .map(|(i, d)| res(i as u32 + 1, &tempo::data_iso_de_dias(d)))
            .collect()
    }

    #[test]
    fn relatorio_em_dia_usa_o_concurso_de_maior_numero() {
        let h = historico("2026-10-11");
        let ultimo = h.len() as u32;
        let r = montar_relatorio_calendario(quina(), &h, brasilia("2026-10-12", 12), None)
            .expect("relatório");
        assert_eq!(r.ultimo_concurso, (ultimo, "2026-10-11".to_string()));
        assert_eq!(r.situacao.estado, EstadoCalendario::EmDia);
        assert_eq!(r.consulta.data, "2026-10-12");
        assert!(r.consulta.eh_dia_de_sorteio);
        assert!(r.avisos.is_empty());
    }

    #[test]
    fn feriado_vira_aguardando_e_depois_atrasado() {
        // Sem o concurso de segunda 2026-10-12 (feriado).
        let h = historico("2026-10-11");
        let aguardando = montar_relatorio_calendario(quina(), &h, brasilia("2026-10-12", 22), None)
            .expect("relatório");
        assert_eq!(aguardando.situacao.estado, EstadoCalendario::Aguardando);
        assert_eq!(
            aguardando.situacao.esperado_em.as_deref(),
            Some("2026-10-12")
        );
        let atrasado = montar_relatorio_calendario(quina(), &h, brasilia("2026-10-13", 21), None)
            .expect("relatório");
        assert_eq!(atrasado.situacao.estado, EstadoCalendario::Atrasado);
    }

    #[test]
    fn data_consultada_muda_so_a_consulta() {
        let h = historico("2026-10-11");
        let agora = brasilia("2026-10-12", 12);
        let r = montar_relatorio_calendario(quina(), &h, agora, Some(20_743)).expect("relatório");
        assert_eq!(r.consulta.data, "2026-10-17");
        assert!(!r.consulta.eh_dia_de_sorteio);
        assert_eq!(r.situacao.estado, EstadoCalendario::EmDia);
    }

    #[test]
    fn banco_vazio_e_data_invalida_sao_erros_claros() {
        let agora = brasilia("2026-10-12", 12);
        let vazio = montar_relatorio_calendario(quina(), &[], agora, None).expect_err("vazio");
        assert!(matches!(vazio, SpiderError::SemResultados(ref s) if s == "quina"));
        let ruim = montar_relatorio_calendario(quina(), &[res(1, "ontem")], agora, None)
            .expect_err("data inválida");
        assert!(ruim.to_string().contains("sync validar quina"), "{ruim}");
    }

    #[test]
    fn banco_ausente_nao_e_criado() {
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_calendario_inexistente_{}.db",
            std::process::id()
        ));
        let erro = consultar_calendario(quina(), &caminho, 0, None).expect_err("sem banco");
        assert!(matches!(erro, SpiderError::BancoNaoEncontrado(_)), "{erro}");
        assert!(
            !caminho.exists(),
            "o comando de leitura não pode criar o banco"
        );
    }

    /// Apaga o arquivo temporário também se o teste entrar em pânico.
    struct ApagaAoSair(std::path::PathBuf);

    impl Drop for ApagaAoSair {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn le_o_banco_em_disco_somente_leitura() {
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_calendario_leitura_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&caminho);
        let _limpeza = ApagaAoSair(caminho.clone());
        {
            let mut conn = db::abrir(&caminho).expect("abrir");
            db::salvar_resultados(&mut conn, &historico("2026-10-11")).expect("salvar");
        }
        let r = consultar_calendario(quina(), &caminho, brasilia("2026-10-12", 12), None)
            .expect("relatório");
        assert_eq!(r.ultimo_concurso.1, "2026-10-11");
        assert_eq!(r.situacao.estado, EstadoCalendario::EmDia);
    }
}
