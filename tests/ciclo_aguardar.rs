//! Testes do `sync ciclo --aguardar` (3.3) sem rede e sem dormir de verdade: a
//! página e a planilha são falsas, o relógio é uma `Cell` que só avança quando
//! o ciclo "dorme", e o destino é um repositório bare local (testes com `git`,
//! pulados se ele não estiver instalado) ou um destino falso. Bancos e pastas
//! ficam em diretórios temporários; nada toca nos bancos reais nem em
//! `../rust_spider_dados`.
//!
//! Datas: o concurso `c` tem data `base + (c - 1)` dias, com base padrão
//! 2026-10-01. Com N = 12 o último é 2026-10-12 (segunda) e o concurso 13 é
//! 2026-10-13 (terça, dia de sorteio das três loterias; procura a partir das
//! 21h00 de Brasília). Brasília é UTC-3.

mod comum;

use comum::{Celula, Planilha};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    executar_ciclo_aguardando, EtapaCiclo, Evento, EventoCiclo, PastasCiclo, RelatorioAguardar,
};
use rust_spider::publicacao::{Desfecho, Destino, DestinoGit, Preparo};
use rust_spider::{tempo, Result, Resultado};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs;
use std::future::{ready, Ready};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Histórico pequeno: com janela 10 já há features para o snapshot do 13.
const N: u32 = 12;
const SLUGS: [&str; 3] = ["lotofacil", "megasena", "quina"];
const BASE_PADRAO: &str = "2026-10-01";
const TERCA: &str = "2026-10-13";
const QUARTA: &str = "2026-10-14";

const PERFIL_MODELO: &str = r#"nome = "padrao_v1"
versao = 1
loteria = "@@"
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

// ---------------------------------------------------------------------------
// Apoio
// ---------------------------------------------------------------------------

fn git_instalado() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

macro_rules! exige_git {
    () => {
        if !git_instalado() {
            eprintln!("git não instalado: teste pulado");
            return;
        }
    };
}

fn git(pasta: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(pasta)
        .args(args)
        .output()
        .expect("rodar git");
    assert!(
        o.status.success(),
        "git {args:?} falhou: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

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

fn terca_23h() -> u64 {
    brasilia(TERCA, 23, 0)
}

/// Data (`AAAA-MM-DD`) do concurso `c` para a `base` (dias desde 1970).
fn data_do(c: u32, base: i64) -> String {
    tempo::data_iso_de_dias(base + i64::from(c) - 1)
}

/// Histórico sintético determinístico (xorshift64), com datas coerentes.
fn historico(cfg: &LoteriaConfig, n: u32, base: i64) -> Vec<Resultado> {
    let mut x = SEMENTE;
    let faixa: Vec<u32> = cfg.dezenas().collect();
    (1..=n)
        .map(|c| {
            let mut pool = faixa.clone();
            let mut dezenas = Vec::with_capacity(cfg.qtd_dezenas);
            for _ in 0..cfg.qtd_dezenas {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                dezenas.push(pool.swap_remove((x % pool.len() as u64) as usize));
            }
            Resultado {
                loteria: cfg.slug.to_string(),
                concurso: c,
                data_sorteio: data_do(c, base),
                dezenas,
            }
        })
        .collect()
}

/// O que a página mostraria para o concurso `c`.
fn pagina(cfg: &LoteriaConfig, c: u32, base: i64) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso: c,
        data_sorteio: data_do(c, base),
        dezenas: cfg.dezenas().take(cfg.qtd_dezenas).collect(),
    }
}

/// Planilha `.xlsx` com os `resultados` (formato da Caixa) numa pasta.
fn gravar_planilha(pasta: &Path, cfg: &LoteriaConfig, resultados: &[Resultado]) -> PathBuf {
    fn vaza(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }
    let modelo: Planilha = match cfg.slug {
        "lotofacil" => comum::lotofacil(),
        "megasena" => comum::megasena(),
        _ => comum::quina(),
    };
    let linhas: Vec<&'static [Celula]> = resultados
        .iter()
        .map(|r| {
            let d: Vec<&str> = r.data_sorteio.split('-').collect();
            let mut celulas = vec![
                Celula::N(vaza(r.concurso.to_string())),
                Celula::S(vaza(format!("{}/{}/{}", d[2], d[1], d[0]))),
            ];
            celulas.extend(r.dezenas.iter().map(|d| Celula::N(vaza(format!("{d:02}")))));
            &*Box::leak(celulas.into_boxed_slice())
        })
        .collect();
    let planilha = Planilha {
        nome_aba: modelo.nome_aba,
        cabecalho: modelo.cabecalho,
        linhas,
    };
    fs::create_dir_all(pasta).unwrap();
    let caminho = pasta.join(format!("{}.xlsx", cfg.slug));
    fs::write(&caminho, comum::bytes_xlsx(&planilha)).unwrap();
    caminho
}

/// Destino falso: não precisa de git.
#[derive(Default)]
struct DestinoFalso {
    chamadas: Vec<&'static str>,
}

impl Destino for DestinoFalso {
    fn preparar(&mut self) -> Result<Preparo> {
        self.chamadas.push("preparar");
        Ok(Preparo {
            index_atual: None,
            envio_pendente: false,
        })
    }

    fn espelhar(&mut self, _: &Path, _: &[String]) -> Result<()> {
        self.chamadas.push("espelhar");
        Ok(())
    }

    fn registrar(&mut self, _: &str) -> Result<bool> {
        self.chamadas.push("registrar");
        Ok(true)
    }

    fn enviar(&mut self) -> Result<()> {
        self.chamadas.push("enviar");
        Ok(())
    }
}

impl DestinoFalso {
    fn contar(&self, chamada: &str) -> usize {
        self.chamadas.iter().filter(|c| **c == chamada).count()
    }
}

