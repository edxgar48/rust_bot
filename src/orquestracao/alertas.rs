//! Alertas do `sync ciclo` (tarefa 3.6): conta as noites seguidas com falha
//! por alvo (cada loteria e o "geral") e, quando a regra manda, grava um
//! rascunho de e-mail (`.eml`) em `alertas/`. Nada é enviado: o usuário abre o
//! rascunho e o envia quando quiser (decisão de 2026-10-13; sem SMTP nem
//! credencial no projeto).
//!
//! Regras (`docs/PLANEJAMENTO.md`, 3.6, decisões de 2026-10-13):
//!
//! - **Falha de uma loteria** = noite em que ela termina em erro no `ciclo`
//!   (qualquer etapa, inclusive publicar) ou fica `Atrasado` pelo calendário
//!   (3.2) fora da janela de um sorteio especial (`config.rs`). Concurso que
//!   só não saiu até 01h00 é aviso e não conta. Sucesso (ou `EmDia`) zera.
//! - **N = 2 noites seguidas** ([`NOITES_PARA_ALERTA`]). Conta noites, não
//!   execuções: duas execuções na mesma noite valem uma falha só.
//! - **Mudança aparente no site**: erro de forma de página em todas as
//!   rodadas da noite ([`SpiderError::eh_mudanca_de_pagina`]) alerta já na
//!   primeira noite; rede e navegador seguem o N.
//! - **Geral**: trava ocupada e processo morto (avisados pelo
//!   `ciclo_agendado.ps1` por `sync alerta --geral`) contam num contador
//!   próprio, com o mesmo N; um `ciclo` que chega ao fim o zera.
//! - **Repetição**: um rascunho por (alvo, motivo) enquanto o incidente
//!   durar; lembrete a cada [`NOITES_DO_LEMBRETE`] noites seguidas ainda em
//!   falha, contadas do último rascunho; voltar ao normal só gera evento
//!   (linha no log).
//!
//! O histórico fica em `alertas/estado.json`, nunca no banco. Arquivo
//! ilegível recomeça do zero com aviso; nenhuma falha daqui (disco, estado,
//! destinatário) derruba o `ciclo`: vira aviso e o ciclo segue.
//!
//! O relógio entra por parâmetro e a biblioteca não imprime: tudo o que a CLI
//! mostra e grava no log é um [`EventoAlerta`].

use super::ciclo::{cadeia_do_erro, RelatorioCiclo};
use super::{carregar_existentes, Destino, Evento, PublicacaoLoteria, RelatorioAguardar};
use crate::calendario::{self, EstadoCalendario};
use crate::config::LoteriaConfig;
use crate::db;
use crate::error::{Result, SpiderError};
use crate::log_arquivo::{self, Nivel};
use crate::tempo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// Pasta dos alertas, na pasta de trabalho (a mesma do `sync.lock`).
pub const PASTA_ALERTAS: &str = "alertas";

/// Histórico de falhas seguidas, dentro de [`PASTA_ALERTAS`].
pub const ARQUIVO_ESTADO_ALERTAS: &str = "estado.json";

/// Endereço do destinatário (local, fora do git), dentro de [`PASTA_ALERTAS`].
/// Primeira linha que não esteja vazia nem comece com `#`.
pub const ARQUIVO_DESTINATARIO: &str = "destinatario.txt";

/// Rascunhos já enviados: o usuário os move para cá e `sync alertas` os
/// deixa de listar.
pub const PASTA_ENVIADOS: &str = "enviados";

/// Alvo dos problemas da execução em si (trava ocupada, processo morto).
pub const ALVO_GERAL: &str = "geral";

/// Noites seguidas com falha para alertar.
pub const NOITES_PARA_ALERTA: u32 = 2;

/// De quantas em quantas noites seguidas ainda em falha o alerta é lembrado.
pub const NOITES_DO_LEMBRETE: u32 = 7;

const VERSAO_ESTADO: u32 = 1;
const SEGUNDOS_POR_DIA: i64 = 86_400;

// ---------------------------------------------------------------------------
// Eventos
// ---------------------------------------------------------------------------

/// O que os alertas têm a dizer; o `Display` é o texto que a CLI mostra e
/// grava no log.
#[derive(Debug, Clone, PartialEq)]
pub enum EventoAlerta {
    /// Rascunho de e-mail gravado: é o aviso de "há alerta novo".
    RascunhoGerado {
        alvo: String,
        motivo: String,
        caminho: PathBuf,
        lembrete: bool,
    },
    /// Falha registrada, ainda sem alerta (abaixo do N).
    FalhaRegistrada {
        alvo: String,
        noites: u32,
        limite: u32,
    },
    /// O alvo voltou a dar certo depois de `noites` noites de falha.
    VoltouAoNormal { alvo: String, noites: u32 },
    /// `Atrasado` dentro da janela de um sorteio especial: aviso, não falha.
    AtrasoEmJanelaEspecial {
        slug: &'static str,
        esperado_em: String,
        especial: &'static str,
    },
    /// `estado.json` ilegível: o histórico recomeça do zero.
    EstadoIlegivel { caminho: PathBuf, motivo: String },
    /// `destinatario.txt` inválido ou ilegível: o rascunho sai sem `To:`.
    DestinatarioInvalido { caminho: PathBuf, motivo: String },
    /// Não foi possível gravar `caminho` (rascunho ou estado).
    NaoGravou { caminho: PathBuf, motivo: String },
}

impl EventoAlerta {
    pub fn destino(&self) -> Destino {
        match self {
            EventoAlerta::VoltouAoNormal { .. } => Destino::Padrao,
            _ => Destino::Erro,
        }
    }

    /// Rascunho novo, falha contada, atraso tolerado e problemas com os
    /// arquivos são `Aviso`; voltar ao normal é `Info`.
    pub fn nivel(&self) -> Nivel {
        match self {
            EventoAlerta::VoltouAoNormal { .. } => Nivel::Info,
            _ => Nivel::Aviso,
        }
    }
}

