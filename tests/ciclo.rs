//! Testes do `sync ciclo` (3.1b) sem rede: a leitura da página e a planilha
//! são falsas, o relógio é injetado, o destino é um repositório "remoto" bare
//! local (testes com `git`, pulados se ele não estiver instalado) ou um
//! destino falso. Os bancos e as pastas ficam em diretórios temporários; nada
//! toca nos bancos reais nem em `../rust_spider_dados`.

mod comum;

use comum::{Celula, Planilha};
use rust_spider::config::{self, LoteriaConfig};
use rust_spider::db;
use rust_spider::error::SpiderError;
use rust_spider::orquestracao::{
    executar_ciclo, listar_pendentes, noites_do_ciclo, registrar_noite, EtapaCiclo, Evento,
    EventoCiclo, PastasAlertas, PastasCiclo, RelatorioCiclo,
};
use rust_spider::publicacao::{Desfecho, Destino, DestinoGit, Preparo};
use rust_spider::{Result, Resultado};
use std::cell::{Cell, RefCell};
use std::fs;
use std::future::{ready, Ready};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

const SEMENTE: u64 = 0x2545_F491_4F6C_DD1D;
/// Histórico pequeno: com janela 10 já há features para o snapshot do 13.
const N: u32 = 12;
const SLUGS: [&str; 3] = ["lotofacil", "megasena", "quina"];
/// 2026-10-13T20:00:00Z.
const RELOGIO_INICIAL: u64 = 1_791_921_600;

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

/// Histórico sintético determinístico (xorshift64), como em `tests/exportar.rs`.
fn historico(cfg: &LoteriaConfig, n: u32) -> Vec<Resultado> {
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
                data_sorteio: format!("2020-01-{:02}", 1 + c % 28),
                dezenas,
            }
        })
        .collect()
}

/// O que a página mostraria para o concurso `c` (válido para a loteria).
fn pagina(cfg: &LoteriaConfig, c: u32) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso: c,
        data_sorteio: format!("2020-01-{:02}", 1 + c % 28),
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

/// Pastas do ciclo, o "remoto" bare e o clone (quando `com_git`).
struct Amb {
    raiz: PathBuf,
    remoto: PathBuf,
    clone: PathBuf,
    relogio: Cell<u64>,
}

impl Drop for Amb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

