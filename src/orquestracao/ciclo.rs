//! O `sync ciclo` (tarefa 3.1b): compõe, por loteria, os passos que já existem
//! (atualizar, backfill, validar, features, índice, exportar) e, depois de
//! todas, um único `publicar`. Nenhuma regra de negócio nova: cada passo é a
//! função que o comando individual já usa.
//!
//! Decisões de 2026-10-13 (`docs/PLANEJAMENTO.md`, tarefa 3.1b):
//! - **falha isolada**: uma loteria que falha não interrompe as outras; o
//!   relatório diz onde cada uma parou e quem chama decide o código de saída;
//! - **publicação parcial**: só as loterias que passaram são exportadas
//!   (`index.json` com o que há no disco) e o `publicar` roda uma vez, mesmo
//!   que nenhuma tenha passado (é idempotente e reenvia push pendente);
//! - **sem concurso novo**: segue até o `publicar` (todos os passos são
//!   idempotentes; o `publicar` diz "nada a publicar");
//! - **backfill**: baixar a planilha inteira pelo caminho do `baixar`; se o
//!   buraco persistir, só aviso e o `validar` reprova a loteria.
//!
//! Rede, relógio e destino entram por parâmetro, então tudo roda nos testes
//! sem rede. A biblioteca não imprime: emite [`Evento::Ciclo`].

use super::gravacao::{atualizar_reaproveitando_backup, hoje_de};
use super::{
    carregar_existentes, descrever_faixa, exportar_com_relatorio, faixas_faltando,
    processar_e_salvar, sincronizar_features, sincronizar_indice, sincronizar_ranking_com,
    DesfechoAtualizacao, Evento, PastasAtualizar, RelatorioRanking, ResultadoComRelatorio,
    SituacaoRanking,
};
use crate::avaliacao::SEMENTE_ESPERADO;
use crate::backup;
use crate::config::LoteriaConfig;
use crate::db;
use crate::error::{Result, SpiderError};
use crate::log_arquivo::Nivel;
use crate::model::Resultado;
use crate::perfil;
use crate::publicacao::{self, Desfecho};
use crate::tempo;
use crate::validacao;
use std::fmt;
use std::future::Future;
use std::path::{Path, PathBuf};

/// Passo do ciclo em que uma loteria parou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtapaCiclo {
    Atualizar,
    Backfill,
    Validar,
    Features,
    Indice,
    Exportar,
}

impl EtapaCiclo {
    fn como_str(self) -> &'static str {
        match self {
            EtapaCiclo::Atualizar => "atualizar",
            EtapaCiclo::Backfill => "backfill",
            EtapaCiclo::Validar => "validar",
            EtapaCiclo::Features => "features",
            EtapaCiclo::Indice => "indice",
            EtapaCiclo::Exportar => "exportar",
        }
    }
}

impl fmt::Display for EtapaCiclo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.como_str())
    }
}

