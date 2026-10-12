//! Destino real da publicação: um clone git do repositório de dados.
//!
//! Chama o binário `git` da máquina (`std::process::Command`, sem crate
//! nova); identidade do autor e SSH são os que a máquina já tem configurados.
//! Nunca clona sozinho, nunca faz `reset --hard` e nunca mexe fora de `v1/`.
//! `GIT_TERMINAL_PROMPT=0` evita que um `fetch`/`push` fique esperando senha.

use super::{Destino, Preparo};
use crate::error::{Result, SpiderError};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Clone padrão, relativo ao diretório atual.
pub const REPO_PADRAO: &str = "../rust_spider_dados";
/// Endereço para o `git clone` que a mensagem de erro sugere.
pub const URL_CLONE: &str = "git@github.com:edxgar48/rust_spider_dados.git";
/// Único branch publicado.
pub const BRANCH: &str = "main";
/// Remoto do clone.
pub const REMOTO: &str = "origin";

/// Pasta do contrato dentro do clone.
const PASTA_V1: &str = "v1";

fn recusa(mensagem: impl Into<String>) -> SpiderError {
    SpiderError::PublicacaoRecusada(mensagem.into())
}

/// Clone git em `repo`.
#[derive(Debug, Clone)]
pub struct DestinoGit {
    repo: PathBuf,
}

impl DestinoGit {
    pub fn novo(repo: impl Into<PathBuf>) -> Self {
        DestinoGit { repo: repo.into() }
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    /// Roda `git -C <repo> <args>` e devolve a saída, qualquer que seja o código.
    fn executar(&self, args: &[&str]) -> Result<Output> {
        // Sem conversão de fim de linha: os blobs têm de ser sempre os bytes
        // exportados (o sha256 do index.json vale para o que o app baixa), e um
        // `core.safecrlf=true` global não pode quebrar o `add`.
        Command::new("git")
            .args(["-c", "core.autocrlf=false", "-c", "core.safecrlf=false"])
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    recusa("o `git` não foi encontrado no PATH — instale o git antes de publicar")
                } else {
                    recusa(format!("não foi possível rodar o `git`: {e}"))
                }
            })
    }

    /// Como [`Self::executar`], mas código diferente de zero vira recusa com o
    /// que o git disse. `contexto` completa a frase "git <contexto> falhou".
    fn exigir(&self, args: &[&str], contexto: &str) -> Result<String> {
        let saida = self.executar(args)?;
        if saida.status.success() {
            Ok(String::from_utf8_lossy(&saida.stdout).trim().to_string())
        } else {
            Err(recusa(format!(
                "`git {contexto}` falhou em `{}`: {}",
                self.repo.display(),
                texto_do_erro(&saida)
            )))
        }
    }

    /// Desfaz, só dentro de `v1/`, o que `espelhar`/`registrar` fizeram. O
    /// clone estava limpo antes (conferido em `preparar`), então só o trabalho
    /// desta execução é descartado. Melhor esforço: erros são ignorados.
    fn desfazer_espelho(&self) {
        let _ = self.executar(&["reset", "-q", "HEAD", "--", PASTA_V1]);
        let _ = self.executar(&["checkout", "-q", "--", PASTA_V1]);
        let _ = self.executar(&["clean", "-fdq", "--", PASTA_V1]);
    }
}

fn texto_do_erro(saida: &Output) -> String {
    let e = String::from_utf8_lossy(&saida.stderr).trim().to_string();
    if e.is_empty() {
        String::from_utf8_lossy(&saida.stdout).trim().to_string()
    } else {
        e
    }
}

