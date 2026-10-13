//! Fluxos de gravação dos resultados (`sync baixar`, `importar`, `atualizar`),
//! movidos da CLI para a biblioteca (tarefa 3.1a): validação do lote,
//! conciliação com o cache de inserções manuais, backup do salto e o caminho
//! do `baixar` quando o `atualizar` encontra buraco.
//!
//! Nada aqui imprime: tudo o que a CLI mostrava vira um [`Evento`], entregue
//! na hora a um observador (`&mut dyn FnMut(Evento)`). Assim a ordem visível
//! das mensagens (inclusive entre stdout e stderr) é a de sempre, mesmo com a
//! rede no meio, e o `sync ciclo` pode mandar os mesmos eventos para o log.
//! O relógio, a leitura da página e o download da planilha entram como
//! funções, para testar sem rede. Nenhum SQL: tudo passa por `db`.

use super::ciclo::EventoCiclo;
use crate::backup;
use crate::config::LoteriaConfig;
use crate::correcoes::{self, Ocorrencia};
use crate::db;
use crate::error::{Result, SpiderError};
use crate::manuais::{self, Conciliacao};
use crate::model::Resultado;
use crate::parser;
use crate::validacao::{self, DecisaoGravacao, Problema};
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};

/// Quantos problemas listar antes de resumir o resto em "e mais N".
pub const LIMITE_PROBLEMAS_IMPRESSOS: usize = 50;

/// Onde a CLI mostra o evento: o relatório vai para a saída padrão; avisos de
/// dados divergentes e a recusa de um lote, para a saída de erro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    Padrao,
    Erro,
}