impl fmt::Display for EventoAlerta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EventoAlerta::RascunhoGerado {
                alvo,
                motivo,
                caminho,
                lembrete,
            } => {
                let quem = if *lembrete {
                    "lembrete do rascunho de e-mail de alerta"
                } else {
                    "rascunho de e-mail de alerta"
                };
                write!(
                    f,
                    "AVISO: {quem} gravado para {alvo} ({motivo}): {}. Abra, revise e envie; \
                     depois mova-o para {PASTA_ALERTAS}/{PASTA_ENVIADOS}/ (`sync alertas` lista \
                     os pendentes).",
                    caminho.display()
                )
            }
            EventoAlerta::FalhaRegistrada {
                alvo,
                noites,
                limite,
            } => write!(
                f,
                "Alertas: {alvo} com {noites} noite(s) seguida(s) de falha; o rascunho de alerta \
                 sai com {limite}."
            ),
            EventoAlerta::VoltouAoNormal { alvo, noites } => write!(
                f,
                "Alertas: {alvo} voltou ao normal depois de {noites} noite(s) de falha."
            ),
            EventoAlerta::AtrasoEmJanelaEspecial {
                slug,
                esperado_em,
                especial,
            } => write!(
                f,
                "Aviso: {slug} sem o concurso do sorteio de {esperado_em}, dentro da janela de \
                 parada do especial `{especial}`; não conta como falha."
            ),
            EventoAlerta::EstadoIlegivel { caminho, motivo } => write!(
                f,
                "Aviso: o histórico de alertas {} está ilegível ({motivo}); recomeçando a \
                 contagem de falhas do zero.",
                caminho.display()
            ),
            EventoAlerta::DestinatarioInvalido { caminho, motivo } => write!(
                f,
                "Aviso: destinatário de alertas ignorado em {} ({motivo}); o rascunho sai sem \
                 `To:`.",
                caminho.display()
            ),
            EventoAlerta::NaoGravou { caminho, motivo } => write!(
                f,
                "Aviso: não foi possível gravar {} ({motivo}); o alerta fica para a próxima \
                 execução.",
                caminho.display()
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Entrada: o que aconteceu na noite
// ---------------------------------------------------------------------------

/// Por que um alvo falhou na noite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FalhaNoite {
    /// Etapa do ciclo (`atualizar`, ..., `publicar`), `calendario` para o
    /// atraso, `execucao` para o geral.
    pub etapa: String,
    /// Erro com a cadeia completa de causas.
    pub erro: String,
    /// Erro de forma de página em todas as rodadas da noite: alerta já na
    /// primeira noite.
    pub mudanca_de_pagina: bool,
}

/// Resultado de um alvo (uma loteria ou o geral) na noite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoiteAlvo {
    /// Slug da loteria ou [`ALVO_GERAL`].
    pub alvo: String,
    /// Nome para o e-mail (`Lotofácil`).
    pub nome: String,
    /// Último concurso no banco, se conhecido.
    pub ultimo_concurso: Option<u32>,
    /// `None` = sucesso (ou `EmDia`).
    pub falha: Option<FalhaNoite>,
}

impl NoiteAlvo {
    pub fn loteria(
        cfg: &LoteriaConfig,
        ultimo_concurso: Option<u32>,
        falha: Option<FalhaNoite>,
    ) -> NoiteAlvo {
        NoiteAlvo {
            alvo: cfg.slug.to_string(),
            nome: cfg.nome_exibicao.to_string(),
            ultimo_concurso,
            falha,
        }
    }

    /// O geral deu certo: a execução chegou ao fim.
    pub fn geral_ok() -> NoiteAlvo {
        NoiteAlvo {
            alvo: ALVO_GERAL.to_string(),
            nome: ALVO_GERAL.to_string(),
            ultimo_concurso: None,
            falha: None,
        }
    }

    /// O geral falhou por `motivo` (texto livre; a parte antes do primeiro
    /// `:` é o que identifica o motivo na repetição).
    pub fn geral_falhou(motivo: &str) -> NoiteAlvo {
        NoiteAlvo {
            falha: Some(FalhaNoite {
                etapa: "execucao".to_string(),
                erro: motivo.trim().to_string(),
                mudanca_de_pagina: false,
            }),
            ..NoiteAlvo::geral_ok()
        }
    }
}

