//! `sync ciclo --aguardar` (tarefa 3.3): o mesmo ciclo por loteria, repetido
//! em rodadas até a janela fechar, para quando o concurso do dia ainda não
//! saiu ou o navegador/rede falhou de passagem. Nenhuma regra de negócio nova:
//! cada rodada chama as mesmas etapas de `ciclo` e o calendário da 3.2.
//!
//! Regras (`docs/PLANEJAMENTO.md`, 3.3, decisões de 2026-10-13):
//!
//! - **O que provoca retentativa**: (a) dia de sorteio pelo calendário
//!   ([`EstadoCalendario::Aguardando`]) e a página ainda mostra o concurso
//!   anterior; (b) falha transitória ([`SpiderError::eh_transitorio`]).
//!   Validação, gravação e o resto são definitivos: a loteria falha na hora.
//! - **Janela e intervalos**: da primeira passada até 01h00 de Brasília, com
//!   esperas de 5, 10, 20, 30, 30… minutos entre as rodadas, só para quem
//!   ainda falta. O dia esperado vem do banco e do instante (`situacao`), não
//!   do dia do relógio: depois da meia-noite continua sendo o da véspera.
//! - **Publicação**: cada loteria que chega segue para features, índice e
//!   exportação e é publicada logo; uma loteria que espera não exporta nem
//!   publica.
//! - **Fim da janela**: loteria ainda aguardando faz, uma vez, o resto do
//!   ciclo sem novidade e termina com aviso; falha transitória persistente é
//!   falha na etapa em que parou.
//! - **Sem espera fora da noite**: `Em dia` ou dia sem sorteio não esperam, e
//!   uma execução que não começa a poucas horas do 01h00 não tem janela
//!   (nenhum processo espera até o outro dia).
//!
//! Relógio e espera entram por parâmetro (testes não dormem); a biblioteca não
//! imprime, só emite [`EventoCiclo`].

use super::ciclo::{
    cadeia_do_erro, etapa_atualizar, etapas_restantes, publicar_e_avisar, EtapaCiclo, EventoCiclo,
    FalhaCiclo, MemoriaBackfill, PastasCiclo, PosAtualizacao, Progresso, ResumoLoteria,
};
use super::{carregar_existentes, Evento};
use crate::calendario::{self, EstadoCalendario};
use crate::config::LoteriaConfig;
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::publicacao::{self, Desfecho};
use crate::tempo;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Esperas entre as rodadas, em minutos; a última se repete.
pub const INTERVALOS_MINUTOS: [u64; 4] = [5, 10, 20, 30];

/// Hora (Brasília) em que a janela fecha.
pub const HORA_LIMITE: i64 = 1;

/// A janela só existe se o próximo 01h00 está a até 4 h do início. Cobre o
/// disparo das 23h00 (2 h) e uma execução manual na noite do sorteio; uma
/// execução às 10h ou às 02h não espera: o dia seguinte é a próxima execução
/// agendada.
pub const ANTECEDENCIA_MAXIMA_SEGUNDOS: i64 = 4 * 3600;

const SEGUNDOS_POR_MINUTO: u64 = 60;
const SEGUNDOS_POR_DIA: i64 = 86_400;

/// Espera antes da rodada seguinte à `rodada`-ésima (a primeira passada é a
/// rodada 1): 5, 10, 20, 30, 30… minutos.
pub fn intervalo_apos_rodada(rodada: u32) -> Duration {
    let indice = usize::try_from(rodada.saturating_sub(1)).unwrap_or(usize::MAX);
    let minutos = INTERVALOS_MINUTOS[indice.min(INTERVALOS_MINUTOS.len() - 1)];
    Duration::from_secs(minutos * SEGUNDOS_POR_MINUTO)
}

