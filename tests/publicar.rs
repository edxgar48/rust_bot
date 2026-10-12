//! Testes do `sync publicar` (2.4) com repositórios git temporários: um
//! "remoto" bare local e um clone, sem rede. Quando o `git` não está instalado
//! os testes são pulados (passam sem fazer nada), não falham.

use rust_spider::error::SpiderError;
use rust_spider::exportacao::{index_json, Impressao, LoteriaNoIndice};
use rust_spider::publicacao::{ler_indice, publicar, Desfecho, DestinoGit};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

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

/// Bytes exatos da saída do git, sem `trim`: diferença de fim de linha tem de aparecer.
fn git_bruto(pasta: &Path, args: &[&str]) -> Vec<u8> {
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
    o.stdout
}

/// Ambiente: `remoto.git` (bare), `clone/` (com um commit inicial `LEIAME`
/// já enviado) e `saida/` (onde o teste monta o `v1/`).
struct Ambiente {
    raiz: PathBuf,
    remoto: PathBuf,
    clone: PathBuf,
    saida: PathBuf,
}

impl Drop for Ambiente {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.raiz);
    }
}

fn configurar_autor(pasta: &Path) {
    git(pasta, &["config", "user.name", "Teste"]);
    git(pasta, &["config", "user.email", "teste@example.com"]);
    // Blindagem contra a config global da máquina: sem assinatura e com os
    // hooks do próprio clone (um `core.hooksPath` global os desviaria).
    git(pasta, &["config", "commit.gpgsign", "false"]);
    git(pasta, &["config", "core.hooksPath", ".git/hooks"]);
}

fn novo_ambiente() -> Ambiente {
    static N: AtomicU32 = AtomicU32::new(0);
    let raiz = std::env::temp_dir().join(format!(
        "spider_publicar_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&raiz);
    fs::create_dir_all(&raiz).unwrap();
    let remoto = raiz.join("remoto.git");
    let clone = raiz.join("clone");
    let saida = raiz.join("saida");
    git(&raiz, &["init", "-q", "--bare", "remoto.git"]);
    git(&remoto, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    // O pre-receive dos testes só vale se os hooks forem os do bare.
    git(&remoto, &["config", "core.hooksPath", "hooks"]);
    git(&raiz, &["clone", "-q", "remoto.git", "clone"]);
    git(&clone, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    configurar_autor(&clone);
    fs::write(clone.join("LEIAME"), "dados\n").unwrap();
    git(&clone, &["add", "-A"]);
    git(&clone, &["commit", "-q", "-m", "inicio"]);
    git(&clone, &["push", "-q", "origin", "main"]);
    Ambiente {
        raiz,
        remoto,
        clone,
        saida,
    }
}

/// Conteúdo dos arquivos de uma loteria na saída.
struct Lote<'a> {
    slug: &'a str,
    concurso: u32,
    ultimo: &'a str,
    tabela: &'a str,
    historico: &'a str,
}

/// Grava `saida/v1/` com os lotes (os slugs `nulos` entram no índice como
/// `null`) e o `index.json` correspondente.
fn gravar_saida(saida: &Path, gerado_em: &str, lotes: &[Lote], nulos: &[&str]) {
    let v1 = saida.join("v1");
    let _ = fs::remove_dir_all(&v1);
    fs::create_dir_all(&v1).unwrap();
    let mut entradas: Vec<(&str, Option<LoteriaNoIndice>)> =
        nulos.iter().map(|s| (*s, None)).collect();
    for l in lotes {
        let pasta = v1.join(l.slug);
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join("ultimo.json"), l.ultimo).unwrap();
        fs::write(pasta.join("tabela_geral.json"), l.tabela).unwrap();
        fs::write(pasta.join("historico.json"), l.historico).unwrap();
        entradas.push((
            l.slug,
            Some(LoteriaNoIndice {
                ultimo_concurso: l.concurso,
                data: "2026-10-11".into(),
                arquivos: [
                    Impressao::de(l.ultimo.as_bytes()),
                    Impressao::de(l.tabela.as_bytes()),
                    Impressao::de(l.historico.as_bytes()),
                ],
            }),
        ));
    }
    entradas.sort_by_key(|(s, _)| *s);
    fs::write(v1.join("index.json"), index_json(gerado_em, &entradas)).unwrap();
}

fn lote<'a>(slug: &'a str, concurso: u32, versao: &'a str) -> Lote<'a> {
    Lote {
        slug,
        concurso,
        ultimo: versao,
        tabela: "tabela\n",
        historico: "historico\n",
    }
}

fn publicar_em(a: &Ambiente) -> rust_spider::Result<Desfecho> {
    publicar(&mut DestinoGit::novo(&a.clone), &a.saida)
}

fn commits(pasta: &Path) -> usize {
    git(pasta, &["rev-list", "--count", "HEAD"])
        .parse()
        .unwrap()
}

fn commits_no_remoto(a: &Ambiente) -> usize {
    git(&a.remoto, &["rev-list", "--count", "main"])
        .parse()
        .unwrap()
}

#[test]
fn primeira_publicacao_copia_commita_e_envia() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("lotofacil", 3802, "a\n"), lote("quina", 7140, "q\n")],
        &["megasena"],
    );
    let r = publicar_em(&a).unwrap();
    assert_eq!(
        r,
        Desfecho::Publicado {
            mensagem: "Dados: lotofacil 3802, quina 7140".into()
        }
    );
    assert_eq!(
        git(&a.remoto, &["log", "-1", "--format=%s", "main"]),
        "Dados: lotofacil 3802, quina 7140"
    );
    assert_eq!(commits_no_remoto(&a), 2);
    // O remoto tem os mesmos bytes que a saída.
    let no_remoto = git_bruto(&a.remoto, &["show", "main:v1/lotofacil/ultimo.json"]);
    assert_eq!(
        no_remoto,
        b"a
"
    );
    assert_eq!(
        fs::read(a.clone.join("v1/index.json")).unwrap(),
        fs::read(a.saida.join("v1/index.json")).unwrap()
    );
    // Autor = o git configurado.
    assert_eq!(
        git(&a.remoto, &["log", "-1", "--format=%an", "main"]),
        "Teste"
    );
}