/// Leituras da página por loteria.
#[derive(Clone, Default)]
struct Chamadas(Rc<RefCell<BTreeMap<&'static str, u32>>>);

impl Chamadas {
    fn incrementar(&self, slug: &'static str) -> u32 {
        let mut mapa = self.0.borrow_mut();
        let n = mapa.entry(slug).or_insert(0);
        *n += 1;
        *n
    }

    fn de(&self, slug: &str) -> u32 {
        self.0.borrow().get(slug).copied().unwrap_or(0)
    }
}

type Leitura = Ready<Result<Resultado>>;

/// `ler_pagina` guiado por `regra(loteria, n-ésima leitura) -> concurso (ou
/// erro)`; as leituras ficam contadas por loteria em `Chamadas`.
fn leitor(
    base: i64,
    regra: impl Fn(&'static LoteriaConfig, u32) -> Result<u32> + 'static,
) -> (impl FnMut(&'static LoteriaConfig) -> Leitura, Chamadas) {
    let chamadas = Chamadas::default();
    let guarda = chamadas.clone();
    let f = move |cfg: &'static LoteriaConfig| {
        let n = guarda.incrementar(cfg.slug);
        ready(regra(cfg, n).map(|c| pagina(cfg, c, base)))
    };
    (f, chamadas)
}

fn navegador() -> SpiderError {
    SpiderError::Navegador {
        url: "https://loterias.caixa.gov.br".into(),
        detalhe: "falha ao iniciar o Playwright: Failed to initialize".into(),
    }
}

/// Planilha que nunca deve ser pedida.
fn sem_planilha(_: &'static LoteriaConfig) -> Ready<Result<PathBuf>> {
    panic!("a planilha não devia ser baixada")
}

/// Planilha fixa por loteria; `chamadas` conta os downloads. Se `falhar_ate`
/// for `n`, as `n` primeiras chamadas falham com erro de navegador.
fn planilha_fixa(
    arquivos: Vec<(&'static str, PathBuf)>,
    chamadas: Rc<Cell<u32>>,
    falhar_ate: u32,
) -> impl FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>> {
    move |cfg| {
        chamadas.set(chamadas.get() + 1);
        if chamadas.get() <= falhar_ate {
            return ready(Err(navegador()));
        }
        let (_, caminho) = arquivos
            .iter()
            .find(|(s, _)| *s == cfg.slug)
            .expect("planilha da loteria");
        ready(Ok(caminho.clone()))
    }
}

/// O que uma execução do `--aguardar` produziu.
struct Execucao {
    relatorio: Result<RelatorioAguardar>,
    eventos: Vec<Evento>,
    sleeps: Vec<Duration>,
    /// A cada `dormir`, as loterias que já tinham arquivos em `saida/v1`.
    exportadas_ao_dormir: Vec<Vec<&'static str>>,
}

impl Execucao {
    fn rel(&self) -> &RelatorioAguardar {
        self.relatorio
            .as_ref()
            .unwrap_or_else(|e| panic!("o ciclo devia devolver relatório: {e}"))
    }

    fn minutos(&self) -> Vec<u64> {
        self.sleeps.iter().map(|d| d.as_secs() / 60).collect()
    }

    fn ciclo(&self) -> impl Iterator<Item = &EventoCiclo> {
        self.eventos.iter().filter_map(|e| match e {
            Evento::Ciclo(c) => Some(c),
            _ => None,
        })
    }

    fn contar(&self, f: impl Fn(&EventoCiclo) -> bool) -> usize {
        self.ciclo().filter(|e| f(e)).count()
    }

    fn posicao(&self, f: impl Fn(&EventoCiclo) -> bool) -> usize {
        self.ciclo()
            .position(f)
            .unwrap_or_else(|| panic!("evento esperado ausente em {:#?}", self.eventos))
    }

    /// `(rodada, hora, espera_minutos, pendentes)` de cada `ProximaRodada`.
    fn proximas(&self) -> Vec<(u32, String, u64, Vec<&'static str>)> {
        self.ciclo()
            .filter_map(|e| match e {
                EventoCiclo::ProximaRodada {
                    rodada,
                    hora,
                    espera_minutos,
                    pendentes,
                } => Some((*rodada, hora.clone(), *espera_minutos, pendentes.clone())),
                _ => None,
            })
            .collect()
    }

    fn esperados_em(&self) -> Vec<String> {
        self.ciclo()
            .filter_map(|e| match e {
                EventoCiclo::AguardandoConcurso { esperado_em, .. } => Some(esperado_em.clone()),
                _ => None,
            })
            .collect()
    }

    fn slugs_publicados(&self) -> Vec<Option<&'static str>> {
        self.rel().publicacoes.iter().map(|p| p.slug).collect()
    }

    fn da(&self, slug: &str) -> &rust_spider::orquestracao::ResumoAguardar {
        self.rel()
            .loterias
            .iter()
            .find(|l| l.resumo.slug == slug)
            .expect("loteria no relatório")
    }
}

/// Pastas do ciclo, o "remoto" bare e o clone (quando `com_git`).
struct Amb {
    raiz: PathBuf,
    remoto: PathBuf,
    clone: PathBuf,
    base: i64,
    relogio: Cell<u64>,
    relogio_invalido: Cell<bool>,
    /// Leituras do relógio nesta execução.
    relogio_leituras: Cell<u32>,
    /// Número (a partir de 1) da única leitura que falha; 0 = nenhuma.
    relogio_falha_na: Cell<u32>,
}

impl Drop for Amb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

impl Amb {
    fn novo(com_git: bool) -> Self {
        Self::com_base(com_git, BASE_PADRAO)
    }

    /// `base`: data do concurso 1.
    fn com_base(com_git: bool, base: &str) -> Self {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let raiz = std::env::temp_dir().join(format!(
            "spider_aguardar_{}_{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&raiz);
        fs::create_dir_all(&raiz).unwrap();
        let remoto = raiz.join("rust_spider_dados.git");
        let clone = raiz.join("clone");
        for slug in SLUGS {
            fs::create_dir_all(raiz.join("perfis").join(slug)).unwrap();
            fs::write(
                raiz.join("perfis").join(slug).join("padrao_v1.toml"),
                PERFIL_MODELO.replace("@@", slug),
            )
            .unwrap();
        }
        if com_git {
            git(&raiz, &["init", "-q", "--bare", "rust_spider_dados.git"]);
            git(&remoto, &["symbolic-ref", "HEAD", "refs/heads/main"]);
            git(&remoto, &["config", "core.hooksPath", "hooks"]);
            git(&raiz, &["clone", "-q", "rust_spider_dados.git", "clone"]);
            git(&clone, &["symbolic-ref", "HEAD", "refs/heads/main"]);
            git(&clone, &["config", "user.name", "Teste"]);
            git(&clone, &["config", "user.email", "teste@example.com"]);
            git(&clone, &["config", "commit.gpgsign", "false"]);
            git(&clone, &["config", "core.hooksPath", ".git/hooks"]);
            fs::write(clone.join("LEIAME"), "dados\n").unwrap();
            git(&clone, &["add", "-A"]);
            git(&clone, &["commit", "-q", "-m", "inicio"]);
            git(&clone, &["push", "-q", "origin", "main"]);
        }
        Amb {
            raiz,
            remoto,
            clone,
            base: dia(base),
            relogio: Cell::new(0),
            relogio_invalido: Cell::new(false),
            relogio_leituras: Cell::new(0),
            relogio_falha_na: Cell::new(0),
        }
    }

    fn db(&self, slug: &str) -> PathBuf {
        self.raiz.join(format!("resultados_{slug}.db"))
    }

    fn saida(&self) -> PathBuf {
        self.raiz.join("publico")
    }

    fn historico(&self, slug: &str, n: u32) -> Vec<Resultado> {
        historico(cfg_de(slug), n, self.base)
    }

    /// Banco da loteria com os concursos `1..=n`, menos os `sem`.
    fn semear(&self, slug: &str, n: u32, sem: &[u32]) {
        let lote: Vec<Resultado> = self
            .historico(slug, n)
            .into_iter()
            .filter(|r| !sem.contains(&r.concurso))
            .collect();
        let mut conn = db::abrir(&self.db(slug)).unwrap();
        db::salvar_resultados(&mut conn, &lote).unwrap();
    }

    fn semear_todas(&self, n: u32) {
        for slug in SLUGS {
            self.semear(slug, n, &[]);
        }
    }

    fn concursos(&self, slug: &str) -> Vec<u32> {
        let conn = db::abrir(&self.db(slug)).unwrap();
        let mut c: Vec<u32> = db::listar(&conn, slug)
            .unwrap()
            .iter()
            .map(|r| r.concurso)
            .collect();
        c.sort_unstable();
        c
    }

    fn backups(&self) -> usize {
        fs::read_dir(self.raiz.join("backups")).map_or(0, Iterator::count)
    }

    fn exportou(&self, slug: &str) -> bool {
        self.saida().join("v1").join(slug).exists()
    }

    fn bytes_da_saida(&self) -> Vec<Vec<u8>> {
        SLUGS
            .iter()
            .flat_map(|s| {
                ["ultimo", "tabela_geral", "historico"]
                    .map(|n| fs::read(self.saida().join(format!("v1/{s}/{n}.json"))).unwrap())
            })
            .collect()
    }

    fn commits_no_remoto(&self) -> usize {
        git(&self.remoto, &["rev-list", "--count", "main"])
            .parse()
            .unwrap()
    }

    /// Mensagens dos commits do remoto, do mais novo ao mais antigo.
    fn mensagens_no_remoto(&self) -> Vec<String> {
        git(&self.remoto, &["log", "--format=%s", "main"])
            .lines()
            .map(str::to_string)
            .collect()
    }

    async fn rodar_git<P, F>(&self, quando: u64, slugs: &[&str], ler: P, obter: F) -> Execucao
    where
        P: FnMut(&'static LoteriaConfig) -> Leitura,
        F: FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>>,
    {
        let mut destino = DestinoGit::novo(&self.clone);
        self.rodar(quando, slugs, &mut destino, ler, obter).await
    }

    /// Roda o `--aguardar` das `slugs` com o relógio em `quando` (segundos
    /// Unix); só o `dormir` avança o relógio.
    async fn rodar<P, F>(
        &self,
        quando: u64,
        slugs: &[&str],
        destino: &mut dyn Destino,
        ler: P,
        obter: F,
    ) -> Execucao
    where
        P: FnMut(&'static LoteriaConfig) -> Leitura,
        F: FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>>,
    {
        self.relogio.set(quando);
        self.relogio_leituras.set(0);
        let alvos: Vec<(&'static LoteriaConfig, PathBuf)> =
            slugs.iter().map(|s| (cfg_de(s), self.db(s))).collect();
        let backups = self.raiz.join("backups");
        let cache = self.raiz.join("cache");
        let perfis = self.raiz.join("perfis");
        let saida = self.saida();
        let pastas = PastasCiclo {
            backups: &backups,
            cache: &cache,
            perfis: &perfis,
            saida: &saida,
        };
        let eventos = RefCell::new(Vec::new());
        let sleeps = RefCell::new(Vec::new());
        let exportadas = RefCell::new(Vec::new());
        let relatorio = executar_ciclo_aguardando(
            &alvos,
            &pastas,
            destino,
            &mut |e| eventos.borrow_mut().push(e),
            &mut || {
                self.relogio_leituras.set(self.relogio_leituras.get() + 1);
                if self.relogio_invalido.get()
                    || self.relogio_leituras.get() == self.relogio_falha_na.get()
                {
                    Err(SpiderError::RelogioInvalido)
                } else {
                    Ok(self.relogio.get())
                }
            },
            &mut |d| {
                sleeps.borrow_mut().push(d);
                exportadas.borrow_mut().push(
                    SLUGS
                        .iter()
                        .copied()
                        .filter(|s| self.exportou(s))
                        .collect::<Vec<&'static str>>(),
                );
                self.relogio.set(self.relogio.get() + d.as_secs());
            },
            ler,
            obter,
        )
        .await;
        Execucao {
            relatorio,
            eventos: eventos.into_inner(),
            sleeps: sleeps.into_inner(),
            exportadas_ao_dormir: exportadas.into_inner(),
        }
    }
}

/// Quina em dia até o 12, sozinha, com destino falso; `regra` guia a página.
async fn so_quina(
    a: &Amb,
    quando: u64,
    regra: impl Fn(&'static LoteriaConfig, u32) -> Result<u32> + 'static,
) -> (Execucao, Chamadas, DestinoFalso) {
    a.semear("quina", N, &[]);
    let (ler, chamadas) = leitor(a.base, regra);
    let mut destino = DestinoFalso::default();
    let e = a
        .rodar(quando, &["quina"], &mut destino, ler, sem_planilha)
        .await;
    (e, chamadas, destino)
}

fn hora_de(rodada: u32, proximas: &[(u32, String, u64, Vec<&'static str>)]) -> String {
    proximas
        .iter()
        .find(|p| p.0 == rodada)
        .map(|p| p.1.clone())
        .unwrap_or_else(|| panic!("sem ProximaRodada da rodada {rodada}"))
}

// ---------------------------------------------------------------------------
// 1. Espera só no dia de sorteio
// ---------------------------------------------------------------------------

#[tokio::test]
async fn quina_na_terca_a_noite_espera_o_concurso_que_ainda_nao_saiu() {
    let a = Amb::novo(false);
    // A página mostra o 12 na 1ª leitura e o 13 a partir da 2ª.
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, n| Ok(if n >= 2 { 13 } else { 12 })).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    // Uma espera de 5 min (1º intervalo) e duas rodadas.
    assert_eq!(e.minutos(), [5]);
    assert_eq!(r.rodadas, 2);
    assert_eq!(chamadas.de("quina"), 2);
    assert_eq!(r.loterias[0].tentativas, 2);
    assert_eq!(r.loterias[0].concurso_ausente, None);
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert_eq!(
        e.esperados_em(),
        [TERCA],
        "uma rodada aguardando, a data é a do sorteio"
    );
}

#[tokio::test]
async fn sabado_sem_sorteio_nao_dorme_e_faz_uma_rodada() {
    // Base 2026-10-05: o concurso 12 cai em 2026-10-16 (sexta). A quina não
    // sorteia sábado; o próximo sorteio (domingo 18, 21h00) ainda não chegou.
    let a = Amb::com_base(false, "2026-10-05");
    assert_eq!(data_do(N, a.base), "2026-10-16");
    let (e, chamadas, _) = so_quina(&a, brasilia("2026-10-17", 23, 0), |_, _| Ok(12)).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert!(e.sleeps.is_empty());
    assert_eq!(r.rodadas, 1);
    assert_eq!(chamadas.de("quina"), 1);
    assert_eq!(r.avisos().count(), 0);
    assert!(r.loterias[0].resumo.exportada);
}

#[tokio::test]
async fn loteria_ja_em_dia_nao_dorme() {
    let a = Amb::novo(false);
    // A página já traz o concurso do dia (13, terça).
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, _| Ok(13)).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert!(e.sleeps.is_empty());
    assert_eq!(r.rodadas, 1);
    assert_eq!(chamadas.de("quina"), 1);
    assert_eq!(r.loterias[0].tentativas, 1);
    assert_eq!(r.loterias[0].resumo.ultimo_concurso, Some(13));
    assert!(r.loterias[0].resumo.concurso_novo);
    assert_eq!(r.loterias[0].concurso_ausente, None);
}

#[tokio::test]
async fn execucao_as_10h_nao_dorme_e_termina_com_aviso() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    // Quarta 10h00: as três deviam ter o sorteio de 13/10 (D2 só às 21h00 de
    // quarta/quinta), mas a janela só existe a até 4 h do 01h00.
    let quando = brasilia(QUARTA, 10, 0);
    let (ler, _) = leitor(a.base, |_, _| Ok(12));
    let mut destino = DestinoFalso::default();
    let e = a
        .rodar(quando, &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert!(e.sleeps.is_empty(), "não dorme fora da noite");
    assert_eq!(r.rodadas, 1);
    assert_eq!(i64::try_from(quando).unwrap(), r.janela_ate, "sem janela");
    assert_eq!(r.avisos().count(), 3);
    assert!(r
        .loterias
        .iter()
        .all(|l| l.concurso_ausente.as_deref() == Some(TERCA)));
    assert!(r.loterias.iter().all(|l| l.resumo.exportada));
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::JanelaEncerrada { .. })),
        1
    );
}

#[tokio::test]
async fn execucao_as_02h_nao_dorme_e_termina_com_aviso() {
    let a = Amb::novo(false);
    // Quarta 02h00: já passou do 01h00 e o próximo está a 23 h.
    let (e, _, _) = so_quina(&a, brasilia(QUARTA, 2, 0), |_, _| Ok(12)).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert!(e.sleeps.is_empty());
    assert_eq!(r.rodadas, 1);
    assert_eq!(r.loterias[0].concurso_ausente.as_deref(), Some(TERCA));
    assert!(r.loterias[0].resumo.exportada);
}

// ---------------------------------------------------------------------------
// 2 e 3. Intervalos e corte em 01h00
// ---------------------------------------------------------------------------

#[tokio::test]
async fn intervalos_de_5_10_20_30_30_e_horas_das_proximas_rodadas() {
    let a = Amb::novo(false);
    let (e, _, _) = so_quina(&a, terca_23h(), |_, _| Ok(12)).await;

    // 23:00 -> 23:05 (5) -> 23:15 (10) -> 23:35 (20) -> 00:05 (30) -> 00:35 (30)
    // -> 01:00 (só 25, cortado no limite).
    assert_eq!(e.minutos(), [5, 10, 20, 30, 30, 25]);
    let proximas = e.proximas();
    let horas: Vec<&str> = proximas.iter().map(|p| p.1.as_str()).collect();
    assert_eq!(
        horas,
        ["23:05", "23:15", "23:35", "00:05", "00:35", "01:00"]
    );
    let rodadas: Vec<u32> = proximas.iter().map(|p| p.0).collect();
    assert_eq!(rodadas, [2, 3, 4, 5, 6, 7]);
    let esperas: Vec<u64> = proximas.iter().map(|p| p.2).collect();
    assert_eq!(esperas, [5, 10, 20, 30, 30, 25]);
    assert!(proximas.iter().all(|p| p.3 == ["quina"]));
}

#[tokio::test]
async fn janela_corta_em_01h00_com_sete_rodadas_e_seis_sonos() {
    let a = Amb::novo(false);
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, _| Ok(12)).await;

    let r = e.rel();
    assert_eq!(r.rodadas, 7, "a rodada do 01h00 ainda acontece");
    assert_eq!(e.sleeps.len(), 6);
    assert_eq!(chamadas.de("quina"), 7, "nenhuma leitura depois do 01h00");
    assert_eq!(r.loterias[0].tentativas, 7);
    let limite = brasilia(QUARTA, 1, 0);
    assert_eq!(
        a.relogio.get(),
        limite,
        "o relógio parou exatamente no 01h00"
    );
    assert_eq!(r.janela_ate, i64::try_from(limite).unwrap());
    // A soma dos sonos é o tempo entre 23h00 e 01h00 (2 h).
    let total: u64 = e.sleeps.iter().map(Duration::as_secs).sum();
    assert_eq!(total, 2 * 3600);
}

#[tokio::test]
async fn concurso_que_nao_saiu_exporta_publica_e_avisa_sem_falhar() {
    let a = Amb::novo(false);
    let (e, _, destino) = so_quina(&a, terca_23h(), |_, _| Ok(12)).await;

    let r = e.rel();
    assert!(r.sucesso(), "aviso não é falha: {r:?}");
    assert_eq!(r.falhas().count(), 0);
    assert_eq!(r.avisos().count(), 1);
    // O sorteio esperado era o da terça 13/10, que não saiu.
    assert_eq!(r.loterias[0].concurso_ausente.as_deref(), Some(TERCA));
    assert!(r.loterias[0].resumo.ok());
    assert!(r.loterias[0].resumo.exportada, "o resto do ciclo roda");
    assert!(a.exportou("quina"));
    assert_eq!(a.concursos("quina"), (1..=12).collect::<Vec<_>>());
    // Publicada uma vez, com resultado presente e Ok.
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
    assert!(matches!(
        r.publicacoes[0].resultado,
        Ok(Desfecho::Publicado { .. })
    ));
    assert_eq!(destino.contar("registrar"), 1);

    let janela = e.posicao(|c| matches!(c, EventoCiclo::JanelaEncerrada { .. }));
    let nao_saiu = e.posicao(|c| {
        matches!(c, EventoCiclo::ConcursoNaoSaiu { slug: "quina", esperado_em } if esperado_em == TERCA)
    });
    let publicando = e.posicao(|c| matches!(c, EventoCiclo::PublicandoLoteria { slug: "quina" }));
    assert!(janela < nao_saiu && nao_saiu < publicando);
    assert!(e.ciclo().any(|c| matches!(
        c,
        EventoCiclo::JanelaEncerrada { hora, pendentes } if hora == "01:00" && pendentes == &["quina"]
    )));
}

// ---------------------------------------------------------------------------
// 4. Travessia da meia-noite
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurso_que_sai_as_00h35_e_pego_exportado_e_publicado() {
    let a = Amb::novo(false);
    // 6ª leitura = rodada das 00:35 (23:00, 23:05, 23:15, 23:35, 00:05, 00:35).
    let (e, chamadas, destino) =
        so_quina(&a, terca_23h(), |_, n| Ok(if n >= 6 { 13 } else { 12 })).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5, 10, 20, 30, 30]);
    assert_eq!(r.rodadas, 6);
    assert_eq!(chamadas.de("quina"), 6);
    assert_eq!(a.relogio.get(), brasilia(QUARTA, 0, 35));
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert!(r.loterias[0].resumo.exportada);
    assert_eq!(r.loterias[0].concurso_ausente, None);
    assert_eq!(r.avisos().count(), 0);
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
    assert_eq!(destino.contar("registrar"), 1);
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::ConcursoNaoSaiu { .. })),
        0
    );
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::JanelaEncerrada { .. })),
        0
    );
}

