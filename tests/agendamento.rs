//! Agendamento no Windows (3.7): sintaxe dos dois `.ps1`, `ciclo_agendado.ps1`
//! com um binário FALSO numa pasta temporária e `registrar_tarefa.ps1 -Mostrar`.
//! Nada aqui toca no Agendador de Tarefas nem nos bancos reais. Pulados quando
//! não há `powershell.exe` (outro sistema).
//!
//! O binário falso é um `.cmd` (o `.ps1` o chama com `&`, como faria com o
//! `.exe`): grava pasta atual, `GIT_SSH_COMMAND` e argumentos em `falso.txt` e
//! sai com o código de `FALSO_SAIDA`. É o mais simples e não exige compilar
//! nada; o `.cmd` só difere do `.exe` na forma de iniciar, não no código de saída.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tem_powershell() -> bool {
    let ok = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "exit 0"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("powershell.exe indisponível: teste de agendamento pulado");
    }
    ok
}

fn powershell(args: &[&str]) -> Output {
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .args(args)
        .output()
        .expect("powershell.exe")
}

fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

struct Pasta(PathBuf);

impl Pasta {
    fn nova() -> Pasta {
        static CONTADOR: AtomicU32 = AtomicU32::new(0);
        let caminho = std::env::temp_dir().join(format!(
            "rust_spider_agend_it_{}_{}",
            std::process::id(),
            CONTADOR.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&caminho);
        std::fs::create_dir_all(&caminho).unwrap();
        Pasta(caminho)
    }

    fn falso(&self) -> PathBuf {
        let caminho = self.0.join("falso.cmd");
        std::fs::write(
            &caminho,
            "@echo off\r\n(\r\ncd\r\necho %GIT_SSH_COMMAND%\r\necho %*\r\n) > falso.txt\r\nexit /b %FALSO_SAIDA%\r\n",
        )
        .unwrap();
        caminho
    }

    /// Binário falso que registra cada chamada (uma linha com os argumentos)
    /// em `chamadas.txt`. `alerta ...` sai com 0; o resto sai com o código de
    /// `FALSO_SAIDA` e, se `FALSO_RASCUNHO` estiver definida, cria
    /// `alertas\<nome>` como o `sync` faria ao gerar um rascunho.
    fn falso_registrador(&self) -> PathBuf {
        let caminho = self.0.join("falso.cmd");
        std::fs::write(
            &caminho,
            "@echo off\r\n\
             echo %* >> chamadas.txt\r\n\
             if \"%1\"==\"alerta\" exit /b 0\r\n\
             if defined FALSO_RASCUNHO (\r\n\
             if not exist alertas mkdir alertas\r\n\
             echo rascunho> alertas\\%FALSO_RASCUNHO%\r\n\
             )\r\n\
             exit /b %FALSO_SAIDA%\r\n",
        )
        .unwrap();
        caminho
    }

    /// Linhas de `chamadas.txt` (sem espaços nas pontas).
    fn chamadas(&self) -> Vec<String> {
        std::fs::read_to_string(self.0.join("chamadas.txt"))
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .collect()
    }

    fn log_agendador(&self) -> String {
        std::fs::read_to_string(self.0.join("logs").join("agendador.log")).unwrap_or_default()
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn rodar_ciclo(pasta: &Path, binario: &Path, argumentos: &str, saida: &str) -> Output {
    rodar_ciclo_com(pasta, binario, argumentos, saida, &[])
}

fn rodar_ciclo_com(
    pasta: &Path,
    binario: &Path,
    argumentos: &str,
    saida: &str,
    extras: &[(&str, &str)],
) -> Output {
    let script = raiz().join("scripts").join("ciclo_agendado.ps1");
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
        ])
        .arg("-File")
        .arg(script)
        .arg("-Pasta")
        .arg(pasta)
        .arg("-Binario")
        .arg(binario)
        .args(["-Argumentos", argumentos])
        .env("FALSO_SAIDA", saida)
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("FALSO_RASCUNHO")
        .envs(extras.iter().copied())
        .output()
        .expect("powershell.exe")
}

#[test]
fn scripts_tem_sintaxe_valida() {
    if !tem_powershell() {
        return;
    }
    for nome in ["ciclo_agendado.ps1", "registrar_tarefa.ps1"] {
        let caminho = raiz().join("scripts").join(nome);
        let comando = format!(
            "$e = $null; $t = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('{}', [ref]$t, [ref]$e); \
             if ($e -and $e.Count -gt 0) {{ $e | ForEach-Object {{ $_.ToString() }}; exit 1 }}; exit 0",
            caminho.display().to_string().replace('\'', "''")
        );
        let o = powershell(&["-Command", &comando]);
        assert!(
            o.status.success(),
            "{nome} com erro de sintaxe: {}",
            texto(&o)
        );
    }
}

#[test]
fn ciclo_agendado_repassa_pasta_ssh_argumentos_e_codigo() {
    if !tem_powershell() {
        return;
    }
    for (saida, codigo) in [("0", 0), ("7", 7)] {
        let pasta = Pasta::nova();
        let falso = pasta.falso();
        let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", saida);
        assert_eq!(o.status.code(), Some(codigo), "{}", texto(&o));

        let visto = std::fs::read_to_string(pasta.0.join("falso.txt")).unwrap();
        let linhas: Vec<&str> = visto.lines().collect();
        let esperada = std::fs::canonicalize(&pasta.0).unwrap();
        let atual = std::fs::canonicalize(linhas[0].trim()).unwrap();
        assert_eq!(atual, esperada, "diretório de trabalho");
        assert!(linhas[1].contains("BatchMode=yes"), "{}", linhas[1]);
        assert!(linhas[1].contains("ConnectTimeout=30"), "{}", linhas[1]);
        assert_eq!(linhas[2].trim(), "ciclo todas --aguardar");

        let log = pasta.log_agendador();
        let linhas_log: Vec<&str> = log.lines().collect();
        assert_eq!(linhas_log.len(), 2, "início e fim por execução: {log}");
        assert!(linhas_log[0].contains(" inicio "), "{log}");
        assert!(linhas_log[1].contains(&format!("exit {codigo}")), "{log}");
        assert!(!log.starts_with('\u{feff}'), "log sem BOM: {log}");
    }
}

#[test]
fn ciclo_agendado_com_binario_ausente_falha_e_deixa_rastro() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let o = rodar_ciclo(&pasta.0, &pasta.0.join("nao_existe.exe"), "ciclo", "0");
    assert_ne!(o.status.code(), Some(0), "{}", texto(&o));
    assert!(
        texto(&o).contains("binario nao encontrado"),
        "{}",
        texto(&o)
    );
    let log = pasta.log_agendador();
    assert!(log.contains("binario ausente"), "{log}");
    assert!(!pasta.0.join("falso.txt").exists());
}