impl Amb {
    fn novo(com_git: bool) -> Self {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let raiz = std::env::temp_dir().join(format!(
            "spider_ciclo_{}_{}",
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
            relogio: Cell::new(RELOGIO_INICIAL),
        }
    }

    fn db(&self, slug: &str) -> PathBuf {
        self.raiz.join(format!("resultados_{slug}.db"))
    }

    fn saida(&self) -> PathBuf {
        self.raiz.join("publico")
    }

    /// Banco da loteria com os concursos `1..=n`, menos os `sem`.
    fn semear(&self, slug: &str, n: u32, sem: &[u32]) {
        let cfg = cfg_de(slug);
        let lote: Vec<Resultado> = historico(cfg, n)
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

    fn commits_no_remoto(&self) -> usize {
        git(&self.remoto, &["rev-list", "--count", "main"])
            .parse()
            .unwrap()
    }

    fn mensagem_do_ultimo_commit(&self) -> String {
        git(&self.remoto, &["log", "-1", "--format=%s", "main"])
    }

    /// Roda o ciclo das `slugs` com o destino git (o clone).
    async fn rodar_git<P, F>(
        &self,
        slugs: &[&str],
        ler: P,
        obter: F,
    ) -> (RelatorioCiclo, Vec<Evento>)
    where
        P: FnMut(&'static LoteriaConfig) -> Ready<Result<Resultado>>,
        F: FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>>,
    {
        let mut destino = DestinoGit::novo(&self.clone);
        self.rodar(slugs, &mut destino, ler, obter).await
    }

    async fn rodar<P, F>(
        &self,
        slugs: &[&str],
        destino: &mut dyn Destino,
        ler: P,
        obter: F,
    ) -> (RelatorioCiclo, Vec<Evento>)
    where
        P: FnMut(&'static LoteriaConfig) -> Ready<Result<Resultado>>,
        F: FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>>,
    {
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
        let relatorio = executar_ciclo(
            &alvos,
            &pastas,
            destino,
            &mut |e| eventos.borrow_mut().push(e),
            &mut || {
                self.relogio.set(self.relogio.get() + 60);
                Ok(self.relogio.get())
            },
            ler,
            obter,
        )
        .await;
        (relatorio, eventos.into_inner())
    }
}

/// Página que mostra o concurso `ultimo` em todas as loterias.
fn pagina_em(ultimo: u32) -> impl FnMut(&'static LoteriaConfig) -> Ready<Result<Resultado>> {
    move |cfg| ready(Ok(pagina(cfg, ultimo)))
}

/// Página com resposta por loteria.
fn pagina_por(
    f: impl Fn(&'static LoteriaConfig) -> Result<Resultado>,
) -> impl FnMut(&'static LoteriaConfig) -> Ready<Result<Resultado>> {
    move |cfg| ready(f(cfg))
}

/// Planilha que nunca deve ser pedida.
fn sem_planilha(_: &'static LoteriaConfig) -> Ready<Result<PathBuf>> {
    panic!("a planilha não devia ser baixada")
}

/// Planilha fixa; `chamadas` conta os downloads.
fn planilha_fixa(
    arquivos: Vec<(&'static str, PathBuf)>,
    chamadas: Rc<Cell<u32>>,
) -> impl FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>> {
    move |cfg| {
        chamadas.set(chamadas.get() + 1);
        let (_, caminho) = arquivos
            .iter()
            .find(|(s, _)| *s == cfg.slug)
            .expect("planilha da loteria");
        ready(Ok(caminho.clone()))
    }
}

fn falha_de<'a>(r: &'a RelatorioCiclo, slug: &str) -> &'a rust_spider::orquestracao::FalhaCiclo {
    r.loterias
        .iter()
        .find(|l| l.slug == slug)
        .and_then(|l| l.falha.as_ref())
        .unwrap_or_else(|| panic!("{slug} devia ter falhado"))
}

/// Destino falso: não precisa de git.
#[derive(Default)]
struct DestinoFalso {
    chamadas: Vec<&'static str>,
    falhar_envio: bool,
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
        if self.falhar_envio {
            Err(SpiderError::PublicacaoEnvioFalhou("rede fora".into()))
        } else {
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Ciclo feliz
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ciclo_feliz_de_uma_loteria_exporta_e_publica() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear("quina", N, &[]);

    let (r, eventos) = a
        .rodar_git(&["quina"], pagina_em(N + 1), sem_planilha)
        .await;

    assert!(r.sucesso(), "{r:?}");
    assert_eq!(r.loterias.len(), 1);
    assert!(r.loterias[0].concurso_novo);
    assert!(r.loterias[0].exportada);
    assert_eq!(r.loterias[0].ultimo_concurso, Some(N + 1));
    assert_eq!(
        r.publicacao.as_ref().unwrap(),
        &Desfecho::Publicado {
            mensagem: "Dados: quina 13".into()
        }
    );
    assert_eq!(a.concursos("quina").last(), Some(&13));
    assert_eq!(a.mensagem_do_ultimo_commit(), "Dados: quina 13");
    assert_eq!(a.commits_no_remoto(), 2);
    let ls = git(&a.remoto, &["ls-tree", "-r", "--name-only", "main"]);
    for arquivo in ["historico.json", "tabela_geral.json", "ultimo.json"] {
        assert!(ls.contains(&format!("v1/quina/{arquivo}")), "{ls}");
    }
    let index = fs::read_to_string(a.clone.join("v1/index.json")).unwrap();
    assert!(index.contains("\"lotofacil\": null"), "{index}");
    // O resumo passou por todas as etapas, em ordem.
    let textos: Vec<String> = eventos.iter().map(ToString::to_string).collect();
    let posicao = |trecho: &str| {
        textos
            .iter()
            .position(|t| t.contains(trecho))
            .unwrap_or_else(|| panic!("evento `{trecho}` ausente em {textos:#?}"))
    };
    assert!(posicao("Concurso 13") < posicao("Validação aprovada"));
    assert!(posicao("Validação aprovada") < posicao("Features"));
    assert!(posicao("Features") < posicao("Índice do perfil padrao_v1"));
    assert!(posicao("Índice do perfil") < posicao("Exportação de quina"));
    assert!(posicao("Exportação de quina") < posicao("=== Publicação ==="));
}

#[tokio::test]
async fn ciclo_de_todas_publica_uma_vez_na_ordem_do_config() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);

    let (r, eventos) = a.rodar_git(&SLUGS, pagina_em(N + 1), sem_planilha).await;

    assert!(r.sucesso(), "{r:?}");
    let slugs: Vec<&str> = r.loterias.iter().map(|l| l.slug).collect();
    assert_eq!(slugs, SLUGS);
    assert_eq!(a.commits_no_remoto(), 2, "um único commit de dados");
    assert_eq!(
        a.mensagem_do_ultimo_commit(),
        "Dados: lotofacil 13, megasena 13, quina 13"
    );
    let publicando = eventos
        .iter()
        .filter(|e| matches!(e, Evento::Ciclo(EventoCiclo::Publicando)))
        .count();
    assert_eq!(publicando, 1);
}

#[tokio::test]
async fn segunda_execucao_nao_muda_nada_nem_republica() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    let (r1, _) = a.rodar_git(&SLUGS, pagina_em(N + 1), sem_planilha).await;
    assert!(r1.sucesso(), "{r1:?}");
    let commits = a.commits_no_remoto();
    let arquivos_antes: Vec<Vec<u8>> = SLUGS
        .iter()
        .flat_map(|s| {
            ["ultimo", "tabela_geral", "historico"]
                .map(|n| fs::read(a.saida().join(format!("v1/{s}/{n}.json"))).unwrap())
        })
        .collect();
    let bancos_antes: Vec<Vec<u32>> = SLUGS.iter().map(|s| a.concursos(s)).collect();

    let (r2, _) = a.rodar_git(&SLUGS, pagina_em(N + 1), sem_planilha).await;

    assert!(r2.sucesso(), "{r2:?}");
    assert!(r2.loterias.iter().all(|l| !l.concurso_novo));
    assert_eq!(r2.publicacao.as_ref().unwrap(), &Desfecho::NadaAPublicar);
    assert_eq!(a.commits_no_remoto(), commits);
    assert_eq!(git(&a.clone, &["status", "--porcelain"]), "");
    let arquivos_depois: Vec<Vec<u8>> = SLUGS
        .iter()
        .flat_map(|s| {
            ["ultimo", "tabela_geral", "historico"]
                .map(|n| fs::read(a.saida().join(format!("v1/{s}/{n}.json"))).unwrap())
        })
        .collect();
    assert_eq!(arquivos_antes, arquivos_depois, "mesmos bytes");
    let bancos_depois: Vec<Vec<u32>> = SLUGS.iter().map(|s| a.concursos(s)).collect();
    assert_eq!(bancos_antes, bancos_depois);
}

#[tokio::test]
async fn sem_concurso_novo_segue_ate_o_publicar() {
    exige_git!();
    let a = Amb::novo(true);
    // O banco já está em dia com a página: não há concurso novo.
    a.semear("quina", N, &[]);

    let (r, _) = a.rodar_git(&["quina"], pagina_em(N), sem_planilha).await;

    assert!(r.sucesso(), "{r:?}");
    assert!(!r.loterias[0].concurso_novo);
    assert!(r.loterias[0].exportada);
    assert_eq!(
        r.publicacao.as_ref().unwrap(),
        &Desfecho::Publicado {
            mensagem: "Dados: quina 12".into()
        }
    );
    let (r, _) = a.rodar_git(&["quina"], pagina_em(N), sem_planilha).await;
    assert_eq!(r.publicacao.as_ref().unwrap(), &Desfecho::NadaAPublicar);
}

#[tokio::test]
async fn concurso_novo_republica_so_o_que_mudou() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    let (r, _) = a.rodar_git(&SLUGS, pagina_em(N + 1), sem_planilha).await;
    assert!(r.sucesso(), "{r:?}");

    // Só a quina tem concurso novo.
    let (r, _) = a
        .rodar_git(
            &SLUGS,
            pagina_por(|cfg| Ok(pagina(cfg, if cfg.slug == "quina" { 14 } else { 13 }))),
            sem_planilha,
        )
        .await;
    assert!(r.sucesso(), "{r:?}");
    assert_eq!(
        a.mensagem_do_ultimo_commit(),
        "Dados: lotofacil 13, megasena 13, quina 14"
    );
    assert_eq!(a.commits_no_remoto(), 3);
}

// ---------------------------------------------------------------------------
// Falhas
// ---------------------------------------------------------------------------

#[tokio::test]
async fn seletor_errado_e_rede_fora_isolam_a_loteria_e_o_resto_publica() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);

    let (r, eventos) = a
        .rodar_git(
            &SLUGS,
            pagina_por(|cfg| match cfg.slug {
                // Seletor da página mudou.
                "megasena" => Err(SpiderError::ElementoNaoEncontrado("#ultimo".into())),
                // Rede fora.
                "quina" => Err(SpiderError::Navegador {
                    url: "https://loterias.caixa.gov.br".into(),
                    detalhe: "net::ERR_INTERNET_DISCONNECTED".into(),
                }),
                _ => Ok(pagina(cfg, 13)),
            }),
            sem_planilha,
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(
        r.falhas().map(|l| l.slug).collect::<Vec<_>>(),
        ["megasena", "quina"]
    );
    assert_eq!(falha_de(&r, "megasena").etapa, EtapaCiclo::Atualizar);
    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Atualizar);
    assert!(r.loterias[0].ok() && r.loterias[0].exportada);
    // A lotofacil foi publicada; as falhadas ficam como estavam no banco.
    assert_eq!(
        r.publicacao.as_ref().unwrap(),
        &Desfecho::Publicado {
            mensagem: "Dados: lotofacil 13".into()
        }
    );
    assert_eq!(a.concursos("megasena").last(), Some(&12));
    assert_eq!(a.concursos("quina").last(), Some(&12));
    assert!(!a.saida().join("v1/megasena").exists());
    let index = fs::read_to_string(a.clone.join("v1/index.json")).unwrap();
    assert!(index.contains("\"megasena\": null"), "{index}");
    // A falha vira evento (saída de erro) com a etapa.
    assert!(eventos.iter().any(|e| matches!(
        e,
        Evento::Ciclo(EventoCiclo::LoteriaFalhou {
            slug: "megasena",
            etapa: EtapaCiclo::Atualizar,
            ..
        })
    )));
}