/// O que o ciclo tem a dizer enquanto roda; o `Display` é o texto que a CLI
/// imprime.
#[derive(Debug, Clone, PartialEq)]
pub enum EventoCiclo {
    /// Início do ciclo de uma loteria.
    Loteria {
        nome: &'static str,
        slug: &'static str,
    },
    /// Buraco interno no banco depois do `atualizar`: vai baixar a planilha.
    BackfillIniciado {
        faixas: Vec<(u32, u32)>,
    },
    /// O `atualizar` já seguiu o caminho da planilha (salto) e o buraco ficou:
    /// baixar de novo não traria nada, e o site da Caixa merece poucas visitas.
    BackfillDispensado {
        faixas: Vec<(u32, u32)>,
    },
    /// Depois do backfill o buraco continua (aviso; o `validar` reprova).
    BackfillPersiste {
        faixas: Vec<(u32, u32)>,
    },
    /// Banco sem erros de integridade.
    ValidacaoAprovada {
        concursos: usize,
        ultimo: u32,
    },
    /// Resumo das features.
    FeaturesEmDia {
        modo: String,
        incorporados: usize,
        ultimo: u32,
    },
    /// Resumo do índice.
    IndiceEmDia {
        perfil: String,
        concursos: usize,
        ultimo: u32,
    },
    /// Ranking por taxa regenerado (`sync ranking`, sem o relatório/CSV).
    RankingEmDia {
        perfil: String,
        linhas: usize,
        ultimo: u32,
    },
    /// O passo do ranking deu erro: o ciclo segue sem o `ranking_taxa.json`
    /// (a falha vira alerta pela regra das noites seguidas).
    RankingFalhou {
        slug: &'static str,
        mensagem: String,
    },
    /// O `ranking_taxa.json` ficou fora da exportação (não é erro).
    RankingOmitido {
        slug: &'static str,
        motivo: String,
    },
    /// Um `ranking_taxa.json` antigo foi apagado da saída por causa da omissão.
    RankingAntigoRemovido {
        caminho: PathBuf,
    },
    /// Arquivos da loteria gravados na saída.
    Exportada {
        slug: &'static str,
        arquivos: usize,
    },
    /// A loteria parou; as outras seguem.
    LoteriaFalhou {
        slug: &'static str,
        etapa: EtapaCiclo,
        mensagem: String,
    },
    /// Início do `publicar` único.
    Publicando,
    Publicado {
        mensagem: String,
    },
    EnvioRetomado,
    NadaAPublicar,
    PublicacaoFalhou {
        mensagem: String,
    },
    // --- `--aguardar` (tarefa 3.3) ---
    /// Dia de sorteio e a página ainda mostra o concurso anterior: a loteria
    /// espera a próxima rodada (nada é exportado nem publicado por enquanto).
    AguardandoConcurso {
        slug: &'static str,
        /// Data (`AAAA-MM-DD`) do sorteio que deveria ter saído.
        esperado_em: String,
        ultimo: Option<u32>,
    },
    /// Falha de navegador/rede: será repetida na próxima rodada.
    FalhaTransitoria {
        slug: &'static str,
        etapa: EtapaCiclo,
        mensagem: String,
    },
    /// Antes de dormir: quando e para quem é a próxima rodada.
    ProximaRodada {
        rodada: u32,
        /// `HH:MM` no relógio de Brasília.
        hora: String,
        espera_minutos: u64,
        pendentes: Vec<&'static str>,
    },
    /// A janela fechou (01h00): o que ainda faltava não será mais esperado.
    JanelaEncerrada {
        /// `HH:MM` no relógio de Brasília.
        hora: String,
        pendentes: Vec<&'static str>,
    },
    /// A janela fechou sem o concurso esperado: aviso, não falha (o alerta é da
    /// 3.6). O resto do ciclo roda uma vez, sem novidade.
    ConcursoNaoSaiu {
        slug: &'static str,
        esperado_em: String,
    },
    /// Início do `publicar` de uma loteria que chegou.
    PublicandoLoteria {
        slug: &'static str,
    },
}

impl EventoCiclo {
    /// Avisos e falhas vão para a saída de erro.
    pub fn destino(&self) -> super::Destino {
        match self {
            EventoCiclo::BackfillPersiste { .. }
            | EventoCiclo::LoteriaFalhou { .. }
            | EventoCiclo::PublicacaoFalhou { .. }
            | EventoCiclo::RankingFalhou { .. }
            | EventoCiclo::RankingOmitido { .. }
            | EventoCiclo::RankingAntigoRemovido { .. }
            | EventoCiclo::FalhaTransitoria { .. }
            | EventoCiclo::ConcursoNaoSaiu { .. } => super::Destino::Erro,
            _ => super::Destino::Padrao,
        }
    }

    /// Nível no log em arquivo (3.5; regra em `log_arquivo`). Esperar o
    /// concurso é o andamento normal do `--aguardar` (`Info`); falha de
    /// loteria ou da publicação é `Erro`; buraco, falha que será repetida e
    /// concurso que não saiu são `Aviso`.
    pub fn nivel(&self) -> Nivel {
        match self {
            EventoCiclo::LoteriaFalhou { .. } | EventoCiclo::PublicacaoFalhou { .. } => Nivel::Erro,
            EventoCiclo::BackfillIniciado { .. }
            | EventoCiclo::BackfillDispensado { .. }
            | EventoCiclo::BackfillPersiste { .. }
            | EventoCiclo::RankingFalhou { .. }
            | EventoCiclo::RankingOmitido { .. }
            | EventoCiclo::RankingAntigoRemovido { .. }
            | EventoCiclo::FalhaTransitoria { .. }
            | EventoCiclo::ConcursoNaoSaiu { .. } => Nivel::Aviso,
            EventoCiclo::JanelaEncerrada { pendentes, .. } if !pendentes.is_empty() => Nivel::Aviso,
            _ => Nivel::Info,
        }
    }
}