/// Fim da janela para uma execução que começa em `inicio` (segundos Unix): o
/// próximo 01h00 de Brasília depois do início, se estiver a até
/// [`ANTECEDENCIA_MAXIMA_SEGUNDOS`]; senão o próprio `inicio` (sem janela:
/// uma passada só). Uma rodada pode começar exatamente no limite.
pub fn fim_da_janela(inicio: i64) -> i64 {
    let dia = calendario::dia_local(inicio);
    let mut limite = dia * SEGUNDOS_POR_DIA + HORA_LIMITE * 3600 - calendario::FUSO_SEGUNDOS;
    if limite <= inicio {
        limite += SEGUNDOS_POR_DIA;
    }
    if limite - inicio > ANTECEDENCIA_MAXIMA_SEGUNDOS {
        inicio
    } else {
        limite
    }
}

/// `HH:MM` de Brasília para mostrar nos eventos.
fn hora_brasilia(segundos: i64) -> String {
    let texto = calendario::instante_brasilia(segundos);
    texto.get(11..16).unwrap_or(&texto).to_string()
}

/// Como terminou uma loteria no `--aguardar`.
#[derive(Debug)]
pub struct ResumoAguardar {
    pub resumo: ResumoLoteria,
    /// Passadas feitas (leitura da página ou retomada).
    pub tentativas: u32,
    /// Data (`AAAA-MM-DD`) do sorteio cujo concurso não saiu até o fim da
    /// janela: aviso, não falha.
    pub concurso_ausente: Option<String>,
    /// Em quantas passadas o erro foi de forma de página
    /// ([`SpiderError::eh_mudanca_de_pagina`]); base do alerta de "mudança
    /// aparente no site" (3.6).
    pub erros_de_forma: u32,
}

impl ResumoAguardar {
    /// `true` se a loteria terminou em falha e **todas** as passadas da noite
    /// deram erro de forma de página: o site provavelmente mudou, e esperar
    /// N noites não acrescenta nada (alerta já na primeira, 3.6).
    pub fn mudanca_de_pagina_em_todas_as_rodadas(&self) -> bool {
        !self.resumo.ok() && self.tentativas > 0 && self.erros_de_forma == self.tentativas
    }
}

/// Um `publicar` feito durante o `--aguardar`.
#[derive(Debug)]
pub struct PublicacaoLoteria {
    /// A loteria que chegou; `None` no `publicar` final de reenvio, quando
    /// nenhuma loteria foi publicada na execução.
    pub slug: Option<&'static str>,
    pub resultado: Result<Desfecho>,
}

/// Relatório do `ciclo --aguardar`.
#[derive(Debug)]
pub struct RelatorioAguardar {
    /// Uma entrada por loteria, na ordem de `alvos`.
    pub loterias: Vec<ResumoAguardar>,
    pub publicacoes: Vec<PublicacaoLoteria>,
    /// Rodadas feitas (a primeira passada conta como 1).
    pub rodadas: u32,
    /// Fim da janela (segundos Unix).
    pub janela_ate: i64,
}

impl RelatorioAguardar {
    /// `true` se nenhuma loteria falhou e nenhum `publicar` falhou. Concurso
    /// que não saiu é aviso e não entra aqui.
    pub fn sucesso(&self) -> bool {
        self.loterias.iter().all(|l| l.resumo.ok())
            && self.publicacoes.iter().all(|p| p.resultado.is_ok())
    }

    pub fn falhas(&self) -> impl Iterator<Item = &ResumoAguardar> {
        self.loterias.iter().filter(|l| !l.resumo.ok())
    }

    pub fn avisos(&self) -> impl Iterator<Item = &ResumoAguardar> {
        self.loterias
            .iter()
            .filter(|l| l.concurso_ausente.is_some())
    }
}

/// O que falta a uma loteria que já rodou ao menos uma passada.
enum Pendencia {
    /// O `atualizar` passou, mas o concurso esperado não saiu.
    Aguardando {
        esperado_em: String,
        pos: PosAtualizacao,
    },
    /// Falha de navegador/rede em `falha.etapa`. `pos` existe se o `atualizar`
    /// já tinha passado (falha no backfill): a próxima rodada retoma dali, sem
    /// reler a página.
    Transitoria {
        falha: FalhaCiclo,
        pos: Option<PosAtualizacao>,
    },
}