#[tokio::test]
async fn loteria_que_falha_mantem_o_que_ja_estava_publicado() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    let (r, _) = a.rodar_git(&SLUGS, pagina_em(13), sem_planilha).await;
    assert!(r.sucesso(), "{r:?}");
    let megasena_antes = git(&a.remoto, &["rev-parse", "main:v1/megasena"]);

    let (r, _) = a
        .rodar_git(
            &SLUGS,
            pagina_por(|cfg| match cfg.slug {
                "megasena" => Err(SpiderError::ElementoNaoEncontrado("#ultimo".into())),
                "lotofacil" => Ok(pagina(cfg, 14)),
                _ => Ok(pagina(cfg, 13)),
            }),
            sem_planilha,
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(
        a.mensagem_do_ultimo_commit(),
        "Dados: lotofacil 14, megasena 13, quina 13"
    );
    assert_eq!(
        git(&a.remoto, &["rev-parse", "main:v1/megasena"]),
        megasena_antes,
        "a megasena publicada não mudou"
    );
}

#[tokio::test]
async fn buraco_fechado_pelo_backfill_com_a_planilha_inteira() {
    let a = Amb::novo(false);
    // Falta o 5 no meio; a página mostra o próximo (sem salto).
    a.semear("quina", N, &[5]);
    let planilha = gravar_planilha(
        &a.raiz.join("downloads"),
        cfg_de("quina"),
        &historico(cfg_de("quina"), N),
    );
    let chamadas = Rc::new(Cell::new(0));
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &["quina"],
            &mut destino,
            pagina_em(N + 1),
            planilha_fixa(vec![("quina", planilha)], chamadas.clone()),
        )
        .await;

    assert!(r.sucesso(), "{r:?}");
    assert!(r.loterias[0].backfill);
    assert_eq!(chamadas.get(), 1);
    assert_eq!(a.concursos("quina"), (1..=13).collect::<Vec<_>>());
    assert_eq!(a.backups(), 1, "backup antes do caminho da planilha");
    assert!(eventos
        .iter()
        .any(|e| matches!(e, Evento::Ciclo(EventoCiclo::BackfillIniciado { .. }))));
    assert!(!eventos
        .iter()
        .any(|e| matches!(e, Evento::Ciclo(EventoCiclo::BackfillPersiste { .. }))));
    assert_eq!(
        destino.chamadas,
        ["preparar", "espelhar", "registrar", "enviar"]
    );
}