fn faixas_texto(faixas: &[(u32, u32)]) -> String {
    faixas
        .iter()
        .map(|&(i, f)| descrever_faixa(i, f))
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for EventoCiclo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventoCiclo::Loteria { nome, slug } => write!(f, "=== {nome} ({slug}) ==="),
            EventoCiclo::BackfillIniciado { faixas } => write!(
                f,
                "Backfill: faltam no banco os concursos {}; baixando a planilha completa.",
                faixas_texto(faixas)
            ),
            EventoCiclo::BackfillDispensado { faixas } => write!(
                f,
                "Backfill dispensado: o `atualizar` já baixou a planilha completa e ainda \
                 faltam os concursos {}.",
                faixas_texto(faixas)
            ),
            EventoCiclo::BackfillPersiste { faixas } => write!(
                f,
                "Atenção: o buraco persiste depois do backfill (concursos {}); o `validar` vai \
                 reprovar a loteria. Feche com `sync inserir` (veja `sync buracos`).",
                faixas_texto(faixas)
            ),
            EventoCiclo::ValidacaoAprovada { concursos, ultimo } => write!(
                f,
                "Validação aprovada: {concursos} concurso(s), até o {ultimo}."
            ),
            EventoCiclo::FeaturesEmDia {
                modo,
                incorporados,
                ultimo,
            } => write!(
                f,
                "Features ({modo}): {incorporados} concurso(s) incorporado(s); em dia até o {ultimo}."
            ),
            EventoCiclo::IndiceEmDia {
                perfil,
                concursos,
                ultimo,
            } => write!(
                f,
                "Índice do perfil {perfil}: {concursos} concurso(s) indexado(s); em dia até o {ultimo}."
            ),
            EventoCiclo::RankingEmDia {
                perfil,
                linhas,
                ultimo,
            } => write!(
                f,
                "Ranking do perfil {perfil}: {linhas} linha(s) em ranking_taxa; em dia até o {ultimo}."
            ),
            EventoCiclo::RankingFalhou { slug, mensagem } => write!(
                f,
                "Aviso: o ranking de {slug} falhou ({mensagem}); o ciclo segue e o \
                 ranking_taxa.json fica fora desta exportação (rode `sync ranking {slug}`)."
            ),
            EventoCiclo::RankingOmitido { slug, motivo } => {
                write!(f, "Aviso: {slug}: ranking_taxa.json omitido: {motivo}.")
            }
            EventoCiclo::RankingAntigoRemovido { caminho } => write!(
                f,
                "Aviso: ranking_taxa.json antigo removido: {}.",
                caminho.display()
            ),
            EventoCiclo::Exportada { slug, arquivos } => {
                write!(f, "Exportação de {slug}: {arquivos} arquivo(s) gravado(s).")
            }
            EventoCiclo::LoteriaFalhou {
                slug,
                etapa,
                mensagem,
            } => write!(f, "Erro em {slug} (etapa {etapa}): {mensagem}"),
            EventoCiclo::Publicando => write!(f, "=== Publicação ==="),
            EventoCiclo::Publicado { mensagem } => write!(f, "Publicado e enviado: {mensagem}"),
            EventoCiclo::EnvioRetomado => write!(
                f,
                "Nada novo a publicar, mas havia commit local sem push: enviado."
            ),
            EventoCiclo::NadaAPublicar => write!(
                f,
                "Nada a publicar: os arquivos são os mesmos do clone (só `gerado_em` mudou, se mudou)."
            ),
            EventoCiclo::PublicacaoFalhou { mensagem } => {
                write!(f, "Erro na publicação: {mensagem}")
            }
            EventoCiclo::AguardandoConcurso {
                slug,
                esperado_em,
                ultimo,
            } => {
                let ultimo = ultimo.map_or_else(|| "?".to_string(), |u| u.to_string());
                write!(
                    f,
                    "Aguardando {slug}: o concurso do sorteio de {esperado_em} ainda não saiu \
                     (o último é o {ultimo})."
                )
            }
            EventoCiclo::FalhaTransitoria {
                slug,
                etapa,
                mensagem,
            } => write!(
                f,
                "Falha transitória em {slug} (etapa {etapa}): {mensagem}; nova tentativa na \
                 próxima rodada."
            ),
            EventoCiclo::ProximaRodada {
                rodada,
                hora,
                espera_minutos,
                pendentes,
            } => write!(
                f,
                "Rodada {rodada} às {hora} (Brasília), daqui a {espera_minutos} min, para: {}.",
                pendentes.join(", ")
            ),
            EventoCiclo::JanelaEncerrada { hora, pendentes } => write!(
                f,
                "Janela de espera encerrada às {hora} (Brasília); sem tentativas novas para: {}.",
                pendentes.join(", ")
            ),
            EventoCiclo::ConcursoNaoSaiu { slug, esperado_em } => write!(
                f,
                "Aviso: o concurso do sorteio de {esperado_em} de {slug} não saiu até o fim da \
                 janela; o resto do ciclo roda sem novidade (feriado, adiamento ou site atrasado; \
                 a próxima execução agendada tenta de novo)."
            ),
            EventoCiclo::PublicandoLoteria { slug } => write!(f, "=== Publicação ({slug}) ==="),
        }
    }
}

