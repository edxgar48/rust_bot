//! Trava contra execução simultânea (tarefa 3.4).
//!
//! Os comandos que gravam (`baixar`, `atualizar`, `ciclo`...) tomam, antes de
//! qualquer gravação, uma trava do sistema operacional sobre o arquivo
//! `sync.lock` e a mantêm até o fim do comando. Uma trava só, na pasta de
//! trabalho: `publico/` e o clone de dados são comuns às três loterias.
//!
//! Por que trava do SO (`File::try_lock`) e não "existe o arquivo": ela some
//! sozinha se o processo morrer ou a máquina reiniciar, então não há trava
//! órfã para detectar nem apagar à mão.
//!
//! Quem encontra a trava ocupada falha na hora, sem esperar
//! ([`SpiderError::TravaOcupada`]); a mensagem diz quem a tem.
//!
//! **Onde ficam PID, comando e início**: no Windows a trava é obrigatória
//! (outro processo não consegue ler o `sync.lock` travado), então os dados do
//! dono ficam num arquivo ao lado, `sync.lock.dono`, que ninguém trava. Ele só
//! serve à mensagem: quem toma a trava o reescreve, e quem a solta o apaga.
//! Se faltar ou não for legível, a mensagem sai sem detalhes; a decisão de
//! falhar nunca depende dele.

use crate::calendario;
use crate::error::{Result, SpiderError};
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Nome do arquivo de trava, na pasta de trabalho.
pub const NOME_ARQUIVO: &str = "sync.lock";

/// Sufixo do arquivo com os dados do dono (só para a mensagem).
const SUFIXO_DONO: &str = ".dono";

/// Quem tem a trava, como gravado por ela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DonoTrava {
    pub pid: u32,
    pub comando: String,
    /// Início da execução, segundos Unix.
    pub desde: i64,
}

impl DonoTrava {
    /// "PID 12, comando ciclo, desde 23:00" (horário de Brasília).
    pub fn descricao(&self) -> String {
        let texto = calendario::instante_brasilia(self.desde);
        let hora = texto.get(11..16).unwrap_or(&texto);
        format!("PID {}, comando {}, desde {hora}", self.pid, self.comando)
    }

    fn serializar(&self) -> String {
        format!(
            "pid={}\ncomando={}\ndesde={}\n",
            self.pid, self.comando, self.desde
        )
    }

    fn ler(texto: &str) -> Option<DonoTrava> {
        let (mut pid, mut comando, mut desde) = (None, None, None);
        for linha in texto.lines() {
            match linha.split_once('=')? {
                ("pid", v) => pid = v.parse().ok(),
                ("comando", v) => comando = Some(v.to_string()),
                ("desde", v) => desde = v.parse().ok(),
                _ => {}
            }
        }
        Some(DonoTrava {
            pid: pid?,
            comando: comando?,
            desde: desde?,
        })
    }
}

/// Trava tomada: solta quando o valor sai de escopo (ou o processo morre).
#[derive(Debug)]
pub struct Trava {
    // Manter aberto mantém a trava; o SO a solta ao fechar o handle.
    _arquivo: File,
    dono: PathBuf,
}

impl Trava {
    /// Toma a trava `sync.lock` em `pasta` para o `comando`, que começou em
    /// `agora` (segundos Unix). Ocupada: [`SpiderError::TravaOcupada`], sem
    /// esperar e sem tocar no arquivo do dono.
    pub fn obter(pasta: &Path, comando: &str, agora: i64) -> Result<Trava> {
        let caminho = pasta.join(NOME_ARQUIVO);
        let io = |origem| SpiderError::Io {
            caminho: caminho.clone(),
            origem,
        };
        let arquivo = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&caminho)
            .map_err(io)?;
        match arquivo.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(SpiderError::TravaOcupada {
                    dono: ler_dono(&caminho_dono(&caminho)),
                });
            }
            Err(TryLockError::Error(e)) => return Err(io(e)),
        }
        // Gravar o dono depois de travar: o arquivo antigo (de um processo que
        // morreu) é sobrescrito, nunca confiado.
        let dono = caminho_dono(&caminho);
        let atual = DonoTrava {
            pid: std::process::id(),
            comando: comando.to_string(),
            desde: agora,
        };
        std::fs::write(&dono, atual.serializar()).map_err(|origem| SpiderError::Io {
            caminho: dono.clone(),
            origem,
        })?;
        Ok(Trava {
            _arquivo: arquivo,
            dono,
        })
    }
}

impl Drop for Trava {
    fn drop(&mut self) {
        // Apaga antes de o handle fechar (campos caem depois deste corpo):
        // quem pegar a trava em seguida reescreve. Falha aqui só deixa um
        // arquivo velho, que a próxima execução sobrescreve.
        let _ = std::fs::remove_file(&self.dono);
    }
}