#[tokio::test]
async fn backfill_com_planilha_de_data_futura_recusa_o_lote_e_deixa_o_banco() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[5]);
    // O relógio do ambiente é de 2026-10: o último concurso da planilha, em 2099, é futuro.
    let mut cheia = historico(cfg_de("quina"), N);
    cheia.last_mut().unwrap().data_sorteio = "2099-01-01".to_string();
    let planilha = gravar_planilha(&a.raiz.join("downloads"), cfg_de("quina"), &cheia);
    let chamadas = Rc::new(Cell::new(0));
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &["quina"],
            &mut destino,
            pagina_em(N + 1),
            planilha_fixa(vec![("quina", planilha)], chamadas.clone()),
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Backfill);
    assert!(!r.loterias[0].backfill);
    // Banco intacto: só o concurso novo do atualizar; o buraco continua.
    let mut esperado: Vec<u32> = (1..=13).collect();
    esperado.retain(|c| *c != 5);
    assert_eq!(a.concursos("quina"), esperado);
    assert_eq!(a.backups(), 1);
    let mensagem = mensagem_de_falha(&eventos, "quina");
    assert!(mensagem.contains("backup"), "{mensagem}");
    assert!(mensagem.contains("lote recusado"), "{mensagem}");
}

#[tokio::test]
async fn buraco_que_persiste_so_avisa_e_o_validar_reprova_a_loteria() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear("lotofacil", N, &[]);
    a.semear("megasena", N, &[5]);
    a.semear("quina", N, &[]);
    // A planilha da megasena também não traz o concurso 5.
    let sem_5: Vec<Resultado> = historico(cfg_de("megasena"), N)
        .into_iter()
        .filter(|r| r.concurso != 5)
        .collect();
    let planilha = gravar_planilha(&a.raiz.join("downloads"), cfg_de("megasena"), &sem_5);
    let chamadas = Rc::new(Cell::new(0));

    let (r, eventos) = a
        .rodar_git(
            &SLUGS,
            pagina_em(N + 1),
            planilha_fixa(vec![("megasena", planilha)], chamadas.clone()),
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(chamadas.get(), 1, "só a megasena baixou a planilha");
    let falha = falha_de(&r, "megasena");
    assert_eq!(falha.etapa, EtapaCiclo::Validar);
    assert!(matches!(
        falha.erro,
        SpiderError::ResultadosReprovados { .. }
    ));
    assert!(eventos.iter().any(|e| matches!(
        e,
        Evento::Ciclo(EventoCiclo::BackfillPersiste { faixas }) if faixas == &[(5, 5)]
    )));
    // As outras duas foram exportadas e publicadas; a megasena não.
    assert_eq!(
        a.mensagem_do_ultimo_commit(),
        "Dados: lotofacil 13, quina 13"
    );
    assert!(!a.saida().join("v1/megasena").exists());
}

