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
//!
//! Também registra perfis de pesos na tabela `perfis` ([`registrar_perfil`]),
//! com a regra de imutabilidade do plano, e grava o índice de força de um
//! perfil na tabela `indice` ([`sincronizar_indice`], comando `sync indice`).
//!
//! E decide o que o `sync atualizar` faz com o concurso lido na página
//! ([`classificar_atualizacao`], tarefa 0.8).

use crate::config::LoteriaConfig;
use crate::db::{self, MotorEstadoSalvo, PerfilRegistrado};
use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use crate::motor::{self, EstadoMotor, FeaturesDezena, FEATURE_VERSION, JANELAS};
use crate::perfil::{Perfil, PerfilCarregado};
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

// ---------------------------------------------------------------------------
// Registro de perfis de pesos (`perfis`), usado pelo `sync indice` (1b.3).
// ---------------------------------------------------------------------------

/// O que [`registrar_perfil`] fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistroPerfil {
    /// Primeira vez: a linha foi criada com o `criado_em` recebido.
    Novo,
    /// Já estava registrado com o mesmo hash: nada foi alterado.
    JaRegistrado,
}

/// Registra o perfil na tabela `perfis` (criando a tabela se preciso),
/// aplicando a imutabilidade do plano: mesmo `nome` e mesmo hash → nada muda
/// (nem `criado_em`); mesmo `nome` com hash diferente → erro pedindo para
/// subir a versão, sem tocar na linha existente.
///
/// `criado_em` vem de quem chama (a CLI lê o relógio), para esta função não
/// depender do relógio e ser testável; só é gravado no primeiro registro.
///
/// Recebe `&Connection` para poder rodar dentro da transação de quem grava o
/// índice (`db::iniciar_escrita`): assim conferir e inserir é atômico junto
/// com o índice. Mesmo fora de transação, `db::inserir_perfil` é um `INSERT`
/// simples: uma corrida faz o segundo falhar na chave, nunca sobrescreve.
///
/// O perfil tem de ser da loteria do banco (um banco por loteria).
pub fn registrar_perfil(
    conn: &Connection,
    cfg: &LoteriaConfig,
    carregado: &PerfilCarregado,
    criado_em: &str,
) -> Result<RegistroPerfil> {
    let perfil = &carregado.perfil;
    conferir_loteria_do_perfil(cfg, perfil)?;

    db::criar_tabela_perfis(conn)?;
    match db::buscar_perfil(conn, &perfil.nome)? {
        Some(registrado) if registrado.hash_toml == carregado.hash_toml => {
            Ok(RegistroPerfil::JaRegistrado)
        }
        Some(registrado) => {
            // Não sugere versão que já tem arquivo ao lado ou já está no banco.
            let (proximo_nome, proxima_versao) = perfil.proxima_versao(|nome| {
                carregado
                    .origem
                    .with_file_name(format!("{nome}.toml"))
                    .exists()
                    || matches!(db::buscar_perfil(conn, nome), Ok(Some(_)))
            });
            Err(SpiderError::PerfilAlterado {
                nome: perfil.nome.clone(),
                hash_registrado: registrado.hash_toml,
                hash_arquivo: carregado.hash_toml.clone(),
                proximo_nome,
                proxima_versao,
            })
        }
        None => {
            db::inserir_perfil(
                conn,
                &PerfilRegistrado {
                    nome: perfil.nome.clone(),
                    versao: perfil.versao,
                    loteria: perfil.loteria.clone(),
                    janela: perfil.janela,
                    normalizacao: perfil.normalizacao.como_str().to_string(),
                    hash_toml: carregado.hash_toml.clone(),
                    toml: carregado.toml.clone(),
                    criado_em: criado_em.to_string(),
                },
            )?;
            Ok(RegistroPerfil::Novo)
        }
    }
}