struct Item<'a> {
    cfg: &'static LoteriaConfig,
    db: &'a Path,
    progresso: Progresso,
    memoria: MemoriaBackfill,
    /// `None` até a primeira passada, e de novo durante uma passada.
    pendencia: Option<Pendencia>,
    tentativas: u32,
    /// Passadas que terminaram em erro de forma de página.
    erros_de_forma: u32,
    /// Preenchido quando a loteria termina (com ou sem falha).
    fim: Option<(ResumoLoteria, Option<String>)>,
}

impl Item<'_> {
    fn pendente(&self) -> bool {
        self.fim.is_none()
    }

    fn encerrar(&mut self, falha: Option<FalhaCiclo>, concurso_ausente: Option<String>) {
        self.fim = Some((
            self.progresso.resumo(self.cfg.slug, falha),
            concurso_ausente,
        ));
    }
}

/// Resultado de uma passada.
enum Passada {
    /// Exportada: falta publicar.
    Chegou,
    Aguardando {
        esperado_em: String,
        pos: PosAtualizacao,
    },
    Transitoria {
        falha: FalhaCiclo,
        pos: Option<PosAtualizacao>,
    },
    Falhou(FalhaCiclo),
}

/// Segundos Unix do relógio, ou `None` se o relógio falhou.
fn instante(agora: &mut dyn FnMut() -> Result<u64>) -> Option<i64> {
    agora().ok().map(|s| i64::try_from(s).unwrap_or(i64::MAX))
}

/// Data do sorteio esperado e ausente, se a loteria está `Aguardando` pelo
/// calendário no instante `agora`.
fn concurso_esperado_ausente(
    cfg: &LoteriaConfig,
    db_caminho: &Path,
    agora: i64,
) -> Result<Option<String>> {
    let resultados = carregar_existentes(cfg, db_caminho)?;
    let Some(ultimo) = resultados.iter().max_by_key(|r| r.concurso) else {
        return Ok(None);
    };
    // Data ilegível: o `validar` explica; aqui não há o que esperar.
    let Some(dia_ultimo) = tempo::dias_da_data_iso(&ultimo.data_sorteio) else {
        return Ok(None);
    };
    let situacao = calendario::situacao(cfg, dia_ultimo, agora);
    Ok((situacao.estado == EstadoCalendario::Aguardando)
        .then_some(situacao.esperado_em)
        .flatten())
}

