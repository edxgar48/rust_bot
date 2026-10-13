//! Log em arquivo (tarefa 3.5): `logs/sync_AAAA-MM-DD.log`.
//!
//! Os comandos que gravam (os mesmos que tomam a trava, ver `trava`) copiam
//! para o arquivo tudo o que sai na tela. Cada linha leva a hora de Brasília
//! (`HH:MM:SS`) e o nível; a execução tem uma linha de abertura e uma de
//! fechamento. A biblioteca não imprime: este módulo só escreve no arquivo, e
//! quem mostra na tela (a CLI) usa o mesmo caminho para os dois.
//!
//! ```text
//! 23:00:01 INFO  Início: PID 4120, comando `sync ciclo todas --aguardar`, versão 0.1.0
//! 23:00:03 INFO  === Lotofácil (lotofacil) ===
//! 23:05:10 INFO  Aguardando lotofacil: o concurso do sorteio de 2026-10-13 ainda não saiu ...
//! 00:12:44 ERRO  Erro na publicação: ...
//! 00:12:45 ERRO  Fim: falha, duração 1 h 12 min 44 s
//! ```
//!
//! **Níveis** (regra única, ver `Evento::nivel` e `EventoCiclo::nivel`):
//! - `INFO`: andamento normal e resultado esperado (inclui "ainda aguardando o
//!   concurso", que é o que `--aguardar` faz);
//! - `AVISO`: o comando seguiu, mas há algo que o operador deveria ver (aviso
//!   da validação, buraco, concurso que não saiu, falha transitória que será
//!   repetida, divergência de dado manual/correção);
//! - `ERRO`: falha de uma etapa, de uma loteria ou do comando, incluindo a
//!   cadeia completa de causas do erro final.
//!
//! **Data do arquivo**: a do *início* da execução em Brasília; a noite do
//! `ciclo --aguardar` (23h às 01h) cai num arquivo só. Aberto em modo de
//! acréscimo, então duas execuções no mesmo dia (ou uma recusada pela trava)
//! convivem no mesmo arquivo.
//!
//! **Retenção**: ao abrir, apaga os `sync_AAAA-MM-DD.log` com mais de
//! [`RETENCAO_DIAS`] dias (pela data do nome). Arquivos com outro nome ficam.
//!
//! **Log que não grava** nunca derruba o comando: abrir ou escrever com falha
//! devolve um erro para a CLI avisar na tela, e depois da primeira falha de
//! escrita o log se desliga (avisa uma vez só).
//!
//! O relógio é injetado (segundos Unix), como em `trava` e `tempo`.

use crate::calendario::FUSO_SEGUNDOS;
use crate::error::{Result, SpiderError};
use crate::tempo;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Pasta dos logs, na pasta de trabalho (a mesma do `sync.lock`).
pub const PASTA: &str = "logs";

/// Logs com mais dias que isto (pela data no nome) são apagados ao abrir.
pub const RETENCAO_DIAS: i64 = 90;

const PREFIXO: &str = "sync_";
const SUFIXO: &str = ".log";
const SEGUNDOS_POR_DIA: i64 = 86_400;

/// Nível de uma linha do log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nivel {
    Info,
    Aviso,
    Erro,
}

impl Nivel {
    pub fn rotulo(self) -> &'static str {
        match self {
            Nivel::Info => "INFO",
            Nivel::Aviso => "AVISO",
            Nivel::Erro => "ERRO",
        }
    }
}

/// Nome do arquivo do dia `dias` (desde 1970-01-01, relógio de Brasília).
fn nome_do_dia(dias: i64) -> String {
    format!("{PREFIXO}{}{SUFIXO}", tempo::data_iso_de_dias(dias))
}

/// Dia (desde 1970-01-01) de um nome `sync_AAAA-MM-DD.log`; `None` para
/// qualquer outro nome (esses arquivos nunca são apagados).
fn dia_do_nome(nome: &str) -> Option<i64> {
    let data = nome.strip_prefix(PREFIXO)?.strip_suffix(SUFIXO)?;
    tempo::dias_da_data_iso(data)
}