impl Destino for DestinoGit {
    fn preparar(&mut self) -> Result<Preparo> {
        let repo = self.repo.display().to_string();
        if !self.repo.is_dir() {
            return Err(recusa(format!(
                "o clone `{repo}` não existe — clone o repositório de dados antes (o comando não clona sozinho): git clone {URL_CLONE} {repo}"
            )));
        }

        // `--show-prefix` vazio = a pasta é a raiz do repositório. Uma
        // subpasta do repositório do código (ou de qualquer outro) não serve:
        // o commit iria para o repositório errado.
        let prefixo = self.exigir(&["rev-parse", "--show-prefix"], "rev-parse")?;
        if !prefixo.is_empty() {
            return Err(recusa(format!(
                "`{repo}` está dentro de outro repositório git (em `{prefixo}`), não é a raiz do clone de dados — aponte `--repo` para a pasta do clone"
            )));
        }

        self.exigir(&["fetch", REMOTO], &format!("fetch {REMOTO}"))?;

        // Remoto sem nenhum commit em `main`: não há o que comparar nem de onde
        // partir. Recusa antes da checagem de branch, que num clone vazio
        // diria algo enganoso (HEAD ainda sem commit).
        let remoto = format!("refs/remotes/{REMOTO}/{BRANCH}");
        let tem_ref_remota = self
            .executar(&["rev-parse", "--verify", "-q", &remoto])?
            .status
            .success();
        if !tem_ref_remota {
            let heads = self.exigir(
                &["ls-remote", "--heads", REMOTO, BRANCH],
                &format!("ls-remote --heads {REMOTO} {BRANCH}"),
            )?;
            if heads.is_empty() {
                return Err(recusa(format!(
                    "o repositório remoto de `{repo}` está vazio (sem `{REMOTO}/{BRANCH}`) — faça um commit inicial fora de v1/ (por exemplo um README) e rode `git -C {repo} push -u {REMOTO} {BRANCH}`; nada foi copiado"
                )));
            }
        }

        let atual = self
            .executar(&["symbolic-ref", "--short", "-q", "HEAD"])?
            .stdout;
        let atual = String::from_utf8_lossy(&atual).trim().to_string();
        if atual != BRANCH {
            let onde = if atual.is_empty() {
                "com HEAD solto".to_string()
            } else {
                format!("no branch `{atual}`")
            };
            return Err(recusa(format!(
                "o clone `{repo}` está {onde}, e só se publica em `{BRANCH}` — rode `git -C {repo} checkout {BRANCH}`"
            )));
        }

        let sujeira = self.exigir(&["status", "--porcelain"], "status")?;
        if !sujeira.is_empty() {
            return Err(recusa(format!(
                "o clone `{repo}` tem alterações locais (git status não está limpo) — confirme-as ou descarte-as à mão; nada foi copiado"
            )));
        }

        if !tem_ref_remota {
            return Err(recusa(format!(
                "o clone `{repo}` não tem `{REMOTO}/{BRANCH}` — confira o remoto com `git -C {repo} remote -v`"
            )));
        }
        let contagem = self.exigir(
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("{REMOTO}/{BRANCH}...HEAD"),
            ],
            "rev-list",
        )?;
        let mut numeros = contagem.split_whitespace().map(|n| n.parse::<u64>().ok());
        let (Some(Some(atras)), Some(Some(a_frente))) = (numeros.next(), numeros.next()) else {
            return Err(recusa(format!(
                "resposta inesperada do `git rev-list`: `{contagem}`"
            )));
        };
        if atras > 0 {
            let situacao = if a_frente > 0 {
                "divergiu de"
            } else {
                "está atrás de"
            };
            return Err(recusa(format!(
                "o clone `{repo}` {situacao} `{REMOTO}/{BRANCH}` ({atras} commit(s) só no remoto) — atualize-o à mão (`git -C {repo} pull --ff-only`); nada foi copiado"
            )));
        }

        let caminho_index = self.repo.join(PASTA_V1).join("index.json");
        let index_atual = match fs::read(&caminho_index) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(origem) => {
                return Err(SpiderError::Io {
                    caminho: caminho_index,
                    origem,
                })
            }
        };
        Ok(Preparo {
            index_atual,
            envio_pendente: a_frente > 0,
        })
    }

    fn espelhar(&mut self, v1: &Path, arquivos: &[String]) -> Result<()> {
        let destino = self.repo.join(PASTA_V1);
        let io = |caminho: &Path, origem| SpiderError::Io {
            caminho: caminho.to_path_buf(),
            origem,
        };
        let copiado = (|| -> Result<()> {
            if destino.exists() {
                fs::remove_dir_all(&destino).map_err(|e| io(&destino, e))?;
            }
            for relativo in arquivos {
                let de = v1.join(relativo);
                let para = destino.join(relativo);
                if let Some(pasta) = para.parent() {
                    fs::create_dir_all(pasta).map_err(|e| io(pasta, e))?;
                }
                fs::copy(&de, &para).map_err(|e| io(&para, e))?;
            }
            Ok(())
        })();
        if let Err(e) = copiado {
            self.desfazer_espelho();
            return Err(e);
        }
        Ok(())
    }

    fn registrar(&mut self, mensagem: &str) -> Result<bool> {
        let resultado = (|| -> Result<bool> {
            self.exigir(&["add", "-A", "--", PASTA_V1], "add")?;
            // Código 0 = nada em staging: nunca se cria commit vazio.
            let sem_mudanca = self
                .executar(&["diff", "--cached", "--quiet", "--", PASTA_V1])?
                .status
                .success();
            if sem_mudanca {
                return Ok(false);
            }
            self.exigir(&["commit", "-q", "-m", mensagem], "commit")?;
            Ok(true)
        })();
        if resultado.is_err() {
            self.desfazer_espelho();
        }
        resultado
    }

    fn enviar(&mut self) -> Result<()> {
        let saida = self.executar(&["push", REMOTO, BRANCH])?;
        if saida.status.success() {
            Ok(())
        } else {
            Err(SpiderError::PublicacaoEnvioFalhou(texto_do_erro(&saida)))
        }
    }
}