#[test]
fn registrar_tarefa_mostrar_imprime_definicao_sem_registrar() {
    if !tem_powershell() {
        return;
    }
    let script = raiz().join("scripts").join("registrar_tarefa.ps1");
    let o = powershell(&["-File", script.to_str().unwrap(), "-Mostrar"]);
    let t = texto(&o);
    assert!(o.status.success(), "{t}");
    for esperado in [
        "23:00",
        "S4U",
        "StartWhenAvailable",
        "IgnoreNew",
        "ExecutionTimeLimit 3h",
        "ciclo_agendado.ps1",
        "nada foi registrado",
    ] {
        assert!(t.contains(esperado), "faltou `{esperado}` em: {t}");
    }
}

#[test]
fn registrar_tarefa_mostrar_so_conectado_usa_logon_interativo() {
    if !tem_powershell() {
        return;
    }
    let script = raiz().join("scripts").join("registrar_tarefa.ps1");
    let o = powershell(&[
        "-File",
        script.to_str().unwrap(),
        "-Mostrar",
        "-SoConectado",
    ]);
    let t = texto(&o);
    assert!(o.status.success(), "{t}");
    for esperado in [
        "LogonType Interactive",
        "-WindowStyle Hidden",
        "23:00",
        "nada foi registrado",
    ] {
        assert!(t.contains(esperado), "faltou `{esperado}` em: {t}");
    }
    assert!(!t.contains("S4U"), "{t}");
}