#[test]
fn so_gerado_em_diferente_nao_copia_nem_commita() {
    exige_git!();
    let a = novo_ambiente();
    let lotes = [lote("quina", 7140, "q\n")];
    gravar_saida(&a.saida, "2026-10-12T10:00:00Z", &lotes, &[]);
    publicar_em(&a).unwrap();
    let antes = commits(&a.clone);
    let index_do_clone = fs::read(a.clone.join("v1/index.json")).unwrap();

    gravar_saida(&a.saida, "2026-10-13T10:00:00Z", &lotes, &[]);
    assert_eq!(publicar_em(&a).unwrap(), Desfecho::NadaAPublicar);
    assert_eq!(commits(&a.clone), antes);
    assert_eq!(commits_no_remoto(&a), antes);
    assert_eq!(
        fs::read(a.clone.join("v1/index.json")).unwrap(),
        index_do_clone,
        "o clone não foi tocado"
    );
    assert_eq!(git(&a.clone, &["status", "--porcelain"]), "");
}

#[test]
fn mudanca_de_hash_gera_novo_commit_e_apaga_o_que_sumiu() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("lotofacil", 3802, "a\n"), lote("quina", 7140, "q\n")],
        &[],
    );
    publicar_em(&a).unwrap();

    // Quina com concurso novo; a lotofacil some do índice (vira null).
    gravar_saida(
        &a.saida,
        "2026-10-13T10:00:00Z",
        &[lote("quina", 7141, "q2\n")],
        &["lotofacil"],
    );
    assert_eq!(
        publicar_em(&a).unwrap(),
        Desfecho::Publicado {
            mensagem: "Dados: quina 7141".into()
        }
    );
    assert_eq!(commits_no_remoto(&a), 3);
    assert!(!a.clone.join("v1/lotofacil").exists(), "espelho apaga");
    assert_eq!(
        git_bruto(&a.remoto, &["show", "main:v1/quina/ultimo.json"]),
        b"q2
"
    );
    assert_eq!(git(&a.clone, &["status", "--porcelain"]), "");
}