/// Duração em texto curto: `5 s`, `3 min 07 s`, `1 h 02 min 03 s`.
pub fn formatar_duracao(segundos: u64) -> String {
    let (h, m, s) = (segundos / 3600, segundos % 3600 / 60, segundos % 60);
    if h > 0 {
        format!("{h} h {m:02} min {s:02} s")
    } else if m > 0 {
        format!("{m} min {s:02} s")
    } else {
        format!("{s} s")
    }
}

/// Comando como o operador o digitou (argv), com aspas nos argumentos que
/// têm espaço.
pub fn formatar_comando(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty() || a.contains(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quem recebe os diagnósticos que a biblioteca emite no meio de uma operação
/// (as tentativas do navegador que falharam, o layout da planilha que não
/// bateu): essas mensagens não passam por `Evento` porque nascem dentro de
/// `downloader` e `parser`, longe do observador. A CLI instala aqui o mesmo
/// caminho com que mostra e registra tudo o mais.
static DIAGNOSTICO: OnceLock<fn(Nivel, &str)> = OnceLock::new();

/// Instala o destino dos diagnósticos (uma vez por processo; a segunda
/// chamada é ignorada).
///
/// Este `OnceLock` é a única exceção de estado global do projeto, aceita só
/// para a camada de I/O (`downloader`/`parser`), cujos diagnósticos nascem
/// longe do observador. Não deve virar precedente em `motor/` nem em
/// `avaliacao/`, que seguem puras: recebem dados e devolvem structs.
pub fn instalar_diagnostico(destino: fn(Nivel, &str)) {
    let _ = DIAGNOSTICO.set(destino);
}

/// Emite um diagnóstico da biblioteca. Sem destino instalado (testes,
/// chamadas fora da CLI) vai para a saída de erro, como sempre foi.
pub fn diagnosticar(nivel: Nivel, texto: &str) {
    match DIAGNOSTICO.get() {
        Some(destino) => destino(nivel, texto),
        None => eprintln!("{texto}"),
    }
}

/// Escritor do log do dia. Veja o módulo.
pub struct LogArquivo {
    /// `None` depois de uma falha de escrita: o log está desligado.
    arquivo: Option<File>,
    caminho: PathBuf,
    /// Início da execução, segundos Unix.
    inicio: i64,
    /// Última leitura do relógio, para o caso de ele falhar.
    ultimo: i64,
    relogio: Box<dyn FnMut() -> Result<u64> + Send>,
}

impl std::fmt::Debug for LogArquivo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogArquivo")
            .field("caminho", &self.caminho)
            .field("ativo", &self.arquivo.is_some())
            .finish()
    }
}