fn caminho_dono(trava: &Path) -> PathBuf {
    let mut nome = trava.as_os_str().to_os_string();
    nome.push(SUFIXO_DONO);
    PathBuf::from(nome)
}

fn ler_dono(caminho: &Path) -> Option<DonoTrava> {
    DonoTrava::ler(&std::fs::read_to_string(caminho).ok()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pasta(rotulo: &str) -> PathBuf {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let p = std::env::temp_dir().join(format!(
            "rust_spider_trava_{rotulo}_{}_{unico}",
            std::process::id()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 2026-10-13T23:00:00-03:00 = 2026-10-14T02:00:00Z.
    const AGORA: i64 = 1_791_943_200;

    #[test]
    fn segunda_trava_na_mesma_pasta_falha_na_hora_dizendo_quem_tem() {
        let p = pasta("ocupada");
        let _primeira = Trava::obter(&p, "ciclo", AGORA).unwrap();
        let erro = Trava::obter(&p, "exportar", AGORA + 60).unwrap_err();
        let SpiderError::TravaOcupada { dono } = &erro else {
            panic!("esperava TravaOcupada, veio {erro:?}");
        };
        assert_eq!(
            dono.as_ref().map(|d| (d.pid, d.comando.as_str(), d.desde)),
            Some((std::process::id(), "ciclo", AGORA))
        );
        assert_eq!(
            erro.to_string(),
            format!(
                "outra execução em andamento: PID {}, comando ciclo, desde 23:00",
                std::process::id()
            )
        );
        // A tentativa que falhou não mexe nos dados de quem tem a trava.
        let dono = ler_dono(&caminho_dono(&p.join(NOME_ARQUIVO))).unwrap();
        assert_eq!(dono.comando, "ciclo");
    }

    #[test]
    fn soltar_a_trava_libera_e_apaga_os_dados_do_dono() {
        let p = pasta("soltar");
        let primeira = Trava::obter(&p, "baixar", AGORA).unwrap();
        drop(primeira);
        assert!(!caminho_dono(&p.join(NOME_ARQUIVO)).exists());
        let segunda = Trava::obter(&p, "atualizar", AGORA + 1).unwrap();
        let dono = ler_dono(&caminho_dono(&p.join(NOME_ARQUIVO))).unwrap();
        assert_eq!(dono.comando, "atualizar");
        drop(segunda);
    }

    #[test]
    fn dono_ausente_ou_ilegivel_vira_mensagem_sem_detalhes() {
        let p = pasta("sem_dono");
        let _primeira = Trava::obter(&p, "ciclo", AGORA).unwrap();
        let dono = caminho_dono(&p.join(NOME_ARQUIVO));
        for conteudo in [None, Some("lixo"), Some("pid=abc\ncomando=x\ndesde=1\n")] {
            match conteudo {
                None => {
                    let _ = std::fs::remove_file(&dono);
                }
                Some(c) => std::fs::write(&dono, c).unwrap(),
            }
            let erro = Trava::obter(&p, "exportar", AGORA).unwrap_err();
            assert!(matches!(erro, SpiderError::TravaOcupada { dono: None }));
            assert_eq!(erro.to_string(), "outra execução em andamento");
        }
    }

    #[test]
    fn arquivo_de_dono_velho_nao_atrapalha_quem_pega_a_trava_livre() {
        // Processo que morreu: a trava do SO sumiu, o arquivo do dono ficou.
        let p = pasta("orfa");
        std::fs::write(
            caminho_dono(&p.join(NOME_ARQUIVO)),
            "pid=1\ncomando=ciclo\ndesde=0\n",
        )
        .unwrap();
        std::fs::write(p.join(NOME_ARQUIVO), "").unwrap();
        let _t = Trava::obter(&p, "features", AGORA).unwrap();
        let dono = ler_dono(&caminho_dono(&p.join(NOME_ARQUIVO))).unwrap();
        assert_eq!(
            (dono.pid, dono.comando.as_str()),
            (std::process::id(), "features")
        );
    }

    #[test]
    fn pasta_inexistente_e_erro_de_io() {
        let p = pasta("inexistente").join("nao_existe");
        let erro = Trava::obter(&p, "ciclo", AGORA).unwrap_err();
        assert!(matches!(erro, SpiderError::Io { .. }), "{erro:?}");
    }

    #[test]
    fn dono_serializa_e_le_de_volta() {
        let d = DonoTrava {
            pid: 42,
            comando: "avaliar".into(),
            desde: AGORA,
        };
        assert_eq!(DonoTrava::ler(&d.serializar()), Some(d));
    }
}