#[test]
fn fora_de_v1_nada_e_tocado() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    publicar_em(&a).unwrap();
    assert_eq!(
        fs::read_to_string(a.clone.join("LEIAME")).unwrap(),
        "dados\n"
    );
    let arquivos = git(&a.remoto, &["ls-tree", "-r", "--name-only", "main"]);
    assert!(arquivos.lines().any(|l| l == "LEIAME"));
    assert!(arquivos
        .lines()
        .all(|l| l == "LEIAME" || l.starts_with("v1/")));
}

#[test]
fn saida_adulterada_e_recusada_sem_tocar_no_clone() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    let antes = git(&a.clone, &["rev-parse", "HEAD"]);

    // Mesmo tamanho, outro conteúdo: só o sha256 acusa.
    fs::write(a.saida.join("v1/quina/historico.json"), "historicx\n").unwrap();
    assert!(matches!(
        publicar_em(&a),
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
    // Outro tamanho.
    fs::write(a.saida.join("v1/quina/historico.json"), "curto").unwrap();
    assert!(matches!(
        publicar_em(&a),
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
    // Arquivo do índice ausente.
    fs::remove_file(a.saida.join("v1/quina/historico.json")).unwrap();
    assert!(matches!(
        publicar_em(&a),
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
    assert_eq!(git(&a.clone, &["rev-parse", "HEAD"]), antes);
    assert!(!a.clone.join("v1").exists());
}

#[test]
fn sobra_na_saida_ou_falta_de_index_sao_recusadas() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    fs::write(a.saida.join("v1/quina/ultimo.json.tmp"), "x").unwrap();
    assert!(matches!(
        publicar_em(&a),
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
    fs::remove_file(a.saida.join("v1/quina/ultimo.json.tmp")).unwrap();
    fs::remove_file(a.saida.join("v1/index.json")).unwrap();
    assert!(matches!(
        publicar_em(&a),
        Err(SpiderError::PublicacaoSaidaInvalida { .. })
    ));
}

#[test]
fn clone_inexistente_mostra_o_comando_de_clone() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    let ausente = a.raiz.join("nao_existe");
    let e = publicar(&mut DestinoGit::novo(&ausente), &a.saida).unwrap_err();
    let texto = e.to_string();
    assert!(
        texto.contains("git clone git@github.com:edxgar48/rust_spider_dados.git"),
        "{texto}"
    );
    assert!(!ausente.exists(), "nunca clona sozinho");
}

#[test]
fn clone_sujo_e_recusado() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    fs::write(a.clone.join("rascunho.txt"), "x").unwrap();
    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)));
    assert!(e.to_string().contains("alterações locais"), "{e}");
    assert!(!a.clone.join("v1").exists());
    assert!(a.clone.join("rascunho.txt").exists(), "nada descartado");
}

#[test]
fn branch_diferente_de_main_e_recusado() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    git(&a.clone, &["checkout", "-q", "-b", "outro"]);
    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)));
    assert!(e.to_string().contains("outro"), "{e}");
    assert!(!a.clone.join("v1").exists());
}

/// Outro clone empurra um commit para o remoto.
fn empurrar_de_outro_clone(a: &Ambiente) {
    let outro = a.raiz.join("outro_clone");
    git(&a.raiz, &["clone", "-q", "remoto.git", "outro_clone"]);
    configurar_autor(&outro);
    fs::write(outro.join("alheio.txt"), "x").unwrap();
    git(&outro, &["add", "-A"]);
    git(&outro, &["commit", "-q", "-m", "alheio"]);
    git(&outro, &["push", "-q", "origin", "main"]);
}