/// Tudo o que os fluxos de gravação têm a dizer enquanto rodam. O `Display`
/// é o texto exato que a CLI imprime (sem a quebra de linha final).
#[derive(Debug, Clone, PartialEq)]
pub enum Evento {
    /// Início do `atualizar`, antes de raspar a página.
    Verificando { nome: &'static str },
    /// Quantos concursos a planilha trouxe.
    Extraidos { quantidade: usize },
    /// Correção conhecida aplicada (padrão) ou divergente (erro).
    Correcao(Ocorrencia),
    /// Inserções manuais reaplicadas junto com o lote.
    ManuaisReaplicados { quantidade: usize, cache: PathBuf },
    /// Avisos do lote (nunca vazio).
    Avisos(Vec<Problema>),
    /// Como fechar buracos na numeração.
    DicaBuracos { slug: &'static str },
    /// Erros que recusaram o lote (nunca vazio).
    Erros(Vec<Problema>),
    /// Resultado da gravação de uma planilha ou de um lote.
    Gravados { gravados: usize, banco: PathBuf },
    /// Conciliação de uma inserção manual com o dado oficial.
    Conciliacao(Conciliacao),
    /// A página não traz nada além do último salvo.
    JaAtualizado { ultimo: u32, pagina: u32 },
    /// O concurso da página foi gravado (salto de 1 ou planilha atrasada).
    ConcursoGravado {
        concurso: u32,
        data: String,
        banco: PathBuf,
        gravados: usize,
    },
    /// Salto com o banco sem concursos da loteria.
    BancoSemConcursos { nome: &'static str, pagina: u32 },
    /// Banco existente, mas sem concursos da loteria: nada a copiar.
    SemBackupBancoVazio { nome: &'static str, banco: PathBuf },
    /// Banco inexistente: nada a copiar.
    SemBackupSemBanco { banco: PathBuf },
    /// Salto com banco: vai baixar a planilha completa.
    Salto {
        ultimo: u32,
        pagina: u32,
        faltando: u32,
    },
    /// Backup feito antes do caminho da planilha.
    BackupCriado { caminho: PathBuf },
    /// A planilha não traz o concurso da página.
    PlanilhaAtrasada { concurso: u32 },
    /// Buracos que sobraram no fim do `atualizar` (só aviso).
    BuracosRestantes {
        nome: &'static str,
        total: u32,
        faixas: Vec<(u32, u32)>,
    },
    /// Passos do `sync ciclo` (3.1b).
    Ciclo(EventoCiclo),
}

impl Evento {
    pub fn destino(&self) -> Destino {
        match self {
            Evento::Correcao(Ocorrencia::Divergente { .. }) | Evento::Erros(_) => Destino::Erro,
            Evento::Conciliacao(Conciliacao::Resolvida { .. }) => Destino::Padrao,
            Evento::Conciliacao(_) => Destino::Erro,
            Evento::Ciclo(e) => e.destino(),
            _ => Destino::Padrao,
        }
    }
}

impl fmt::Display for Evento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Evento::Verificando { nome } => {
                write!(f, "Verificando o último resultado de {nome}...")
            }
            Evento::Extraidos { quantidade } => {
                write!(f, "{quantidade} concurso(s) extraído(s) do arquivo.")
            }
            Evento::Correcao(o @ Ocorrencia::Aplicada(_)) => {
                write!(f, "Correção conhecida: {o}")
            }
            Evento::Correcao(o) => write!(f, "{o}"),
            Evento::ManuaisReaplicados { quantidade, cache } => write!(
                f,
                "{quantidade} concurso(s) inserido(s) à mão reaplicado(s) a partir de {}.",
                cache.display()
            ),
            Evento::Avisos(p) => write!(f, "{}", formatar_problemas("aviso(s)", p).join("\n")),
            Evento::DicaBuracos { slug } => write!(
                f,
                "Para fechar os buracos: `sync buracos {slug}` lista os concursos que faltam e \
                 `sync inserir {slug} <concurso> --data AAAA-MM-DD --dezenas ...` fecha um a um \
                 (ou `sync baixar {slug}` para o histórico completo)."
            ),
            Evento::Erros(p) => write!(f, "{}", formatar_problemas("erro(s)", p).join("\n")),
            Evento::Gravados { gravados, banco } => write!(
                f,
                "{gravados} registro(s) novo(s)/atualizado(s) em {}",
                banco.display()
            ),
            Evento::Conciliacao(c) => write!(f, "{c}"),
            Evento::JaAtualizado { ultimo, pagina } => write!(
                f,
                "Já está atualizado: último concurso salvo é {ultimo}, a página mostra {pagina}."
            ),
            Evento::ConcursoGravado {
                concurso,
                data,
                banco,
                gravados,
            } => write!(
                f,
                "Concurso {concurso} ({data}) gravado em {} ({gravados} registro(s) \
                 novo(s)/atualizado(s)).",
                banco.display()
            ),
            Evento::BancoSemConcursos { nome, pagina } => write!(
                f,
                "O banco não tem concursos de {nome} e a página mostra o {pagina}: \
                 baixando a planilha completa."
            ),
            Evento::SemBackupBancoVazio { nome, banco } => write!(
                f,
                "O banco {} não tem concursos de {nome}: nenhum backup feito.",
                banco.display()
            ),
            Evento::SemBackupSemBanco { banco } => write!(
                f,
                "Não há banco para copiar ({}): nenhum backup feito.",
                banco.display()
            ),
            Evento::Salto {
                ultimo,
                pagina,
                faltando,
            } => write!(
                f,
                "Salto: o último concurso salvo é {ultimo} e a página mostra {pagina} \
                 ({faltando} concurso(s) perdido(s) no meio): baixando a planilha completa."
            ),
            Evento::BackupCriado { caminho } => {
                write!(f, "Backup do banco em {}", caminho.display())
            }
            Evento::PlanilhaAtrasada { concurso } => write!(
                f,
                "A planilha ainda não traz o concurso {concurso} da página; \
                 gravando-o a partir da página."
            ),
            Evento::BuracosRestantes {
                nome,
                total,
                faixas,
            } => {
                let faixas: Vec<String> =
                    faixas.iter().map(|&(i, f)| descrever_faixa(i, f)).collect();
                write!(
                    f,
                    "Atenção: ainda falta(m) {total} concurso(s) de {nome} no banco (nem a \
                     planilha nem a página os trazem): {}.",
                    faixas.join(", ")
                )
            }
            Evento::Ciclo(e) => write!(f, "{e}"),
        }
    }
}

/// Observador que descarta os eventos (testes e chamadas sem saída).
pub fn ignorar_eventos(_: Evento) {}

/// Linhas do bloco de problemas (`N rótulo:`, um item por linha, resumo do
/// excedente); vazio se não há problemas.
pub fn formatar_problemas<P: fmt::Display>(rotulo: &str, problemas: &[P]) -> Vec<String> {
    if problemas.is_empty() {
        return Vec::new();
    }
    let mut linhas = vec![format!("{} {rotulo}:", problemas.len())];
    for p in problemas.iter().take(LIMITE_PROBLEMAS_IMPRESSOS) {
        linhas.push(format!("  - {p}"));
    }
    if problemas.len() > LIMITE_PROBLEMAS_IMPRESSOS {
        linhas.push(format!(
            "  ... e mais {} não exibido(s).",
            problemas.len() - LIMITE_PROBLEMAS_IMPRESSOS
        ));
    }
    linhas
}

/// Faixas de concursos faltando (início, fim), como o `validar` as vê.
pub fn faixas_faltando(resultados: &[Resultado]) -> Vec<(u32, u32)> {
    validacao::validar_sequencia(resultados)
        .into_iter()
        .filter_map(|p| match p {
            Problema::ConcursosFaltando { inicio, fim } => Some((inicio, fim)),
            _ => None,
        })
        .collect()
}

pub fn descrever_faixa(inicio: u32, fim: u32) -> String {
    if inicio == fim {
        format!("{inicio}")
    } else {
        format!("{inicio}–{fim}")
    }
}

/// O que já está gravado, sem criar o banco: `db::abrir` cria o arquivo, e um
/// lote recusado sobre banco inexistente não deve deixar um `.db` vazio.
pub fn carregar_existentes(cfg: &LoteriaConfig, db_caminho: &Path) -> Result<Vec<Resultado>> {
    if !db_caminho.exists() {
        return Ok(Vec::new());
    }
    let conn = db::abrir(db_caminho)?;
    db::listar(&conn, cfg.slug)
}

/// Valida o lote contra o que já está no banco antes de gravar. Emite os
/// avisos; se houver erro, emite-os e falha sem gravar nada (tudo ou nada,
/// [`SpiderError::LoteRecusado`]).
pub fn validar_lote(
    cfg: &LoteriaConfig,
    existentes: &[Resultado],
    novos: &[Resultado],
    db_caminho: &Path,
    obs: &mut dyn FnMut(Evento),
) -> Result<DecisaoGravacao> {
    let decisao = validacao::avaliar_gravacao(cfg, existentes, novos);
    if !decisao.avisos.is_empty() {
        obs(Evento::Avisos(decisao.avisos.clone()));
    }
    if decisao
        .avisos
        .iter()
        .any(|p| matches!(p, Problema::ConcursosFaltando { .. }))
    {
        obs(Evento::DicaBuracos { slug: cfg.slug });
    }
    if !decisao.pode_gravar() {
        obs(Evento::Erros(decisao.erros.clone()));
        return Err(SpiderError::LoteRecusado {
            loteria: cfg.slug.to_string(),
            erros: decisao.erros,
            caminho: db_caminho.to_path_buf(),
        });
    }
    Ok(decisao)
}

/// O que um `processar_e_salvar` fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoGravacao {
    /// Concursos lidos da planilha.
    pub extraidos: usize,
    /// Inserções manuais reaplicadas junto com o lote.
    pub manuais_reaplicados: usize,
    /// Registros novos ou atualizados no banco.
    pub gravados: usize,
}

