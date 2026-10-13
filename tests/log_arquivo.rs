//! Log em arquivo (3.5) com o binário `sync` de verdade, sem rede: o comando
//! que grava cria `logs/sync_AAAA-MM-DD.log` com abertura e fechamento, o de
//! leitura não cria nada, a trava ocupada deixa rastro e o log que não abre
//! não derruba o comando.

mod comum;

use rust_spider::trava::Trava;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const AGORA: i64 = 1_791_943_200; // 2026-10-13 23:00 em Brasília

struct Pasta(PathBuf);

impl Pasta {
    fn nova(rotulo: &str) -> Pasta {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_log_it_{rotulo}_{}_{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&caminho);
        std::fs::create_dir_all(&caminho).unwrap();
        Pasta(caminho)
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sync(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sync"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binário sync executado")
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// O único arquivo de log da pasta de trabalho, com o nome no padrão.
fn ler_log(dir: &Path) -> (String, Vec<String>) {
    let nomes: Vec<String> = std::fs::read_dir(dir.join("logs"))
        .expect("pasta logs/ criada")
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(nomes.len(), 1, "{nomes:?}");
    let nome = nomes[0].clone();
    assert!(
        nome.starts_with("sync_")
            && nome.ends_with(".log")
            && nome.len() == "sync_2026-10-13.log".len(),
        "{nome}"
    );
    let conteudo = std::fs::read_to_string(dir.join("logs").join(&nome)).unwrap();
    let linhas = conteudo.lines().map(str::to_string).collect();
    (nome, linhas)
}

/// `HH:MM:SS NIVEL texto` -> (nível, texto); falha se o formato estiver errado.
fn separar(linha: &str) -> (String, String) {
    let b = linha.as_bytes();
    assert!(
        b.len() > 15
            && b[2] == b':'
            && b[5] == b':'
            && b[..8].iter().filter(|c| c.is_ascii_digit()).count() == 6
            && b[8] == b' ',
        "linha fora do formato: {linha:?}"
    );
    let nivel = linha[9..14].trim_end().to_string();
    assert!(
        ["INFO", "AVISO", "ERRO"].contains(&nivel.as_str()),
        "{linha:?}"
    );
    assert_eq!(&linha[14..15], " ", "{linha:?}");
    (nivel, linha[15..].to_string())
}

#[test]
fn comando_que_grava_cria_o_log_com_abertura_copia_da_tela_e_fechamento() {
    let p = Pasta::nova("sucesso");
    let planilha = p.0.join("quina.xlsx");
    std::fs::write(&planilha, comum::bytes_xlsx(&comum::quina())).unwrap();

    let saida = sync(&p.0, &["importar", "quina", "quina.xlsx"]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));

    let (_, linhas) = ler_log(&p.0);
    let (nivel, abertura) = separar(&linhas[0]);
    assert_eq!(nivel, "INFO");
    assert!(abertura.starts_with("Início: PID "), "{abertura}");
    assert!(
        abertura.contains("comando `sync importar quina quina.xlsx`"),
        "{abertura}"
    );
    assert!(
        abertura.ends_with(&format!("versão {}", env!("CARGO_PKG_VERSION"))),
        "{abertura}"
    );
    let (nivel, fechamento) = separar(linhas.last().unwrap());
    assert_eq!(nivel, "INFO");
    assert!(
        fechamento.starts_with("Fim: sucesso, duração "),
        "{fechamento}"
    );

    // Cada linha da tela está no log, na mesma ordem.
    let tela = texto(&saida.stdout);
    let no_log: Vec<String> = linhas[1..linhas.len() - 1]
        .iter()
        .map(|l| separar(l).1)
        .collect();
    let esperado: Vec<&str> = tela.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!esperado.is_empty());
    assert_eq!(no_log, esperado);
}

#[test]
fn falha_vai_para_o_log_como_erro_com_a_cadeia_e_o_fechamento_de_falha() {
    let p = Pasta::nova("falha");

    let saida = sync(&p.0, &["features", "quina"]);
    assert!(!saida.status.success());
    let tela = texto(&saida.stderr);
    assert!(tela.starts_with("Error: banco não encontrado"), "{tela}");

    let (_, linhas) = ler_log(&p.0);
    let niveis: Vec<(String, String)> = linhas.iter().map(|l| separar(l)).collect();
    assert_eq!(niveis[0].0, "INFO");
    assert_eq!(
        niveis
            .iter()
            .find(|(_, t)| t.starts_with("Error: banco não encontrado"))
            .map(|(n, _)| n.as_str()),
        Some("ERRO")
    );
    let ultimo = niveis.last().unwrap();
    assert_eq!(ultimo.0, "ERRO");
    assert!(ultimo.1.starts_with("Fim: falha, duração "), "{ultimo:?}");
}

#[test]
fn comando_de_leitura_nao_cria_log() {
    let p = Pasta::nova("leitura");
    for args in [
        &["listar", "quina"][..],
        &["validar", "quina"],
        &["buracos", "quina"],
        &["calendario", "quina"],
    ] {
        let _ = sync(&p.0, args);
        assert!(!p.0.join("logs").exists(), "{args:?} criou logs/");
    }
}

#[test]
fn trava_ocupada_deixa_rastro_no_log() {
    let p = Pasta::nova("trava");
    let _dono = Trava::obter(&p.0, "ciclo", AGORA).unwrap();

    let saida = sync(&p.0, &["features", "quina"]);
    assert!(!saida.status.success());

    let (_, linhas) = ler_log(&p.0);
    let tudo = linhas.join("\n");
    assert!(
        tudo.contains("ERRO  Error: outra execução em andamento: PID"),
        "{tudo}"
    );
    assert!(tudo.contains("Fim: falha"), "{tudo}");
}

#[test]
fn log_que_nao_abre_avisa_na_tela_e_o_comando_segue() {
    let p = Pasta::nova("sem_log");
    std::fs::write(p.0.join("quina.xlsx"), comum::bytes_xlsx(&comum::quina())).unwrap();
    // Um arquivo no lugar da pasta `logs`.
    std::fs::write(p.0.join("logs"), "sou arquivo").unwrap();

    let saida = sync(&p.0, &["importar", "quina", "quina.xlsx"]);

    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let erro = texto(&saida.stderr);
    assert!(
        erro.contains("Aviso: não foi possível abrir o log em arquivo; o comando segue sem log"),
        "{erro}"
    );
    assert!(p.0.join("resultados_quina.db").exists());
}