#[tokio::test]
async fn salto_nao_repete_o_download_da_planilha() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[]);
    // A página pula do 12 para o 15; a planilha (atrasada) não traz 13 e 14.
    let planilha = gravar_planilha(
        &a.raiz.join("downloads"),
        cfg_de("quina"),
        &historico(cfg_de("quina"), N),
    );
    let chamadas = Rc::new(Cell::new(0));
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &["quina"],
            &mut destino,
            pagina_em(15),
            planilha_fixa(vec![("quina", planilha)], chamadas.clone()),
        )
        .await;

    assert_eq!(
        chamadas.get(),
        1,
        "o atualizar já baixou; o backfill não repete"
    );
    assert!(!r.loterias[0].backfill);
    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Validar);
    assert!(eventos.iter().any(|e| matches!(
        e,
        Evento::Ciclo(EventoCiclo::BackfillDispensado { faixas }) if faixas == &[(13, 14)]
    )));
    assert!(matches!(r.loterias[0].ultimo_concurso, Some(15)));
}

/// Planilha que falha com o erro de rede dado.
fn planilha_com_erro(
    detalhe: &'static str,
) -> impl FnMut(&'static LoteriaConfig) -> Ready<Result<PathBuf>> {
    move |_| {
        ready(Err(SpiderError::Navegador {
            url: "https://loterias.caixa.gov.br".into(),
            detalhe: detalhe.into(),
        }))
    }
}

fn mensagem_de_falha(eventos: &[Evento], alvo: &str) -> String {
    eventos
        .iter()
        .find_map(|e| match e {
            Evento::Ciclo(EventoCiclo::LoteriaFalhou { slug, mensagem, .. }) if *slug == alvo => {
                Some(mensagem.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("sem LoteriaFalhou de {alvo}"))
}

#[tokio::test]
async fn salto_com_download_falhando_mostra_a_causa_no_evento() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[]);
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &["quina"],
            &mut destino,
            pagina_em(15),
            planilha_com_erro("net::ERR_INTERNET_DISCONNECTED"),
        )
        .await;

    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Atualizar);
    let mensagem = mensagem_de_falha(&eventos, "quina");
    assert!(
        mensagem.contains("net::ERR_INTERNET_DISCONNECTED"),
        "a causa original se perdeu: {mensagem}"
    );
    assert!(mensagem.contains("o caminho da planilha completa falhou"));
    assert_eq!(a.concursos("quina"), (1..=12).collect::<Vec<_>>());
}

#[tokio::test]
async fn perfil_ausente_falha_no_indice_e_as_outras_seguem() {
    exige_git!();
    let a = Amb::novo(true);
    a.semear_todas(N);
    fs::remove_file(a.raiz.join("perfis/quina/padrao_v1.toml")).unwrap();

    let (r, eventos) = a.rodar_git(&SLUGS, pagina_em(N + 1), sem_planilha).await;

    assert!(!r.sucesso());
    assert_eq!(r.falhas().map(|l| l.slug).collect::<Vec<_>>(), ["quina"]);
    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Indice);
    assert!(!r.loterias[2].exportada);
    assert!(r.loterias[0].exportada && r.loterias[1].exportada);
    assert_eq!(
        r.publicacao.as_ref().unwrap(),
        &Desfecho::Publicado {
            mensagem: "Dados: lotofacil 13, megasena 13".into()
        }
    );
    assert!(!a.saida().join("v1/quina").exists());
    let index = fs::read_to_string(a.clone.join("v1/index.json")).unwrap();
    assert!(index.contains("\"quina\": null"), "{index}");
    assert!(!index.contains("\"lotofacil\": null"), "{index}");
    assert!(!index.contains("\"megasena\": null"), "{index}");
    assert!(mensagem_de_falha(&eventos, "quina").contains("padrao_v1"));
}

#[tokio::test]
async fn backfill_com_download_falhando_deixa_o_banco_e_as_outras_seguem() {
    let a = Amb::novo(false);
    a.semear("lotofacil", N, &[]);
    a.semear("megasena", N, &[]);
    a.semear("quina", N, &[5]);
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &SLUGS,
            &mut destino,
            pagina_em(N + 1),
            planilha_com_erro("tempo esgotado"),
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(r.falhas().map(|l| l.slug).collect::<Vec<_>>(), ["quina"]);
    assert_eq!(falha_de(&r, "quina").etapa, EtapaCiclo::Backfill);
    assert!(!r.loterias[2].backfill);
    // Banco intacto: só o concurso novo do atualizar, o buraco continua.
    let mut esperado: Vec<u32> = (1..=13).collect();
    esperado.retain(|c| *c != 5);
    assert_eq!(a.concursos("quina"), esperado);
    assert_eq!(a.backups(), 1);
    let mensagem = mensagem_de_falha(&eventos, "quina");
    assert!(mensagem.contains("tempo esgotado"), "{mensagem}");
    assert!(mensagem.contains("backup"), "{mensagem}");
    assert!(r.loterias[0].exportada && r.loterias[1].exportada);
    assert_eq!(
        destino.chamadas,
        ["preparar", "espelhar", "registrar", "enviar"]
    );
}