/// Lê a planilha `arquivo`, aplica as correções conhecidas, junta as
/// inserções manuais que ela ainda não traz, valida o lote inteiro e grava
/// (tudo ou nada). Depois concilia o cache de manuais. É o caminho comum do
/// `importar`, do `baixar` e do salto do `atualizar`.
pub fn processar_e_salvar(
    arquivo: &Path,
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    cache_dir: &Path,
    obs: &mut dyn FnMut(Evento),
) -> Result<ResumoGravacao> {
    let mut resultados = parser::parsear_arquivo(arquivo, cfg)?;
    let extraidos = resultados.len();
    obs(Evento::Extraidos {
        quantidade: extraidos,
    });
    for o in correcoes::aplicar(cfg.slug, &mut resultados) {
        obs(Evento::Correcao(o));
    }

    // Concursos inseridos à mão que a planilha ainda não traz entram no mesmo
    // lote: assim um banco recriado do zero recupera os manuais.
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let (restantes, mut conciliacoes) = manuais::conciliar(&entradas, &resultados);
    let existentes = carregar_existentes(cfg, db_caminho)?;
    let (pendentes, difere_do_banco) =
        manuais::pendentes(&restantes, &resultados, &existentes, cfg.slug);
    conciliacoes.extend(difere_do_banco);
    let manuais_reaplicados = pendentes.len();
    if !pendentes.is_empty() {
        obs(Evento::ManuaisReaplicados {
            quantidade: manuais_reaplicados,
            cache: cache_caminho.clone(),
        });
        resultados.extend(pendentes);
    }

    validar_lote(cfg, &existentes, &resultados, db_caminho, obs)?;
    // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, &resultados)?;
    obs(Evento::Gravados {
        gravados,
        banco: db_caminho.to_path_buf(),
    });

    for c in conciliacoes {
        obs(Evento::Conciliacao(c));
    }
    if restantes != entradas {
        manuais::salvar(&cache_caminho, cfg.slug, &restantes)?;
    }
    Ok(ResumoGravacao {
        extraidos,
        manuais_reaplicados,
        gravados,
    })
}

/// Depois de gravar dados oficiais: remove do cache as inserções manuais
/// confirmadas e avisa as divergentes.
pub fn conciliar_cache(
    cfg: &LoteriaConfig,
    cache_dir: &Path,
    oficiais: &[Resultado],
    obs: &mut dyn FnMut(Evento),
) -> Result<()> {
    let cache_caminho = manuais::caminho(cache_dir, cfg.slug);
    let entradas = manuais::carregar(&cache_caminho, cfg.slug)?;
    let (restantes, conciliacoes) = manuais::conciliar(&entradas, oficiais);
    for c in conciliacoes {
        obs(Evento::Conciliacao(c));
    }
    if restantes != entradas {
        manuais::salvar(&cache_caminho, cfg.slug, &restantes)?;
    }
    Ok(())
}

/// Pastas usadas pelo `atualizar`: backups do banco e cache de inserções
/// manuais. A pasta de downloads fica com quem fornece `obter_planilha`.
pub struct PastasAtualizar<'a> {
    pub backups: &'a Path,
    pub cache: &'a Path,
}

/// Como terminou um `atualizar`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesfechoAtualizacao {
    /// A página não trouxe nada além do último salvo; nada gravado.
    SemNovidade { ultimo: u32 },
    /// A página trouxe o próximo concurso e só ele foi gravado.
    ProximoGravado,
    /// Salto: caminho da planilha completa. `backup` é a cópia feita antes
    /// (`None` com banco inexistente ou sem concursos da loteria).
    SaltoResolvido { backup: Option<PathBuf> },
}

/// O que um `atualizar` fez, para a CLI e para o resumo do ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioAtualizacao {
    pub desfecho: DesfechoAtualizacao,
    /// Maior concurso no banco ao final.
    pub ultimo_concurso: Option<u32>,
    /// Faixas (início, fim) de concursos que continuam faltando ao final.
    pub buracos: Vec<(u32, u32)>,
}

/// O `atualizar` completo: lê a página (`ler_pagina`, em geral o scraper),
/// aplica as correções conhecidas e segue com [`aplicar_atualizacao`].
pub async fn atualizar<P, FutP, F, Fut>(
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasAtualizar<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: impl FnOnce() -> Result<u64>,
    ler_pagina: P,
    obter_planilha: F,
) -> Result<RelatorioAtualizacao>
where
    P: FnOnce() -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    obs(Evento::Verificando {
        nome: cfg.nome_exibicao,
    });
    let mut resultado = ler_pagina().await?;
    for o in correcoes::aplicar(cfg.slug, std::slice::from_mut(&mut resultado)) {
        obs(Evento::Correcao(o));
    }
    aplicar_atualizacao(
        cfg,
        &resultado,
        db_caminho,
        pastas,
        obs,
        agora,
        obter_planilha,
    )
    .await
}