/// Roda o ciclo das `alvos` em rodadas até a janela fechar (ver o módulo).
///
/// `agora` lê o relógio (segundos desde 1970) e `dormir` espera; a CLI usa o
/// relógio e o `sleep` reais. `ler_pagina`, `obter_planilha` e o `destino`
/// são os mesmos de [`super::executar_ciclo`]. Só falha se o relógio não
/// funcionar na largada ([`SpiderError::RelogioInvalido`]); o resto vai no
/// relatório.
#[allow(clippy::too_many_arguments)]
pub async fn executar_ciclo_aguardando<P, FutP, F, Fut>(
    alvos: &[(&'static LoteriaConfig, PathBuf)],
    pastas: &PastasCiclo<'_>,
    destino: &mut dyn publicacao::Destino,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    dormir: &mut dyn FnMut(Duration),
    mut ler_pagina: P,
    mut obter_planilha: F,
) -> Result<RelatorioAguardar>
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    let inicio = instante(agora).ok_or(SpiderError::RelogioInvalido)?;
    let limite = fim_da_janela(inicio);

    let mut itens: Vec<Item> = alvos
        .iter()
        .map(|(cfg, db)| Item {
            cfg,
            db,
            progresso: Progresso::default(),
            memoria: MemoriaBackfill::default(),
            pendencia: None,
            tentativas: 0,
            erros_de_forma: 0,
            fim: None,
        })
        .collect();
    let mut publicacoes = Vec::new();
    let mut rodada = 0u32;

    loop {
        rodada += 1;
        for item in itens.iter_mut().filter(|i| i.pendente()) {
            obs(Evento::Ciclo(EventoCiclo::Loteria {
                nome: item.cfg.nome_exibicao,
                slug: item.cfg.slug,
            }));
            let resultado = passada(
                item,
                pastas,
                obs,
                agora,
                &mut ler_pagina,
                &mut obter_planilha,
            )
            .await;
            registrar_passada(item, resultado, pastas, destino, obs, &mut publicacoes);
        }
        if itens.iter().all(|i| !i.pendente()) {
            break;
        }

        // O relógio que falha fecha a janela: sem hora, não há como esperar.
        let agora_s = instante(agora).unwrap_or(limite);
        if agora_s >= limite {
            obs(Evento::Ciclo(EventoCiclo::JanelaEncerrada {
                hora: hora_brasilia(agora_s),
                pendentes: itens
                    .iter()
                    .filter(|i| i.pendente())
                    .map(|i| i.cfg.slug)
                    .collect(),
            }));
            encerrar_janela(
                &mut itens,
                pastas,
                destino,
                obs,
                agora,
                &mut obter_planilha,
                &mut publicacoes,
            )
            .await;
            break;
        }

        // O sono é cortado no limite: a última rodada cai em cima do 01h00.
        let restante = u64::try_from(limite - agora_s).unwrap_or(0);
        let espera = intervalo_apos_rodada(rodada).min(Duration::from_secs(restante));
        obs(Evento::Ciclo(EventoCiclo::ProximaRodada {
            rodada: rodada + 1,
            hora: hora_brasilia(agora_s + i64::try_from(espera.as_secs()).unwrap_or(0)),
            espera_minutos: espera.as_secs().div_ceil(SEGUNDOS_POR_MINUTO),
            pendentes: itens
                .iter()
                .filter(|i| i.pendente())
                .map(|i| i.cfg.slug)
                .collect(),
        }));
        dormir(espera);
    }

    // Se nada foi publicado (todas falharam), o `publicar` roda uma vez
    // mesmo assim: é idempotente e reenvia push pendente (regra da 3.1b).
    if publicacoes.is_empty() {
        obs(Evento::Ciclo(EventoCiclo::Publicando));
        publicacoes.push(PublicacaoLoteria {
            slug: None,
            resultado: publicar_e_avisar(destino, pastas.saida, obs),
        });
    }

    Ok(RelatorioAguardar {
        loterias: itens
            .into_iter()
            .map(|i| {
                // Todas terminam antes de sair do laço; o ramo `None` só evita
                // um `unwrap`.
                let (resumo, concurso_ausente) = match i.fim {
                    Some(fim) => fim,
                    None => (i.progresso.resumo(i.cfg.slug, None), None),
                };
                ResumoAguardar {
                    resumo,
                    tentativas: i.tentativas,
                    concurso_ausente,
                    erros_de_forma: i.erros_de_forma,
                }
            })
            .collect(),
        publicacoes,
        rodadas: rodada,
        janela_ate: limite,
    })
}

/// Uma passada de uma loteria: `atualizar`, conferência do calendário e, se o
/// concurso chegou, o resto do ciclo. Uma loteria que falhou no backfill
/// retoma dali, sem reler a página.
async fn passada<P, FutP, F, Fut>(
    item: &mut Item<'_>,
    pastas: &PastasCiclo<'_>,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    ler_pagina: &mut P,
    obter_planilha: &mut F,
) -> Passada
where
    P: FnMut(&'static LoteriaConfig) -> FutP,
    FutP: Future<Output = Result<Resultado>>,
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    item.tentativas += 1;
    let pos = match item.pendencia.take() {
        Some(Pendencia::Transitoria { pos: Some(pos), .. }) => pos,
        _ => {
            let pos = match etapa_atualizar(
                item.cfg,
                item.db,
                pastas,
                obs,
                agora,
                ler_pagina,
                obter_planilha,
                &mut item.memoria,
                &mut item.progresso,
            )
            .await
            {
                Ok(pos) => pos,
                Err(falha) => return classificar(falha, None),
            };
            let Some(agora_s) = instante(agora) else {
                return Passada::Falhou(FalhaCiclo {
                    etapa: EtapaCiclo::Atualizar,
                    erro: SpiderError::RelogioInvalido,
                });
            };
            match concurso_esperado_ausente(item.cfg, item.db, agora_s) {
                Ok(Some(esperado_em)) => return Passada::Aguardando { esperado_em, pos },
                Ok(None) => {}
                Err(erro) => {
                    return Passada::Falhou(FalhaCiclo {
                        etapa: EtapaCiclo::Atualizar,
                        erro,
                    })
                }
            }
            pos
        }
    };
    match etapas_restantes(
        item.cfg,
        item.db,
        pastas,
        obs,
        agora,
        obter_planilha,
        &pos,
        &mut item.memoria,
        &mut item.progresso,
    )
    .await
    {
        Ok(()) => Passada::Chegou,
        Err(falha) => classificar(falha, Some(pos)),
    }
}

fn classificar(falha: FalhaCiclo, pos: Option<PosAtualizacao>) -> Passada {
    if falha.erro.eh_transitorio() {
        Passada::Transitoria { falha, pos }
    } else {
        Passada::Falhou(falha)
    }
}

/// Guarda o resultado da passada na loteria, emite os eventos e publica se o
/// concurso chegou.
fn registrar_passada(
    item: &mut Item<'_>,
    passada: Passada,
    pastas: &PastasCiclo<'_>,
    destino: &mut dyn publicacao::Destino,
    obs: &mut dyn FnMut(Evento),
    publicacoes: &mut Vec<PublicacaoLoteria>,
) {
    match passada {
        Passada::Chegou => {
            item.encerrar(None, None);
            publicar_loteria(item.cfg.slug, pastas, destino, obs, publicacoes);
        }
        Passada::Aguardando { esperado_em, pos } => {
            obs(Evento::Ciclo(EventoCiclo::AguardandoConcurso {
                slug: item.cfg.slug,
                esperado_em: esperado_em.clone(),
                ultimo: item.progresso.ultimo_concurso,
            }));
            item.pendencia = Some(Pendencia::Aguardando { esperado_em, pos });
        }
        Passada::Transitoria { falha, pos } => {
            item.erros_de_forma += u32::from(falha.erro.eh_mudanca_de_pagina());
            obs(Evento::Ciclo(EventoCiclo::FalhaTransitoria {
                slug: item.cfg.slug,
                etapa: falha.etapa,
                mensagem: cadeia_do_erro(&falha.erro),
            }));
            item.pendencia = Some(Pendencia::Transitoria { falha, pos });
        }
        Passada::Falhou(falha) => {
            item.erros_de_forma += u32::from(falha.erro.eh_mudanca_de_pagina());
            falhar(item, falha, obs);
        }
    }
}

fn falhar(item: &mut Item<'_>, falha: FalhaCiclo, obs: &mut dyn FnMut(Evento)) {
    obs(Evento::Ciclo(EventoCiclo::LoteriaFalhou {
        slug: item.cfg.slug,
        etapa: falha.etapa,
        mensagem: cadeia_do_erro(&falha.erro),
    }));
    item.encerrar(Some(falha), None);
}

fn publicar_loteria(
    slug: &'static str,
    pastas: &PastasCiclo<'_>,
    destino: &mut dyn publicacao::Destino,
    obs: &mut dyn FnMut(Evento),
    publicacoes: &mut Vec<PublicacaoLoteria>,
) {
    obs(Evento::Ciclo(EventoCiclo::PublicandoLoteria { slug }));
    publicacoes.push(PublicacaoLoteria {
        slug: Some(slug),
        resultado: publicar_e_avisar(destino, pastas.saida, obs),
    });
}

/// A janela fechou: quem aguardava faz o resto do ciclo uma vez, sem
/// novidade, e termina com aviso; quem tinha falha transitória falha na etapa
/// em que parou.
async fn encerrar_janela<F, Fut>(
    itens: &mut [Item<'_>],
    pastas: &PastasCiclo<'_>,
    destino: &mut dyn publicacao::Destino,
    obs: &mut dyn FnMut(Evento),
    agora: &mut dyn FnMut() -> Result<u64>,
    obter_planilha: &mut F,
    publicacoes: &mut Vec<PublicacaoLoteria>,
) where
    F: FnMut(&'static LoteriaConfig) -> Fut,
    Fut: Future<Output = Result<PathBuf>>,
{
    for item in itens.iter_mut().filter(|i| i.pendente()) {
        // Quem está pendente depois de uma rodada sempre tem pendência
        // (`registrar_passada` encerra ou preenche); sem ela não há o que
        // fechar e o item cai no resumo sem falha inventada.
        let Some(pendencia) = item.pendencia.take() else {
            continue;
        };
        match pendencia {
            Pendencia::Aguardando { esperado_em, pos } => {
                obs(Evento::Ciclo(EventoCiclo::ConcursoNaoSaiu {
                    slug: item.cfg.slug,
                    esperado_em: esperado_em.clone(),
                }));
                obs(Evento::Ciclo(EventoCiclo::Loteria {
                    nome: item.cfg.nome_exibicao,
                    slug: item.cfg.slug,
                }));
                match etapas_restantes(
                    item.cfg,
                    item.db,
                    pastas,
                    obs,
                    agora,
                    obter_planilha,
                    &pos,
                    &mut item.memoria,
                    &mut item.progresso,
                )
                .await
                {
                    Ok(()) => {
                        item.encerrar(None, Some(esperado_em));
                        publicar_loteria(item.cfg.slug, pastas, destino, obs, publicacoes);
                    }
                    Err(falha) => falhar(item, falha, obs),
                }
            }
            Pendencia::Transitoria { falha, .. } => falhar(item, falha, obs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Segundos Unix de `data` às `hora:minuto` em Brasília.
    fn brasilia(data: &str, hora: i64, minuto: i64) -> i64 {
        tempo::dias_da_data_iso(data).expect("data válida") * SEGUNDOS_POR_DIA
            + hora * 3600
            + minuto * 60
            - calendario::FUSO_SEGUNDOS
    }

    #[test]
    fn intervalos_crescem_e_o_ultimo_se_repete() {
        let minutos: Vec<u64> = (1..=7)
            .map(|r| intervalo_apos_rodada(r).as_secs() / 60)
            .collect();
        assert_eq!(minutos, [5, 10, 20, 30, 30, 30, 30]);
        assert_eq!(
            intervalo_apos_rodada(0).as_secs(),
            300,
            "sem pânico no zero"
        );
        assert_eq!(intervalo_apos_rodada(u32::MAX).as_secs(), 1800);
    }

    #[test]
    fn janela_das_23h_fecha_a_01h_do_dia_seguinte() {
        let inicio = brasilia("2026-10-13", 23, 0);
        assert_eq!(fim_da_janela(inicio), brasilia("2026-10-14", 1, 0));
        // Depois da meia-noite o limite é o 01h00 do mesmo dia local.
        let depois = brasilia("2026-10-14", 0, 30);
        assert_eq!(fim_da_janela(depois), brasilia("2026-10-14", 1, 0));
        // Um segundo antes ainda tem janela; no próprio 01h00 não.
        let quase = brasilia("2026-10-14", 1, 0) - 1;
        assert_eq!(fim_da_janela(quase), brasilia("2026-10-14", 1, 0));
    }

    #[test]
    fn execucao_longe_do_limite_nao_tem_janela() {
        for (data, h, m) in [
            ("2026-10-14", 1, 0),
            ("2026-10-14", 2, 0),
            ("2026-10-14", 10, 0),
            ("2026-10-13", 20, 59),
        ] {
            let inicio = brasilia(data, h, m);
            assert_eq!(fim_da_janela(inicio), inicio, "{data} {h}:{m}");
        }
        // 21h00 (hora de procura) ainda cabe: 4 h até o 01h00.
        let nove = brasilia("2026-10-13", 21, 0);
        assert_eq!(fim_da_janela(nove), brasilia("2026-10-14", 1, 0));
    }

    #[test]
    fn hora_de_brasilia_para_eventos() {
        assert_eq!(hora_brasilia(brasilia("2026-10-13", 23, 5)), "23:05");
        assert_eq!(hora_brasilia(brasilia("2026-10-14", 0, 35)), "00:35");
        assert_eq!(hora_brasilia(brasilia("2026-10-14", 1, 0)), "01:00");
    }
}
