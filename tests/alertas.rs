//! Alertas (3.6) sem rede e sem relógio de verdade: loterias falsas falham
//! no `ciclo` (uma passada e `--aguardar`), o histórico em `alertas/` fica
//! numa pasta temporária e os rascunhos `.eml` são conferidos. O binário
//! `sync` entra só nos testes de CLI (código de saída 3, `alerta --geral` e
//! `alertas`), também numa pasta temporária. Nada toca nos bancos reais.

use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    executar_ciclo, executar_ciclo_aguardando, listar_pendentes, noites_do_aguardar,
    noites_do_ciclo, registrar_falha_geral, registrar_noite, EstadoAlertas, Evento, EventoAlerta,
    NoiteAlvo, PastasAlertas, PastasCiclo, PublicacaoLoteria, RankingDoCiclo, RelatorioAguardar,
    RelatorioCiclo, ResumoAguardar, ResumoLoteria, ALVO_GERAL, ARQUIVO_DESTINATARIO,
    ARQUIVO_ESTADO_ALERTAS, NOITES_DO_LEMBRETE, NOITES_PARA_ALERTA, PASTA_ENVIADOS,
};
use rust_spider::publicacao::{Desfecho, Destino, Preparo};
use rust_spider::trava::Trava;
use rust_spider::{tempo, Result, Resultado};
use std::cell::{Cell, RefCell};
use std::fs;
use std::future::{ready, Ready};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

// ---------------------------------------------------------------------------
// Apoio
// ---------------------------------------------------------------------------