fn falha_do_erro(
    etapa: &str,
    erro: &(dyn std::error::Error + 'static),
    mudanca_de_pagina: bool,
) -> FalhaNoite {
    FalhaNoite {
        etapa: etapa.to_string(),
        erro: cadeia_do_erro(erro),
        mudanca_de_pagina,
    }
}

/// Falha do passo do ranking (tarefa 2.5): não reprova a loteria nem muda o
/// código de saída, mas conta como noite de falha. Só o erro do passo conta;
/// ranking omitido sem erro (ex.: atrasado por outro motivo) é aviso no log.
fn falha_do_ranking(resumo: &super::ResumoLoteria) -> Option<FalhaNoite> {
    resumo.ranking.falha.as_ref().map(|erro| FalhaNoite {
        etapa: "ranking".to_string(),
        erro: erro.clone(),
        mudanca_de_pagina: false,
    })
}

/// Falha do passo das posições / G-1 (tarefa 2.6), com o mesmo tratamento do
/// ranking: não reprova a loteria, mas conta como noite de falha.
fn falha_das_posicoes(resumo: &super::ResumoLoteria) -> Option<FalhaNoite> {
    resumo.posicoes.falha.as_ref().map(|erro| FalhaNoite {
        etapa: "posicoes".to_string(),
        erro: erro.clone(),
        mudanca_de_pagina: false,
    })
}

/// A falha dos passos opcionais (ranking e posições) que conta para a noite
/// quando a loteria em si não falhou. Se os dois falharam na mesma noite, uma
/// só falha cita as duas etapas (`ranking, posicoes`) e os dois erros.
fn falha_opcional(resumo: &super::ResumoLoteria) -> Option<FalhaNoite> {
    match (falha_do_ranking(resumo), falha_das_posicoes(resumo)) {
        (Some(r), Some(p)) => Some(FalhaNoite {
            etapa: format!("{}, {}", r.etapa, p.etapa),
            erro: format!("{}: {}; {}: {}", r.etapa, r.erro, p.etapa, p.erro),
            mudanca_de_pagina: false,
        }),
        (r, p) => r.or(p),
    }
}

/// Último concurso: o que o ciclo viu ou, se ele nem chegou lá, o do banco
/// (aberto só para leitura, sem criar nada).
fn ultimo_do_banco(slug: &str, db_caminho: &Path, visto: Option<u32>) -> Option<u32> {
    visto.or_else(|| {
        if !db_caminho.exists() {
            return None;
        }
        let conn = db::abrir_somente_leitura(db_caminho).ok()?;
        db::ultimo_concurso(&conn, slug).ok().flatten()
    })
}

/// A loteria está `Atrasado` pelo calendário no instante `agora`? Fora da
/// janela de um sorteio especial é falha; dentro dela, só o evento de aviso.
fn verificar_atraso(
    cfg: &'static LoteriaConfig,
    db_caminho: &Path,
    agora: i64,
    obs: &mut dyn FnMut(Evento),
) -> Option<FalhaNoite> {
    let resultados = carregar_existentes(cfg, db_caminho).ok()?;
    let ultimo = resultados.iter().max_by_key(|r| r.concurso)?;
    let dia_ultimo = tempo::dias_da_data_iso(&ultimo.data_sorteio)?;
    let situacao = calendario::situacao(cfg, dia_ultimo, agora);
    if situacao.estado != EstadoCalendario::Atrasado {
        return None;
    }
    let esperado = situacao.esperado_em.unwrap_or_default();
    if let Some(especial) = calendario::especial_em(cfg, calendario::dia_local(agora)) {
        obs(Evento::Alerta(EventoAlerta::AtrasoEmJanelaEspecial {
            slug: cfg.slug,
            esperado_em: esperado,
            especial: especial.nome,
        }));
        return None;
    }
    Some(FalhaNoite {
        etapa: "calendario".to_string(),
        erro: format!(
            "o concurso do sorteio de {esperado} não está no banco e o prazo do calendário \
             passou (último concurso: {} de {})",
            ultimo.concurso, ultimo.data_sorteio
        ),
        mudanca_de_pagina: false,
    })
}

/// Resultado de cada loteria de um `sync ciclo` de uma passada, mais o geral
/// (que chegou ao fim). `agora` é o instante do fim do ciclo (segundos Unix),
/// para o calendário. A falha de `publicar` vale para as loterias que foram
/// exportadas nessa execução (o `publicar` é um só).
pub fn noites_do_ciclo(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    relatorio: &RelatorioCiclo,
    agora: i64,
    obs: &mut dyn FnMut(Evento),
) -> Vec<NoiteAlvo> {
    let mut noites = Vec::with_capacity(relatorio.loterias.len() + 1);
    for resumo in &relatorio.loterias {
        let Some((cfg, db_caminho)) = alvos.iter().find(|(c, _)| c.slug == resumo.slug) else {
            continue;
        };
        let falha = match (&resumo.falha, &relatorio.publicacao) {
            (Some(f), _) => Some(falha_do_erro(
                &f.etapa.to_string(),
                &f.erro,
                f.erro.eh_mudanca_de_pagina(),
            )),
            (None, Err(e)) if resumo.exportada => Some(falha_do_erro("publicar", e, false)),
            _ => falha_opcional(resumo).or_else(|| verificar_atraso(cfg, db_caminho, agora, obs)),
        };
        let ultimo = ultimo_do_banco(cfg.slug, db_caminho, resumo.ultimo_concurso);
        noites.push(NoiteAlvo::loteria(cfg, ultimo, falha));
    }
    noites.push(NoiteAlvo::geral_ok());
    noites
}

/// Como [`noites_do_ciclo`], para o `ciclo --aguardar`. A mudança de página
/// exige erro de forma em **todas** as passadas da loteria; concurso que não
/// saiu até o fim da janela (`concurso_ausente`) não é falha.
pub fn noites_do_aguardar(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    relatorio: &RelatorioAguardar,
    agora: i64,
    obs: &mut dyn FnMut(Evento),
) -> Vec<NoiteAlvo> {
    let mut noites = Vec::with_capacity(relatorio.loterias.len() + 1);
    for r in &relatorio.loterias {
        let slug = r.resumo.slug;
        let Some((cfg, db_caminho)) = alvos.iter().find(|(c, _)| c.slug == slug) else {
            continue;
        };
        let publicacao_com_erro = relatorio
            .publicacoes
            .iter()
            .find_map(|p: &PublicacaoLoteria| match (&p.slug, &p.resultado) {
                (Some(s), Err(e)) if *s == slug => Some(e),
                _ => None,
            });
        let falha = match (&r.resumo.falha, publicacao_com_erro) {
            (Some(f), _) => Some(falha_do_erro(
                &f.etapa.to_string(),
                &f.erro,
                r.mudanca_de_pagina_em_todas_as_rodadas(),
            )),
            (None, Some(e)) => Some(falha_do_erro("publicar", e, false)),
            // Falha do ranking conta mesmo sem concurso novo; o concurso ausente
            // só dispensa a checagem de atraso do calendário.
            (None, None) => falha_opcional(&r.resumo).or_else(|| {
                if r.concurso_ausente.is_some() {
                    None
                } else {
                    verificar_atraso(cfg, db_caminho, agora, obs)
                }
            }),
        };
        let ultimo = ultimo_do_banco(slug, db_caminho, r.resumo.ultimo_concurso);
        noites.push(NoiteAlvo::loteria(cfg, ultimo, falha));
    }
    // O `publicar` final de reenvio não tem loteria (`slug: None`): sua falha é
    // do geral, para não passar em branco.
    let falha_geral = relatorio
        .publicacoes
        .iter()
        .find_map(|p| match (&p.slug, &p.resultado) {
            (None, Err(e)) => Some(e),
            _ => None,
        });
    noites.push(match falha_geral {
        Some(e) => NoiteAlvo::geral_falhou(&format!("falha ao publicar: {}", cadeia_do_erro(e))),
        None => NoiteAlvo::geral_ok(),
    });
    noites
}

// ---------------------------------------------------------------------------
// Estado em disco
// ---------------------------------------------------------------------------

/// Último rascunho emitido para o alvo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UltimoAlerta {
    /// Instante (Brasília, ISO-8601).
    pub em: String,
    pub motivo: String,
    /// Nome do arquivo `.eml`.
    pub arquivo: String,
}

/// Histórico de um alvo.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstadoAlvo {
    /// Noites seguidas em falha; zero = tudo bem.
    pub falhas_seguidas: u32,
    pub primeira_falha: Option<String>,
    pub ultima_falha: Option<String>,
    /// Data (`AAAA-MM-DD`) da última noite contada, para duas execuções na
    /// mesma noite valerem uma falha só.
    pub ultima_noite: Option<String>,
    pub ultimo_erro: Option<String>,
    pub ultimo_alerta: Option<UltimoAlerta>,
    /// Motivos (identificadores) já avisados neste incidente.
    pub motivos_alertados: Vec<String>,
    /// Contador de `falhas_seguidas` quando o último rascunho saiu, base do
    /// lembrete.
    pub falhas_no_ultimo_alerta: u32,
}

/// Conteúdo de `alertas/estado.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EstadoAlertas {
    pub versao: u32,
    /// Por alvo (slug da loteria ou `geral`), em ordem.
    pub alvos: BTreeMap<String, EstadoAlvo>,
}

impl Default for EstadoAlertas {
    fn default() -> Self {
        EstadoAlertas {
            versao: VERSAO_ESTADO,
            alvos: BTreeMap::new(),
        }
    }
}