/// Onde o ciclo lê e grava (os bancos vêm em `alvos`; a rede, nos
/// parâmetros de [`executar_ciclo`]).
pub struct PastasCiclo<'a> {
    /// Backups do banco antes de um caminho da planilha.
    pub backups: &'a Path,
    /// Cache de inserções manuais.
    pub cache: &'a Path,
    /// Raiz dos perfis de pesos (`<perfis>/<loteria>/<nome>.toml`).
    pub perfis: &'a Path,
    /// Saída do `exportar` (arquivos em `<saida>/v1/`) e entrada do `publicar`.
    pub saida: &'a Path,
}

/// Falha de uma loteria: onde parou e por quê.
#[derive(Debug)]
pub struct FalhaCiclo {
    pub etapa: EtapaCiclo,
    pub erro: SpiderError,
}

/// O que aconteceu com o ranking por taxa no ciclo de uma loteria. Nunca
/// reprova a loteria (decisão de 2026-10-27, tarefa 2.5): o resto é publicado
/// sem o `ranking_taxa.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RankingDoCiclo {
    /// O passo do `sync ranking` deu erro (mensagem com a cadeia de causas);
    /// é isto que conta como falha da noite para os alertas.
    pub falha: Option<String>,
    /// Motivo de o `ranking_taxa.json` não ter entrado na exportação.
    pub omitido: Option<String>,
}

/// Como terminou o ciclo de uma loteria.
#[derive(Debug)]
pub struct ResumoLoteria {
    pub slug: &'static str,
    /// Maior concurso no banco ao fim do `atualizar`, se chegou lá.
    pub ultimo_concurso: Option<u32>,
    /// A página trouxe concurso novo (próximo ou salto).
    pub concurso_novo: bool,
    /// O backfill baixou a planilha para fechar buraco interno.
    pub backfill: bool,
    /// Os arquivos da loteria foram gravados na saída.
    pub exportada: bool,
    pub ranking: RankingDoCiclo,
    pub falha: Option<FalhaCiclo>,
}

impl ResumoLoteria {
    pub fn ok(&self) -> bool {
        self.falha.is_none()
    }
}

impl fmt::Display for ResumoLoteria {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.falha {
            Some(falha) => write!(f, "{}: falhou em {}", self.slug, falha.etapa),
            None => {
                let ultimo = self
                    .ultimo_concurso
                    .map_or_else(|| "?".to_string(), |u| u.to_string());
                let novo = if self.concurso_novo {
                    "concurso novo"
                } else {
                    "sem concurso novo"
                };
                let backfill = if self.backfill { ", com backfill" } else { "" };
                let ranking = if self.ranking.omitido.is_some() {
                    ", sem ranking_taxa.json"
                } else {
                    ""
                };
                write!(
                    f,
                    "{}: ok (último {ultimo}, {novo}{backfill}{ranking})",
                    self.slug
                )
            }
        }
    }
}

/// Relatório do ciclo inteiro.
#[derive(Debug)]
pub struct RelatorioCiclo {
    /// Uma entrada por loteria, na ordem de `alvos`.
    pub loterias: Vec<ResumoLoteria>,
    /// Resultado do `publicar` final (sempre tentado).
    pub publicacao: Result<Desfecho>,
}