#[tokio::test]
async fn depois_da_meia_noite_o_esperado_em_continua_sendo_a_vespera() {
    let a = Amb::novo(false);
    let (e, _, _) = so_quina(&a, terca_23h(), |_, n| Ok(if n >= 6 { 13 } else { 12 })).await;

    // Rodadas 1 a 5 aguardam; 4 delas antes e 1 (00:05) depois da meia-noite;
    // todas dizem "2026-10-13", nunca "2026-10-14".
    assert_eq!(e.esperados_em(), [TERCA; 5]);
}

#[tokio::test]
async fn aviso_final_depois_da_meia_noite_tambem_aponta_a_vespera() {
    let a = Amb::novo(false);
    let (e, _, _) = so_quina(&a, terca_23h(), |_, _| Ok(12)).await;

    let esperados = e.esperados_em();
    assert_eq!(esperados.len(), 7);
    assert!(esperados.iter().all(|d| d == TERCA), "{esperados:?}");
    assert_eq!(e.rel().loterias[0].concurso_ausente.as_deref(), Some(TERCA));
}

#[tokio::test]
async fn concurso_que_sai_na_rodada_exata_do_01h00_ainda_e_pego() {
    let a = Amb::novo(false);
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, n| Ok(if n >= 7 { 13 } else { 12 })).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(r.rodadas, 7);
    assert_eq!(chamadas.de("quina"), 7);
    assert_eq!(a.relogio.get(), brasilia(QUARTA, 1, 0));
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert_eq!(r.loterias[0].concurso_ausente, None);
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::ConcursoNaoSaiu { .. })),
        0
    );
}