/// Um banco por loteria: o perfil tem de ser da loteria do banco.
fn conferir_loteria_do_perfil(cfg: &LoteriaConfig, perfil: &Perfil) -> Result<()> {
    if perfil.loteria != cfg.slug {
        return Err(SpiderError::PerfilDeOutraLoteria {
            nome: perfil.nome.clone(),
            loteria_perfil: perfil.loteria.clone(),
            loteria_banco: cfg.slug.to_string(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Índice de força (`sync indice`, 1b.3).
// ---------------------------------------------------------------------------

/// O que uma execução de [`sincronizar_indice`] fez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoIndice {
    /// Nome do perfil (coluna `indice.perfil`).
    pub perfil: String,
    /// Se o perfil foi registrado agora em `perfis` ou já estava.
    pub registro: RegistroPerfil,
    /// Concursos indexados: do primeiro ao snapshot do próximo, inclusive.
    pub concursos: usize,
    pub primeiro_concurso: u32,
    /// Último concurso de `resultados`.
    pub ultimo_concurso: u32,
    /// Snapshot do próximo concurso (último + 1), também indexado.
    pub proximo_concurso: u32,
    /// Linhas do perfil que estavam em `indice` e foram substituídas.
    pub linhas_apagadas: usize,
    /// Linhas inseridas em `indice` (concursos × dezenas).
    pub linhas_gravadas: usize,
}

/// Calcula e grava o índice de força do perfil `carregado` no banco `conn`
/// da loteria `cfg` (comando `sync indice`, regras de 2026-10-05 em
/// `docs/PLANEJAMENTO.md`, Fase 1b).
///
/// Recusa sem gravar nada (nem criar tabelas):
/// - perfil de outra loteria ([`SpiderError::PerfilDeOutraLoteria`]);
/// - banco sem resultados ([`SpiderError::SemResultados`]);
/// - `features_dezena` ausente ou vazia ([`SpiderError::SemFeatures`]);
/// - snapshot mais recente das features diferente do último concurso + 1
///   ([`SpiderError::FeaturesDesatualizadas`]);
/// - resultados diferentes dos usados no último `sync features` (impressão
///   de `motor_estado`), como um resultado antigo corrigido
///   ([`SpiderError::FeaturesDeResultadosAlterados`]);
/// - linha da janela do perfil com outra `feature_version`
///   ([`SpiderError::FeaturesDeOutraVersao`]);
/// - janela do perfil sem a série completa: concursos `1 … último + 1`
///   seguidos, cada um com exatamente as dezenas de `cfg`
///   ([`SpiderError::FeaturesIncoerentes`]);
/// - perfil alterado sob o mesmo nome ([`SpiderError::PerfilAlterado`]).
///
/// Senão, numa única transação: registra o perfil em `perfis`
/// ([`registrar_perfil`], `criado_em` só no primeiro registro), apaga as
/// linhas do perfil em `indice` e insere o índice inteiro de novo. Outros
/// perfis não são tocados. Regenerar tudo (e não só os concursos novos) é
/// necessário porque o primeiro concurso da entrada de
/// [`motor::calcular_indice`] tem variação `NULL`.
///
/// Ponto no tempo: o índice de `t` vem só das features de `t` (que olham
/// `1 … t−1`) e da posição de `t−1`; `saiu` não é lido.
pub fn sincronizar_indice(
    conn: &mut Connection,
    cfg: &LoteriaConfig,
    carregado: &PerfilCarregado,
    criado_em: &str,
) -> Result<ResumoIndice> {
    let perfil = &carregado.perfil;
    conferir_loteria_do_perfil(cfg, perfil)?;

    // Leitura e escrita na mesma transação (`BEGIN IMMEDIATE`): o índice
    // gravado corresponde exatamente às features lidas. Qualquer erro antes
    // do `commit` desfaz tudo, inclusive tabelas criadas e o registro do
    // perfil. As checagens vêm antes de qualquer `CREATE`.
    let tx = db::iniciar_escrita(conn)?;
    let Some(ultimo) = db::ultimo_concurso(&tx, cfg.slug)? else {
        return Err(SpiderError::SemResultados(cfg.slug.to_string()));
    };
    let sem_features = || SpiderError::SemFeatures(cfg.slug.to_string());
    if !db::tabela_existe(&tx, "features_dezena")? {
        return Err(sem_features());
    }
    let Some(snapshot) = db::maior_concurso_features(&tx)? else {
        return Err(sem_features());
    };
    let proximo = ultimo + 1;
    if snapshot != proximo {
        return Err(SpiderError::FeaturesDesatualizadas {
            loteria: cfg.slug.to_string(),
            ultimo_concurso: ultimo,
            snapshot,
        });
    }
    conferir_resultados_das_features(&tx, cfg, ultimo)?;

    let linhas = db::ler_features_da_janela(&tx, perfil.janela)?;
    if let Some(l) = linhas.iter().find(|l| l.feature_version != FEATURE_VERSION) {
        return Err(SpiderError::FeaturesDeOutraVersao {
            loteria: cfg.slug.to_string(),
            encontrada: l.feature_version,
            atual: FEATURE_VERSION,
        });
    }
    let features: Vec<FeaturesDezena> = linhas.into_iter().map(|l| l.features).collect();
    let (primeiro, concursos) = conferir_serie(cfg, perfil.janela, &features, proximo)?;

    let indice =
        motor::calcular_indice(&features, perfil.janela, perfil.normalizacao, &perfil.pesos)?;
    drop(features);

    let registro = registrar_perfil(&tx, cfg, carregado, criado_em)?;
    db::criar_tabela_indice(&tx)?;
    let linhas_apagadas = db::apagar_indice_do_perfil(&tx, &perfil.nome)?;
    let linhas_gravadas = db::inserir_indice(&tx, &perfil.nome, &indice)?;
    tx.commit()?;

    Ok(ResumoIndice {
        perfil: perfil.nome.clone(),
        registro,
        concursos,
        primeiro_concurso: primeiro,
        ultimo_concurso: ultimo,
        proximo_concurso: proximo,
        linhas_apagadas,
        linhas_gravadas,
    })
}

/// Confere que as features foram calculadas com os resultados que estão hoje
/// no banco: o `motor_estado` gravado pelo `sync features` tem de ir até
/// `ultimo` e ter a mesma impressão de `resultados`. Pega o resultado antigo
/// corrigido sem rodar `sync features`, que não muda o último concurso nem o
/// snapshot (decisão de 2026-10-05).
fn conferir_resultados_das_features(
    conn: &Connection,
    cfg: &LoteriaConfig,
    ultimo: u32,
) -> Result<()> {
    let alterados = || SpiderError::FeaturesDeResultadosAlterados {
        loteria: cfg.slug.to_string(),
    };
    if !db::tabela_existe(conn, "motor_estado")? {
        return Err(alterados());
    }
    let Some(salvo) = db::ler_motor_estado(conn)? else {
        return Err(alterados());
    };
    let resultados = db::listar_crescente(conn, cfg.slug)?;
    if salvo.loteria != cfg.slug
        || salvo.ultimo_concurso != ultimo
        || impressao_resultados(&resultados) != salvo.impressao
    {
        return Err(alterados());
    }
    Ok(())
}

/// Confere que `features` (só a janela do perfil, ordenadas por (concurso,
/// dezena), como `db::ler_features_da_janela` devolve) formam a série que o
/// `sync features` grava: concursos seguidos de 1 até `ultimo_esperado`, cada
/// um com exatamente as dezenas de `cfg.dezenas()`. Devolve o primeiro
/// concurso e quantos concursos há.
///
/// O motor calcula as features sempre a partir do concurso 1 (exige
/// numeração contínua desde o 1); série que começa depois, tem buraco ou
/// dezena fora do universo só aparece se a tabela foi mexida por fora.
fn conferir_serie(
    cfg: &LoteriaConfig,
    janela: u32,
    features: &[FeaturesDezena],
    ultimo_esperado: u32,
) -> Result<(u32, usize)> {
    let incoerente = |detalhe: String| SpiderError::FeaturesIncoerentes {
        loteria: cfg.slug.to_string(),
        janela,
        detalhe,
    };
    if features.is_empty() {
        return Err(incoerente("nenhuma linha desta janela".to_string()));
    }

    let mut esperado = 1;
    let mut concursos = 0;
    for grupo in features.chunk_by(|a, b| a.concurso == b.concurso) {
        let concurso = grupo[0].concurso;
        if concurso != esperado {
            return Err(incoerente(if esperado == 1 {
                format!("a série começa no concurso {concurso}, e não no 1")
            } else {
                format!("depois do concurso {} vem o {concurso}", esperado - 1)
            }));
        }
        // Grupo ordenado por dezena: igual à faixa de `cfg` elemento a elemento
        // ⇔ mesmas dezenas, sem repetida nem fora do universo.
        if !grupo.iter().map(|f| f.dezena).eq(cfg.dezenas()) {
            // Só no caminho de erro: quadrático, mas com no máximo ~100 dezenas.
            let presentes: Vec<u32> = grupo.iter().map(|f| f.dezena).collect();
            let faltam: Vec<u32> = cfg.dezenas().filter(|d| !presentes.contains(d)).collect();
            let sobram: Vec<u32> = presentes
                .iter()
                .enumerate()
                .filter(|&(i, d)| !cfg.contem(*d) || presentes[..i].contains(d))
                .map(|(_, &d)| d)
                .collect();
            return Err(incoerente(format!(
                "o concurso {concurso} não tem exatamente as dezenas {}..={} da {} \
                 (faltam {faltam:?}; sobram, fora da faixa ou repetidas, {sobram:?})",
                cfg.dezena_min, cfg.dezena_max, cfg.nome_exibicao
            )));
        }
        esperado += 1;
        concursos += 1;
    }
    let ultimo = esperado - 1;
    if ultimo != ultimo_esperado {
        return Err(incoerente(format!(
            "a série vai até o concurso {ultimo}, mas as features vão até o {ultimo_esperado}"
        )));
    }
    Ok((features[0].concurso, concursos))
}

/// O que fazer com o concurso lido na página pelo `sync atualizar`, comparado
/// com o último salvo no banco (tarefa 0.8 de `docs/PLANEJAMENTO.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atualizacao {
    /// A página não traz concurso além do último salvo: nada a gravar.
    SemNovidade { ultimo: u32 },
    /// A página traz exatamente o próximo concurso (ou o 1, com o banco sem
    /// concursos): grava só ele.
    Proximo,
    /// Faltam concursos entre o último salvo e o da página: gravar só o da
    /// página deixaria buraco, então segue o caminho do `baixar` (planilha
    /// completa). `ultimo` é 0 quando o banco não existe ou está vazio.
    Salto { ultimo: u32, pagina: u32 },
}

impl Atualizacao {
    /// Quantos concursos faltam entre o último salvo e o da página (0 fora
    /// do salto).
    pub fn faltando(&self) -> u32 {
        match *self {
            Atualizacao::Salto { ultimo, pagina } => {
                pagina.saturating_sub(ultimo).saturating_sub(1)
            }
            _ => 0,
        }
    }
}

/// Classifica o concurso `da_pagina` contra o `ultimo_salvo` no banco
/// (`None` = banco inexistente ou sem concursos da loteria, tratado como
/// último = 0).
pub fn classificar_atualizacao(ultimo_salvo: Option<u32>, da_pagina: u32) -> Atualizacao {
    match ultimo_salvo {
        Some(ultimo) if da_pagina <= ultimo => Atualizacao::SemNovidade { ultimo },
        Some(ultimo) if da_pagina == ultimo + 1 => Atualizacao::Proximo,
        Some(ultimo) => Atualizacao::Salto {
            ultimo,
            pagina: da_pagina,
        },
        // Concurso 0 não existe: deixa a validação do salto de 1 recusá-lo.
        None if da_pagina <= 1 => Atualizacao::Proximo,
        None => Atualizacao::Salto {
            ultimo: 0,
            pagina: da_pagina,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atualizacao_sem_novidade_quando_a_pagina_nao_passa_do_ultimo() {
        assert_eq!(
            classificar_atualizacao(Some(7134), 7134),
            Atualizacao::SemNovidade { ultimo: 7134 }
        );
        // Página atrasada em relação ao banco (cache do site): também nada a fazer.
        assert_eq!(
            classificar_atualizacao(Some(7134), 7130),
            Atualizacao::SemNovidade { ultimo: 7134 }
        );
        assert_eq!(classificar_atualizacao(Some(7134), 7134).faltando(), 0);
    }

    #[test]
    fn atualizacao_proximo_quando_salta_um() {
        assert_eq!(
            classificar_atualizacao(Some(7133), 7134),
            Atualizacao::Proximo
        );
        assert_eq!(classificar_atualizacao(Some(7133), 7134).faltando(), 0);
    }

    #[test]
    fn atualizacao_salto_quando_falta_concurso_no_meio() {
        let a = classificar_atualizacao(Some(7132), 7134);
        assert_eq!(
            a,
            Atualizacao::Salto {
                ultimo: 7132,
                pagina: 7134
            }
        );
        assert_eq!(a.faltando(), 1);
        assert_eq!(classificar_atualizacao(Some(1), 100).faltando(), 98);
    }

    #[test]
    fn atualizacao_banco_vazio_conta_como_ultimo_zero() {
        assert_eq!(classificar_atualizacao(None, 1), Atualizacao::Proximo);
        let a = classificar_atualizacao(None, 2);
        assert_eq!(
            a,
            Atualizacao::Salto {
                ultimo: 0,
                pagina: 2
            }
        );
        assert_eq!(a.faltando(), 1);
        let a = classificar_atualizacao(None, 7134);
        assert_eq!(
            a,
            Atualizacao::Salto {
                ultimo: 0,
                pagina: 7134
            }
        );
        assert_eq!(a.faltando(), 7133);
    }

    #[test]
    fn atualizacao_concurso_zero_com_banco_vazio_vai_para_a_validacao() {
        // A validação do salto de 1 recusa o concurso 0; não é salto.
        assert_eq!(classificar_atualizacao(None, 0), Atualizacao::Proximo);
    }

    #[test]
    fn atualizacao_nao_estoura_no_maior_concurso() {
        assert_eq!(
            classificar_atualizacao(Some(u32::MAX), u32::MAX),
            Atualizacao::SemNovidade { ultimo: u32::MAX }
        );
        assert_eq!(
            classificar_atualizacao(Some(u32::MAX - 1), u32::MAX),
            Atualizacao::Proximo
        );
    }

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

    // --- registrar_perfil -------------------------------------------------

    const PERFIL: &str = r#"nome = "teste_v1"
versao = 1
loteria = "quina"
janela = 10
normalizacao = "minmax"

[pesos]
frequencia = 1.0
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"#;

    fn carregado(texto: &str) -> PerfilCarregado {
        crate::perfil::interpretar(texto, std::path::Path::new("perfis/quina/teste_v1.toml"))
            .expect("perfil válido")
    }

    fn quina() -> &'static LoteriaConfig {
        crate::config::buscar("quina").expect("quina")
    }

    fn banco() -> Connection {
        db::abrir(std::path::Path::new(":memory:")).expect("banco em memória")
    }

    #[test]
    fn registrar_perfil_cria_a_linha_com_todos_os_campos() {
        let conn = banco();
        let p = carregado(PERFIL);
        assert_eq!(
            registrar_perfil(&conn, quina(), &p, "2026-10-04T10:00:00Z").expect("registra"),
            RegistroPerfil::Novo
        );
        assert_eq!(
            db::buscar_perfil(&conn, "teste_v1").expect("busca"),
            Some(PerfilRegistrado {
                nome: "teste_v1".to_string(),
                versao: 1,
                loteria: "quina".to_string(),
                janela: 10,
                normalizacao: "minmax".to_string(),
                hash_toml: crate::perfil::hash_toml(PERFIL),
                toml: PERFIL.to_string(),
                criado_em: "2026-10-04T10:00:00Z".to_string(),
            })
        );
    }

    #[test]
    fn registrar_de_novo_e_idempotente_e_preserva_criado_em() {
        let conn = banco();
        registrar_perfil(&conn, quina(), &carregado(PERFIL), "2026-10-04T10:00:00Z")
            .expect("registra");
        let antes = db::buscar_perfil(&conn, "teste_v1").expect("busca");
        // Mesmo conteúdo, até com CRLF (mesmo hash), em outro momento.
        let crlf = carregado(&PERFIL.replace('\n', "\r\n"));
        assert_eq!(
            registrar_perfil(&conn, quina(), &crlf, "2030-01-01T00:00:00Z").expect("de novo"),
            RegistroPerfil::JaRegistrado
        );
        assert_eq!(db::buscar_perfil(&conn, "teste_v1").expect("busca"), antes);
    }

    #[test]
    fn mesmo_nome_com_hash_diferente_e_erro_e_nao_altera_a_linha() {
        let conn = banco();
        registrar_perfil(&conn, quina(), &carregado(PERFIL), "2026-10-04T10:00:00Z")
            .expect("registra");
        let antes = db::buscar_perfil(&conn, "teste_v1").expect("busca");
        let editado = carregado(&PERFIL.replace("frequencia = 1.0", "frequencia = 2.0"));
        match registrar_perfil(&conn, quina(), &editado, "2026-10-05T10:00:00Z") {
            Err(SpiderError::PerfilAlterado {
                nome,
                hash_registrado,
                hash_arquivo,
                proximo_nome,
                proxima_versao,
            }) => {
                assert_eq!(nome, "teste_v1");
                assert_eq!(hash_registrado, crate::perfil::hash_toml(PERFIL));
                assert_eq!(hash_arquivo, editado.hash_toml);
                assert_eq!(proximo_nome, "teste_v2");
                assert_eq!(proxima_versao, 2);
            }
            outro => panic!("esperava PerfilAlterado, veio {outro:?}"),
        }
        assert_eq!(db::buscar_perfil(&conn, "teste_v1").expect("busca"), antes);
    }

    #[test]
    fn sugestao_de_versao_pula_registradas_e_arquivos_existentes() {
        // teste_v2 já registrado no banco; teste_v3.toml existe na pasta.
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("relógio")
            .as_nanos();
        let pasta = std::env::temp_dir().join(format!("rust_spider_perfis_{unico}"));
        std::fs::create_dir_all(&pasta).expect("cria pasta");
        std::fs::write(pasta.join("teste_v3.toml"), "").expect("cria teste_v3.toml");

        let conn = banco();
        let v2 = PERFIL
            .replace("teste_v1", "teste_v2")
            .replace("versao = 1", "versao = 2");
        registrar_perfil(&conn, quina(), &carregado(&v2), "2026-10-04T10:00:00Z")
            .expect("registra v2");
        let em_disco = |texto: &str| {
            crate::perfil::interpretar(texto, &pasta.join("teste_v1.toml")).expect("válido")
        };
        registrar_perfil(&conn, quina(), &em_disco(PERFIL), "2026-10-04T10:00:00Z")
            .expect("registra v1");

        let editado = em_disco(&PERFIL.replace("frequencia = 1.0", "frequencia = 2.0"));
        let resultado = registrar_perfil(&conn, quina(), &editado, "2026-10-05T10:00:00Z");
        std::fs::remove_dir_all(&pasta).ok();
        match resultado {
            Err(SpiderError::PerfilAlterado {
                proximo_nome,
                proxima_versao,
                ..
            }) => {
                assert_eq!(proximo_nome, "teste_v4");
                assert_eq!(proxima_versao, 4);
            }
            outro => panic!("esperava PerfilAlterado, veio {outro:?}"),
        }
    }

    #[test]
    fn perfil_de_outra_loteria_e_erro_e_nao_grava() {
        let conn = banco();
        let megasena = crate::config::buscar("megasena").expect("megasena");
        assert!(matches!(
            registrar_perfil(&conn, megasena, &carregado(PERFIL), "2026-10-04T10:00:00Z"),
            Err(SpiderError::PerfilDeOutraLoteria { .. })
        ));
        db::criar_tabela_perfis(&conn).expect("cria");
        assert_eq!(db::buscar_perfil(&conn, "teste_v1").expect("busca"), None);
    }

    #[test]
    fn registrar_perfil_dentro_de_transacao_sem_commit_nao_grava() {
        let mut conn = banco();
        {
            let tx = db::iniciar_escrita(&mut conn).expect("transação");
            registrar_perfil(&tx, quina(), &carregado(PERFIL), "2026-10-04T10:00:00Z")
                .expect("registra");
            // Sem commit: rollback.
        }
        db::criar_tabela_perfis(&conn).expect("cria");
        assert_eq!(db::buscar_perfil(&conn, "teste_v1").expect("busca"), None);
    }

    // --- sincronizar_indice ------------------------------------------------

    /// Histórico sintético da Quina: concurso `c` sorteia 5 dezenas que
    /// dependem de `c` e de `deslocamento` (sem repetidas).
    fn historico_quina(n: u32, deslocamento: u32) -> Vec<Resultado> {
        (1..=n)
            .map(|c| {
                let base = (c * 7 + deslocamento) % 80;
                let dezenas: Vec<u32> = (0..5).map(|k| 1 + (base + k * 17) % 80).collect();
                r(c, "2020-01-01", &dezenas)
            })
            .collect()
    }

    /// Banco em memória com `n` resultados e as features calculadas.
    fn banco_com_features(n: u32) -> Connection {
        let mut conn = banco();
        db::salvar_resultados(&mut conn, &historico_quina(n, 3)).expect("salva");
        sincronizar_features(&mut conn, quina(), false).expect("features");
        conn
    }

    fn tabelas(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepara");
        let nomes = stmt
            .query_map([], |row| row.get(0))
            .expect("consulta")
            .collect::<rusqlite::Result<Vec<String>>>()
            .expect("nomes");
        nomes
    }

    #[test]
    fn indice_grava_todos_os_concursos_igual_ao_motor() {
        let mut conn = banco_com_features(40);
        let p = carregado(PERFIL);
        let resumo =
            sincronizar_indice(&mut conn, quina(), &p, "2026-10-05T10:00:00Z").expect("índice");
        assert_eq!(
            resumo,
            ResumoIndice {
                perfil: "teste_v1".to_string(),
                registro: RegistroPerfil::Novo,
                concursos: 41,
                primeiro_concurso: 1,
                ultimo_concurso: 40,
                proximo_concurso: 41,
                linhas_apagadas: 0,
                linhas_gravadas: 41 * 80,
            }
        );

        let (estado, mut features) =
            motor::calcular(quina(), &historico_quina(40, 3)).expect("motor");
        features.extend(estado.snapshot());
        let esperado = motor::calcular_indice(
            &features,
            p.perfil.janela,
            p.perfil.normalizacao,
            &p.perfil.pesos,
        )
        .expect("índice puro");
        assert_eq!(db::ler_indice(&conn, "teste_v1").expect("lê"), esperado);
        assert_eq!(
            db::buscar_perfil(&conn, "teste_v1")
                .expect("busca")
                .map(|p| p.criado_em),
            Some("2026-10-05T10:00:00Z".to_string())
        );

        // De novo: mesmas linhas, perfil já registrado, `criado_em` mantido.
        let de_novo =
            sincronizar_indice(&mut conn, quina(), &p, "2030-01-01T00:00:00Z").expect("de novo");
        assert_eq!(de_novo.registro, RegistroPerfil::JaRegistrado);
        assert_eq!(de_novo.linhas_apagadas, 41 * 80);
        assert_eq!(db::ler_indice(&conn, "teste_v1").expect("lê"), esperado);
        assert_eq!(
            db::buscar_perfil(&conn, "teste_v1")
                .expect("busca")
                .map(|p| p.criado_em),
            Some("2026-10-05T10:00:00Z".to_string())
        );
    }

    #[test]
    fn indice_sem_resultados_ou_sem_features_nao_cria_tabelas() {
        let mut conn = banco();
        let p = carregado(PERFIL);
        assert!(matches!(
            sincronizar_indice(&mut conn, quina(), &p, "x"),
            Err(SpiderError::SemResultados(_))
        ));
        db::salvar_resultados(&mut conn, &historico_quina(5, 0)).expect("salva");
        assert!(matches!(
            sincronizar_indice(&mut conn, quina(), &p, "x"),
            Err(SpiderError::SemFeatures(_))
        ));
        db::criar_tabelas_features(&conn).expect("cria vazia");
        assert!(matches!(
            sincronizar_indice(&mut conn, quina(), &p, "x"),
            Err(SpiderError::SemFeatures(_))
        ));
        assert_eq!(
            tabelas(&conn),
            ["features_dezena", "motor_estado", "resultados"]
        );
    }

    #[test]
    fn indice_recusa_features_desatualizadas_e_de_outra_versao() {
        let mut conn = banco_com_features(20);
        let p = carregado(PERFIL);
        db::salvar_resultados(&mut conn, &historico_quina(21, 3)[20..]).expect("concurso 21");
        match sincronizar_indice(&mut conn, quina(), &p, "x") {
            Err(SpiderError::FeaturesDesatualizadas {
                ultimo_concurso,
                snapshot,
                ..
            }) => assert_eq!((ultimo_concurso, snapshot), (21, 21)),
            outro => panic!("esperava FeaturesDesatualizadas, veio {outro:?}"),
        }
        sincronizar_features(&mut conn, quina(), false).expect("em dia");
        conn.execute(
            "UPDATE features_dezena SET feature_version = ?1 WHERE concurso = 7",
            [FEATURE_VERSION + 1],
        )
        .expect("versão adulterada");
        match sincronizar_indice(&mut conn, quina(), &p, "x") {
            Err(SpiderError::FeaturesDeOutraVersao {
                encontrada, atual, ..
            }) => assert_eq!((encontrada, atual), (FEATURE_VERSION + 1, FEATURE_VERSION)),
            outro => panic!("esperava FeaturesDeOutraVersao, veio {outro:?}"),
        }
        assert_eq!(
            tabelas(&conn),
            ["features_dezena", "motor_estado", "resultados"]
        );
    }

    fn serie(concursos: std::ops::RangeInclusive<u32>, dezenas: &[u32]) -> Vec<FeaturesDezena> {
        let mut v = Vec::new();
        for c in concursos {
            for &d in dezenas {
                v.push(FeaturesDezena {
                    concurso: c,
                    dezena: d,
                    janela: 10,
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
                });
            }
        }
        v
    }

    #[test]
    fn conferir_serie_aceita_so_a_serie_completa() {
        let cfg = quina();
        let todas: Vec<u32> = cfg.dezenas().collect();
        assert_eq!(
            conferir_serie(cfg, 10, &serie(1..=3, &todas), 3).expect("completa"),
            (1, 3)
        );

        let incoerente = |features: &[FeaturesDezena], ultimo: u32| {
            matches!(
                conferir_serie(cfg, 10, features, ultimo),
                Err(SpiderError::FeaturesIncoerentes { janela: 10, .. })
            )
        };
        assert!(incoerente(&[], 3), "vazia");
        assert!(incoerente(&serie(2..=3, &todas), 3), "não começa no 1");
        assert!(incoerente(&serie(1..=2, &todas), 3), "termina antes");
        let mut com_buraco = serie(1..=1, &todas);
        com_buraco.extend(serie(3..=3, &todas));
        assert!(incoerente(&com_buraco, 3), "buraco");
        assert!(
            incoerente(&serie(1..=3, &todas[1..]), 3),
            "falta a dezena 1"
        );
        let mut fora = todas.clone();
        fora.push(81);
        assert!(incoerente(&serie(1..=3, &fora), 3), "dezena 81");
        let mut so_um = serie(1..=3, &todas);
        so_um.retain(|f| !(f.concurso == 2 && f.dezena == 40));
        assert!(incoerente(&so_um, 3), "falta uma dezena num concurso");
        let mut repetida = serie(1..=3, &todas);
        repetida.insert(5, repetida[4].clone());
        assert!(incoerente(&repetida, 3), "dezena repetida");

        // A mensagem diz o que falta e o que sobra.
        let mut trocada = serie(1..=3, &todas);
        for f in trocada
            .iter_mut()
            .filter(|f| f.concurso == 2 && f.dezena == 80)
        {
            f.dezena = 81;
        }
        let erro = conferir_serie(cfg, 10, &trocada, 3).expect_err("dezena 81");
        let texto = erro.to_string();
        assert!(texto.contains("concurso 2"), "{texto}");
        assert!(texto.contains("faltam [80]"), "{texto}");
        assert!(texto.contains("[81]"), "{texto}");
        assert!(texto.contains("--completo"), "{texto}");
    }
}