/// Lê o estado. Ausente = vazio, sem aviso; ilegível = vazio **com** evento.
pub fn carregar_estado(pasta: &Path, obs: &mut dyn FnMut(Evento)) -> EstadoAlertas {
    let caminho = pasta.join(ARQUIVO_ESTADO_ALERTAS);
    let texto = match std::fs::read_to_string(&caminho) {
        Ok(texto) => texto,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return EstadoAlertas::default(),
        Err(e) => {
            obs(Evento::Alerta(EventoAlerta::EstadoIlegivel {
                caminho,
                motivo: e.to_string(),
            }));
            return EstadoAlertas::default();
        }
    };
    match serde_json::from_str::<EstadoAlertas>(texto.trim_start_matches('\u{feff}')) {
        Ok(estado) => estado,
        Err(e) => {
            obs(Evento::Alerta(EventoAlerta::EstadoIlegivel {
                caminho,
                motivo: e.to_string(),
            }));
            EstadoAlertas::default()
        }
    }
}

/// Grava o estado por arquivo temporário e troca de nome, para uma execução
/// que morre no meio (ou o `sync alerta --geral` do agendador, que não tem a
/// trava) nunca deixar o JSON pela metade.
fn salvar_estado(pasta: &Path, estado: &EstadoAlertas) -> std::io::Result<()> {
    std::fs::create_dir_all(pasta)?;
    let json = serde_json::to_string_pretty(estado).map_err(std::io::Error::other)?;
    let temporario = pasta.join(format!(
        "{ARQUIVO_ESTADO_ALERTAS}.{}.tmp",
        std::process::id()
    ));
    std::fs::write(&temporario, format!("{json}\n"))?;
    std::fs::rename(&temporario, pasta.join(ARQUIVO_ESTADO_ALERTAS)).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporario);
    })
}

// ---------------------------------------------------------------------------
// Decisão
// ---------------------------------------------------------------------------

/// Por que um rascunho sai.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Motivo {
    /// Identificador estável (dedup e nome do arquivo): ASCII, minúsculo.
    slug: String,
    /// Texto do assunto e do corpo.
    texto: String,
}

impl Motivo {
    fn site_mudou() -> Motivo {
        Motivo {
            slug: "site-mudou".to_string(),
            texto: "site possivelmente mudou".to_string(),
        }
    }

    fn falhas_seguidas(noites: u32) -> Motivo {
        Motivo {
            slug: "falhas-seguidas".to_string(),
            texto: format!("falhas seguidas ({noites} noites)"),
        }
    }

    /// O motivo do geral é o texto antes do primeiro `:` (o resto é detalhe,
    /// como o PID da trava, e não pode criar um incidente novo a cada noite).
    fn geral(texto: &str) -> Motivo {
        let curto = texto.split(':').next().unwrap_or(texto).trim();
        let curto = if curto.is_empty() {
            "falha geral"
        } else {
            curto
        };
        Motivo {
            slug: slug_ascii(curto),
            texto: curto.to_string(),
        }
    }
}

/// Minúsculas, sem acento, só `a-z0-9` e `-`; no máximo 40 caracteres.
fn slug_ascii(texto: &str) -> String {
    let mut slug = String::new();
    for c in texto.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'í' | 'ì' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            outro => outro,
        };
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(40);
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "alerta".to_string()
    } else {
        slug
    }
}

/// Um rascunho a gravar.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Emissao {
    motivo: Motivo,
    lembrete: bool,
}

impl EstadoAlvo {
    fn normalizar(&mut self) {
        self.falhas_seguidas = 0;
        self.primeira_falha = None;
        self.ultima_noite = None;
        self.motivos_alertados.clear();
        self.falhas_no_ultimo_alerta = 0;
    }

    /// Conta a falha da noite e diz se sai rascunho. `ts` é o instante
    /// (Brasília) e `noite` a data da execução.
    fn contar_falha(
        &mut self,
        falha: &FalhaNoite,
        geral: bool,
        noite: &str,
        ts: &str,
    ) -> Option<Emissao> {
        if self.ultima_noite.as_deref() != Some(noite) {
            self.falhas_seguidas += 1;
            self.ultima_noite = Some(noite.to_string());
        }
        self.primeira_falha.get_or_insert_with(|| ts.to_string());
        self.ultima_falha = Some(ts.to_string());
        self.ultimo_erro = Some(falha.erro.clone());

        let n = self.falhas_seguidas;
        let motivo = if geral {
            if n < NOITES_PARA_ALERTA {
                return None;
            }
            Motivo::geral(&falha.erro)
        } else if falha.mudanca_de_pagina {
            Motivo::site_mudou()
        } else if n >= NOITES_PARA_ALERTA {
            Motivo::falhas_seguidas(n)
        } else {
            return None;
        };

        let ja_avisado = self.motivos_alertados.contains(&motivo.slug);
        let lembrete = ja_avisado && n >= self.falhas_no_ultimo_alerta + NOITES_DO_LEMBRETE;
        (!ja_avisado || lembrete).then_some(Emissao { motivo, lembrete })
    }