impl RelatorioCiclo {
    /// `true` se todas as loterias e a publicação deram certo.
    pub fn sucesso(&self) -> bool {
        self.publicacao.is_ok() && self.loterias.iter().all(ResumoLoteria::ok)
    }

    pub fn falhas(&self) -> impl Iterator<Item = &ResumoLoteria> {
        self.loterias.iter().filter(|l| !l.ok())
    }
}

/// Roda o ciclo das `alvos` (na ordem dada) e o `publicar` final.
///
/// `ler_pagina` raspa o último resultado da página de uma loteria e
/// `obter_planilha` baixa a planilha completa (caminho do `baixar`); ambos
/// são chamados no máximo uma vez por loteria e por finalidade. `agora` é o
/// único ponto que lê o relógio (segundos desde 1970).
#[allow(clippy::too_many_arguments)]
pub async fn executar_ciclo<P, FutP, F, Fut>(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    pastas: &PastasCiclo<'_>,
    destino: &mut dyn publicacao::Destino,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    mut ler_pagina: P,
    mut obter_planilha: F,
) -> RelatorioCiclo
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let mut loterias = Vec::with_capacity(alvos.len());
    for (cfg, db_caminho) in alvos {
        obs(Evento::Ciclo(EventoCiclo::Loteria {
            nome: cfg.nome_exibicao,
            slug: cfg.slug,
        }));
        let resumo = ciclo_da_loteria(
            cfg,
            db_caminho,
            pastas,
            obs,
            agora,
            &mut ler_pagina,
            &mut obter_planilha,
        )
        .await;
        if let Some(falha) = &resumo.falha {
            obs(Evento::Ciclo(EventoCiclo::LoteriaFalhou {
                slug: cfg.slug,
                etapa: falha.etapa,
                mensagem: cadeia_do_erro(&falha.erro),
            }));
        }
        loterias.push(resumo);
    }

    obs(Evento::Ciclo(EventoCiclo::Publicando));
    let publicacao = publicar_e_avisar(destino, pastas.saida, obs);
    RelatorioCiclo {
        loterias,
        publicacao,
    }
}

/// Roda o `publicar` e conta o desfecho em eventos (o mesmo texto no ciclo de
/// uma passada e no `--aguardar`).
pub(super) fn publicar_e_avisar(
    destino: &mut dyn publicacao::Destino,
    saida: &Path,
    obs: &mut dyn FnMut(Evento),
) -> Result<Desfecho> {
    let publicacao = publicacao::publicar(destino, saida);
    obs(Evento::Ciclo(match &publicacao {
        Ok(Desfecho::Publicado { mensagem }) => EventoCiclo::Publicado {
            mensagem: mensagem.clone(),
        },
        Ok(Desfecho::EnvioRetomado) => EventoCiclo::EnvioRetomado,
        Ok(Desfecho::NadaAPublicar) => EventoCiclo::NadaAPublicar,
        Err(e) => EventoCiclo::PublicacaoFalhou {
            mensagem: cadeia_do_erro(e),
        },
    }));
    publicacao
}