fn cfg_de(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

fn dia(data: &str) -> i64 {
    tempo::dias_da_data_iso(data).expect("data válida no teste")
}

/// Segundos Unix de `data` às `hora:minuto` em Brasília (UTC-3).
fn brasilia(data: &str, hora: i64, minuto: i64) -> u64 {
    u64::try_from(dia(data) * 86_400 + hora * 3600 + minuto * 60 + 3 * 3600)
        .expect("instante positivo")
}

fn noite_n(n: u32) -> String {
    tempo::data_iso_de_dias(dia("2026-10-01") + i64::from(n) - 1)
}

/// Pasta temporária de trabalho, apagada ao sair.
struct Cena {
    raiz: PathBuf,
    alvos: Vec<(&'static LoteriaConfig, PathBuf)>,
}

impl Cena {
    fn nova(slugs: &[&str]) -> Cena {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let raiz = std::env::temp_dir().join(format!(
            "rust_spider_alertas_it_{}_{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&raiz);
        fs::create_dir_all(&raiz).unwrap();
        let alvos = slugs
            .iter()
            .map(|s| (cfg_de(s), raiz.join(format!("resultados_{s}.db"))))
            .collect();
        Cena { raiz, alvos }
    }

    fn alertas(&self) -> PathBuf {
        self.raiz.join("alertas")
    }

    fn logs(&self) -> PathBuf {
        self.raiz.join("logs")
    }

    fn pastas_alertas<'a>(&'a self, alertas: &'a Path, logs: &'a Path) -> PastasAlertas<'a> {
        PastasAlertas { alertas, logs }
    }

    fn estado(&self) -> EstadoAlertas {
        let texto = fs::read_to_string(self.alertas().join(ARQUIVO_ESTADO_ALERTAS))
            .expect("estado.json gravado");
        serde_json::from_str(&texto).expect("estado.json legível")
    }

    fn rascunhos(&self) -> Vec<PathBuf> {
        listar_pendentes(&self.alertas())
            .unwrap()
            .into_iter()
            .map(|r| r.caminho)
            .collect()
    }

    /// Semeia o banco da loteria com um único concurso na `data`.
    fn semear(&self, slug: &str, concurso: u32, data: &str) {
        let cfg = cfg_de(slug);
        let (_, db_caminho) = self.alvos.iter().find(|(c, _)| c.slug == slug).unwrap();
        let mut conn = db::abrir(db_caminho).unwrap();
        db::salvar_resultados(
            &mut conn,
            &[Resultado {
                loteria: slug.to_string(),
                concurso,
                data_sorteio: data.to_string(),
                dezenas: cfg.dezenas().take(cfg.qtd_dezenas).collect(),
            }],
        )
        .unwrap();
    }
}

impl Drop for Cena {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

/// Destino que aceita tudo (a publicação não é o assunto aqui).
#[derive(Default)]
struct DestinoFalso;

impl Destino for DestinoFalso {
    fn preparar(&mut self) -> Result<Preparo> {
        Ok(Preparo {
            index_atual: None,
            envio_pendente: false,
        })
    }
    fn espelhar(&mut self, _: &Path, _: &[String]) -> Result<()> {
        Ok(())
    }
    fn registrar(&mut self, _: &str) -> Result<bool> {
        Ok(true)
    }
    fn enviar(&mut self) -> Result<()> {
        Ok(())
    }
}

fn navegador() -> SpiderError {
    SpiderError::Navegador {
        url: "https://loterias.caixa.gov.br".into(),
        detalhe: "falha ao iniciar o Playwright: Failed to initialize".into(),
    }
}

fn seletor_sumiu() -> SpiderError {
    SpiderError::ElementoNaoEncontrado("#tituloResultadoConcurso".into())
}

type Leitura = Ready<Result<Resultado>>;
type Planilha = Ready<Result<PathBuf>>;

fn sem_planilha(_: &'static LoteriaConfig) -> Planilha {
    ready(Err(navegador()))
}

/// Resultado de uma noite: rascunhos gravados e eventos.
struct Noite {
    gerados: Vec<PathBuf>,
    eventos: Vec<Evento>,
}

impl Noite {
    fn alertas(&self) -> Vec<&EventoAlerta> {
        self.eventos
            .iter()
            .filter_map(|e| match e {
                Evento::Alerta(a) => Some(a),
                _ => None,
            })
            .collect()
    }
}

/// Uma noite do `ciclo` (uma passada, começando às 23h00 de `data`) cuja
/// página falha sempre com `erro`; termina às 23h10 e registra os alertas.
async fn noite_de_ciclo_com_falha(c: &Cena, data: &str, erro: fn() -> SpiderError) -> Noite {
    let inicio = brasilia(data, 23, 0);
    let fim = Cell::new(inicio + 600);
    let eventos = RefCell::new(Vec::new());
    let mut obs = |e: Evento| eventos.borrow_mut().push(e);
    let backups = c.raiz.join("backups");
    let cache = c.raiz.join("cache");
    let perfis = c.raiz.join("perfis");
    let saida = c.raiz.join("publico");
    let pastas = PastasCiclo {
        backups: &backups,
        cache: &cache,
        perfis: &perfis,
        saida: &saida,
    };
    let relatorio = executar_ciclo(
        &c.alvos,
        &pastas,
        &mut DestinoFalso,
        &mut obs,
        &mut || Ok(fim.get()),
        move |_| -> Leitura { ready(Err(erro())) },
        sem_planilha,
    )
    .await;
    assert!(!relatorio.sucesso());
    let noites = noites_do_ciclo(&c.alvos, &relatorio, fim.get() as i64, &mut obs);
    let (alertas, logs) = (c.alertas(), c.logs());
    let gerados = registrar_noite(
        &c.pastas_alertas(&alertas, &logs),
        &noites,
        inicio as i64,
        &mut || Ok(fim.get()),
        &mut obs,
    )
    .unwrap();
    Noite {
        gerados,
        eventos: eventos.into_inner(),
    }
}

/// Registra uma noite já resolvida, sem rodar o ciclo (`falha` para cada
/// alvo; `None` = sucesso).
fn registrar_direto(c: &Cena, data: &str, noites: &[NoiteAlvo]) -> Noite {
    let inicio = brasilia(data, 23, 0);
    let eventos = RefCell::new(Vec::new());
    let (alertas, logs) = (c.alertas(), c.logs());
    let gerados = registrar_noite(
        &c.pastas_alertas(&alertas, &logs),
        noites,
        inicio as i64,
        &mut || Ok(inicio + 600),
        &mut |e| eventos.borrow_mut().push(e),
    )
    .unwrap();
    Noite {
        gerados,
        eventos: eventos.into_inner(),
    }
}

fn falha_de_rede() -> rust_spider::orquestracao::FalhaNoite {
    rust_spider::orquestracao::FalhaNoite {
        etapa: "atualizar".into(),
        erro: "erro do navegador: sem rede".into(),
        mudanca_de_pagina: false,
    }
}

fn quina_falha() -> NoiteAlvo {
    NoiteAlvo::loteria(cfg_de("quina"), Some(7140), Some(falha_de_rede()))
}

fn quina_ok() -> NoiteAlvo {
    NoiteAlvo::loteria(cfg_de("quina"), Some(7141), None)
}

fn ler(p: &Path) -> String {
    fs::read_to_string(p).unwrap()
}

// ---------------------------------------------------------------------------
// N noites seguidas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn falha_de_rede_so_alerta_na_segunda_noite_e_o_eml_traz_o_necessario() {
    let c = Cena::nova(&["quina"]);
    c.semear("quina", 7140, "2026-10-11");

    let primeira = noite_de_ciclo_com_falha(&c, "2026-10-13", navegador).await;
    assert!(primeira.gerados.is_empty(), "{:?}", primeira.gerados);
    assert!(c.rascunhos().is_empty());
    assert!(primeira.alertas().iter().any(|a| matches!(
        a,
        EventoAlerta::FalhaRegistrada { alvo, noites: 1, limite }
            if alvo == "quina" && *limite == NOITES_PARA_ALERTA
    )));
    let estado = c.estado();
    let quina = &estado.alvos["quina"];
    assert_eq!(quina.falhas_seguidas, 1);
    assert!(quina.ultimo_erro.as_deref().unwrap().contains("Playwright"));
    assert!(quina.primeira_falha.is_some() && quina.ultimo_alerta.is_none());

    let segunda = noite_de_ciclo_com_falha(&c, "2026-10-14", navegador).await;
    assert_eq!(segunda.gerados.len(), 1);
    let eml = ler(&segunda.gerados[0]);
    let nome = segunda.gerados[0].file_name().unwrap().to_string_lossy();
    assert!(
        nome.starts_with("2026-10-14_2310") && nome.ends_with("_quina-falhas-seguidas.eml"),
        "{nome}"
    );
    for esperado in [
        "X-Unsent: 1\r\n",
        "Subject: [rust_spider] quina: falhas seguidas (2 noites)\r\n",
        "Content-Type: text/plain; charset=utf-8\r\n",
        "Date: Wed, 14 Oct 2026 23:10:00 -0300\r\n",
        "Loteria: Quina (quina)",
        "Falhas seguidas: 2 noite(s)",
        "Último concurso no banco: 7140",
        "Etapa em que falhou: atualizar",
        "erro do navegador",
        "Playwright",
        "sync_2026-10-14.log",
        "README.md",
        "alertas/enviados/",
    ] {
        assert!(eml.contains(esperado), "faltou `{esperado}` em:\n{eml}");
    }
    assert!(!eml.contains("\nTo:"), "sem destinatario.txt não há To:");
    // O aviso de rascunho novo chega pelos eventos (tela e log do dia).
    assert!(segunda.alertas().iter().any(|a| {
        matches!(a, EventoAlerta::RascunhoGerado { alvo, lembrete: false, .. } if alvo == "quina")
    }));
    let aviso = segunda
        .eventos
        .iter()
        .find(|e| matches!(e, Evento::Alerta(EventoAlerta::RascunhoGerado { .. })))
        .unwrap();
    assert!(aviso.to_string().starts_with("AVISO: rascunho"), "{aviso}");
    assert_eq!(aviso.nivel(), rust_spider::log_arquivo::Nivel::Aviso);
    assert_eq!(c.rascunhos().len(), 1);
}

#[tokio::test]
async fn terceira_noite_nao_repete_o_rascunho() {
    let c = Cena::nova(&["quina"]);
    for n in 1..=3 {
        noite_de_ciclo_com_falha(&c, &noite_n(n), navegador).await;
    }
    assert_eq!(c.rascunhos().len(), 1, "deduplicado por (alvo, motivo)");
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 3);
}

#[tokio::test]
async fn duas_execucoes_na_mesma_noite_valem_uma_falha() {
    let c = Cena::nova(&["quina"]);
    noite_de_ciclo_com_falha(&c, "2026-10-13", navegador).await;
    let repetida = noite_de_ciclo_com_falha(&c, "2026-10-13", navegador).await;
    assert!(repetida.gerados.is_empty());
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 1);
}