// ---------------------------------------------------------------------------
// 5. Publicar por loteria que chega
// ---------------------------------------------------------------------------

/// quina na 1ª leitura, lotofacil na 2ª, megasena na 3ª.
fn chegada_escalonada(cfg: &'static LoteriaConfig, n: u32) -> Result<u32> {
    let chega_na = match cfg.slug {
        "quina" => 1,
        "lotofacil" => 2,
        _ => 3,
    };
    Ok(if n >= chega_na { 13 } else { 12 })
}

#[tokio::test]
async fn cada_loteria_e_exportada_e_publicada_na_rodada_em_que_chega() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let (ler, chamadas) = leitor(a.base, chegada_escalonada);
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(terca_23h(), &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5, 10]);
    assert_eq!(r.rodadas, 3);
    assert_eq!(
        e.slugs_publicados(),
        [Some("quina"), Some("lotofacil"), Some("megasena")]
    );
    assert_eq!(destino.contar("registrar"), 3);
    assert_eq!(destino.contar("enviar"), 3);
    // Cada loteria é lida até chegar e não mais.
    assert_eq!(chamadas.de("quina"), 1);
    assert_eq!(chamadas.de("lotofacil"), 2);
    assert_eq!(chamadas.de("megasena"), 3);
    // Enquanto espera, nada da loteria foi exportado: ao dormir só existe o
    // que já chegou.
    assert_eq!(
        e.exportadas_ao_dormir,
        [vec!["quina"], vec!["lotofacil", "quina"]]
    );
    let proximas = e.proximas();
    assert_eq!(proximas[0].3, ["lotofacil", "megasena"]);
    assert_eq!(proximas[1].3, ["megasena"]);
    assert!(a.exportou("megasena"), "e no fim tudo foi exportado");
}