#[test]
fn clone_atras_do_remoto_e_recusado() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    empurrar_de_outro_clone(&a);
    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)));
    assert!(e.to_string().contains("atrás"), "{e}");
    assert!(!a.clone.join("v1").exists());
}

#[test]
fn clone_divergente_e_recusado() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    empurrar_de_outro_clone(&a);
    fs::write(a.clone.join("local.txt"), "y").unwrap();
    git(&a.clone, &["add", "-A"]);
    git(&a.clone, &["commit", "-q", "-m", "local"]);
    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)));
    assert!(e.to_string().contains("divergiu"), "{e}");
    assert!(!a.clone.join("v1").exists());
}

#[test]
fn subpasta_de_outro_repositorio_nao_serve_de_clone() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    let sub = a.clone.join("sub");
    fs::create_dir_all(&sub).unwrap();
    let e = publicar(&mut DestinoGit::novo(&sub), &a.saida).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)));
    assert!(e.to_string().contains("dentro de outro repositório"), "{e}");
}

#[test]
fn push_que_falha_deixa_o_commit_local_e_a_proxima_execucao_reenvia() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    // Hook que rejeita qualquer push (o `fetch` continua funcionando).
    let hook = a.remoto.join("hooks/pre-receive");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\necho recusado de proposito >&2\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoEnvioFalhou(_)), "{e}");
    assert_eq!(commits(&a.clone), 2, "o commit ficou local");
    assert_eq!(commits_no_remoto(&a), 1);

    // Com o clone à frente de origin/main e nenhuma mudança, tenta de novo.
    assert!(publicar_em(&a).is_err(), "o hook ainda recusa");
    assert_eq!(commits(&a.clone), 2, "sem commit novo");

    fs::remove_file(&hook).unwrap();
    gravar_saida(
        &a.saida,
        "2026-10-13T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    assert_eq!(publicar_em(&a).unwrap(), Desfecho::EnvioRetomado);
    assert_eq!(commits_no_remoto(&a), 2);
    assert_eq!(commits(&a.clone), 2, "reenvio não cria commit");
    assert_eq!(publicar_em(&a).unwrap(), Desfecho::NadaAPublicar);
}

#[test]
fn push_que_falha_com_mudanca_nova_junta_tudo_num_envio_so() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote("quina", 7140, "q\n")],
        &[],
    );
    let hook = a.remoto.join("hooks/pre-receive");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(publicar_em(&a).is_err());
    fs::remove_file(&hook).unwrap();

    gravar_saida(
        &a.saida,
        "2026-10-13T10:00:00Z",
        &[lote("quina", 7141, "q2\n")],
        &[],
    );
    assert_eq!(
        publicar_em(&a).unwrap(),
        Desfecho::Publicado {
            mensagem: "Dados: quina 7141".into()
        }
    );
    assert_eq!(commits_no_remoto(&a), 3);
}

#[test]
fn blobs_no_remoto_sao_os_bytes_exportados_e_batem_com_o_index() {
    exige_git!();
    let a = novo_ambiente();
    // Config hostil: conversão de fim de linha ligada no clone.
    git(&a.clone, &["config", "core.autocrlf", "true"]);
    git(&a.clone, &["config", "core.safecrlf", "true"]);
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[
            lote(
                "lotofacil",
                3802,
                "a
",
            ),
            lote(
                "quina", 7140, "q
",
            ),
        ],
        &["megasena"],
    );
    publicar_em(&a).unwrap();

    let bytes_index = fs::read(a.saida.join("v1/index.json")).unwrap();
    let indice = ler_indice(&bytes_index).unwrap();
    assert_eq!(
        git_bruto(&a.remoto, &["show", "main:v1/index.json"]),
        bytes_index
    );
    let mut conferidos = 0;
    for entrada in indice.loterias.values().flatten() {
        for arq in &entrada.arquivos {
            let blob = git_bruto(
                &a.remoto,
                &["cat-file", "-p", &format!("main:v1/{}", arq.caminho)],
            );
            assert_eq!(
                blob,
                fs::read(a.saida.join("v1").join(&arq.caminho)).unwrap()
            );
            let imp = Impressao::de(&blob);
            assert_eq!(imp.sha256, arq.sha256, "{}", arq.caminho);
            assert_eq!(imp.bytes, arq.bytes, "{}", arq.caminho);
            conferidos += 1;
        }
    }
    assert_eq!(conferidos, 6);
}