#[tokio::test]
async fn todas_falhando_ainda_tentam_o_publicar_e_o_ciclo_falha() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let mut destino = DestinoFalso::default();

    let (r, _) = a
        .rodar(
            &SLUGS,
            &mut destino,
            pagina_por(|_| Err(SpiderError::ElementoNaoEncontrado("#ultimo".into()))),
            sem_planilha,
        )
        .await;

    assert!(!r.sucesso());
    assert_eq!(r.falhas().count(), 3);
    // Nada exportado: a saída não existe e o publicar recusa, sem derrubar o ciclo.
    assert!(matches!(
        r.publicacao,
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
    assert!(destino.chamadas.is_empty(), "o destino não foi tocado");
}

#[tokio::test]
async fn push_que_falha_faz_o_ciclo_falhar_mesmo_com_as_loterias_ok() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let mut destino = DestinoFalso {
        falhar_envio: true,
        ..DestinoFalso::default()
    };

    let (r, eventos) = a
        .rodar(&SLUGS, &mut destino, pagina_em(N + 1), sem_planilha)
        .await;

    assert!(r.loterias.iter().all(|l| l.ok() && l.exportada));
    assert!(matches!(
        r.publicacao,
        Err(SpiderError::PublicacaoEnvioFalhou(_))
    ));
    assert!(!r.sucesso());
    assert!(eventos
        .iter()
        .any(|e| matches!(e, Evento::Ciclo(EventoCiclo::PublicacaoFalhou { .. }))));
}

#[tokio::test]
async fn banco_inexistente_com_salto_baixa_a_planilha_e_conclui() {
    let a = Amb::novo(false);
    let planilha = gravar_planilha(
        &a.raiz.join("downloads"),
        cfg_de("quina"),
        &historico(cfg_de("quina"), N),
    );
    let chamadas = Rc::new(Cell::new(0));
    let mut destino = DestinoFalso::default();

    let (r, _) = a
        .rodar(
            &["quina"],
            &mut destino,
            pagina_em(N),
            planilha_fixa(vec![("quina", planilha)], chamadas.clone()),
        )
        .await;

    assert!(r.sucesso(), "{r:?}");
    assert_eq!(chamadas.get(), 1);
    assert_eq!(a.concursos("quina"), (1..=12).collect::<Vec<_>>());
}

#[test]
fn eventos_do_ciclo_dizem_para_onde_vao() {
    let aviso = Evento::Ciclo(EventoCiclo::BackfillPersiste {
        faixas: vec![(5, 5)],
    });
    assert_eq!(aviso.destino(), rust_spider::orquestracao::Destino::Erro);
    assert!(aviso.to_string().contains("persiste"));
    let ok = Evento::Ciclo(EventoCiclo::NadaAPublicar);
    assert_eq!(ok.destino(), rust_spider::orquestracao::Destino::Padrao);
}

/// Pendência da 3.1b: o validar do ciclo mostra a dica de como fechar o
/// buraco, como o `sync validar` e o `sync features`, logo depois dos erros.
#[tokio::test]
async fn validar_do_ciclo_emite_a_dica_de_buracos_depois_dos_erros() {
    let a = Amb::novo(false);
    a.semear("megasena", N, &[5]);
    let sem_5: Vec<Resultado> = historico(cfg_de("megasena"), N)
        .into_iter()
        .filter(|r| r.concurso != 5)
        .collect();
    let planilha = gravar_planilha(&a.raiz.join("downloads"), cfg_de("megasena"), &sem_5);
    let chamadas = Rc::new(Cell::new(0));
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(
            &["megasena"],
            &mut destino,
            pagina_em(N),
            planilha_fixa(vec![("megasena", planilha)], chamadas),
        )
        .await;

    assert_eq!(falha_de(&r, "megasena").etapa, EtapaCiclo::Validar);
    let posicao_erros = eventos
        .iter()
        .position(|e| matches!(e, Evento::Erros(_)))
        .expect("o validar emitiu os erros");
    assert_eq!(
        eventos[posicao_erros + 1],
        Evento::DicaBuracos { slug: "megasena" }
    );
}

/// Banco sem buraco: o validar do ciclo não emite a dica.
#[tokio::test]
async fn validar_do_ciclo_sem_buraco_nao_emite_a_dica() {
    let a = Amb::novo(false);
    a.semear_todas(N);
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(&["quina"], &mut destino, pagina_em(N), sem_planilha)
        .await;

    assert!(r.sucesso());
    assert!(!eventos
        .iter()
        .any(|e| matches!(e, Evento::DicaBuracos { .. })));
}