#[test]
fn sucesso_zera_o_contador_e_so_gera_linha_no_log() {
    let c = Cena::nova(&["quina"]);
    registrar_direto(&c, &noite_n(1), &[quina_falha()]);
    let ok = registrar_direto(&c, &noite_n(2), &[quina_ok(), NoiteAlvo::geral_ok()]);
    assert!(ok.gerados.is_empty());
    assert!(ok.alertas().iter().any(|a| matches!(
        a,
        EventoAlerta::VoltouAoNormal { alvo, noites: 1 } if alvo == "quina"
    )));
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 0);

    // Uma falha depois do sucesso é a primeira de novo: sem rascunho.
    let de_novo = registrar_direto(&c, &noite_n(3), &[quina_falha()]);
    assert!(de_novo.gerados.is_empty());
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 1);
    assert!(c.rascunhos().is_empty());
}

#[test]
fn lembrete_a_cada_sete_noites_ainda_em_falha() {
    let c = Cena::nova(&["quina"]);
    let mut noites_com_rascunho = Vec::new();
    for n in 1..=17 {
        let r = registrar_direto(&c, &noite_n(n), &[quina_falha()]);
        if !r.gerados.is_empty() {
            noites_com_rascunho.push(n);
        }
    }
    // 2ª noite (N) e depois a cada 7: 9ª e 16ª.
    assert_eq!(
        noites_com_rascunho,
        [
            NOITES_PARA_ALERTA,
            NOITES_PARA_ALERTA + NOITES_DO_LEMBRETE,
            NOITES_PARA_ALERTA + 2 * NOITES_DO_LEMBRETE
        ]
    );
    let todos = c.rascunhos();
    assert_eq!(todos.len(), 3);
    assert!(!ler(&todos[0]).contains("(lembrete)"));
    assert!(ler(&todos[1])
        .contains("Subject: [rust_spider] quina: falhas seguidas (9 noites) (lembrete)"));
    assert!(ler(&todos[2]).contains("lembrete: o problema continua"));
}

#[test]
fn rascunho_enviado_sai_da_lista_de_pendentes() {
    let c = Cena::nova(&["quina"]);
    registrar_direto(&c, &noite_n(1), &[quina_falha()]);
    registrar_direto(&c, &noite_n(2), &[quina_falha()]);
    let pendentes = listar_pendentes(&c.alertas()).unwrap();
    assert_eq!(pendentes.len(), 1);
    assert_eq!(
        pendentes[0].assunto.as_deref(),
        Some("[rust_spider] quina: falhas seguidas (2 noites)")
    );
    // Mover para enviados/ (como o usuário faz) tira da lista; `estado.json`
    // e outros arquivos nunca entram.
    let enviados = c.alertas().join(PASTA_ENVIADOS);
    fs::create_dir_all(&enviados).unwrap();
    let nome = pendentes[0].caminho.file_name().unwrap();
    fs::rename(&pendentes[0].caminho, enviados.join(nome)).unwrap();
    assert!(listar_pendentes(&c.alertas()).unwrap().is_empty());
    assert!(listar_pendentes(&c.raiz.join("nao_existe"))
        .unwrap()
        .is_empty());
}

// ---------------------------------------------------------------------------
// Mudança aparente no site
// ---------------------------------------------------------------------------