    /// Anota o rascunho gravado; só depois de o arquivo existir, para uma
    /// falha de disco tentar de novo na noite seguinte.
    fn anotar_alerta(&mut self, emissao: &Emissao, ts: &str, arquivo: &str) {
        if !self.motivos_alertados.contains(&emissao.motivo.slug) {
            self.motivos_alertados.push(emissao.motivo.slug.clone());
        }
        self.falhas_no_ultimo_alerta = self.falhas_seguidas;
        self.ultimo_alerta = Some(UltimoAlerta {
            em: ts.to_string(),
            motivo: emissao.motivo.slug.clone(),
            arquivo: arquivo.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// Registro da noite
// ---------------------------------------------------------------------------

/// Onde os alertas leem e gravam.
pub struct PastasAlertas<'a> {
    /// `alertas/` (rascunhos, `estado.json`, `destinatario.txt`).
    pub alertas: &'a Path,
    /// `logs/`, para citar o log do dia no e-mail.
    pub logs: &'a Path,
}

fn segundos(agora: &mut dyn FnMut() -> Result<u64>) -> Result<i64> {
    Ok(i64::try_from(agora()?).unwrap_or(i64::MAX))
}

/// Aplica a regra a cada alvo da noite e grava os rascunhos devidos. `inicio`
/// é o instante em que a execução começou (a noite e o log do dia são os do
/// início; a noite do `--aguardar` atravessa a meia-noite). Devolve os
/// caminhos dos rascunhos gravados. Só falha se o relógio falhar: arquivos
/// que não gravam viram [`EventoAlerta::NaoGravou`].
pub fn registrar_noite(
    pastas: &PastasAlertas<'_>,
    noites: &[NoiteAlvo],
    inicio: i64,
    agora: &mut dyn FnMut() -> Result<u64>,
    obs: &mut dyn FnMut(Evento),
) -> Result<Vec<PathBuf>> {
    let agora_s = segundos(agora)?;
    let ts = calendario::instante_brasilia(agora_s);
    let noite = tempo::data_iso_de_dias(calendario::dia_local(inicio));
    let destinatario = ler_destinatario(pastas.alertas, obs);
    let log_do_dia = log_arquivo::caminho_do_dia(pastas.logs, inicio);
    let log_do_dia = std::path::absolute(&log_do_dia).unwrap_or(log_do_dia);
    let log_texto = log_do_dia.display().to_string();

    let mut estado = carregar_estado(pastas.alertas, obs);
    estado.versao = VERSAO_ESTADO;
    let mut gerados = Vec::new();
    for n in noites {
        let historico = estado.alvos.entry(n.alvo.clone()).or_default();
        let Some(falha) = &n.falha else {
            if historico.falhas_seguidas > 0 {
                obs(Evento::Alerta(EventoAlerta::VoltouAoNormal {
                    alvo: n.alvo.clone(),
                    noites: historico.falhas_seguidas,
                }));
            }
            historico.normalizar();
            continue;
        };
        let geral = n.alvo == ALVO_GERAL;
        let emissao = historico.contar_falha(falha, geral, &noite, &ts);
        let Some(emissao) = emissao else {
            obs(Evento::Alerta(EventoAlerta::FalhaRegistrada {
                alvo: n.alvo.clone(),
                noites: historico.falhas_seguidas,
                limite: NOITES_PARA_ALERTA,
            }));
            continue;
        };
        let dados = DadosRascunho {
            alvo: &n.alvo,
            nome: &n.nome,
            motivo: &emissao.motivo.texto,
            lembrete: emissao.lembrete,
            falhas_seguidas: historico.falhas_seguidas,
            primeira_falha: historico.primeira_falha.as_deref().unwrap_or(&ts),
            ultima_falha: &ts,
            ultimo_concurso: n.ultimo_concurso,
            falha,
            log_do_dia: &log_texto,
            destinatario: destinatario.as_deref(),
        };
        let eml = montar_eml(&dados, agora_s);
        match gravar_rascunho(pastas.alertas, agora_s, &n.alvo, &emissao.motivo.slug, &eml) {
            Ok(caminho) => {
                let arquivo = caminho
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                historico.anotar_alerta(&emissao, &ts, &arquivo);
                obs(Evento::Alerta(EventoAlerta::RascunhoGerado {
                    alvo: n.alvo.clone(),
                    motivo: emissao.motivo.texto.clone(),
                    caminho: caminho.clone(),
                    lembrete: emissao.lembrete,
                }));
                gerados.push(caminho);
            }
            Err(e) => obs(Evento::Alerta(EventoAlerta::NaoGravou {
                caminho: pastas.alertas.to_path_buf(),
                motivo: e.to_string(),
            })),
        }
    }
    if let Err(e) = salvar_estado(pastas.alertas, &estado) {
        obs(Evento::Alerta(EventoAlerta::NaoGravou {
            caminho: pastas.alertas.join(ARQUIVO_ESTADO_ALERTAS),
            motivo: e.to_string(),
        }));
    }
    Ok(gerados)
}

/// `sync alerta --geral "<motivo>"`: registra uma falha do geral (trava
/// ocupada, processo morto) agora. Não pega a trava.
pub fn registrar_falha_geral(
    pastas: &PastasAlertas<'_>,
    motivo: &str,
    agora: &mut dyn FnMut() -> Result<u64>,
    obs: &mut dyn FnMut(Evento),
) -> Result<Vec<PathBuf>> {
    let inicio = segundos(agora)?;
    registrar_noite(
        pastas,
        &[NoiteAlvo::geral_falhou(motivo)],
        inicio,
        agora,
        obs,
    )
}

// ---------------------------------------------------------------------------
// Rascunhos
// ---------------------------------------------------------------------------

/// Primeiro endereço de `alertas/destinatario.txt`; `None` (com evento, se o
/// arquivo existe e não serve) deixa o rascunho sem `To:`.
fn ler_destinatario(pasta: &Path, obs: &mut dyn FnMut(Evento)) -> Option<String> {
    let caminho = pasta.join(ARQUIVO_DESTINATARIO);
    let texto = match std::fs::read_to_string(&caminho) {
        Ok(texto) => texto,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            obs(Evento::Alerta(EventoAlerta::DestinatarioInvalido {
                caminho,
                motivo: e.to_string(),
            }));
            return None;
        }
    };
    let linha = texto
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))?;
    // Um endereço por linha, sem controle: o valor vai direto para o
    // cabeçalho `To:`, e uma quebra de linha abriria outro cabeçalho.
    if !linha.contains('@') || linha.chars().any(char::is_control) {
        obs(Evento::Alerta(EventoAlerta::DestinatarioInvalido {
            caminho,
            motivo: "a primeira linha útil não parece um endereço de e-mail".to_string(),
        }));
        return None;
    }
    Some(linha.to_string())
}

/// Dados de um rascunho.
struct DadosRascunho<'a> {
    alvo: &'a str,
    nome: &'a str,
    motivo: &'a str,
    lembrete: bool,
    falhas_seguidas: u32,
    primeira_falha: &'a str,
    ultima_falha: &'a str,
    ultimo_concurso: Option<u32>,
    falha: &'a FalhaNoite,
    log_do_dia: &'a str,
    destinatario: Option<&'a str>,
}