/// Mapeamento de níveis do log (3.5): um evento de cada nível.
#[test]
fn eventos_do_ciclo_dizem_o_nivel_no_log() {
    use rust_spider::log_arquivo::Nivel;
    let nivel = |e: EventoCiclo| Evento::Ciclo(e).nivel();
    assert_eq!(
        nivel(EventoCiclo::Loteria {
            nome: "Quina",
            slug: "quina"
        }),
        Nivel::Info
    );
    assert_eq!(
        nivel(EventoCiclo::AguardandoConcurso {
            slug: "quina",
            esperado_em: "2026-10-13".into(),
            ultimo: Some(1)
        }),
        Nivel::Info
    );
    for aviso in [
        EventoCiclo::BackfillPersiste { faixas: vec![] },
        EventoCiclo::FalhaTransitoria {
            slug: "quina",
            etapa: EtapaCiclo::Atualizar,
            mensagem: "rede".into(),
        },
        EventoCiclo::ConcursoNaoSaiu {
            slug: "quina",
            esperado_em: "2026-10-13".into(),
        },
        EventoCiclo::JanelaEncerrada {
            hora: "01:00".into(),
            pendentes: vec!["quina"],
        },
    ] {
        assert_eq!(nivel(aviso.clone()), Nivel::Aviso, "{aviso:?}");
    }
    assert_eq!(
        nivel(EventoCiclo::JanelaEncerrada {
            hora: "01:00".into(),
            pendentes: vec![]
        }),
        Nivel::Info
    );
    assert_eq!(
        nivel(EventoCiclo::LoteriaFalhou {
            slug: "quina",
            etapa: EtapaCiclo::Validar,
            mensagem: "x".into()
        }),
        Nivel::Erro
    );
    assert_eq!(
        nivel(EventoCiclo::PublicacaoFalhou {
            mensagem: "x".into()
        }),
        Nivel::Erro
    );
    assert_eq!(Evento::DicaBuracos { slug: "quina" }.nivel(), Nivel::Aviso);
    assert_eq!(Evento::Avisos(vec![]).nivel(), Nivel::Aviso);
    assert_eq!(Evento::Erros(vec![]).nivel(), Nivel::Erro);
    assert_eq!(
        Evento::JaAtualizado {
            ultimo: 1,
            pagina: 1
        }
        .nivel(),
        Nivel::Info
    );
}

// ---------------------------------------------------------------------------
// Ranking por taxa no ciclo (tarefa 2.5)
// ---------------------------------------------------------------------------

fn arquivo_ranking(a: &Amb, slug: &str) -> PathBuf {
    a.saida().join(format!("v1/{slug}/ranking_taxa.json"))
}

fn index_tem_ranking(a: &Amb) -> bool {
    fs::read_to_string(a.saida().join("v1/index.json"))
        .unwrap()
        .contains("ranking_taxa")
}

/// Tabelas do ranking como texto, para comparar dois estados do banco.
fn despejo_ranking(a: &Amb, slug: &str) -> String {
    let conn = db::abrir(&a.db(slug)).unwrap();
    let mut saida = String::new();
    for tabela in ["ranking_taxa", "taxa_posicao", "taxa_faixa"] {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {tabela} ORDER BY 1, 2, 3"))
            .unwrap();
        let colunas = stmt.column_count();
        let mut linhas = stmt.query([]).unwrap();
        while let Some(l) = linhas.next().unwrap() {
            for i in 0..colunas {
                let v: rusqlite::types::Value = l.get(i).unwrap();
                saida.push_str(&format!("{v:?}|"));
            }
            saida.push('\n');
        }
    }
    saida
}

#[tokio::test]
async fn ciclo_com_ranking_ok_publica_o_ranking_taxa() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[]);
    let mut destino = DestinoFalso::default();

    let (r, eventos) = a
        .rodar(&["quina"], &mut destino, pagina_em(N + 1), sem_planilha)
        .await;

    assert!(r.sucesso(), "{r:?}");
    assert!(r.loterias[0].exportada);
    assert_eq!(r.loterias[0].ranking, Default::default());
    assert!(arquivo_ranking(&a, "quina").exists());
    assert!(index_tem_ranking(&a));
    assert!(eventos.iter().any(
        |e| matches!(e, Evento::Ciclo(EventoCiclo::RankingEmDia { ultimo, .. }) if *ultimo == N + 1)
    ));
    assert!(!eventos.iter().any(|e| matches!(
        e,
        Evento::Ciclo(EventoCiclo::RankingFalhou { .. } | EventoCiclo::RankingOmitido { .. })
    )));
}

#[tokio::test]
async fn ranking_rodado_de_novo_e_idempotente() {
    let a = Amb::novo(false);
    a.semear("lotofacil", N, &[]);
    let mut destino = DestinoFalso::default();

    let (r1, _) = a
        .rodar(&["lotofacil"], &mut destino, pagina_em(N + 1), sem_planilha)
        .await;
    assert!(r1.sucesso(), "{r1:?}");
    let tabelas = despejo_ranking(&a, "lotofacil");
    let arquivo = fs::read(arquivo_ranking(&a, "lotofacil")).unwrap();
    assert!(!tabelas.is_empty());

    let (r2, _) = a
        .rodar(&["lotofacil"], &mut destino, pagina_em(N + 1), sem_planilha)
        .await;
    assert!(r2.sucesso(), "{r2:?}");
    assert!(!r2.loterias[0].concurso_novo);
    assert_eq!(despejo_ranking(&a, "lotofacil"), tabelas);
    assert_eq!(fs::read(arquivo_ranking(&a, "lotofacil")).unwrap(), arquivo);
}