#[tokio::test]
async fn seletor_sumiu_alerta_ja_na_primeira_noite_e_nao_repete() {
    let c = Cena::nova(&["quina"]);
    let primeira = noite_de_ciclo_com_falha(&c, "2026-10-13", seletor_sumiu).await;
    assert_eq!(primeira.gerados.len(), 1);
    let eml = ler(&primeira.gerados[0]);
    assert!(
        eml.contains("Subject: [rust_spider] quina: site possivelmente mudou\r\n"),
        "{eml}"
    );
    assert!(eml.contains("#tituloResultadoConcurso"), "{eml}");
    assert!(eml.contains("Parece mudanca no site"), "{eml}");
    assert!(primeira.gerados[0]
        .to_string_lossy()
        .ends_with("_quina-site-mudou.eml"));

    // Na noite seguinte o mesmo motivo não gera outro rascunho.
    let segunda = noite_de_ciclo_com_falha(&c, "2026-10-14", seletor_sumiu).await;
    assert!(segunda.gerados.is_empty());
    assert_eq!(c.rascunhos().len(), 1);
}

#[tokio::test]
async fn linha_invalida_e_pagina_incompleta_tambem_sao_mudanca_de_pagina() {
    for erro in [
        (|| SpiderError::LinhaInvalida("esperava 5 dezenas".into())) as fn() -> SpiderError,
        || SpiderError::PaginaIncompleta("cabeçalho ilegível".into()),
        || SpiderError::BotaoDownloadNaoEncontrado("#btnResultados".into()),
    ] {
        let c = Cena::nova(&["quina"]);
        let r = noite_de_ciclo_com_falha(&c, "2026-10-13", erro).await;
        assert_eq!(r.gerados.len(), 1, "{}", erro());
    }
}

#[tokio::test]
async fn rede_e_navegador_nao_contam_como_mudanca_de_pagina() {
    for erro in [navegador as fn() -> SpiderError, || {
        SpiderError::DownloadNaoIniciado("#btnResultados".into())
    }] {
        let c = Cena::nova(&["quina"]);
        let r = noite_de_ciclo_com_falha(&c, "2026-10-13", erro).await;
        assert!(r.gerados.is_empty(), "{}", erro());
    }
}

/// `--aguardar` às 23h00 até 01h00; a página falha com os erros de `erros`,
/// um por rodada (o último se repete). Devolve a noite já registrada.
async fn noite_aguardando(c: &Cena, data: &str, erros: Vec<fn() -> SpiderError>) -> Noite {
    let inicio = brasilia(data, 23, 0);
    let relogio = Rc::new(Cell::new(inicio));
    let eventos = RefCell::new(Vec::new());
    let mut obs = |e: Evento| eventos.borrow_mut().push(e);
    let (backups, cache) = (c.raiz.join("backups"), c.raiz.join("cache"));
    let (perfis, saida) = (c.raiz.join("perfis"), c.raiz.join("publico"));
    let pastas = PastasCiclo {
        backups: &backups,
        cache: &cache,
        perfis: &perfis,
        saida: &saida,
    };
    let leituras = Rc::new(Cell::new(0usize));
    let contador = Rc::clone(&leituras);
    let (le, dorme) = (Rc::clone(&relogio), Rc::clone(&relogio));
    let relatorio = executar_ciclo_aguardando(
        &c.alvos,
        &pastas,
        &mut DestinoFalso,
        &mut obs,
        &mut || Ok(le.get()),
        &mut |espera| dorme.set(dorme.get() + espera.as_secs()),
        move |_| -> Leitura {
            let i = contador.get();
            contador.set(i + 1);
            ready(Err(erros[i.min(erros.len() - 1)]()))
        },
        sem_planilha,
    )
    .await
    .expect("relatório");
    assert!(!relatorio.sucesso());
    assert!(relatorio.rodadas > 1, "esperava várias rodadas");
    let fim = relogio.get();
    let noites = noites_do_aguardar(&c.alvos, &relatorio, fim as i64, &mut obs);
    let (alertas, logs) = (c.alertas(), c.logs());
    let gerados = registrar_noite(
        &c.pastas_alertas(&alertas, &logs),
        &noites,
        inicio as i64,
        &mut || Ok(fim),
        &mut obs,
    )
    .unwrap();
    Noite {
        gerados,
        eventos: eventos.into_inner(),
    }
}

#[tokio::test]
async fn aguardar_com_erro_de_pagina_em_todas_as_rodadas_alerta_na_primeira_noite() {
    let c = Cena::nova(&["quina"]);
    let r = noite_aguardando(&c, "2026-10-13", vec![seletor_sumiu as fn() -> SpiderError]).await;
    assert_eq!(r.gerados.len(), 1);
    assert!(ler(&r.gerados[0]).contains("site possivelmente mudou"));
}

#[tokio::test]
async fn aguardar_com_uma_rodada_de_rede_no_meio_segue_a_regra_das_n_noites() {
    // Primeira rodada: seletor; as seguintes: rede. Nem todas as rodadas são
    // erro de forma, então vale o N (2 noites).
    let c = Cena::nova(&["quina"]);
    let r = noite_aguardando(
        &c,
        "2026-10-13",
        vec![seletor_sumiu as fn() -> SpiderError, navegador],
    )
    .await;
    assert!(r.gerados.is_empty(), "{:?}", r.gerados);
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 1);
    let r = noite_aguardando(&c, "2026-10-14", vec![navegador as fn() -> SpiderError]).await;
    // Sem nada exportado, o `publicar` final de reenvio também falha nesta
    // cena: a falha agora é do geral (segunda noite seguida, também alerta).
    let quina: Vec<_> = r
        .gerados
        .iter()
        .filter(|g| g.to_string_lossy().contains("_quina-"))
        .collect();
    assert_eq!(quina.len(), 1, "{:?}", r.gerados);
    assert!(ler(quina[0]).contains("falhas seguidas (2 noites)"));
    assert_eq!(r.gerados.len(), 2, "{:?}", r.gerados);
    assert!(r
        .gerados
        .iter()
        .any(|g| g.to_string_lossy().contains("_geral-falha-ao-publicar")));
}