const SEMANA: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MESES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `Date:` do RFC 5322 no relógio de Brasília (`Tue, 13 Oct 2026 23:05:00 -0300`).
fn data_rfc5322(segundos: i64) -> String {
    let local = segundos + calendario::FUSO_SEGUNDOS;
    let dias = local.div_euclid(SEGUNDOS_POR_DIA);
    let no_dia = local.rem_euclid(SEGUNDOS_POR_DIA);
    let (ano, mes, dia) = tempo::data_civil(dias);
    let semana = (dias + 3).rem_euclid(7) as usize;
    format!(
        "{}, {dia:02} {} {ano:04} {:02}:{:02}:{:02} -0300",
        SEMANA[semana],
        MESES[(mes as usize).saturating_sub(1).min(11)],
        no_dia / 3600,
        no_dia % 3600 / 60,
        no_dia % 60
    )
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut saida = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for bloco in bytes.chunks(3) {
        let b = [
            bloco[0],
            *bloco.get(1).unwrap_or(&0),
            *bloco.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= bloco.len() {
                saida.push(BASE64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                saida.push('=');
            }
        }
    }
    saida
}

/// Valor de cabeçalho seguro: ASCII sem quebra de linha passa como está; o
/// resto vira palavras codificadas (RFC 2047, `=?UTF-8?B?...?=`, no máximo 75
/// caracteres cada, separadas por quebra com dobra), para o Outlook e o
/// Thunderbird lerem o acento certo.
fn codificar_cabecalho(texto: &str) -> String {
    let limpo: String = texto
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if limpo.is_ascii() {
        return limpo;
    }
    // 45 bytes viram 60 caracteres base64; com o envelope, 72 (< 75).
    const MAXIMO_BYTES: usize = 45;
    let mut palavras = Vec::new();
    let mut atual = String::new();
    for c in limpo.chars() {
        if atual.len() + c.len_utf8() > MAXIMO_BYTES {
            palavras.push(std::mem::take(&mut atual));
        }
        atual.push(c);
    }
    if !atual.is_empty() {
        palavras.push(atual);
    }
    palavras
        .iter()
        .map(|p| format!("=?UTF-8?B?{}?=", base64(p.as_bytes())))
        .collect::<Vec<_>>()
        .join("\r\n ")
}

/// Passos curtos do que fazer; os procedimentos completos estão na seção "Runbook" do README.
fn passos(falha: &FalhaNoite, motivo: &str) -> Vec<&'static str> {
    let mut passos = vec![
        "Leia o erro acima e o log do dia (caminho abaixo).",
        "Rode `sync ciclo <loteria>` a mao, na pasta de trabalho, para ver se o problema \
         continua.",
    ];
    if falha.mudanca_de_pagina {
        passos.push(
            "Parece mudanca no site da Caixa: abra a pagina da loteria no navegador, compare \
             com os seletores de src/config.rs e rode `sync atualizar <loteria> --visivel`.",
        );
    }
    let etapa_citada = |nome: &str| falha.etapa.split(", ").any(|e| e == nome);
    if etapa_citada("ranking") {
        passos.push(
            "Falha no ranking: o ciclo seguiu e publicou sem o ranking_taxa.json. Rode \
             `sync ranking <loteria>` a mao para ver o erro (precisa de features e indice em dia).",
        );
    }
    if etapa_citada("posicoes") {
        passos.push(
            "Falha nas posicoes: o ciclo seguiu e publicou sem o ranking_posicoes.json. Rode \
             `sync posicoes <loteria>` a mao para ver o erro (precisa de features e indice em dia).",
        );
    }
    if falha.etapa == "publicar" {
        passos.push(
            "Falha ao publicar: confira o SSH e o clone do repositorio de dados \
             (`sync publicar` tenta o envio de novo).",
        );
    }
    if motivo.contains("trava") {
        passos.push(
            "Trava ocupada: veja `sync.lock.dono` (PID e comando) e confira no Gerenciador de \
             Tarefas se a execucao ainda esta viva.",
        );
    }
    if motivo.contains("processo morto") {
        passos.push(
            "Processo morto: veja logs\\agendador.log e o log do dia; a execucao foi \
             interrompida (limite de 3 h, queda de energia ou encerramento forcado).",
        );
    }
    passos.push(
        "Passos completos: secao \"Runbook\" do README.md (a tabela de entrada leva do motivo \
         deste e-mail ao procedimento).",
    );
    passos.push("Depois de enviar este e-mail, mova o arquivo para alertas/enviados/.");
    passos
}

/// Monta o `.eml` (RFC 822, UTF-8, `X-Unsent: 1`, quebras CRLF).
fn montar_eml(d: &DadosRascunho<'_>, agora: i64) -> String {
    let assunto = format!(
        "[rust_spider] {}: {}{}",
        d.alvo,
        d.motivo,
        if d.lembrete { " (lembrete)" } else { "" }
    );
    let mut cabecalhos = vec!["X-Unsent: 1".to_string()];
    if let Some(para) = d.destinatario {
        cabecalhos.push(format!("To: {para}"));
    }
    cabecalhos.push(format!("Subject: {}", codificar_cabecalho(&assunto)));
    cabecalhos.push(format!("Date: {}", data_rfc5322(agora)));
    cabecalhos.push("MIME-Version: 1.0".to_string());
    cabecalhos.push("Content-Type: text/plain; charset=utf-8".to_string());
    cabecalhos.push("Content-Transfer-Encoding: 8bit".to_string());
    cabecalhos.push("X-Mailer: rust_spider".to_string());

    let alvo = if d.alvo == ALVO_GERAL {
        "geral (a execução em si, não uma loteria)".to_string()
    } else {
        format!("{} ({})", d.nome, d.alvo)
    };
    let ultimo = d
        .ultimo_concurso
        .map_or_else(|| "não disponível".to_string(), |c| c.to_string());
    let mut corpo = vec![
        "Alerta do rust_spider".to_string(),
        String::new(),
        format!("Loteria: {alvo}"),
        format!(
            "Motivo: {}{}",
            d.motivo,
            if d.lembrete {
                " (lembrete: o problema continua)"
            } else {
                ""
            }
        ),
        format!(
            "Falhas seguidas: {} noite(s) (primeira: {}; última: {})",
            d.falhas_seguidas, d.primeira_falha, d.ultima_falha
        ),
        format!("Último concurso no banco: {ultimo}"),
        format!("Etapa em que falhou: {}", d.falha.etapa),
        String::new(),
        "Erro (com a cadeia de causas):".to_string(),
    ];
    corpo.extend(d.falha.erro.lines().map(|l| format!("  {l}")));
    corpo.push(String::new());
    corpo.push(format!("Log do dia: {}", d.log_do_dia));
    corpo.push(String::new());
    corpo.push("O que fazer (resumo):".to_string());
    for (i, passo) in passos(d.falha, d.motivo).iter().enumerate() {
        corpo.push(format!("  {}. {passo}", i + 1));
    }
    format!(
        "{}\r\n\r\n{}\r\n",
        cabecalhos.join("\r\n"),
        corpo.join("\r\n")
    )
}