/// Mensagem do erro com toda a cadeia de causas (`source()`), separadas por
/// ": " como o `{:#}` do anyhow; sem isso a causa guardada em `#[source]`
/// (ex.: o erro de rede dentro de `CaminhoDaPlanilhaFalhou`) se perde.
pub(super) fn cadeia_do_erro(erro: &(dyn std::error::Error + 'static)) -> String {
    let mut texto = erro.to_string();
    let mut atual = erro.source();
    while let Some(causa) = atual {
        texto.push_str(": ");
        texto.push_str(&causa.to_string());
        atual = causa.source();
    }
    texto
}

/// O que as etapas já produziram, para o resumo mesmo quando uma falha.
#[derive(Default)]
pub(super) struct Progresso {
    pub(super) ultimo_concurso: Option<u32>,
    pub(super) concurso_novo: bool,
    pub(super) backfill: bool,
    pub(super) exportada: bool,
    pub(super) ranking: RankingDoCiclo,
}

impl Progresso {
    pub(super) fn resumo(&self, slug: &'static str, falha: Option<FalhaCiclo>) -> ResumoLoteria {
        ResumoLoteria {
            slug,
            ultimo_concurso: self.ultimo_concurso,
            concurso_novo: self.concurso_novo,
            backfill: self.backfill,
            exportada: self.exportada,
            ranking: self.ranking.clone(),
            falha,
        }
    }
}

/// O que o `atualizar` deixou para as etapas seguintes.
#[derive(Debug, Clone)]
pub(super) struct PosAtualizacao {
    /// Faixas que continuam faltando no banco.
    pub(super) buracos: Vec<(u32, u32)>,
    /// O `atualizar` já seguiu o caminho da planilha (salto).
    pub(super) salto: bool,
}

/// O que o salto e o backfill já fizeram, para não repetir backup e download
/// da planilha quando o ciclo roda em várias rodadas (`--aguardar`, tarefa
/// 3.3): no máximo um backup por loteria por execução, seja do salto do
/// `atualizar` ou do backfill.
#[derive(Debug, Default)]
pub(super) struct MemoriaBackfill {
    backup: Option<PathBuf>,
    concluido: bool,
}

async fn ciclo_da_loteria<P, FutP, F, Fut>(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasCiclo<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    ler_pagina: &mut P,
    obter_planilha: &mut F,
) -> ResumoLoteria
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let mut progresso = Progresso::default();
    let falha = etapas(
        cfg,
        db_caminho,
        pastas,
        obs,
        agora,
        ler_pagina,
        obter_planilha,
        &mut progresso,
    )
    .await
    .err();
    progresso.resumo(cfg.slug, falha)
}

pub(super) fn em(etapa: EtapaCiclo) -> impl Fn(SpiderError) -> FalhaCiclo {
    move |erro| FalhaCiclo { etapa, erro }
}

#[allow(clippy::too_many_arguments)]
async fn etapas<P, FutP, F, Fut>(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasCiclo<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    ler_pagina: &mut P,
    obter_planilha: &mut F,
    progresso: &mut Progresso,
) -> std::result::Result<(), FalhaCiclo>
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let mut memoria = MemoriaBackfill::default();
    let pos = etapa_atualizar(
        cfg,
        db_caminho,
        pastas,
        obs,
        agora,
        ler_pagina,
        obter_planilha,
        &mut memoria,
        progresso,
    )
    .await?;
    etapas_restantes(
        cfg,
        db_caminho,
        pastas,
        obs,
        agora,
        obter_planilha,
        &pos,
        &mut memoria,
        progresso,
    )
    .await
}

/// 1. atualizar (já resolve o salto pelo caminho da planilha).
#[allow(clippy::too_many_arguments)]
pub(super) async fn etapa_atualizar<P, FutP, F, Fut>(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasCiclo<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    ler_pagina: &mut P,
    obter_planilha: &mut F,
    memoria: &mut MemoriaBackfill,
    progresso: &mut Progresso,
) -> std::result::Result<PosAtualizacao, FalhaCiclo>
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let atualizar_pastas = PastasAtualizar {
        backups: pastas.backups,
        cache: pastas.cache,
    };
    let relatorio = atualizar_reaproveitando_backup(
        cfg,
        db_caminho,
        &atualizar_pastas,
        obs,
        &mut *agora,
        || ler_pagina(cfg),
        || obter_planilha(cfg),
        &mut memoria.backup,
    )
    .await
    .map_err(em(EtapaCiclo::Atualizar))?;
    progresso.ultimo_concurso = relatorio.ultimo_concurso;
    progresso.concurso_novo |=
        !matches!(relatorio.desfecho, DesfechoAtualizacao::SemNovidade { .. });
    Ok(PosAtualizacao {
        buracos: relatorio.buracos,
        salto: matches!(
            relatorio.desfecho,
            DesfechoAtualizacao::SaltoResolvido { .. }
        ),
    })
}