// ---------------------------------------------------------------------------
// Calendário (Atrasado) e janelas dos especiais
// ---------------------------------------------------------------------------

/// Relatório de um ciclo que correu bem para a Quina.
fn ciclo_ok(publicacao: Result<Desfecho>, exportada: bool) -> RelatorioCiclo {
    RelatorioCiclo {
        loterias: vec![ResumoLoteria {
            slug: "quina",
            ultimo_concurso: Some(1),
            concurso_novo: false,
            backfill: false,
            exportada,
            ranking: Default::default(),
            posicoes: Default::default(),
            falha: None,
        }],
        publicacao,
    }
}

#[test]
fn atrasado_fora_da_janela_do_especial_e_falha_e_alerta_na_segunda_noite() {
    let c = Cena::nova(&["quina"]);
    // Último concurso: segunda 2026-10-05. Na segunda 2026-10-12 às 23h00 o
    // sorteio de 10/6 e o seguinte já passaram do prazo: Atrasado.
    c.semear("quina", 1, "2026-10-05");
    let agora = brasilia("2026-10-12", 23, 0) as i64;
    let mut eventos = Vec::new();
    let noites = noites_do_ciclo(
        &c.alvos,
        &ciclo_ok(Ok(Desfecho::NadaAPublicar), false),
        agora,
        &mut |e| eventos.push(e),
    );
    let falha = noites[0].falha.as_ref().expect("Atrasado conta como falha");
    assert_eq!(falha.etapa, "calendario");
    assert!(falha.erro.contains("2026-10-06"), "{}", falha.erro);
    assert!(eventos.is_empty());
    assert_eq!(
        noites[1].alvo, ALVO_GERAL,
        "o geral fecha a lista, sem falha"
    );
    assert!(noites[1].falha.is_none());

    // Duas noites seguidas Atrasado: rascunho na segunda.
    let r1 = registrar_direto(&c, "2026-10-12", &noites);
    assert!(r1.gerados.is_empty());
    let r2 = registrar_direto(&c, "2026-10-13", &noites);
    assert_eq!(r2.gerados.len(), 1);
    let eml = ler(&r2.gerados[0]);
    assert!(eml.contains("Etapa em que falhou: calendario"), "{eml}");
    assert!(eml.contains("Último concurso no banco: 1"), "{eml}");
}

#[test]
fn atrasado_dentro_da_janela_da_quina_de_sao_joao_e_so_aviso() {
    let c = Cena::nova(&["quina"]);
    // Último concurso: segunda 2026-06-08; em 2026-06-15 já passou o prazo,
    // mas é a parada que antecede a Quina de São João (janela 9/6 a 28/6).
    c.semear("quina", 1, "2026-06-08");
    let agora = brasilia("2026-06-15", 23, 0) as i64;
    let mut eventos = Vec::new();
    let relatorio = ciclo_ok(Ok(Desfecho::NadaAPublicar), false);
    let noites = noites_do_ciclo(&c.alvos, &relatorio, agora, &mut |e| eventos.push(e));
    assert!(noites[0].falha.is_none(), "dentro da janela não é falha");
    assert!(matches!(
        eventos.as_slice(),
        [Evento::Alerta(EventoAlerta::AtrasoEmJanelaEspecial { slug: "quina", especial, .. })]
            if *especial == "Quina de São João"
    ));
    for n in 0..5 {
        let r = registrar_direto(&c, &noite_n(n + 1), &noites);
        assert!(r.gerados.is_empty());
    }
    assert!(c.rascunhos().is_empty());
}

#[test]
fn concurso_que_so_nao_saiu_ate_a_janela_fechar_nao_conta() {
    // Em dia (último = o sorteio de hoje) e sem erro: sucesso.
    let c = Cena::nova(&["quina"]);
    c.semear("quina", 1, "2026-10-12");
    let agora = brasilia("2026-10-12", 23, 30) as i64;
    let noites = noites_do_ciclo(
        &c.alvos,
        &ciclo_ok(Ok(Desfecho::NadaAPublicar), false),
        agora,
        &mut |_| {},
    );
    assert!(noites[0].falha.is_none());
    // Aguardando (o sorteio de terça 13 ainda não saiu às 01h00): também.
    let apos = brasilia("2026-10-14", 1, 0) as i64;
    let noites = noites_do_ciclo(
        &c.alvos,
        &ciclo_ok(Ok(Desfecho::NadaAPublicar), false),
        apos,
        &mut |_| {},
    );
    assert!(noites[0].falha.is_none(), "Aguardando não é Atrasado");
}

/// Relatório de uma noite do `--aguardar` em que o concurso novo não saiu até
/// o fim da janela; `falha_ranking` é o erro do passo do ranking, se houver.
fn aguardar_ausente(falha_ranking: Option<&str>) -> RelatorioAguardar {
    RelatorioAguardar {
        loterias: vec![ResumoAguardar {
            resumo: ResumoLoteria {
                slug: "quina",
                ultimo_concurso: Some(1),
                concurso_novo: false,
                backfill: false,
                exportada: false,
                ranking: RankingDoCiclo {
                    falha: falha_ranking.map(str::to_string),
                    omitido: None,
                },
                posicoes: Default::default(),
                falha: None,
            },
            tentativas: 3,
            concurso_ausente: Some("2026-10-13".into()),
            erros_de_forma: 0,
        }],
        publicacoes: Vec::new(),
        rodadas: 3,
        janela_ate: 0,
    }
}