#[test]
fn remoto_vazio_e_recusado_com_mensagem_clara() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote(
            "quina", 7140, "q
",
        )],
        &[],
    );
    // Bare sem nenhum commit e um clone dele (HEAD ainda sem commit).
    git(&a.raiz, &["init", "-q", "--bare", "vazio.git"]);
    let vazio = a.raiz.join("vazio.git");
    git(&vazio, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&a.raiz, &["clone", "-q", "vazio.git", "clone_vazio"]);
    let clone_vazio = a.raiz.join("clone_vazio");
    git(&clone_vazio, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    configurar_autor(&clone_vazio);

    let e = publicar(&mut DestinoGit::novo(&clone_vazio), &a.saida).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)), "{e}");
    let texto = e.to_string();
    assert!(texto.contains("vazio"), "{texto}");
    assert!(texto.contains("commit inicial"), "{texto}");
    assert!(texto.contains("push -u origin main"), "{texto}");
    assert!(!clone_vazio.join("v1").exists());
    // Também com HEAD já com commit local, mas nunca enviado.
    fs::write(
        clone_vazio.join("LEIAME"),
        "x
",
    )
    .unwrap();
    git(&clone_vazio, &["add", "-A"]);
    git(&clone_vazio, &["commit", "-q", "-m", "inicio"]);
    let e = publicar(&mut DestinoGit::novo(&clone_vazio), &a.saida).unwrap_err();
    assert!(e.to_string().contains("vazio"), "{e}");
    assert_eq!(git(&vazio, &["branch", "--list"]), "");
}

#[test]
fn commit_que_falha_desfaz_o_espelho_e_a_proxima_execucao_publica() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote(
            "quina", 7140, "q
",
        )],
        &[],
    );
    let hook = a.clone.join(".git/hooks/pre-commit");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(
        &hook,
        "#!/bin/sh
echo recusado de proposito >&2
exit 1
",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let e = publicar_em(&a).unwrap_err();
    assert!(matches!(e, SpiderError::PublicacaoRecusada(_)), "{e}");
    assert_eq!(commits(&a.clone), 1, "nenhum commit criado");
    assert_eq!(git(&a.clone, &["status", "--porcelain"]), "");
    assert!(!a.clone.join("v1").exists(), "espelho desfeito");

    fs::remove_file(&hook).unwrap();
    assert_eq!(
        publicar_em(&a).unwrap(),
        Desfecho::Publicado {
            mensagem: "Dados: quina 7140".into()
        }
    );
    assert_eq!(commits_no_remoto(&a), 2);
}

#[test]
fn caminho_inseguro_no_index_e_recusado_pelo_conferir() {
    exige_git!();
    let a = novo_ambiente();
    gravar_saida(
        &a.saida,
        "2026-10-12T10:00:00Z",
        &[lote(
            "quina", 7140, "q
",
        )],
        &[],
    );
    let caminho_index = a.saida.join("v1/index.json");
    let original = fs::read_to_string(&caminho_index).unwrap();
    assert!(original.contains("quina/ultimo.json"));
    for ruim in ["../fora/ultimo.json", "/abs/ultimo.json", "a/../b.json"] {
        fs::write(&caminho_index, original.replace("quina/ultimo.json", ruim)).unwrap();
        let e = publicar_em(&a).unwrap_err();
        assert!(
            matches!(e, SpiderError::PublicacaoSaidaInvalida { .. }),
            "{ruim}: {e}"
        );
        assert!(e.to_string().contains("não é relativo"), "{ruim}: {e}");
    }
    assert!(!a.clone.join("v1").exists());
    assert_eq!(commits(&a.clone), 1);
}