/// O `atualizar` depois de raspar a página: nada a fazer, gravar o próximo
/// concurso ou, num salto, fazer backup e seguir o caminho do `baixar`
/// (tarefa 0.8). O relógio (`agora`, lido só no salto com banco) e o download
/// da planilha (`obter_planilha`) vêm de fora para o fluxo ser testável sem
/// rede. Falha no caminho da planilha vira
/// [`SpiderError::CaminhoDaPlanilhaFalhou`], que aponta o backup.
pub async fn aplicar_atualizacao<F, Fut>(
    cfg: &LoteriaConfig,
    resultado: &Resultado,
    db_caminho: &Path,
    pastas: &PastasAtualizar<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: impl FnOnce() -> Result<u64>,
    obter_planilha: F,
) -> Result<RelatorioAtualizacao>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let existentes = carregar_existentes(cfg, db_caminho)?;
    let ultimo_salvo = existentes.iter().map(|r| r.concurso).max();

    let classificacao = super::classificar_atualizacao(ultimo_salvo, resultado.concurso);
    let (ultimo, pagina) = match classificacao {
        super::Atualizacao::SemNovidade { ultimo } => {
            obs(Evento::JaAtualizado {
                ultimo,
                pagina: resultado.concurso,
            });
            // Uma inserção manual do mesmo concurso ainda pode ser conciliada.
            conciliar_cache(cfg, pastas.cache, std::slice::from_ref(resultado), obs)?;
            return Ok(relatorio(
                DesfechoAtualizacao::SemNovidade { ultimo },
                &existentes,
            ));
        }
        super::Atualizacao::Proximo => {
            gravar_da_pagina(cfg, &existentes, resultado, db_caminho, pastas.cache, obs)?;
            // Sem reler o banco: a gravação já deu certo e uma falha de leitura
            // só para montar o relatório não pode virar erro do comando.
            let mut finais = existentes;
            finais.push(resultado.clone());
            return Ok(relatorio(DesfechoAtualizacao::ProximoGravado, &finais));
        }
        super::Atualizacao::Salto { ultimo, pagina } => (ultimo, pagina),
    };

    // Salto: gravar só o concurso da página deixaria buraco.
    let copia = if ultimo == 0 {
        obs(Evento::BancoSemConcursos {
            nome: cfg.nome_exibicao,
            pagina,
        });
        if db_caminho.exists() {
            obs(Evento::SemBackupBancoVazio {
                nome: cfg.nome_exibicao,
                banco: db_caminho.to_path_buf(),
            });
        } else {
            obs(Evento::SemBackupSemBanco {
                banco: db_caminho.to_path_buf(),
            });
        }
        None
    } else {
        obs(Evento::Salto {
            ultimo,
            pagina,
            faltando: classificacao.faltando(),
        });
        let copia = backup::criar(db_caminho, pastas.backups, cfg.slug, agora()?)?;
        obs(Evento::BackupCriado {
            caminho: copia.clone(),
        });
        Some(copia)
    };

    let feito = async {
        let arquivo = obter_planilha().await?;
        processar_e_salvar(&arquivo, cfg, db_caminho, pastas.cache, obs)?;

        // A planilha pode estar atrasada em relação à página.
        let mut existentes = carregar_existentes(cfg, db_caminho)?;
        if !existentes.iter().any(|r| r.concurso == resultado.concurso) {
            obs(Evento::PlanilhaAtrasada {
                concurso: resultado.concurso,
            });
            gravar_da_pagina(cfg, &existentes, resultado, db_caminho, pastas.cache, obs)?;
            existentes = carregar_existentes(cfg, db_caminho)?;
        }
        Ok::<_, SpiderError>(existentes)
    }
    .await;
    let existentes = feito.map_err(|e| SpiderError::CaminhoDaPlanilhaFalhou {
        loteria: cfg.slug.to_string(),
        backup: copia.clone(),
        origem: Box::new(e),
    })?;

    // Buraco que sobrou é só aviso (decisão da 0.7): o comando termina bem.
    // Sem `DicaBuracos` aqui: o estado final é exatamente o que o último
    // `validar_lote` (da planilha ou da página) avaliou, e ele reporta todos
    // os buracos desse estado; logo a dica já saiu se há buraco.
    let relatorio = relatorio(
        DesfechoAtualizacao::SaltoResolvido { backup: copia },
        &existentes,
    );
    if !relatorio.buracos.is_empty() {
        obs(Evento::BuracosRestantes {
            nome: cfg.nome_exibicao,
            total: relatorio.buracos.iter().map(|(i, f)| f - i + 1).sum(),
            faixas: relatorio.buracos.clone(),
        });
    }
    Ok(relatorio)
}

fn relatorio(desfecho: DesfechoAtualizacao, finais: &[Resultado]) -> RelatorioAtualizacao {
    RelatorioAtualizacao {
        desfecho,
        ultimo_concurso: finais.iter().map(|r| r.concurso).max(),
        buracos: faixas_faltando(finais),
    }
}