#[tokio::test]
async fn publicacao_de_cada_loteria_vem_antes_da_rodada_seguinte() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let (ler, _) = leitor(a.base, chegada_escalonada);
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(terca_23h(), &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let rodada = |n: u32| {
        e.posicao(|c| matches!(c, EventoCiclo::ProximaRodada { rodada, .. } if *rodada == n))
    };
    let publicando = |s: &'static str| {
        e.posicao(move |c| matches!(c, EventoCiclo::PublicandoLoteria { slug } if *slug == s))
    };
    assert!(publicando("quina") < rodada(2));
    assert!(rodada(2) < publicando("lotofacil") && publicando("lotofacil") < rodada(3));
    assert!(rodada(3) < publicando("megasena"));
}

#[tokio::test]
async fn publicacao_git_gera_um_commit_por_loteria_que_chega() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    let (ler, _) = leitor(a.base, chegada_escalonada);

    let e = a.rodar_git(terca_23h(), &SLUGS, ler, sem_planilha).await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(a.commits_no_remoto(), 1 + 3);
    assert_eq!(
        a.mensagens_no_remoto(),
        [
            "Dados: lotofacil 13, megasena 13, quina 13",
            "Dados: lotofacil 13, quina 13",
            "Dados: quina 13",
            "inicio",
        ]
    );
    for p in &r.publicacoes {
        assert!(
            matches!(p.resultado, Ok(Desfecho::Publicado { .. })),
            "{p:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 6. Falha definitiva não é repetida
// ---------------------------------------------------------------------------

#[tokio::test]
async fn falha_definitiva_na_pagina_nao_e_repetida_e_as_outras_seguem() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let (ler, chamadas) = leitor(a.base, |cfg, _| {
        if cfg.slug == "megasena" {
            Err(SpiderError::LinhaInvalida("dezenas fora da faixa".into()))
        } else {
            Ok(13)
        }
    });
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(terca_23h(), &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let r = e.rel();
    assert!(!r.sucesso());
    assert_eq!(r.rodadas, 1);
    assert!(e.sleeps.is_empty());
    assert_eq!(chamadas.de("megasena"), 1);
    let falhas: Vec<&str> = r.falhas().map(|l| l.resumo.slug).collect();
    assert_eq!(falhas, ["megasena"]);
    let falha = e.da("megasena").resumo.falha.as_ref().unwrap();
    assert_eq!(falha.etapa, EtapaCiclo::Atualizar);
    assert!(matches!(falha.erro, SpiderError::LinhaInvalida(_)));
    assert_eq!(e.da("megasena").tentativas, 1);
    assert_eq!(e.slugs_publicados(), [Some("lotofacil"), Some("quina")]);
    assert!(!a.exportou("megasena"));
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::FalhaTransitoria { .. })),
        0
    );
    assert_eq!(
        e.contar(|c| matches!(
            c,
            EventoCiclo::LoteriaFalhou {
                slug: "megasena",
                ..
            }
        )),
        1
    );
}