#[test]
fn aguardar_com_concurso_ausente_conta_a_falha_do_ranking_e_alerta_na_segunda_noite() {
    let c = Cena::nova(&["quina"]);
    c.semear("quina", 1, "2026-10-12");
    let agora = brasilia("2026-10-14", 1, 0) as i64;
    let relatorio = aguardar_ausente(Some("tabela ranking_taxa corrompida"));
    let noites = noites_do_aguardar(&c.alvos, &relatorio, agora, &mut |_| {});
    let falha = noites[0].falha.as_ref().expect("falha do ranking conta");
    assert_eq!(falha.etapa, "ranking");
    assert!(!falha.mudanca_de_pagina);

    let r1 = registrar_direto(&c, "2026-10-13", &noites);
    assert!(r1.gerados.is_empty());
    let r2 = registrar_direto(&c, "2026-10-14", &noites);
    assert_eq!(r2.gerados.len(), 1);
    let eml = ler(&r2.gerados[0]);
    assert!(eml.contains("Etapa em que falhou: ranking"), "{eml}");
}

#[test]
fn aguardar_com_concurso_ausente_sem_falha_do_ranking_continua_sem_falha() {
    let c = Cena::nova(&["quina"]);
    // Último concurso bem antigo: o calendário diria Atrasado, mas o concurso
    // ausente dispensa essa checagem (comportamento anterior).
    c.semear("quina", 1, "2026-10-05");
    let agora = brasilia("2026-10-14", 1, 0) as i64;
    let mut eventos = Vec::new();
    let noites = noites_do_aguardar(&c.alvos, &aguardar_ausente(None), agora, &mut |e| {
        eventos.push(e)
    });
    assert!(noites[0].falha.is_none());
    assert!(eventos.is_empty());
}

// ---------------------------------------------------------------------------
// Publicar
// ---------------------------------------------------------------------------

#[test]
fn falha_ao_publicar_conta_para_a_loteria_exportada() {
    let c = Cena::nova(&["quina"]);
    let erro = || Err(SpiderError::PublicacaoEnvioFalhou("sem rede".into()));
    let noites = noites_do_ciclo(&c.alvos, &ciclo_ok(erro(), true), 0, &mut |_| {});
    let falha = noites[0].falha.as_ref().expect("publicar falhou");
    assert_eq!(falha.etapa, "publicar");
    assert!(falha.erro.contains("sem rede"));
    assert!(!falha.mudanca_de_pagina);
    // Loteria que nem foi exportada não leva a culpa do publicar.
    let noites = noites_do_ciclo(&c.alvos, &ciclo_ok(erro(), false), 0, &mut |_| {});
    assert!(noites[0].falha.is_none());
}

// ---------------------------------------------------------------------------
// Estado, destinatário, geral
// ---------------------------------------------------------------------------

#[test]
fn estado_ilegivel_recomeca_do_zero_com_aviso_e_nao_derruba() {
    let c = Cena::nova(&["quina"]);
    fs::create_dir_all(c.alertas()).unwrap();
    fs::write(
        c.alertas().join(ARQUIVO_ESTADO_ALERTAS),
        "{ isto nao e json",
    )
    .unwrap();
    let r = registrar_direto(&c, &noite_n(1), &[quina_falha()]);
    assert!(r
        .alertas()
        .iter()
        .any(|a| matches!(a, EventoAlerta::EstadoIlegivel { .. })));
    let aviso = r
        .eventos
        .iter()
        .find(|e| matches!(e, Evento::Alerta(EventoAlerta::EstadoIlegivel { .. })))
        .unwrap();
    assert!(aviso.to_string().contains("recomeçando"), "{aviso}");
    // O estado foi reescrito e legível; a falha de agora é a primeira.
    assert_eq!(c.estado().alvos["quina"].falhas_seguidas, 1);
    // Sem o arquivo (primeira vez) não há aviso nenhum.
    let c2 = Cena::nova(&["quina"]);
    let r2 = registrar_direto(&c2, &noite_n(1), &[quina_falha()]);
    assert!(r2
        .alertas()
        .iter()
        .all(|a| !matches!(a, EventoAlerta::EstadoIlegivel { .. })));
}

#[test]
fn destinatario_do_arquivo_vira_to_e_ausente_ou_invalido_fica_sem() {
    // Presente (com comentário, linhas em branco e espaços, BOM).
    let c = Cena::nova(&["quina"]);
    fs::create_dir_all(c.alertas()).unwrap();
    fs::write(
        c.alertas().join(ARQUIVO_DESTINATARIO),
        "\u{feff}# quem recebe\r\n\r\n  eu@exemplo.com  \r\noutro@exemplo.com\r\n",
    )
    .unwrap();
    let seletor = NoiteAlvo::loteria(
        cfg_de("quina"),
        None,
        Some(rust_spider::orquestracao::FalhaNoite {
            mudanca_de_pagina: true,
            ..falha_de_rede()
        }),
    );
    let r = registrar_direto(&c, &noite_n(1), std::slice::from_ref(&seletor));
    assert_eq!(r.gerados.len(), 1);
    let eml = ler(&r.gerados[0]);
    assert!(
        eml.starts_with("X-Unsent: 1\r\nTo: eu@exemplo.com\r\n"),
        "{eml}"
    );
    assert!(!eml.contains("outro@exemplo.com"));
    assert!(eml.contains("Último concurso no banco: não disponível"));

    // Ausente: sem To:.
    let c = Cena::nova(&["quina"]);
    let r = registrar_direto(&c, &noite_n(1), std::slice::from_ref(&seletor));
    assert!(!ler(&r.gerados[0]).contains("To:"));

    // Inválido: sem To: e com aviso.
    let c = Cena::nova(&["quina"]);
    fs::create_dir_all(c.alertas()).unwrap();
    fs::write(c.alertas().join(ARQUIVO_DESTINATARIO), "sem arroba\n").unwrap();
    let r = registrar_direto(&c, &noite_n(1), &[seletor]);
    assert!(!ler(&r.gerados[0]).contains("To:"));
    assert!(r
        .alertas()
        .iter()
        .any(|a| matches!(a, EventoAlerta::DestinatarioInvalido { .. })));
}