#[test]
fn registrar_tarefa_mostrar_normaliza_pasta_com_barra_final() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    // canonicalize devolve o prefixo `\\?\`; o script usa o caminho comum.
    let absoluta = std::fs::canonicalize(&pasta.0).unwrap();
    let absoluta = absoluta
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    let com_barra = format!("{}\\", pasta.0.display());
    let script = raiz().join("scripts").join("registrar_tarefa.ps1");
    let o = powershell(&[
        "-File",
        script.to_str().unwrap(),
        "-Mostrar",
        "-Pasta",
        &com_barra,
    ]);
    let t = texto(&o);
    assert!(o.status.success(), "{t}");
    let acao = t.lines().find(|l| l.starts_with("Acao:")).expect(&t);
    assert!(!acao.contains("\\\""), "aspa escapada na ação: {acao}");
    assert!(
        acao.contains(&format!("-Pasta \"{absoluta}\"")),
        "esperava `{absoluta}` sem barra final em: {acao}"
    );
}

#[test]
fn registrar_tarefa_mostrar_com_pasta_inexistente_falha() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let inexistente = pasta.0.join("nao_existe");
    let script = raiz().join("scripts").join("registrar_tarefa.ps1");
    let o = powershell(&[
        "-File",
        script.to_str().unwrap(),
        "-Mostrar",
        "-Pasta",
        inexistente.to_str().unwrap(),
    ]);
    assert_ne!(o.status.code(), Some(0), "{}", texto(&o));
}

// ---------------------------------------------------------------------------
// Alertas (3.6): trava ocupada, processo morto e rascunho novo
// ---------------------------------------------------------------------------

#[test]
fn ciclo_agendado_com_exit_3_chama_alerta_geral_e_devolve_3() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "3");
    assert_eq!(o.status.code(), Some(3), "{}", texto(&o));

    let chamadas = pasta.chamadas();
    assert_eq!(chamadas.len(), 2, "{chamadas:?}");
    assert_eq!(chamadas[0], "ciclo todas --aguardar");
    assert!(
        chamadas[1].starts_with("alerta --geral \"trava ocupada: "),
        "{}",
        chamadas[1]
    );
    let log = pasta.log_agendador();
    assert!(log.lines().last().unwrap().contains("exit 3"), "{log}");
}

#[test]
fn ciclo_agendado_com_sucesso_ou_outras_falhas_nao_chama_alerta() {
    if !tem_powershell() {
        return;
    }
    for saida in ["0", "1", "7"] {
        let pasta = Pasta::nova();
        let falso = pasta.falso_registrador();
        let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", saida);
        assert_eq!(
            o.status.code(),
            Some(saida.parse().unwrap()),
            "{}",
            texto(&o)
        );
        assert_eq!(
            pasta.chamadas(),
            ["ciclo todas --aguardar"],
            "saída {saida}"
        );
        assert_eq!(pasta.log_agendador().lines().count(), 2);
    }
}

#[test]
fn ciclo_agendado_acha_processo_morto_avisa_uma_vez_e_segue() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    // Execução anterior que começou e nunca terminou: PID que não existe.
    std::fs::create_dir_all(pasta.0.join("logs")).unwrap();
    std::fs::write(
        pasta.0.join("logs").join("agendador.log"),
        "2026-10-13 23:00:01 inicio | pasta=x | binario=y | args=z | pid=2147483000\r\n",
    )
    .unwrap();

    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "0");
    assert_eq!(o.status.code(), Some(0), "{}", texto(&o));
    let chamadas = pasta.chamadas();
    assert_eq!(chamadas.len(), 2, "{chamadas:?}");
    assert!(
        chamadas[0].starts_with(
            "alerta --geral \"processo morto: a execucao iniciada em 2026-10-13 23:00:01"
        ),
        "{}",
        chamadas[0]
    );
    assert_eq!(chamadas[1], "ciclo todas --aguardar");
    let log = pasta.log_agendador();
    assert!(log.contains("AVISO processo morto"), "{log}");
    assert!(texto(&o).contains("processo morto"), "{}", texto(&o));

    // A linha pendente foi fechada: a próxima execução não avisa de novo.
    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "0");
    assert_eq!(o.status.code(), Some(0), "{}", texto(&o));
    let alertas = pasta
        .chamadas()
        .into_iter()
        .filter(|c| c.starts_with("alerta "))
        .count();
    assert_eq!(alertas, 1, "{:?}", pasta.chamadas());
}