/// Grava o concurso lido na página pelo caminho do salto de 1: validação
/// contra o banco, gravação e conciliação com o cache de inserções manuais.
fn gravar_da_pagina(
    cfg: &LoteriaConfig,
    existentes: &[Resultado],
    resultado: &Resultado,
    db_caminho: &Path,
    cache: &Path,
    obs: &mut dyn FnMut(Evento),
) -> Result<()> {
    let lote = std::slice::from_ref(resultado);
    validar_lote(cfg, existentes, lote, db_caminho, obs)?;

    // Só depois de aprovado: `db::abrir` cria o arquivo se não existir.
    let mut conn = db::abrir(db_caminho)?;
    let gravados = db::salvar_resultados(&mut conn, lote)?;
    obs(Evento::ConcursoGravado {
        concurso: resultado.concurso,
        data: resultado.data_sorteio.clone(),
        banco: db_caminho.to_path_buf(),
        gravados,
    });
    conciliar_cache(cfg, cache, lote, obs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use std::cell::RefCell;

    fn quina() -> &'static LoteriaConfig {
        config::buscar("quina").expect("quina configurada")
    }

    /// 2026-10-04T18:30:00Z.
    const AGORA: u64 = 1_791_138_600;

    /// Pasta temporária com o banco, o cache e `backups/`.
    struct Cenario {
        raiz: PathBuf,
        db: PathBuf,
        cache: PathBuf,
        backups: PathBuf,
    }

    impl Cenario {
        fn novo(rotulo: &str) -> Self {
            let unico = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default();
            let raiz = std::env::temp_dir().join(format!(
                "rust_spider_gravacao_{rotulo}_{}_{unico}",
                std::process::id()
            ));
            std::fs::create_dir_all(&raiz).unwrap();
            Cenario {
                db: raiz.join("resultados_quina.db"),
                cache: raiz.join("cache"),
                backups: raiz.join("backups"),
                raiz,
            }
        }

        fn pastas(&self) -> PastasAtualizar<'_> {
            PastasAtualizar {
                backups: &self.backups,
                cache: &self.cache,
            }
        }

        fn gravar(&self, lote: &[Resultado]) {
            let mut conn = db::abrir(&self.db).unwrap();
            db::salvar_resultados(&mut conn, lote).unwrap();
        }

        fn concursos(&self) -> Vec<u32> {
            let conn = db::abrir(&self.db).unwrap();
            let mut c: Vec<u32> = db::listar(&conn, "quina")
                .unwrap()
                .iter()
                .map(|r| r.concurso)
                .collect();
            c.sort_unstable();
            c
        }

        fn backups_feitos(&self) -> Vec<PathBuf> {
            match std::fs::read_dir(&self.backups) {
                Ok(it) => it.map(|e| e.unwrap().path()).collect(),
                Err(_) => Vec::new(),
            }
        }
    }

    impl Drop for Cenario {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.raiz);
        }
    }

    fn fixture_quina() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("quina.xlsx")
    }

    /// Concursos 1 a 5 da fixture, como a planilha os entrega.
    fn planilha_quina() -> Vec<Resultado> {
        parser::parsear_arquivo(&fixture_quina(), quina()).unwrap()
    }

    fn da_planilha(concurso: u32) -> Resultado {
        planilha_quina()
            .into_iter()
            .find(|r| r.concurso == concurso)
            .unwrap()
    }

    /// Concurso que a planilha (fixture) ainda não traz.
    fn so_na_pagina(concurso: u32) -> Resultado {
        Resultado {
            loteria: "quina".to_string(),
            concurso,
            data_sorteio: "1994-04-30".to_string(),
            dezenas: vec![3, 17, 28, 42, 66],
        }
    }

    fn relogio_proibido() -> Result<u64> {
        panic!("o relógio só é lido num salto com banco")
    }

    async fn planilha_proibida() -> Result<PathBuf> {
        panic!("a planilha só é baixada num salto")
    }

    fn sem_rede() -> SpiderError {
        SpiderError::Navegador {
            url: "https://example.invalid".to_string(),
            detalhe: "sem rede".to_string(),
        }
    }

    /// Mensagem e causas, como o `{:#}` do anyhow as mostra.
    fn cadeia(erro: &(dyn std::error::Error + 'static)) -> String {
        let mut texto = erro.to_string();
        let mut atual = erro.source();
        while let Some(causa) = atual {
            texto.push_str(": ");
            texto.push_str(&causa.to_string());
            atual = causa.source();
        }
        texto
    }

    /// Chama `aplicar_atualizacao` guardando os eventos.
    async fn aplicar<F, Fut>(
        c: &Cenario,
        pagina: &Resultado,
        agora: impl FnOnce() -> Result<u64>,
        planilha: F,
    ) -> (Result<RelatorioAtualizacao>, Vec<Evento>)
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<PathBuf>>,
    {
        let eventos = RefCell::new(Vec::new());
        let r = aplicar_atualizacao(
            quina(),
            pagina,
            &c.db,
            &c.pastas(),
            &mut |e| eventos.borrow_mut().push(e),
            agora,
            planilha,
        )
        .await;
        (r, eventos.into_inner())
    }

    #[tokio::test]
    async fn atualizar_sem_novidade_nao_baixa_nem_copia() {
        let c = Cenario::novo("sem_novidade");
        c.gravar(&planilha_quina());
        let (r, eventos) = aplicar(&c, &da_planilha(5), relogio_proibido, planilha_proibida).await;
        let r = r.unwrap();
        assert_eq!(r.desfecho, DesfechoAtualizacao::SemNovidade { ultimo: 5 });
        assert_eq!(r.ultimo_concurso, Some(5));
        assert!(r.buracos.is_empty());
        assert_eq!(
            eventos,
            [Evento::JaAtualizado {
                ultimo: 5,
                pagina: 5
            }]
        );
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_salto_de_um_grava_so_o_da_pagina_sem_backup() {
        let c = Cenario::novo("proximo");
        c.gravar(&planilha_quina()[..4]);
        let (r, eventos) = aplicar(&c, &da_planilha(5), relogio_proibido, planilha_proibida).await;
        let r = r.unwrap();
        assert_eq!(r.desfecho, DesfechoAtualizacao::ProximoGravado);
        assert_eq!(r.ultimo_concurso, Some(5));
        assert!(matches!(
            eventos.as_slice(),
            [Evento::ConcursoGravado { concurso: 5, .. }]
        ));
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_salto_faz_backup_e_grava_a_planilha_completa() {
        let c = Cenario::novo("salto");
        c.gravar(&planilha_quina()[..2]);
        let (r, eventos) = aplicar(
            &c,
            &da_planilha(5),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await;
        let r = r.unwrap();

        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        let backup = c.backups.join("resultados_quina_20261004-183000.db");
        assert_eq!(
            r.desfecho,
            DesfechoAtualizacao::SaltoResolvido {
                backup: Some(backup.clone())
            }
        );
        assert_eq!(c.backups_feitos(), std::slice::from_ref(&backup));
        let copia = db::listar_crescente(&db::abrir(&backup).unwrap(), "quina").unwrap();
        assert_eq!(copia, planilha_quina()[..2].to_vec());

        // A ordem dos eventos é a das mensagens que a CLI sempre imprimiu.
        assert!(matches!(
            eventos.as_slice(),
            [
                Evento::Salto {
                    ultimo: 2,
                    pagina: 5,
                    faltando: 2
                },
                Evento::BackupCriado { .. },
                Evento::Extraidos { quantidade: 5 },
                Evento::Gravados { .. },
            ]
        ));
    }

    #[tokio::test]
    async fn atualizar_salto_com_planilha_atrasada_grava_tambem_o_da_pagina() {
        let c = Cenario::novo("atrasada");
        c.gravar(&planilha_quina()[..2]);
        let (r, eventos) = aplicar(
            &c,
            &so_na_pagina(6),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await;
        r.unwrap();
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5, 6]);
        assert_eq!(c.backups_feitos().len(), 1);
        assert!(eventos.contains(&Evento::PlanilhaAtrasada { concurso: 6 }));
    }

    #[tokio::test]
    async fn atualizar_salto_que_ainda_deixa_buraco_avisa_e_termina_bem() {
        let c = Cenario::novo("buraco");
        c.gravar(&planilha_quina()[..2]);
        let (r, eventos) = aplicar(
            &c,
            &so_na_pagina(8),
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await;
        let r = r.expect("buraco é só aviso");
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5, 8]);
        assert_eq!(r.buracos, [(6, 7)]);
        let ultimo = eventos.last().unwrap();
        assert_eq!(
            ultimo.to_string(),
            "Atenção: ainda falta(m) 2 concurso(s) de Quina no banco (nem a planilha nem a \
             página os trazem): 6–7."
        );
    }

    #[tokio::test]
    async fn atualizar_salto_com_download_falhando_nao_grava_e_aponta_o_backup() {
        let c = Cenario::novo("download_falha");
        c.gravar(&planilha_quina()[..2]);
        let (r, _) = aplicar(
            &c,
            &da_planilha(5),
            || Ok(AGORA),
            || async { Err(sem_rede()) },
        )
        .await;
        let erro = r.expect_err("download falhou");

        let backup = c.backups.join("resultados_quina_20261004-183000.db");
        let texto = cadeia(&erro);
        assert!(texto.contains("sem rede"), "{texto}");
        assert!(texto.contains(&backup.display().to_string()), "{texto}");
        assert!(matches!(erro, SpiderError::CaminhoDaPlanilhaFalhou { .. }));
        assert_eq!(c.concursos(), [1, 2]);
        assert_eq!(c.backups_feitos(), [backup]);
    }

    #[tokio::test]
    async fn atualizar_salto_com_planilha_ilegivel_nao_grava_nem_o_da_pagina() {
        let c = Cenario::novo("planilha_ruim");
        c.gravar(&planilha_quina()[..2]);
        let corrompida = fixture_quina().with_file_name("corrompida.xlsx");
        let (r, _) = aplicar(
            &c,
            &so_na_pagina(6),
            || Ok(AGORA),
            || async { Ok(corrompida) },
        )
        .await;
        let erro = r.expect_err("planilha ilegível");
        assert!(cadeia(&erro).contains("backup"), "{}", cadeia(&erro));
        assert_eq!(c.concursos(), [1, 2]);
        assert_eq!(c.backups_feitos().len(), 1);
    }

    #[tokio::test]
    async fn atualizar_salto_com_lote_reprovado_nao_grava_nada() {
        let c = Cenario::novo("lote_reprovado");
        // A página traz um concurso com dezena fora da faixa que a planilha
        // não traz: a planilha (válida) é gravada e o lote da página, recusado.
        c.gravar(&planilha_quina()[..2]);
        let mut invalido = so_na_pagina(6);
        invalido.dezenas = vec![1, 2, 3, 4, 99];
        let (r, eventos) = aplicar(
            &c,
            &invalido,
            || Ok(AGORA),
            || async { Ok(fixture_quina()) },
        )
        .await;
        let erro = r.expect_err("concurso da página inválido");
        let texto = cadeia(&erro);
        assert!(texto.contains("lote recusado"), "{texto}");
        assert!(texto.contains("backup"), "{texto}");
        // A planilha (válida) já foi gravada; o concurso inválido, não.
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        // Os erros saem como evento (stderr na CLI) antes da falha.
        assert!(matches!(eventos.last(), Some(Evento::Erros(e)) if !e.is_empty()));
        assert_eq!(eventos.last().unwrap().destino(), Destino::Erro);
    }

    #[tokio::test]
    async fn atualizar_banco_inexistente_baixa_a_planilha_sem_backup() {
        let c = Cenario::novo("sem_banco");
        let (r, eventos) = aplicar(&c, &da_planilha(5), relogio_proibido, || async {
            Ok(fixture_quina())
        })
        .await;
        assert_eq!(
            r.unwrap().desfecho,
            DesfechoAtualizacao::SaltoResolvido { backup: None }
        );
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert!(!c.backups.exists());
        assert!(matches!(
            eventos.as_slice(),
            [
                Evento::BancoSemConcursos { pagina: 5, .. },
                Evento::SemBackupSemBanco { .. },
                ..
            ]
        ));
    }

    #[tokio::test]
    async fn atualizar_banco_inexistente_com_planilha_falhando_nao_cria_o_banco() {
        let c = Cenario::novo("sem_banco_falha");
        let (r, _) = aplicar(&c, &da_planilha(5), relogio_proibido, || async {
            Err(sem_rede())
        })
        .await;
        let erro = r.expect_err("download falhou");
        assert!(!cadeia(&erro).contains("o backup do banco"), "{erro}");
        assert!(!c.db.exists());
        assert!(!c.backups.exists());
    }

    #[tokio::test]
    async fn atualizar_completo_le_a_pagina_corrige_e_encadeia() {
        let c = Cenario::novo("completo");
        c.gravar(&planilha_quina()[..4]);
        let eventos = RefCell::new(Vec::new());
        let r = atualizar(
            quina(),
            &c.db,
            &c.pastas(),
            &mut |e| eventos.borrow_mut().push(e),
            relogio_proibido,
            || async { Ok(da_planilha(5)) },
            planilha_proibida,
        )
        .await
        .unwrap();
        assert_eq!(r.desfecho, DesfechoAtualizacao::ProximoGravado);
        let eventos = eventos.into_inner();
        assert_eq!(eventos[0], Evento::Verificando { nome: "Quina" });
        // Segunda rodada seguida: idempotente, nada novo.
        let r2 = atualizar(
            quina(),
            &c.db,
            &c.pastas(),
            &mut ignorar_eventos,
            relogio_proibido,
            || async { Ok(da_planilha(5)) },
            planilha_proibida,
        )
        .await
        .unwrap();
        assert_eq!(r2.desfecho, DesfechoAtualizacao::SemNovidade { ultimo: 5 });
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn atualizar_com_leitura_da_pagina_falhando_nao_toca_no_banco() {
        let c = Cenario::novo("pagina_falha");
        let erro = atualizar(
            quina(),
            &c.db,
            &c.pastas(),
            &mut ignorar_eventos,
            relogio_proibido,
            || async { Err(sem_rede()) },
            planilha_proibida,
        )
        .await
        .expect_err("página fora do ar");
        assert!(erro.to_string().contains("sem rede"));
        assert!(!c.db.exists());
    }

    // --- processar_e_salvar, validar_lote e conciliação do cache.

    fn coletar<T>(f: impl FnOnce(&mut dyn FnMut(Evento)) -> T) -> (T, Vec<Evento>) {
        let mut eventos = Vec::new();
        let r = f(&mut |e| eventos.push(e));
        (r, eventos)
    }

    #[test]
    fn processar_e_salvar_grava_a_planilha_e_resume() {
        let c = Cenario::novo("processar");
        let (r, eventos) =
            coletar(|o| processar_e_salvar(&fixture_quina(), quina(), &c.db, &c.cache, o));
        let r = r.unwrap();
        assert_eq!(
            r,
            ResumoGravacao {
                extraidos: 5,
                manuais_reaplicados: 0,
                gravados: 5
            }
        );
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5]);
        assert_eq!(
            eventos
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .first()
                .map(String::as_str),
            Some("5 concurso(s) extraído(s) do arquivo.")
        );
        assert!(matches!(
            eventos.last(),
            Some(Evento::Gravados { gravados: 5, .. })
        ));
    }

    #[test]
    fn processar_e_salvar_concilia_o_cache_e_reaplica_manuais() {
        let c = Cenario::novo("cache");
        let cache = manuais::caminho(&c.cache, "quina");
        // Concurso 3 digitado igual ao da planilha (resolvido e removido do
        // cache); concurso 9, que a planilha não traz, é reaplicado.
        let tres = da_planilha(3);
        let nove = Resultado {
            concurso: 9,
            data_sorteio: "1994-06-01".to_string(),
            ..so_na_pagina(9)
        };
        let entrada = |r: &Resultado| manuais::EntradaManual {
            concurso: r.concurso,
            data_sorteio: r.data_sorteio.clone(),
            dezenas: r.dezenas.clone(),
        };
        manuais::salvar(&cache, "quina", &[entrada(&tres), entrada(&nove)]).unwrap();

        let (r, eventos) =
            coletar(|o| processar_e_salvar(&fixture_quina(), quina(), &c.db, &c.cache, o));
        let r = r.unwrap();
        assert_eq!(r.manuais_reaplicados, 1);
        assert_eq!(c.concursos(), [1, 2, 3, 4, 5, 9]);
        assert!(eventos.contains(&Evento::Conciliacao(Conciliacao::Resolvida { concurso: 3 })));
        assert!(eventos
            .iter()
            .any(|e| matches!(e, Evento::ManuaisReaplicados { quantidade: 1, .. })));
        // O 3 saiu do cache; o 9 continua, até a planilha trazê-lo.
        let restantes = manuais::carregar(&cache, "quina").unwrap();
        assert_eq!(restantes, [entrada(&nove)]);
    }

    #[test]
    fn processar_e_salvar_com_manual_divergente_avisa_e_mantem_no_cache() {
        let c = Cenario::novo("divergente");
        let cache = manuais::caminho(&c.cache, "quina");
        let mut errado = da_planilha(3);
        errado.dezenas = vec![1, 2, 3, 4, 5];
        let entrada = manuais::EntradaManual {
            concurso: 3,
            data_sorteio: errado.data_sorteio.clone(),
            dezenas: errado.dezenas.clone(),
        };
        manuais::salvar(&cache, "quina", std::slice::from_ref(&entrada)).unwrap();

        let (r, eventos) =
            coletar(|o| processar_e_salvar(&fixture_quina(), quina(), &c.db, &c.cache, o));
        r.unwrap();
        let divergente = eventos
            .iter()
            .find(|e| matches!(e, Evento::Conciliacao(Conciliacao::Divergente { .. })))
            .expect("divergência avisada");
        assert_eq!(divergente.destino(), Destino::Erro);
        assert_eq!(manuais::carregar(&cache, "quina").unwrap(), [entrada]);
    }

    #[test]
    fn processar_e_salvar_com_planilha_ilegivel_nao_cria_banco() {
        let c = Cenario::novo("ilegivel");
        let corrompida = fixture_quina().with_file_name("corrompida.xlsx");
        let (r, eventos) =
            coletar(|o| processar_e_salvar(&corrompida, quina(), &c.db, &c.cache, o));
        r.expect_err("planilha ilegível");
        assert!(eventos.is_empty());
        assert!(!c.db.exists());
    }

    #[test]
    fn validar_lote_reprovando_emite_erros_e_nao_deixa_banco() {
        let c = Cenario::novo("reprova");
        let mut ruim = so_na_pagina(1);
        ruim.dezenas = vec![1, 2, 3, 4, 99];
        let (r, eventos) = coletar(|o| validar_lote(quina(), &[], &[ruim], &c.db, o));
        let erro = r.expect_err("dezena fora da faixa");
        match &erro {
            SpiderError::LoteRecusado {
                loteria,
                erros,
                caminho,
            } => {
                assert_eq!(loteria, "quina");
                assert!(!erros.is_empty());
                assert_eq!(caminho, &c.db);
            }
            outro => panic!("esperava LoteRecusado, veio {outro:?}"),
        }
        assert!(erro.to_string().contains("nada foi gravado"));
        assert!(matches!(eventos.as_slice(), [Evento::Erros(_)]));
        assert!(!c.db.exists());
    }

    #[test]
    fn validar_lote_aprovado_com_buraco_avisa_e_sugere_como_fechar() {
        let c = Cenario::novo("aviso_buraco");
        let existentes = planilha_quina()[..2].to_vec();
        let (r, eventos) =
            coletar(|o| validar_lote(quina(), &existentes, &[so_na_pagina(6)], &c.db, o));
        assert!(r.unwrap().pode_gravar());
        assert!(matches!(
            eventos.as_slice(),
            [Evento::Avisos(_), Evento::DicaBuracos { slug: "quina" }]
        ));
        let texto = eventos[0].to_string();
        assert!(texto.starts_with("1 aviso(s):\n  - "), "{texto}");
    }

    #[test]
    fn formatar_problemas_limita_a_lista() {
        let muitos: Vec<String> = (0..LIMITE_PROBLEMAS_IMPRESSOS + 3)
            .map(|i| format!("p{i}"))
            .collect();
        let linhas = formatar_problemas("erro(s)", &muitos);
        assert_eq!(linhas.len(), 1 + LIMITE_PROBLEMAS_IMPRESSOS + 1);
        assert_eq!(linhas[0], "53 erro(s):");
        assert_eq!(linhas.last().unwrap(), "  ... e mais 3 não exibido(s).");
        assert!(formatar_problemas::<String>("erro(s)", &[]).is_empty());
    }

    #[test]
    fn conciliar_cache_sem_cache_nao_cria_arquivo() {
        let c = Cenario::novo("cache_vazio");
        let (r, eventos) = coletar(|o| conciliar_cache(quina(), &c.cache, &planilha_quina(), o));
        r.unwrap();
        assert!(eventos.is_empty());
        assert!(!manuais::caminho(&c.cache, "quina").exists());
    }

    #[test]
    fn carregar_existentes_sem_banco_devolve_vazio_sem_criar() {
        let c = Cenario::novo("sem_banco_existentes");
        assert!(carregar_existentes(quina(), &c.db).unwrap().is_empty());
        assert!(!c.db.exists());
    }

    #[test]
    fn faixas_e_descricao() {
        let mut r = planilha_quina();
        r.remove(3); // tira o 4
        assert_eq!(faixas_faltando(&r), [(4, 4)]);
        assert_eq!(descrever_faixa(4, 4), "4");
        assert_eq!(descrever_faixa(6, 7), "6–7");
    }
}