#[test]
fn dois_alvos_no_mesmo_instante_geram_arquivos_diferentes() {
    let c = Cena::nova(&["quina", "megasena"]);
    let seletor = |slug: &str| {
        NoiteAlvo::loteria(
            cfg_de(slug),
            None,
            Some(rust_spider::orquestracao::FalhaNoite {
                mudanca_de_pagina: true,
                ..falha_de_rede()
            }),
        )
    };
    let r = registrar_direto(&c, &noite_n(1), &[seletor("quina"), seletor("megasena")]);
    assert_eq!(r.gerados.len(), 2);
    assert_ne!(r.gerados[0], r.gerados[1]);
    let assuntos: Vec<String> = listar_pendentes(&c.alertas())
        .unwrap()
        .into_iter()
        .filter_map(|p| p.assunto)
        .collect();
    assert!(assuntos.contains(&"[rust_spider] quina: site possivelmente mudou".to_string()));
    assert!(assuntos.contains(&"[rust_spider] megasena: site possivelmente mudou".to_string()));
}

#[test]
fn geral_conta_noites_por_motivo_e_um_ciclo_inteiro_o_zera() {
    let c = Cena::nova(&["quina"]);
    let (alertas, logs) = (c.alertas(), c.logs());
    let pastas = c.pastas_alertas(&alertas, &logs);
    let registrar = |data: &str, hora: i64, motivo: &str| {
        let t = brasilia(data, hora, 0);
        registrar_falha_geral(&pastas, motivo, &mut || Ok(t), &mut |_| {}).unwrap()
    };
    // Primeira noite: só conta. O detalhe depois do `:` não muda o motivo.
    assert!(registrar("2026-10-13", 23, "trava ocupada: PID 10").is_empty());
    let gerados = registrar("2026-10-14", 23, "trava ocupada: PID 11, outro comando");
    assert_eq!(gerados.len(), 1);
    let eml = ler(&gerados[0]);
    assert!(
        eml.contains("Subject: [rust_spider] geral: trava ocupada\r\n"),
        "{eml}"
    );
    assert!(eml.contains("geral (a execução em si, não uma loteria)"));
    assert!(eml.contains("PID 11, outro comando"));
    assert!(gerados[0]
        .to_string_lossy()
        .ends_with("_geral-trava-ocupada.eml"));
    // Mesmo motivo na noite seguinte: deduplicado.
    assert!(registrar("2026-10-15", 23, "trava ocupada: PID 12").is_empty());
    // Outro motivo do geral, com o contador já acima do N: rascunho novo.
    // Duas chamadas na mesma noite contam uma só.
    let morto = registrar("2026-10-16", 23, "processo morto: x");
    assert_eq!(morto.len(), 1);
    let eml_morto = ler(&morto[0]);
    assert!(
        eml_morto.contains("Subject: [rust_spider] geral: processo morto\r\n"),
        "{eml_morto}"
    );
    assert!(registrar("2026-10-16", 23, "processo morto: y").is_empty());
    assert_eq!(c.estado().alvos[ALVO_GERAL].falhas_seguidas, 4);

    // Um ciclo que chega ao fim zera o geral (e só o geral).
    let r = registrar_direto(&c, "2026-10-17", &[NoiteAlvo::geral_ok()]);
    assert!(r.alertas().iter().any(|a| matches!(
        a,
        EventoAlerta::VoltouAoNormal { alvo, noites: 4 } if alvo == ALVO_GERAL
    )));
    assert_eq!(c.estado().alvos[ALVO_GERAL].falhas_seguidas, 0);
}

// ---------------------------------------------------------------------------
// Binário `sync`
// ---------------------------------------------------------------------------