#[test]
fn ciclo_agendado_nao_chama_de_morto_um_processo_vivo() {
    if !tem_powershell() {
        return;
    }
    // Um PowerShell vivo faz o papel da execução anterior ainda em andamento.
    let mut vivo = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 60",
        ])
        .spawn()
        .expect("powershell vivo");
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    std::fs::create_dir_all(pasta.0.join("logs")).unwrap();
    std::fs::write(
        pasta.0.join("logs").join("agendador.log"),
        // O processo vivo nasceu antes do `inicio` registrado (data futura).
        format!(
            "2099-01-01 00:00:01 inicio | pasta=x | binario=y | args=z | pid={}\r\n",
            vivo.id()
        ),
    )
    .unwrap();

    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "0");
    let _ = vivo.kill();
    let _ = vivo.wait();
    assert_eq!(o.status.code(), Some(0), "{}", texto(&o));
    assert_eq!(pasta.chamadas(), ["ciclo todas --aguardar"]);
    assert!(!pasta.log_agendador().contains("processo morto"));
}

#[test]
fn ciclo_agendado_pid_reaproveitado_por_powershell_mais_novo_conta_como_morto() {
    if !tem_powershell() {
        return;
    }
    // Um PowerShell vivo, mas que nasceu depois do `inicio` registrado: o PID
    // da execucao anterior foi reaproveitado.
    let mut intruso = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 60",
        ])
        .spawn()
        .expect("powershell vivo");
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    std::fs::create_dir_all(pasta.0.join("logs")).unwrap();
    std::fs::write(
        pasta.0.join("logs").join("agendador.log"),
        format!(
            "2000-01-01 00:00:01 inicio | pasta=x | binario=y | args=z | pid={}\r\n",
            intruso.id()
        ),
    )
    .unwrap();

    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "0");
    let _ = intruso.kill();
    let _ = intruso.wait();
    assert_eq!(o.status.code(), Some(0), "{}", texto(&o));
    assert!(
        pasta.chamadas()[0].starts_with("alerta --geral \"processo morto"),
        "{:?}",
        pasta.chamadas()
    );
    assert!(pasta.log_agendador().contains("AVISO processo morto"));
}

#[test]
fn ciclo_agendado_nao_avalia_linha_de_inicio_sem_pid() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    std::fs::create_dir_all(pasta.0.join("logs")).unwrap();
    std::fs::write(
        pasta.0.join("logs").join("agendador.log"),
        "2026-10-13 23:00:01 inicio | pasta=x | binario=y | args=z\r\n",
    )
    .unwrap();
    let o = rodar_ciclo(&pasta.0, &falso, "ciclo todas --aguardar", "0");
    assert_eq!(o.status.code(), Some(0), "{}", texto(&o));
    assert_eq!(pasta.chamadas(), ["ciclo todas --aguardar"]);
}

#[test]
fn ciclo_agendado_avisa_so_do_rascunho_novo_no_log_e_na_tela() {
    if !tem_powershell() {
        return;
    }
    let pasta = Pasta::nova();
    let falso = pasta.falso_registrador();
    // Um rascunho antigo, ainda pendente, não é "novo".
    std::fs::create_dir_all(pasta.0.join("alertas")).unwrap();
    std::fs::write(pasta.0.join("alertas").join("velho.eml"), "x").unwrap();

    let o = rodar_ciclo_com(
        &pasta.0,
        &falso,
        "ciclo todas --aguardar",
        "1",
        &[("FALSO_RASCUNHO", "2026-10-14_000500_quina-site-mudou.eml")],
    );
    assert_eq!(o.status.code(), Some(1), "{}", texto(&o));
    let log = pasta.log_agendador();
    let linhas: Vec<&str> = log.lines().collect();
    assert_eq!(linhas.len(), 3, "início, aviso e fim: {log}");
    assert!(
        linhas[1].contains(
            "AVISO: rascunho de alerta novo: alertas\\2026-10-14_000500_quina-site-mudou.eml"
        ),
        "{log}"
    );
    assert!(!log.contains("velho.eml"), "{log}");
    assert!(linhas[2].contains("exit 1"), "{log}");
    assert!(
        texto(&o).contains("AVISO: rascunho de alerta novo"),
        "{}",
        texto(&o)
    );
}