#[tokio::test]
async fn falha_definitiva_nao_volta_nas_rodadas_seguintes() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    // A lotofacil nunca sai (mantém o laço rodando); a megasena falha de vez.
    let (ler, chamadas) = leitor(a.base, |cfg, _| match cfg.slug {
        "megasena" => Err(SpiderError::LinhaInvalida("dado ruim".into())),
        "lotofacil" => Ok(12),
        _ => Ok(13),
    });
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(terca_23h(), &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let r = e.rel();
    assert_eq!(r.rodadas, 7);
    assert_eq!(chamadas.de("lotofacil"), 7);
    assert_eq!(
        chamadas.de("megasena"),
        1,
        "falha definitiva não é repetida"
    );
    assert_eq!(chamadas.de("quina"), 1);
    assert!(e.proximas().iter().all(|p| p.3 == ["lotofacil"]));
}

#[tokio::test]
async fn buraco_que_persiste_reprova_na_validacao_e_nao_e_repetido() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[5]);
    let sem_5: Vec<Resultado> = a
        .historico("quina", N)
        .into_iter()
        .filter(|r| r.concurso != 5)
        .collect();
    let planilha = gravar_planilha(&a.raiz.join("downloads"), cfg_de("quina"), &sem_5);
    let downloads = Rc::new(Cell::new(0));
    let (ler, chamadas) = leitor(a.base, |_, _| Ok(13));
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(
            terca_23h(),
            &["quina"],
            &mut destino,
            ler,
            planilha_fixa(vec![("quina", planilha)], downloads.clone(), 0),
        )
        .await;

    let r = e.rel();
    assert!(!r.sucesso());
    assert_eq!(r.rodadas, 1);
    assert!(e.sleeps.is_empty());
    assert_eq!(chamadas.de("quina"), 1);
    assert_eq!(downloads.get(), 1);
    let falha = e.da("quina").resumo.falha.as_ref().unwrap();
    assert_eq!(falha.etapa, EtapaCiclo::Validar);
    assert!(matches!(
        falha.erro,
        SpiderError::ResultadosReprovados { .. }
    ));
    assert!(!a.exportou("quina"));
}

// ---------------------------------------------------------------------------
// 7. Falha transitória é repetida
// ---------------------------------------------------------------------------

#[tokio::test]
async fn falha_transitoria_e_repetida_ate_dar_certo() {
    let a = Amb::novo(false);
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, n| {
        if n <= 2 {
            Err(navegador())
        } else {
            Ok(13)
        }
    })
    .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5, 10]);
    assert_eq!(r.rodadas, 3);
    assert_eq!(chamadas.de("quina"), 3);
    assert_eq!(r.loterias[0].tentativas, 3);
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert!(r.loterias[0].resumo.exportada);
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
    let mensagens: Vec<&String> = e
        .ciclo()
        .filter_map(|c| match c {
            EventoCiclo::FalhaTransitoria {
                slug: "quina",
                etapa: EtapaCiclo::Atualizar,
                mensagem,
            } => Some(mensagem),
            _ => None,
        })
        .collect();
    assert_eq!(mensagens.len(), 2);
    assert!(mensagens.iter().all(|m| m.contains("Failed to initialize")));
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::LoteriaFalhou { .. })),
        0
    );
}

#[tokio::test]
async fn falha_transitoria_persistente_falha_na_etapa_atualizar_ao_fim_da_janela() {
    let a = Amb::novo(false);
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, _| Err(navegador())).await;

    let r = e.rel();
    assert!(!r.sucesso());
    assert_eq!(r.falhas().count(), 1);
    let falha = e.da("quina").resumo.falha.as_ref().unwrap();
    assert_eq!(falha.etapa, EtapaCiclo::Atualizar);
    assert!(matches!(falha.erro, SpiderError::Navegador { .. }));
    assert_eq!(r.rodadas, 7);
    assert_eq!(chamadas.de("quina"), r.rodadas, "uma leitura por rodada");
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::FalhaTransitoria { .. })),
        7
    );
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::JanelaEncerrada { .. })),
        1
    );
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::LoteriaFalhou { slug: "quina", .. })),
        1
    );
    assert_eq!(r.avisos().count(), 0, "falha não é concurso ausente");
    assert!(!a.exportou("quina"));
    assert_eq!(a.concursos("quina"), (1..=12).collect::<Vec<_>>());
}

#[tokio::test]
async fn sem_nada_publicado_o_publicar_final_de_reenvio_ainda_roda() {
    let a = Amb::novo(false);
    let (e, _, _) = so_quina(&a, terca_23h(), |_, _| Err(navegador())).await;

    // Uma única publicação, sem loteria (reenvio), e o evento correspondente.
    assert_eq!(e.slugs_publicados(), [None]);
    assert_eq!(e.contar(|c| matches!(c, EventoCiclo::Publicando)), 1);
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::PublicandoLoteria { .. })),
        0
    );
}