/// 2 a 6: backfill, validar, features, índice e exportar.
#[allow(clippy::too_many_arguments)]
pub(super) async fn etapas_restantes<F, Fut>(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasCiclo<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    obter_planilha: &mut F,
    pos: &PosAtualizacao,
    memoria: &mut MemoriaBackfill,
    progresso: &mut Progresso,
) -> std::result::Result<(), FalhaCiclo>
where
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    // 2. backfill: buraco interno que o `atualizar` deixou. Depois de um
    // backfill concluído não se baixa de novo na mesma execução: se o buraco
    // persiste, o `validar` abaixo reprova a loteria.
    if !pos.buracos.is_empty() {
        if pos.salto {
            obs(Evento::Ciclo(EventoCiclo::BackfillDispensado {
                faixas: pos.buracos.clone(),
            }));
        } else if !memoria.concluido {
            backfill(
                cfg,
                db_caminho,
                pastas,
                &pos.buracos,
                obs,
                agora,
                obter_planilha,
                memoria,
            )
            .await
            .map_err(em(EtapaCiclo::Backfill))?;
            progresso.backfill = true;
        }
    }

    // 3. validar: erro de integridade (buraco incluso) reprova a loteria.
    validar(cfg, db_caminho, obs).map_err(em(EtapaCiclo::Validar))?;

    // 4. features (incremental; a orquestração decide quando regenerar).
    {
        let mut conn = db::abrir(db_caminho).map_err(em(EtapaCiclo::Features))?;
        let resumo =
            sincronizar_features(&mut conn, cfg, false).map_err(em(EtapaCiclo::Features))?;
        if !resumo.avisos.is_empty() {
            obs(Evento::Avisos(resumo.avisos.clone()));
        }
        obs(Evento::Ciclo(EventoCiclo::FeaturesEmDia {
            modo: resumo.modo.to_string(),
            incorporados: resumo.concursos_incorporados,
            ultimo: resumo.ultimo_concurso,
        }));
    }

    // 5. índice do perfil ativo.
    let carregado =
        perfil::carregar(pastas.perfis, cfg, cfg.perfil_ativo).map_err(em(EtapaCiclo::Indice))?;
    {
        let criado_em = tempo::iso8601_utc(agora().map_err(em(EtapaCiclo::Indice))?);
        let mut conn = db::abrir(db_caminho).map_err(em(EtapaCiclo::Indice))?;
        let resumo = sincronizar_indice(&mut conn, cfg, &carregado, &criado_em)
            .map_err(em(EtapaCiclo::Indice))?;
        obs(Evento::Ciclo(EventoCiclo::IndiceEmDia {
            perfil: resumo.perfil,
            concursos: resumo.concursos,
            ultimo: resumo.ultimo_concurso,
        }));
    }

    // 5b. ranking por taxa (tarefa 2.5). Falha NÃO derruba a loteria: vira
    // aviso no log e alerta (pela contagem de noites), e o exportar abaixo
    // omite o `ranking_taxa.json` sozinho (ranking ausente ou atrasado).
    // Chama só a função de biblioteca que grava as tabelas: o relatório e o
    // CSV são da CLI `sync ranking`. O ciclo não toma a trava `ranking`; a
    // trava do próprio ciclo já exclui os comandos que gravam.
    progresso.ranking = RankingDoCiclo::default();
    match ranking_do_ciclo(cfg, db_caminho, &carregado) {
        Ok(r) => obs(Evento::Ciclo(EventoCiclo::RankingEmDia {
            perfil: r.perfil,
            linhas: r.linhas_gravadas,
            ultimo: r.ultimo_concurso,
        })),
        Err(e) => {
            let mensagem = cadeia_do_erro(&e);
            obs(Evento::Ciclo(EventoCiclo::RankingFalhou {
                slug: cfg.slug,
                mensagem: mensagem.clone(),
            }));
            progresso.ranking.falha = Some(mensagem);
        }
    }

    // 6. exportar só esta loteria; o `index.json` reflete o que há no disco.
    let alvo = [(cfg, db_caminho.to_path_buf())];
    let resultado = exportar_com_relatorio(&alvo, pastas.perfis, pastas.saida, || {
        Ok(tempo::iso8601_utc(agora()?))
    })
    .map_err(em(EtapaCiclo::Exportar))?;
    match resultado {
        ResultadoComRelatorio::Gravado(relatorio) => {
            progresso.exportada = true;
            for r in &relatorio.rankings {
                if let SituacaoRanking::Omitido(motivo) = &r.situacao {
                    let motivo = motivo.to_string();
                    obs(Evento::Ciclo(EventoCiclo::RankingOmitido {
                        slug: r.slug,
                        motivo: motivo.clone(),
                    }));
                    progresso.ranking.omitido = Some(motivo);
                }
            }
            for caminho in relatorio.removidos {
                obs(Evento::Ciclo(EventoCiclo::RankingAntigoRemovido {
                    caminho,
                }));
            }
            obs(Evento::Ciclo(EventoCiclo::Exportada {
                slug: cfg.slug,
                arquivos: relatorio.gravados.len(),
            }));
            Ok(())
        }
        ResultadoComRelatorio::Cancelado(mut falhas) => Err(FalhaCiclo {
            etapa: EtapaCiclo::Exportar,
            // `Cancelado` nunca vem vazio: só existe quando alguma loteria falhou.
            erro: falhas.remove(0).erro,
        }),
    }
}