/// Falha simulada do ranking: um gatilho no banco aborta o INSERT em
/// `ranking_taxa` (a transação do ranking desfaz tudo) depois de um ciclo bom.
#[tokio::test]
async fn falha_do_ranking_nao_derruba_o_ciclo_gera_alerta_e_omite_o_arquivo() {
    let a = Amb::novo(false);
    a.semear("quina", N, &[]);
    let mut destino = DestinoFalso::default();

    let (r1, _) = a
        .rodar(&["quina"], &mut destino, pagina_em(N), sem_planilha)
        .await;
    assert!(r1.sucesso(), "{r1:?}");
    assert!(arquivo_ranking(&a, "quina").exists());
    {
        let conn = db::abrir(&a.db("quina")).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER falha_simulada BEFORE INSERT ON ranking_taxa              BEGIN SELECT RAISE(ABORT, 'falha simulada'); END;",
        )
        .unwrap();
    }

    // Saiu o concurso 13: features e índice avançam, o ranking falha.
    let (r2, eventos) = a
        .rodar(&["quina"], &mut destino, pagina_em(N + 1), sem_planilha)
        .await;

    // O resto segue: loteria ok, exportada, publicada; código de saída 0.
    assert!(r2.sucesso(), "{r2:?}");
    assert!(r2.loterias[0].exportada);
    let ranking = &r2.loterias[0].ranking;
    assert!(
        ranking.falha.as_deref().unwrap().contains("falha simulada"),
        "{ranking:?}"
    );
    assert!(ranking
        .omitido
        .as_deref()
        .unwrap()
        .contains("ranking vai até o concurso 13, mas o snapshot é do 14"));
    assert!(r2.loterias[0].to_string().contains("sem ranking_taxa.json"));
    // Sem arquivo (o antigo foi removido) e sem a chave no index.
    assert!(!arquivo_ranking(&a, "quina").exists());
    assert!(!index_tem_ranking(&a));
    assert!(a.saida().join("v1/quina/ultimo.json").exists());
    // Log: falha, omissão e remoção, todas como aviso.
    for e in [
        |e: &Evento| matches!(e, Evento::Ciclo(EventoCiclo::RankingFalhou { .. })),
        |e: &Evento| matches!(e, Evento::Ciclo(EventoCiclo::RankingOmitido { .. })),
        |e: &Evento| matches!(e, Evento::Ciclo(EventoCiclo::RankingAntigoRemovido { .. })),
    ] {
        let achado = eventos.iter().find(|x| e(x)).expect("evento do ranking");
        assert_eq!(achado.nivel(), rust_spider::log_arquivo::Nivel::Aviso);
    }

    // Alerta: conta como falha da noite (etapa `ranking`) e vira rascunho na
    // segunda noite seguida, pela regra de sempre.
    let agora = RELOGIO_INICIAL as i64;
    let noites = noites_do_ciclo(&[(cfg_de("quina"), a.db("quina"))], &r2, agora, &mut |_| {});
    let falha = noites[0].falha.as_ref().expect("falha da noite");
    assert_eq!(falha.etapa, "ranking");
    assert!(falha.erro.contains("falha simulada"));
    let (alertas, logs) = (a.raiz.join("alertas"), a.raiz.join("logs"));
    let pastas = PastasAlertas {
        alertas: &alertas,
        logs: &logs,
    };
    for (noite, esperados) in [(0, 0), (1, 1)] {
        let inicio = agora + noite * 86_400;
        let gerados = registrar_noite(
            &pastas,
            &noites,
            inicio,
            &mut || Ok(inicio as u64),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(gerados.len(), esperados, "noite {noite}");
    }
    let eml = fs::read_to_string(&listar_pendentes(&alertas).unwrap()[0].caminho).unwrap();
    assert!(eml.contains("Etapa em que falhou: ranking"), "{eml}");

    // Consertado o problema, o ranking volta e o arquivo reaparece.
    {
        let conn = db::abrir(&a.db("quina")).unwrap();
        conn.execute_batch("DROP TRIGGER falha_simulada;").unwrap();
    }
    let (r3, _) = a
        .rodar(&["quina"], &mut destino, pagina_em(N + 1), sem_planilha)
        .await;
    assert!(r3.sucesso() && r3.loterias[0].ranking == Default::default());
    assert!(arquivo_ranking(&a, "quina").exists());
    assert!(index_tem_ranking(&a));
}

#[test]
fn eventos_do_ranking_no_ciclo_sao_avisos_na_saida_de_erro() {
    use rust_spider::log_arquivo::Nivel;
    for e in [
        EventoCiclo::RankingFalhou {
            slug: "quina",
            mensagem: "x".into(),
        },
        EventoCiclo::RankingOmitido {
            slug: "quina",
            motivo: "x".into(),
        },
    ] {
        let e = Evento::Ciclo(e);
        assert_eq!(e.nivel(), Nivel::Aviso);
        assert_eq!(e.destino(), rust_spider::orquestracao::Destino::Erro);
    }
}