impl LogArquivo {
    /// Abre (em acréscimo) o log do dia em `pasta`, criando a pasta se
    /// preciso, e apaga os logs vencidos. O dia é o do instante de abertura
    /// em Brasília. Falha de E/S: [`SpiderError::Io`] (a CLI avisa e segue
    /// sem log). A falha de apagar um log velho é ignorada: só deixa o
    /// arquivo para a próxima vez.
    pub fn abrir(
        pasta: &Path,
        mut relogio: Box<dyn FnMut() -> Result<u64> + Send>,
    ) -> Result<LogArquivo> {
        let inicio = relogio().map(|s| i64::try_from(s).unwrap_or(i64::MAX))?;
        let hoje = (inicio + FUSO_SEGUNDOS).div_euclid(SEGUNDOS_POR_DIA);
        let caminho = pasta.join(nome_do_dia(hoje));
        let io = |origem| SpiderError::Io {
            caminho: caminho.clone(),
            origem,
        };
        std::fs::create_dir_all(pasta).map_err(io)?;
        apagar_vencidos(pasta, hoje);
        let arquivo = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&caminho)
            .map_err(io)?;
        Ok(LogArquivo {
            arquivo: Some(arquivo),
            caminho,
            inicio,
            ultimo: inicio,
            relogio,
        })
    }

    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// `false` depois de uma falha de escrita.
    pub fn ativo(&self) -> bool {
        self.arquivo.is_some()
    }

    /// Linha de abertura: PID, comando completo (argv) e versão.
    pub fn abertura(&mut self, pid: u32, argv: &[String], versao: &str) -> Result<()> {
        let texto = format!(
            "Início: PID {pid}, comando `{}`, versão {versao}",
            formatar_comando(argv)
        );
        self.registrar(Nivel::Info, &texto)
    }

    /// Linha de fechamento: duração e sucesso ou falha.
    pub fn fechamento(&mut self, sucesso: bool) -> Result<()> {
        let agora = self.agora();
        let duracao = u64::try_from(agora - self.inicio).unwrap_or(0);
        let (nivel, situacao) = if sucesso {
            (Nivel::Info, "sucesso")
        } else {
            (Nivel::Erro, "falha")
        };
        let texto = format!("Fim: {situacao}, duração {}", formatar_duracao(duracao));
        self.registrar(nivel, &texto)
    }

    /// Grava `texto` com a hora de agora e o nível; um texto de várias linhas
    /// vira várias linhas do log (cada uma com hora e nível) e linhas vazias
    /// não são gravadas. Log desligado: não faz nada. A primeira falha de
    /// escrita desliga o log e devolve o erro (para a CLI avisar uma vez); as
    /// chamadas seguintes devolvem `Ok`.
    pub fn registrar(&mut self, nivel: Nivel, texto: &str) -> Result<()> {
        let hora = self.hora();
        let Some(arquivo) = self.arquivo.as_mut() else {
            return Ok(());
        };
        let mut bloco = String::new();
        for linha in texto.lines().filter(|l| !l.trim().is_empty()) {
            bloco.push_str(&format!("{hora} {:<5} {linha}\n", nivel.rotulo()));
        }
        if bloco.is_empty() {
            return Ok(());
        }
        // Uma escrita só por chamada: duas execuções no mesmo arquivo (a
        // recusada pela trava) não misturam o meio de uma mensagem.
        if let Err(origem) = arquivo.write_all(bloco.as_bytes()) {
            self.arquivo = None;
            return Err(SpiderError::Io {
                caminho: self.caminho.clone(),
                origem,
            });
        }
        Ok(())
    }

    fn agora(&mut self) -> i64 {
        if let Ok(s) = (self.relogio)() {
            self.ultimo = i64::try_from(s).unwrap_or(i64::MAX);
        }
        self.ultimo
    }

    /// `HH:MM:SS` de Brasília.
    fn hora(&mut self) -> String {
        let local = (self.agora() + FUSO_SEGUNDOS).rem_euclid(SEGUNDOS_POR_DIA);
        format!(
            "{:02}:{:02}:{:02}",
            local / 3600,
            local % 3600 / 60,
            local % 60
        )
    }
}

