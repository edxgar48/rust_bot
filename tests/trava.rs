//! Trava `sync.lock` (3.4) entre processos de verdade: um processo filho segura
//! a trava e o pai (a biblioteca e o binário `sync`) tenta a sua. Cobre o que o
//! teste unitário não alcança: trava obrigatória do Windows entre processos,
//! mensagem com os dados do dono e a trava que some quando o dono morre.

use rust_spider::trava::{Trava, NOME_ARQUIVO};
use rust_spider::SpiderError;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const AGORA: i64 = 1_791_943_200; // 2026-10-13 23:00 em Brasília
const VARIAVEL_PASTA: &str = "RUST_SPIDER_TRAVA_FILHO";

fn pasta(rotulo: &str) -> PathBuf {
    let unico = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let p = std::env::temp_dir().join(format!(
        "rust_spider_trava_it_{rotulo}_{}_{unico}",
        std::process::id()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Não é um teste de verdade: roda só no processo filho (com a variável de
/// ambiente), toma a trava, avisa e espera o pai fechar o stdin.
#[test]
fn filho_segura_a_trava() {
    let Ok(p) = std::env::var(VARIAVEL_PASTA) else {
        return;
    };
    let _trava = Trava::obter(Path::new(&p), "ciclo", AGORA).unwrap();
    println!("PRONTO");
    let mut resto = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut resto);
}

/// Sobe o filho e só volta quando ele já tem a trava.
fn filho_com_trava(p: &Path) -> Child {
    let mut filho = Command::new(std::env::current_exe().unwrap())
        .args(["filho_segura_a_trava", "--exact", "--nocapture"])
        .env(VARIAVEL_PASTA, p)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let saida = filho.stdout.take().unwrap();
    let mut linhas = BufReader::new(saida).lines();
    loop {
        let linha = linhas.next().expect("filho terminou sem tomar a trava");
        if linha.unwrap().trim() == "PRONTO" {
            break;
        }
    }
    // Continua drenando o stdout para o filho não travar ao escrever.
    std::thread::spawn(move || for _ in linhas {});
    filho
}

#[test]
fn trava_de_outro_processo_barra_com_pid_e_comando_e_morte_do_dono_libera() {
    let p = pasta("processo");
    let mut filho = filho_com_trava(&p);

    let erro = Trava::obter(&p, "exportar", AGORA + 60).unwrap_err();
    let SpiderError::TravaOcupada { dono } = &erro else {
        panic!("esperava TravaOcupada, veio {erro:?}");
    };
    let dono = dono.as_ref().expect("dono legível por outro processo");
    assert_eq!(dono.pid, filho.id());
    assert_eq!(dono.comando, "ciclo");
    assert_eq!(
        erro.to_string(),
        format!(
            "outra execução em andamento: PID {}, comando ciclo, desde 23:00",
            filho.id()
        )
    );

    // Dono morto à força (sem Drop): a trava do SO some sozinha.
    filho.kill().unwrap();
    filho.wait().unwrap();
    let nova = Trava::obter(&p, "baixar", AGORA + 120).unwrap();
    drop(nova);
}

#[test]
fn binario_sync_comando_que_grava_falha_na_hora_e_o_de_leitura_segue() {
    let p = pasta("binario");
    let mut filho = filho_com_trava(&p);
    let sync = env!("CARGO_BIN_EXE_sync");

    for args in [
        &["features", "quina"][..],
        &["atualizar", "quina"],
        &["publicar"],
        &["avaliar", "quina", "--perfil", "padrao_v1", "--trancado"],
    ] {
        let saida = Command::new(sync)
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap();
        let erro = String::from_utf8_lossy(&saida.stderr);
        assert!(!saida.status.success(), "{args:?} deveria falhar");
        assert!(
            erro.contains(&format!(
                "outra execução em andamento: PID {}, comando ciclo, desde 23:00",
                filho.id()
            )),
            "{args:?}: {erro}"
        );
    }

    // Comando só de leitura não pede a trava: o erro (banco ausente) é outro.
    for args in [&["listar", "quina"][..], &["validar", "quina"]] {
        let saida = Command::new(sync)
            .args(args)
            .current_dir(&p)
            .output()
            .unwrap();
        let erro = String::from_utf8_lossy(&saida.stderr);
        assert!(!erro.contains("outra execução em andamento"), "{erro}");
    }

    // O dono segue com a trava depois de tudo isso.
    assert!(p.join(NOME_ARQUIVO).exists());
    filho.kill().unwrap();
    filho.wait().unwrap();
}