/// Grava o `.eml` em `AAAA-MM-DD_HHMMSS_<alvo>-<motivo>.eml` sem sobrescrever
/// outro (dois alvos no mesmo segundo ganham sufixo `-2`, `-3`...).
fn gravar_rascunho(
    pasta: &Path,
    agora: i64,
    alvo: &str,
    motivo_slug: &str,
    conteudo: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(pasta)?;
    let local = calendario::instante_brasilia(agora);
    let data = &local[0..10];
    let hora = format!("{}{}{}", &local[11..13], &local[14..16], &local[17..19]);
    let base = format!("{data}_{hora}_{}-{motivo_slug}", slug_ascii(alvo));
    for tentativa in 1u32.. {
        let nome = if tentativa == 1 {
            format!("{base}.eml")
        } else {
            format!("{base}-{tentativa}.eml")
        };
        let caminho = pasta.join(nome);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&caminho)
        {
            Ok(mut arquivo) => {
                arquivo.write_all(conteudo.as_bytes())?;
                return Ok(caminho);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("o laço de tentativas só termina com `return`")
}

/// Rascunho ainda não enviado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RascunhoPendente {
    pub caminho: PathBuf,
    /// Linha `Subject:` como está no arquivo (pode estar codificada).
    pub assunto: Option<String>,
}

/// Os `.eml` de `alertas/` que ainda não foram movidos para `enviados/`, em
/// ordem de nome (= de data). Pasta ausente = nenhum. Só lê.
pub fn listar_pendentes(pasta: &Path) -> Result<Vec<RascunhoPendente>> {
    let entradas = match std::fs::read_dir(pasta) {
        Ok(entradas) => entradas,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(origem) => {
            return Err(SpiderError::Io {
                caminho: pasta.to_path_buf(),
                origem,
            })
        }
    };
    let mut caminhos: Vec<PathBuf> = entradas
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("eml")))
        .collect();
    caminhos.sort();
    Ok(caminhos
        .into_iter()
        .map(|caminho| {
            let assunto = std::fs::read_to_string(&caminho).ok().and_then(|t| {
                t.lines()
                    .take_while(|l| !l.trim().is_empty())
                    .find_map(|l| l.strip_prefix("Subject: ").map(str::to_string))
            });
            RascunhoPendente { caminho, assunto }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn falha(erro: &str, pagina: bool) -> FalhaNoite {
        FalhaNoite {
            etapa: "atualizar".into(),
            erro: erro.into(),
            mudanca_de_pagina: pagina,
        }
    }

    #[test]
    fn runbook_cita_uma_linha_por_etapa_opcional_que_falhou() {
        let com = |etapa: &str| FalhaNoite {
            etapa: etapa.into(),
            erro: "x".into(),
            mudanca_de_pagina: false,
        };
        let tem = |f: &FalhaNoite, trecho: &str| passos(f, "").iter().any(|p| p.contains(trecho));
        let so_posicoes = com("posicoes");
        assert!(tem(&so_posicoes, "Falha nas posicoes"));
        assert!(tem(&so_posicoes, "sync posicoes <loteria>"));
        assert!(!tem(&so_posicoes, "Falha no ranking"));
        let so_ranking = com("ranking");
        assert!(tem(&so_ranking, "Falha no ranking") && !tem(&so_ranking, "Falha nas posicoes"));
        let as_duas = com("ranking, posicoes");
        assert!(tem(&as_duas, "Falha no ranking") && tem(&as_duas, "Falha nas posicoes"));
    }

    fn contar(e: &mut EstadoAlvo, f: &FalhaNoite, geral: bool, noite: &str) -> Option<Emissao> {
        e.contar_falha(f, geral, noite, "2026-10-14T00:10:00-03:00")
    }

    #[test]
    fn base64_confere_com_os_vetores_do_rfc_4648() {
        for (entrada, esperado) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(entrada.as_bytes()), esperado, "{entrada:?}");
        }
    }

    #[test]
    fn cabecalho_ascii_passa_e_nao_ascii_vira_palavra_codificada() {
        assert_eq!(codificar_cabecalho("quina: falhas"), "quina: falhas");
        // Quebra de linha no valor não pode abrir outro cabeçalho.
        assert_eq!(codificar_cabecalho("a\r\nBcc: x"), "a  Bcc: x");
        let c = codificar_cabecalho("Lotofácil: execução interrompida");
        assert!(c.is_ascii(), "{c}");
        assert!(c.starts_with("=?UTF-8?B?") && c.ends_with("?="), "{c}");
        // Cada palavra codificada tem no máximo 75 caracteres.
        let longo = codificar_cabecalho(&"ç".repeat(100));
        for palavra in longo.split("\r\n ") {
            assert!(
                palavra.len() <= 75,
                "{} caracteres: {palavra}",
                palavra.len()
            );
        }
        assert!(longo.split("\r\n ").count() > 1);
    }

    #[test]
    fn slug_remove_acento_e_pontuacao() {
        assert_eq!(slug_ascii("Trava ocupada"), "trava-ocupada");
        assert_eq!(slug_ascii("execução  morta!!"), "execucao-morta");
        assert_eq!(slug_ascii("???"), "alerta");
        assert!(slug_ascii(&"a".repeat(100)).len() <= 40);
    }

    #[test]
    fn data_rfc5322_usa_brasilia() {
        // 2026-10-14T02:05:00Z = terça 2026-10-13 23:05 em Brasília.
        let t = tempo::dias_da_data_iso("2026-10-14").unwrap() * 86_400 + 2 * 3600 + 5 * 60;
        assert_eq!(data_rfc5322(t), "Tue, 13 Oct 2026 23:05:00 -0300");
        assert_eq!(data_rfc5322(3 * 3600), "Thu, 01 Jan 1970 00:00:00 -0300");
    }

    #[test]
    fn segunda_noite_de_falha_gera_alerta_e_a_primeira_nao() {
        let mut e = EstadoAlvo::default();
        let f = falha("rede fora", false);
        assert_eq!(contar(&mut e, &f, false, "2026-10-13"), None);
        assert_eq!(e.falhas_seguidas, 1);
        let em = contar(&mut e, &f, false, "2026-10-14").expect("alerta na 2ª noite");
        assert_eq!(em.motivo.slug, "falhas-seguidas");
        assert_eq!(em.motivo.texto, "falhas seguidas (2 noites)");
        assert!(!em.lembrete);
    }

    #[test]
    fn duas_execucoes_na_mesma_noite_contam_uma_falha() {
        let mut e = EstadoAlvo::default();
        let f = falha("rede fora", false);
        assert_eq!(contar(&mut e, &f, false, "2026-10-13"), None);
        assert_eq!(contar(&mut e, &f, false, "2026-10-13"), None);
        assert_eq!(e.falhas_seguidas, 1);
        assert_eq!(e.primeira_falha, e.ultima_falha);
    }

    #[test]
    fn mudanca_de_pagina_alerta_na_primeira_noite_uma_vez() {
        let mut e = EstadoAlvo::default();
        let f = falha("elemento não encontrado", true);
        let em = contar(&mut e, &f, false, "2026-10-13").expect("alerta na 1ª noite");
        assert_eq!(em.motivo.slug, "site-mudou");
        e.anotar_alerta(&em, "2026-10-14T00:10:00-03:00", "a.eml");
        assert_eq!(contar(&mut e, &f, false, "2026-10-14"), None, "deduplicado");
    }

    #[test]
    fn motivo_novo_no_mesmo_incidente_gera_outro_rascunho() {
        let mut e = EstadoAlvo::default();
        let pagina = falha("elemento", true);
        let rede = falha("rede", false);
        let em = contar(&mut e, &pagina, false, "2026-10-13").unwrap();
        e.anotar_alerta(&em, "t", "a.eml");
        // 2ª noite com erro de rede: motivo `falhas-seguidas` ainda não avisado.
        let em = contar(&mut e, &rede, false, "2026-10-14").expect("motivo novo");
        assert_eq!(em.motivo.slug, "falhas-seguidas");
        e.anotar_alerta(&em, "t", "b.eml");
        assert_eq!(e.motivos_alertados, ["site-mudou", "falhas-seguidas"]);
        // Voltar ao erro de página não repete o rascunho de `site-mudou`.
        assert_eq!(contar(&mut e, &pagina, false, "2026-10-15"), None);
    }

    #[test]
    fn lembrete_a_cada_7_noites_depois_do_ultimo_rascunho() {
        let mut e = EstadoAlvo::default();
        let f = falha("rede", false);
        let mut rascunhos = Vec::new();
        for noite in 1..=17u32 {
            let data = format!("2026-10-{noite:02}");
            if let Some(em) = contar(&mut e, &f, false, &data) {
                rascunhos.push((e.falhas_seguidas, em.lembrete));
                e.anotar_alerta(&em, "t", "x.eml");
            }
        }
        // Alerta na 2ª noite; lembretes 7 noites depois (9ª) e de novo (16ª).
        assert_eq!(rascunhos, [(2, false), (9, true), (16, true)]);
    }

    #[test]
    fn normalizar_zera_o_incidente_e_guarda_o_ultimo_alerta() {
        let mut e = EstadoAlvo::default();
        let f = falha("rede", false);
        contar(&mut e, &f, false, "2026-10-13");
        let em = contar(&mut e, &f, false, "2026-10-14").unwrap();
        e.anotar_alerta(&em, "t", "x.eml");
        e.normalizar();
        assert_eq!(e.falhas_seguidas, 0);
        assert!(e.motivos_alertados.is_empty() && e.primeira_falha.is_none());
        assert!(
            e.ultimo_alerta.is_some(),
            "o histórico do último alerta fica"
        );
        // Novo incidente: volta a contar do 1 e alerta de novo na 2ª noite.
        assert_eq!(contar(&mut e, &f, false, "2026-10-20"), None);
        assert!(contar(&mut e, &f, false, "2026-10-21").is_some());
    }

    #[test]
    fn geral_so_alerta_no_n_e_o_motivo_ignora_o_detalhe() {
        let mut e = EstadoAlvo::default();
        let primeira = falha("trava ocupada: PID 10", false);
        assert_eq!(contar(&mut e, &primeira, true, "2026-10-13"), None);
        let segunda = falha("trava ocupada: PID 11, outro comando", false);
        let em = contar(&mut e, &segunda, true, "2026-10-14").expect("N=2");
        assert_eq!(em.motivo.slug, "trava-ocupada");
        assert_eq!(em.motivo.texto, "trava ocupada");
        e.anotar_alerta(&em, "t", "x.eml");
        let terceira = falha("trava ocupada: PID 12", false);
        assert_eq!(contar(&mut e, &terceira, true, "2026-10-15"), None);
        // Outro motivo do geral é outro rascunho.
        let morto = falha("processo morto: execução sem fim", false);
        let em = contar(&mut e, &morto, true, "2026-10-16").expect("motivo novo");
        assert_eq!(em.motivo.slug, "processo-morto");
    }

    #[test]
    fn eml_tem_cabecalhos_corpo_e_crlf() {
        let f = FalhaNoite {
            etapa: "atualizar".into(),
            erro: "erro do navegador: falha\nsegunda linha".into(),
            mudanca_de_pagina: false,
        };
        let d = DadosRascunho {
            alvo: "lotofacil",
            nome: "Lotofácil",
            motivo: "falhas seguidas (2 noites)",
            lembrete: false,
            falhas_seguidas: 2,
            primeira_falha: "2026-10-13T23:00:05-03:00",
            ultima_falha: "2026-10-14T23:00:07-03:00",
            ultimo_concurso: Some(3802),
            falha: &f,
            log_do_dia: "C:\\spider\\logs\\sync_2026-10-14.log",
            destinatario: Some("eu@exemplo.com"),
        };
        let eml = montar_eml(&d, 1_791_943_200);
        assert!(
            eml.starts_with("X-Unsent: 1\r\nTo: eu@exemplo.com\r\n"),
            "{eml}"
        );
        for esperado in [
            "Subject: [rust_spider] lotofacil: falhas seguidas (2 noites)\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "MIME-Version: 1.0\r\n",
            "Loteria: Lotofácil (lotofacil)",
            "Falhas seguidas: 2 noite(s)",
            "Último concurso no banco: 3802",
            "Etapa em que falhou: atualizar",
            "  erro do navegador: falha\r\n  segunda linha",
            "Log do dia: C:\\spider\\logs\\sync_2026-10-14.log",
            "README.md",
        ] {
            assert!(eml.contains(esperado), "faltou `{esperado}` em:\n{eml}");
        }
        assert!(!eml.replace("\r\n", "").contains('\n'), "só CRLF");
        // Cabeçalhos e corpo separados por uma linha vazia.
        assert!(eml.contains("X-Mailer: rust_spider\r\n\r\nAlerta"), "{eml}");
    }

    #[test]
    fn eml_sem_destinatario_nao_tem_to_e_sem_concurso_diz_indisponivel() {
        let f = falha("x", false);
        let d = DadosRascunho {
            alvo: "geral",
            nome: "geral",
            motivo: "trava ocupada",
            lembrete: true,
            falhas_seguidas: 9,
            primeira_falha: "a",
            ultima_falha: "b",
            ultimo_concurso: None,
            falha: &f,
            log_do_dia: "l",
            destinatario: None,
        };
        let eml = montar_eml(&d, 0);
        assert!(!eml.contains("To:"), "{eml}");
        assert!(eml.contains("Subject: [rust_spider] geral: trava ocupada (lembrete)\r\n"));
        assert!(eml.contains("Último concurso no banco: não disponível"));
        assert!(eml.contains("geral (a execução em si, não uma loteria)"));
    }

    #[test]
    fn assunto_com_acento_sai_codificado() {
        let f = falha("x", false);
        let d = DadosRascunho {
            alvo: "geral",
            nome: "geral",
            motivo: "execução interrompida",
            lembrete: false,
            falhas_seguidas: 2,
            primeira_falha: "a",
            ultima_falha: "b",
            ultimo_concurso: None,
            falha: &f,
            log_do_dia: "l",
            destinatario: None,
        };
        let eml = montar_eml(&d, 0);
        let assunto = eml.lines().find(|l| l.starts_with("Subject:")).unwrap();
        assert!(
            assunto.is_ascii() && assunto.contains("=?UTF-8?B?"),
            "{assunto}"
        );
    }
}