/// Apaga `sync_AAAA-MM-DD.log` com mais de [`RETENCAO_DIAS`] dias em relação
/// a `hoje`. Só toca em arquivos com exatamente esse padrão de nome.
fn apagar_vencidos(pasta: &Path, hoje: i64) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for entrada in entradas.flatten() {
        let nome = entrada.file_name();
        let vencido = nome
            .to_str()
            .and_then(dia_do_nome)
            .is_some_and(|dia| hoje - dia > RETENCAO_DIAS);
        if vencido && entrada.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(entrada.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    /// 2026-10-13T23:00:00-03:00 = 2026-10-14T02:00:00Z.
    const NOITE: u64 = 1_791_943_200;

    /// Pasta temporária apagada ao sair do teste (mesmo se ele falhar).
    struct Pasta(PathBuf);

    impl std::ops::Deref for Pasta {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl AsRef<Path> for Pasta {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Pasta {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn pasta(rotulo: &str) -> Pasta {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        Pasta(std::env::temp_dir().join(format!(
            "rust_spider_log_{rotulo}_{}_{unico}",
            std::process::id()
        )))
    }

    fn relogio_fixo(s: u64) -> Box<dyn FnMut() -> Result<u64> + Send> {
        Box::new(move || Ok(s))
    }

    /// Relógio que o teste avança de fora.
    fn relogio_manual(inicial: u64) -> (Arc<AtomicU64>, Box<dyn FnMut() -> Result<u64> + Send>) {
        let marca = Arc::new(AtomicU64::new(inicial));
        let lida = Arc::clone(&marca);
        (marca, Box::new(move || Ok(lida.load(Ordering::SeqCst))))
    }

    fn ler(p: &Path, dia: &str) -> String {
        std::fs::read_to_string(p.join(format!("sync_{dia}.log"))).unwrap()
    }

    #[test]
    fn linha_tem_hora_de_brasilia_nivel_e_texto() {
        let p = pasta("formato");
        let mut log = LogArquivo::abrir(&p, relogio_fixo(NOITE)).unwrap();
        log.registrar(Nivel::Info, "tudo certo").unwrap();
        log.registrar(Nivel::Aviso, "olhe isto").unwrap();
        log.registrar(Nivel::Erro, "quebrou").unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 INFO  tudo certo\n23:00:00 AVISO olhe isto\n23:00:00 ERRO  quebrou\n"
        );
    }

    #[test]
    fn texto_de_varias_linhas_vira_varias_linhas_sem_as_vazias() {
        let p = pasta("multilinha");
        let mut log = LogArquivo::abrir(&p, relogio_fixo(NOITE)).unwrap();
        log.registrar(Nivel::Erro, "falhou\n\nCaused by:\n    disco cheio\n")
            .unwrap();
        log.registrar(Nivel::Info, "").unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 ERRO  falhou\n23:00:00 ERRO  Caused by:\n23:00:00 ERRO      disco cheio\n"
        );
    }

    #[test]
    fn abertura_e_fechamento_trazem_pid_comando_versao_e_duracao() {
        let p = pasta("abertura");
        let (marca, relogio) = relogio_manual(NOITE);
        let mut log = LogArquivo::abrir(&p, relogio).unwrap();
        let argv: Vec<String> = ["sync", "ciclo", "todas", "--dir", "meus downloads"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        log.abertura(4120, &argv, "0.1.0").unwrap();
        marca.store(NOITE + 3723, Ordering::SeqCst);
        log.fechamento(true).unwrap();
        log.fechamento(false).unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 INFO  Início: PID 4120, comando `sync ciclo todas --dir \"meus downloads\"`, versão 0.1.0\n\
             00:02:03 INFO  Fim: sucesso, duração 1 h 02 min 03 s\n\
             00:02:03 ERRO  Fim: falha, duração 1 h 02 min 03 s\n"
        );
    }

    #[test]
    fn duracoes_em_texto_curto() {
        assert_eq!(formatar_duracao(0), "0 s");
        assert_eq!(formatar_duracao(59), "59 s");
        assert_eq!(formatar_duracao(187), "3 min 07 s");
        assert_eq!(formatar_duracao(3723), "1 h 02 min 03 s");
    }

    #[test]
    fn abre_em_acrescimo_sem_apagar_o_que_ja_estava() {
        let p = pasta("acrescimo");
        let mut a = LogArquivo::abrir(&p, relogio_fixo(NOITE)).unwrap();
        a.registrar(Nivel::Info, "primeira").unwrap();
        drop(a);
        let mut b = LogArquivo::abrir(&p, relogio_fixo(NOITE + 60)).unwrap();
        b.registrar(Nivel::Info, "segunda").unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 INFO  primeira\n23:01:00 INFO  segunda\n"
        );
    }

    #[test]
    fn o_arquivo_e_o_dia_do_inicio_mesmo_cruzando_a_meia_noite() {
        let p = pasta("meia_noite");
        let (marca, relogio) = relogio_manual(NOITE);
        let mut log = LogArquivo::abrir(&p, relogio).unwrap();
        log.registrar(Nivel::Info, "antes").unwrap();
        // 00:30 de 14/10 em Brasília.
        marca.store(NOITE + 5400, Ordering::SeqCst);
        log.registrar(Nivel::Info, "depois").unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 INFO  antes\n00:30:00 INFO  depois\n"
        );
        assert!(!p.join("sync_2026-10-14.log").exists());
    }

    #[test]
    fn o_dia_vem_do_relogio_de_brasilia_e_nao_do_utc() {
        // 2026-10-14T02:00Z ainda é 13/10 em Brasília (acima); 2026-10-14T03:00Z
        // já é 14/10 em Brasília.
        let p = pasta("dia_brasilia");
        let mut log = LogArquivo::abrir(&p, relogio_fixo(NOITE + 3600)).unwrap();
        log.registrar(Nivel::Info, "x").unwrap();
        assert_eq!(ler(&p, "2026-10-14"), "00:00:00 INFO  x\n");
    }

    #[test]
    fn retencao_apaga_so_logs_com_mais_de_90_dias_pelo_nome() {
        let p = pasta("retencao");
        std::fs::create_dir_all(&p).unwrap();
        // Hoje = 2026-10-13. 90 dias antes = 2026-07-15 (fica); 91 = 07-14 (sai).
        for nome in [
            "sync_2026-07-14.log",
            "sync_2025-01-01.log",
            "sync_2026-07-15.log",
            "sync_2026-10-12.log",
            "sync_2026-10-14.log",
            "sync_2020-13-45.log",
            "sync_2020-01-01.txt",
            "outro_2020-01-01.log",
            "sync_2020-01-01.log.bak",
            "anotacoes.md",
        ] {
            std::fs::write(p.join(nome), "x").unwrap();
        }
        // Pasta com nome de log velho não é arquivo: fica.
        std::fs::create_dir_all(p.join("sync_2019-01-01.log")).unwrap();
        let _log = LogArquivo::abrir(&p, relogio_fixo(NOITE)).unwrap();
        let mut restam: Vec<String> = std::fs::read_dir(&p)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        restam.sort();
        assert_eq!(
            restam,
            [
                "anotacoes.md",
                "outro_2020-01-01.log",
                "sync_2019-01-01.log",
                "sync_2020-01-01.log.bak",
                "sync_2020-01-01.txt",
                "sync_2020-13-45.log",
                "sync_2026-07-15.log",
                "sync_2026-10-12.log",
                "sync_2026-10-13.log",
                "sync_2026-10-14.log",
            ]
        );
    }

    #[test]
    fn abrir_com_pasta_impossivel_devolve_erro_e_nao_panica() {
        let p = pasta("impossivel");
        std::fs::create_dir_all(&p).unwrap();
        // Um arquivo no lugar da pasta `logs`.
        let arquivo = p.join("logs");
        std::fs::write(&arquivo, "sou arquivo").unwrap();
        let erro = LogArquivo::abrir(&arquivo, relogio_fixo(NOITE)).unwrap_err();
        assert!(matches!(erro, SpiderError::Io { .. }), "{erro:?}");
    }

    #[test]
    fn falha_de_escrita_avisa_uma_vez_e_desliga_sem_derrubar() {
        let p = pasta("escrita");
        let mut log = LogArquivo::abrir(&p, relogio_fixo(NOITE)).unwrap();
        // Simula o disco que passa a recusar a escrita: arquivo aberto só
        // para leitura.
        let somente_leitura = File::open(log.caminho()).unwrap();
        log.arquivo = Some(somente_leitura);
        let primeiro = log.registrar(Nivel::Info, "x");
        assert!(
            matches!(primeiro, Err(SpiderError::Io { .. })),
            "{primeiro:?}"
        );
        assert!(!log.ativo());
        // Depois disso, nada mais falha nem escreve.
        log.registrar(Nivel::Erro, "y").unwrap();
        log.fechamento(false).unwrap();
        assert_eq!(ler(&p, "2026-10-13"), "");
    }

    #[test]
    fn relogio_que_falha_depois_de_aberto_repete_a_ultima_hora() {
        let p = pasta("relogio_falha");
        let mut chamadas = 0;
        let relogio = Box::new(move || {
            chamadas += 1;
            if chamadas <= 2 {
                Ok(NOITE)
            } else {
                Err(SpiderError::RelogioInvalido)
            }
        });
        let mut log = LogArquivo::abrir(&p, relogio).unwrap();
        log.registrar(Nivel::Info, "a").unwrap();
        log.registrar(Nivel::Info, "b").unwrap();
        assert_eq!(
            ler(&p, "2026-10-13"),
            "23:00:00 INFO  a\n23:00:00 INFO  b\n"
        );
    }

    #[test]
    fn relogio_que_falha_na_abertura_e_erro() {
        let p = pasta("relogio_abertura");
        let relogio = Box::new(|| Err(SpiderError::RelogioInvalido));
        assert!(LogArquivo::abrir(&p, relogio).is_err());
    }
}