fn sync(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn texto(o: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn trava_ocupada_sai_com_codigo_3_e_outras_falhas_com_1() {
    let c = Cena::nova(&["quina"]);
    let agora = i64::try_from(brasilia("2026-10-13", 23, 0)).unwrap();
    let _dono = Trava::obter(&c.raiz, "ciclo", agora).unwrap();

    let ocupada = sync(&c.raiz, &["features", "quina"]);
    assert_eq!(ocupada.status.code(), Some(3), "{}", texto(&ocupada));
    assert!(texto(&ocupada).contains("outra execução em andamento"));

    // Comando que não grava passa pela trava ocupada.
    let leitura = sync(&c.raiz, &["alertas"]);
    assert_eq!(leitura.status.code(), Some(0), "{}", texto(&leitura));

    drop(_dono);
    // Sem a trava, a falha comum (banco inexistente) continua saindo com 1.
    let comum = sync(&c.raiz, &["features", "quina"]);
    assert_eq!(comum.status.code(), Some(1), "{}", texto(&comum));
}

#[test]
fn aviso_de_rascunho_tem_a_frase_inteira_sem_espacos_sobrando() {
    let aviso = EventoAlerta::RascunhoGerado {
        alvo: "quina".into(),
        motivo: "erro".into(),
        caminho: PathBuf::from("alertas/x.eml"),
        lembrete: false,
    }
    .to_string();
    let esperado = format!(
        "AVISO: rascunho de e-mail de alerta gravado para quina (erro): {}. Abra, revise e \
         envie; depois mova-o para alertas/{PASTA_ENVIADOS}/ (`sync alertas` lista os \
         pendentes).",
        PathBuf::from("alertas/x.eml").display()
    );
    assert_eq!(aviso, esperado);
    assert!(!aviso.contains("  "), "{aviso}");
}

#[test]
fn aguardar_falha_do_publicar_final_de_reenvio_e_do_geral() {
    let relatorio = RelatorioAguardar {
        loterias: Vec::new(),
        publicacoes: vec![PublicacaoLoteria {
            slug: None,
            resultado: Err(SpiderError::DownloadNaoIniciado("git push".into())),
        }],
        rodadas: 1,
        janela_ate: 0,
    };
    let mut obs = |_: Evento| {};
    let noites = noites_do_aguardar(&[], &relatorio, 0, &mut obs);
    assert_eq!(noites.len(), 1);
    assert_eq!(noites[0].alvo, ALVO_GERAL);
    let falha = noites[0].falha.as_ref().expect("o geral falhou");
    assert!(
        falha.erro.starts_with("falha ao publicar:"),
        "{}",
        falha.erro
    );

    // Sem falha no publicar final, o geral segue ok.
    let ok = RelatorioAguardar {
        publicacoes: vec![PublicacaoLoteria {
            slug: None,
            resultado: Ok(Desfecho::NadaAPublicar),
        }],
        ..relatorio
    };
    let noites = noites_do_aguardar(&[], &ok, 0, &mut obs);
    assert!(noites[0].falha.is_none());
}

#[test]
fn sync_alerta_geral_nao_pega_a_trava_conta_noites_e_o_sync_alertas_lista() {
    let c = Cena::nova(&[]);
    let agora = i64::try_from(brasilia("2026-10-13", 23, 0)).unwrap();
    // Trava ocupada por outro processo: o `alerta` roda mesmo assim.
    let _dono = Trava::obter(&c.raiz, "ciclo", agora).unwrap();

    let vazio = sync(&c.raiz, &["alertas"]);
    assert!(texto(&vazio).contains("Nenhum rascunho de alerta pendente"));

    let primeira = sync(
        &c.raiz,
        &["alerta", "--geral", "trava ocupada: dono de teste"],
    );
    assert_eq!(primeira.status.code(), Some(0), "{}", texto(&primeira));
    assert!(
        texto(&primeira).contains("1 noite(s) seguida(s)"),
        "{}",
        texto(&primeira)
    );
    assert!(c.rascunhos().is_empty());
    // O comando fica no log do dia.
    let logs: Vec<_> = fs::read_dir(c.logs()).unwrap().flatten().collect();
    assert_eq!(logs.len(), 1);
    assert!(ler(&logs[0].path()).contains("comando `sync alerta --geral"));

    // Mesma noite: não conta de novo. Recua a "última noite" para simular a
    // noite seguinte.
    let caminho = c.alertas().join(ARQUIVO_ESTADO_ALERTAS);
    let linha_ultima_noite = |texto: &str| {
        texto
            .lines()
            .find(|l| l.contains("\"ultima_noite\""))
            .expect("ultima_noite no estado")
            .trim()
            .trim_end_matches(',')
            .to_string()
    };
    let noite_da_primeira = linha_ultima_noite(&ler(&caminho));
    let repetida = sync(&c.raiz, &["alerta", "--geral", "trava ocupada: de novo"]);
    assert_eq!(repetida.status.code(), Some(0));
    let texto_estado = ler(&caminho);
    let antigo = linha_ultima_noite(&texto_estado);
    if antigo != noite_da_primeira {
        // O relógio real virou o dia entre as duas chamadas: a "mesma noite"
        // deixou de existir e o resto do cenário não vale. Raro; não é defeito.
        return;
    }
    assert!(c.rascunhos().is_empty());
    fs::write(
        &caminho,
        texto_estado.replace(&antigo, "\"ultima_noite\": \"2000-01-01\""),
    )
    .unwrap();

    let segunda = sync(&c.raiz, &["alerta", "--geral", "trava ocupada: terceira"]);
    assert_eq!(segunda.status.code(), Some(0), "{}", texto(&segunda));
    let saida = texto(&segunda);
    assert!(
        saida.contains("AVISO: rascunho de e-mail de alerta gravado para geral"),
        "{saida}"
    );
    assert_eq!(c.rascunhos().len(), 1);

    let lista = sync(&c.raiz, &["alertas"]);
    let l = texto(&lista);
    assert!(l.contains("1 rascunho(s) de alerta pendente(s)"), "{l}");
    assert!(l.contains("_geral-trava-ocupada.eml"), "{l}");
    assert!(l.contains("[rust_spider] geral: trava ocupada"), "{l}");

    // Motivo vazio é recusado.
    let sem_motivo = sync(&c.raiz, &["alerta", "--geral", "  "]);
    assert_eq!(sem_motivo.status.code(), Some(1));
}