#[tokio::test]
async fn publicar_final_nao_roda_se_alguma_loteria_foi_publicada() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let (ler, _) = leitor(a.base, |cfg, _| {
        if cfg.slug == "quina" {
            Err(navegador())
        } else {
            Ok(13)
        }
    });
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(
            terca_23h(),
            &["lotofacil", "quina"],
            &mut destino,
            ler,
            sem_planilha,
        )
        .await;

    let r = e.rel();
    assert!(!r.sucesso());
    assert_eq!(e.slugs_publicados(), [Some("lotofacil")]);
    assert_eq!(e.contar(|c| matches!(c, EventoCiclo::Publicando)), 0);
    assert_eq!(destino.contar("registrar"), 1);
    assert_eq!(r.rodadas, 7, "a quina insistiu até o fim da janela");
}

// ---------------------------------------------------------------------------
// 8. Buraco persistente baixado uma vez só
// ---------------------------------------------------------------------------

#[tokio::test]
async fn buraco_persistente_baixa_a_planilha_uma_vez_enquanto_outra_loteria_repete() {
    let a = Amb::novo(false);
    a.semear("lotofacil", N, &[]);
    a.semear("quina", N, &[5]);
    let sem_5: Vec<Resultado> = a
        .historico("quina", N)
        .into_iter()
        .filter(|r| r.concurso != 5)
        .collect();
    let planilha = gravar_planilha(&a.raiz.join("downloads"), cfg_de("quina"), &sem_5);
    let downloads = Rc::new(Cell::new(0));
    // A lotofacil falha 3 vezes (rodadas 1 a 3) e chega na 4ª.
    let (ler, chamadas) = leitor(a.base, |cfg, n| {
        if cfg.slug == "lotofacil" && n <= 3 {
            Err(navegador())
        } else {
            Ok(13)
        }
    });
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(
            terca_23h(),
            &["lotofacil", "quina"],
            &mut destino,
            ler,
            planilha_fixa(vec![("quina", planilha)], downloads.clone(), 0),
        )
        .await;

    let r = e.rel();
    assert_eq!(e.minutos(), [5, 10, 20]);
    assert_eq!(r.rodadas, 4);
    assert_eq!(chamadas.de("lotofacil"), 4);
    assert_eq!(chamadas.de("quina"), 1, "reprovada na 1ª rodada, não volta");
    assert_eq!(
        downloads.get(),
        1,
        "a planilha da quina foi baixada uma vez"
    );
    assert_eq!(a.backups(), 1);
    let falha = e.da("quina").resumo.falha.as_ref().unwrap();
    assert_eq!(falha.etapa, EtapaCiclo::Validar);
    assert_eq!(e.da("quina").tentativas, 1);
    assert!(e.da("lotofacil").resumo.ok());
    assert_eq!(e.slugs_publicados(), [Some("lotofacil")]);
    assert!(!r.sucesso());
}

#[tokio::test]
async fn download_do_backfill_que_falha_e_retomado_sem_reler_a_pagina() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[5]);
    // Planilha completa até o 12; o concurso 13 vem da página.
    let planilha = gravar_planilha(
        &a.raiz.join("downloads"),
        cfg_de("quina"),
        &a.historico("quina", N),
    );
    let downloads = Rc::new(Cell::new(0));
    let (ler, chamadas) = leitor(a.base, |_, _| Ok(13));
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(
            terca_23h(),
            &["quina"],
            &mut destino,
            ler,
            // O 1º download falha com erro de navegador; o 2º passa.
            planilha_fixa(vec![("quina", planilha)], downloads.clone(), 1),
        )
        .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5]);
    assert_eq!(r.rodadas, 2);
    assert_eq!(chamadas.de("quina"), 1, "a rodada 2 retoma do backfill");
    assert_eq!(downloads.get(), 2, "uma falha e um sucesso");
    assert_eq!(a.backups(), 1, "o backup não se repete");
    assert_eq!(a.concursos("quina"), (1..=13).collect::<Vec<_>>());
    assert!(r.loterias[0].resumo.backfill);
    assert!(r.loterias[0].resumo.exportada);
    assert_eq!(r.loterias[0].tentativas, 2);
    let transitorias: Vec<EtapaCiclo> = e
        .ciclo()
        .filter_map(|c| match c {
            EventoCiclo::FalhaTransitoria { etapa, .. } => Some(*etapa),
            _ => None,
        })
        .collect();
    assert_eq!(transitorias, [EtapaCiclo::Backfill]);
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
}

// ---------------------------------------------------------------------------
// 9. Idempotência
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rodar_de_novo_depois_de_concluido_nao_muda_nada() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    let primeira = {
        let (ler, _) = leitor(a.base, |_, _| Ok(13));
        a.rodar_git(terca_23h(), &SLUGS, ler, sem_planilha).await
    };
    assert!(primeira.rel().sucesso(), "{:?}", primeira.relatorio);
    let commits = a.commits_no_remoto();
    let bytes_antes = a.bytes_da_saida();
    let bancos_antes: Vec<Vec<u32>> = SLUGS.iter().map(|s| a.concursos(s)).collect();

    // O concurso 13 já está no banco e a hora é de novo 23h00.
    let (ler, _) = leitor(a.base, |_, _| Ok(13));
    let segunda = a.rodar_git(terca_23h(), &SLUGS, ler, sem_planilha).await;

    let r = segunda.rel();
    assert!(r.sucesso(), "{r:?}");
    assert!(segunda.sleeps.is_empty());
    assert_eq!(r.rodadas, 1);
    assert!(r.loterias.iter().all(|l| !l.resumo.concurso_novo));
    assert!(r.publicacoes.len() == 3);
    for p in &r.publicacoes {
        assert!(
            matches!(p.resultado, Ok(Desfecho::NadaAPublicar)),
            "{:?}",
            p.resultado
        );
    }
    assert_eq!(a.commits_no_remoto(), commits);
    assert_eq!(git(&a.clone, &["status", "--porcelain"]), "");
    assert_eq!(a.bytes_da_saida(), bytes_antes, "mesmos bytes");
    let bancos_depois: Vec<Vec<u32>> = SLUGS.iter().map(|s| a.concursos(s)).collect();
    assert_eq!(bancos_antes, bancos_depois);
}

// ---------------------------------------------------------------------------
// 10. Relógio inválido
// ---------------------------------------------------------------------------

#[tokio::test]
async fn relogio_invalido_na_largada_e_erro_sem_tocar_em_nada() {
    let a = Amb::novo(false);
    a.relogio_invalido.set(true);
    let (e, chamadas, destino) = so_quina(&a, terca_23h(), |_, _| Ok(13)).await;

    assert!(
        matches!(e.relatorio, Err(SpiderError::RelogioInvalido)),
        "{:?}",
        e.relatorio
    );
    assert_eq!(chamadas.de("quina"), 0, "nem leu a página");
    assert!(e.sleeps.is_empty());
    assert!(destino.chamadas.is_empty());
    assert_eq!(a.concursos("quina"), (1..=12).collect::<Vec<_>>());
}