/// Grava as tabelas do ranking (a mesma função do `sync ranking`). O esperado
/// sob o acaso só entra no relatório impresso e não é gravado; por isso o
/// ciclo pede o mínimo de réplicas (2) em vez de simular 10 mil à toa.
fn ranking_do_ciclo(
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    carregado: &perfil::PerfilCarregado,
) -> Result<RelatorioRanking> {
    let mut conn = db::abrir(db_caminho)?;
    sincronizar_ranking_com(&mut conn, cfg, carregado, 2, SEMENTE_ESPERADO)
}

/// Baixa a planilha completa pelo caminho do `baixar` (com backup antes) e
/// avisa se o buraco continua. O backup é feito uma vez só por execução
/// (`memoria`), mesmo que o download falhe e seja repetido em outra rodada.
#[allow(clippy::too_many_arguments)]
async fn backfill<F, Fut>(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    pastas: &PastasCiclo<'_>,
    faixas: &[(u32, u32)],
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    obter_planilha: &mut F,
    memoria: &mut MemoriaBackfill,
) -> Result<()>
where
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    obs(Evento::Ciclo(EventoCiclo::BackfillIniciado {
        faixas: faixas.to_vec(),
    }));
    // Antes do backup e fora do bloco abaixo: erro de relógio não é falha do
    // caminho da planilha, e sem ele nada deve ser tocado.
    let hoje = hoje_de(&mut *agora)?;
    let copia = match &memoria.backup {
        Some(copia) => copia.clone(),
        None => {
            let copia = backup::criar(db_caminho, pastas.backups, cfg.slug, agora()?)?;
            obs(Evento::BackupCriado {
                caminho: copia.clone(),
            });
            memoria.backup = Some(copia.clone());
            copia
        }
    };
    // Falha daqui em diante aponta o backup, como no salto do `atualizar`.
    let restantes = async {
        let arquivo = obter_planilha(cfg).await?;
        processar_e_salvar(&arquivo, cfg, db_caminho, pastas.cache, hoje, obs)?;
        Ok::<_, SpiderError>(faixas_faltando(&carregar_existentes(cfg, db_caminho)?))
    }
    .await
    .map_err(|e| SpiderError::CaminhoDaPlanilhaFalhou {
        loteria: cfg.slug.to_string(),
        backup: Some(copia),
        origem: Box::new(e),
    })?;
    memoria.concluido = true;
    if !restantes.is_empty() {
        obs(Evento::Ciclo(EventoCiclo::BackfillPersiste {
            faixas: restantes,
        }));
    }
    Ok(())
}

/// O `validar`: avisos e erros viram eventos; erro reprova.
fn validar(cfg: &LoteriaConfig, db_caminho: &Path, obs: &mut dyn FnMut(Evento)) -> Result<()> {
    let resultados = carregar_existentes(cfg, db_caminho)?;
    let Some(ultimo) = resultados.iter().map(|r| r.concurso).max() else {
        return Err(SpiderError::SemResultados(cfg.slug.to_string()));
    };
    let (erros, avisos) = validacao::validar_separando(cfg, &resultados);
    if !avisos.is_empty() {
        obs(Evento::Avisos(avisos.clone()));
    }
    if !erros.is_empty() {
        obs(Evento::Erros(erros.clone()));
        if validacao::tem_buraco(&erros) {
            obs(Evento::DicaBuracos { slug: cfg.slug });
        }
        return Err(SpiderError::ResultadosReprovados {
            loteria: cfg.slug.to_string(),
            erros,
            avisos,
        });
    }
    obs(Evento::Ciclo(EventoCiclo::ValidacaoAprovada {
        concursos: resultados.len(),
        ultimo,
    }));
    Ok(())
}