// ---------------------------------------------------------------------------
// 11. Todas falham de vez
// ---------------------------------------------------------------------------

#[tokio::test]
async fn todas_falhando_de_vez_fazem_um_unico_publicar_de_reenvio() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    // Uma execução anterior deixou a saída exportada: o publicar final tem o
    // que enviar.
    let (ler, _) = leitor(a.base, |_, _| Ok(13));
    let mut anterior = DestinoFalso::default();
    let primeira = a
        .rodar(terca_23h(), &SLUGS, &mut anterior, ler, sem_planilha)
        .await;
    assert!(primeira.rel().sucesso());

    let (ler, chamadas) = leitor(a.base, |_, _| {
        Err(SpiderError::LinhaInvalida("dado ruim".into()))
    });
    let mut destino = DestinoFalso::default();
    let e = a
        .rodar(terca_23h(), &SLUGS, &mut destino, ler, sem_planilha)
        .await;

    let r = e.rel();
    assert!(!r.sucesso());
    assert_eq!(r.rodadas, 1);
    assert!(e.sleeps.is_empty());
    assert_eq!(r.falhas().count(), 3);
    for slug in SLUGS {
        assert_eq!(chamadas.de(slug), 1);
    }
    assert_eq!(e.slugs_publicados(), [None]);
    assert!(r.publicacoes[0].resultado.is_ok());
    assert_eq!(destino.contar("preparar"), 1);
    assert_eq!(e.contar(|c| matches!(c, EventoCiclo::Publicando)), 1);
}

#[tokio::test]
async fn hora_da_proxima_rodada_sai_no_texto_do_evento() {
    let a = Amb::novo(false);
    let (e, _, _) = so_quina(&a, terca_23h(), |_, n| Ok(if n >= 2 { 13 } else { 12 })).await;

    let proximas = e.proximas();
    assert_eq!(hora_de(2, &proximas), "23:05");
    let texto = e
        .ciclo()
        .find(|c| matches!(c, EventoCiclo::ProximaRodada { .. }))
        .unwrap()
        .to_string();
    assert!(
        texto.contains("23:05") && texto.contains("quina"),
        "{texto}"
    );
}

// ---------------------------------------------------------------------------
// 12. Rodada de correção da 3.3: página parcial, backup do salto, relógio
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pagina_parcial_numa_rodada_e_repetida_ate_a_pagina_completa() {
    let a = Amb::novo(false);
    // Às 23h a Caixa mostra o cabeçalho novo antes das dezenas: 2 leituras
    // parciais e a 3ª completa.
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, n| {
        if n <= 2 {
            Err(SpiderError::PaginaIncompleta(
                "esperava 5 dezenas no seletor `x`, a página retornou 0".into(),
            ))
        } else {
            Ok(13)
        }
    })
    .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5, 10]);
    assert_eq!(chamadas.de("quina"), 3);
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::FalhaTransitoria { .. })),
        2
    );
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::LoteriaFalhou { .. })),
        0
    );
}

#[tokio::test]
async fn salto_com_download_transitorio_faz_um_unico_backup() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[]);
    // A página já mostra o 15 (salto: faltam 13 e 14); a planilha completa
    // existe, mas os 2 primeiros downloads falham de passagem.
    let planilha = gravar_planilha(
        &a.raiz.join("downloads"),
        cfg_de("quina"),
        &a.historico("quina", 15),
    );
    let downloads = Rc::new(Cell::new(0));
    let (ler, chamadas) = leitor(a.base, |_, _| Ok(15));
    let mut destino = DestinoFalso::default();

    let e = a
        .rodar(
            terca_23h(),
            &["quina"],
            &mut destino,
            ler,
            planilha_fixa(vec![("quina", planilha)], downloads.clone(), 2),
        )
        .await;

    let r = e.rel();
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(e.minutos(), [5, 10]);
    assert_eq!(r.rodadas, 3);
    assert_eq!(chamadas.de("quina"), 3, "o salto refaz a leitura da página");
    assert_eq!(downloads.get(), 3, "duas falhas e um sucesso");
    assert_eq!(a.backups(), 1, "um backup por loteria por execução");
    assert_eq!(
        e.eventos
            .iter()
            .filter(|ev| matches!(ev, Evento::BackupCriado { .. }))
            .count(),
        1
    );
    assert_eq!(a.concursos("quina"), (1..=15).collect::<Vec<_>>());
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
}

#[tokio::test]
async fn relogio_que_falha_depois_da_largada_fecha_a_janela_sem_dormir() {
    let a = Amb::novo(false);
    // Leituras: 1 = largada, 2 = conferência do calendário, 3 = decisão de
    // dormir (falha). O relógio volta na leitura seguinte.
    a.relogio_falha_na.set(3);
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, _| Ok(12)).await;

    let r = e.rel();
    assert!(e.sleeps.is_empty(), "sem hora não há como esperar");
    assert_eq!(r.rodadas, 1);
    assert_eq!(chamadas.de("quina"), 1);
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::JanelaEncerrada { .. })),
        1
    );
    assert_eq!(
        e.contar(|c| matches!(c, EventoCiclo::ProximaRodada { .. })),
        0
    );
    // A loteria que aguardava termina como as outras no fim da janela: resto
    // do ciclo e aviso de concurso ausente.
    assert!(r.sucesso(), "{r:?}");
    assert!(e.da("quina").concurso_ausente.is_some());
    assert_eq!(e.slugs_publicados(), [Some("quina")]);
}

#[tokio::test]
async fn aguardou_a_noite_toda_e_falha_transitoria_as_01h_termina_como_falha() {
    let a = Amb::novo(false);
    // Rodadas 1 a 6 veem o concurso anterior; a 7ª (01h00) cai no navegador.
    let (e, chamadas, _) = so_quina(&a, terca_23h(), |_, n| {
        if n <= 6 {
            Ok(12)
        } else {
            Err(navegador())
        }
    })
    .await;

    let r = e.rel();
    assert_eq!(r.rodadas, 7);
    assert_eq!(chamadas.de("quina"), 7);
    assert!(!r.sucesso(), "falha transitória na última rodada é falha");
    let falha = e.da("quina").resumo.falha.as_ref().unwrap();
    assert_eq!(falha.etapa, EtapaCiclo::Atualizar);
    assert!(matches!(falha.erro, SpiderError::Navegador { .. }));
    assert!(e.da("quina").concurso_ausente.is_none(), "não é só aviso");
    assert!(!a.exportou("quina"));
    assert_eq!(e.slugs_publicados(), [None]);
}
