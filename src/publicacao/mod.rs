//! Publicação (Fase 2, `sync publicar`): leva o `publico/v1/` já exportado
//! para o destino, que fica atrás do trait [`Destino`] (hoje, um clone git do
//! repositório de dados em [`git::DestinoGit`]).
//!
//! Divisão: [`indice`] é puro (ler e comparar `index.json`, mensagem do
//! commit); [`saida`] confere a saída no disco; [`git`] fala com o `git`;
//! [`publicar`] só decide a ordem. Não toca no banco nem chama o `exportar`.
//!
//! Ordem: (1) confere a saída contra o próprio `index.json`; (2) o destino se
//! prepara (no git: `fetch`, clone limpo, em `main`, sem estar atrás nem
//! divergir de `origin/main`) e devolve o `index.json` que já publicou; (3)
//! se algum hash ou a lista de arquivos mudou, espelha, registra (commit) e
//! envia (push); se não, só reenvia o que ficou pendente de um push que
//! falhou. Qualquer recusa acontece antes de o destino ser alterado.

pub mod git;
pub mod indice;
pub mod saida;

use crate::error::Result;
use std::path::Path;

pub use git::{DestinoGit, BRANCH, REMOTO, REPO_PADRAO, URL_CLONE};
pub use indice::{ler_indice, IndiceLido};
pub use saida::SaidaConferida;

/// O que o destino achou ao se preparar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preparo {
    /// Bytes do `index.json` já publicado (`None` se o destino ainda não tem).
    pub index_atual: Option<Vec<u8>>,
    /// Há registros locais ainda não enviados (push que falhou antes).
    pub envio_pendente: bool,
}

/// Onde o `v1/` é publicado. A ordem de uso é fixa: `preparar`, depois (se
/// mudou) `espelhar` e `registrar`, e `enviar`.
pub trait Destino {
    /// Confere que o destino está em condições de receber e lê o que ele já
    /// tem. Não altera nada; recusa com erro se não estiver pronto.
    fn preparar(&mut self) -> Result<Preparo>;

    /// Deixa o `v1/` do destino idêntico a `v1` (a pasta `v1` da saída),
    /// copiando só `arquivos` (relativos, separador `/`) e apagando o que
    /// sumiu. Fora do `v1/` nada é tocado.
    fn espelhar(&mut self, v1: &Path, arquivos: &[String]) -> Result<()>;

    /// Registra o espelho com a mensagem dada. `false` se não havia diferença
    /// a registrar (nunca cria registro vazio).
    fn registrar(&mut self, mensagem: &str) -> Result<bool>;

    /// Envia o que está registrado. Se falhar, o registro continua no destino.
    fn enviar(&mut self) -> Result<()>;
}

/// Resultado de uma execução bem-sucedida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Copiou, registrou e enviou.
    Publicado { mensagem: String },
    /// Sem mudança, mas havia registro local de um envio anterior que falhou;
    /// foi enviado agora.
    EnvioRetomado,
    /// Só `gerado_em` mudou (ou nada): nada copiado, nada registrado.
    NadaAPublicar,
}

/// Publica `<saida>/v1` em `destino`.
pub fn publicar(destino: &mut dyn Destino, saida: &Path) -> Result<Desfecho> {
    let conferida = saida::conferir(saida)?;
    let preparo = destino.preparar()?;

    // Índice do destino ilegível conta como mudança: publicar por cima conserta.
    let mudou = match preparo.index_atual.as_deref().map(ler_indice) {
        Some(Ok(atual)) => conferida.indice.difere_de(&atual),
        Some(Err(_)) | None => true,
    };

    if mudou {
        destino.espelhar(&conferida.raiz, &conferida.arquivos)?;
        let mensagem = conferida.indice.mensagem_commit();
        if destino.registrar(&mensagem)? {
            destino.enviar()?;
            return Ok(Desfecho::Publicado { mensagem });
        }
    }
    if preparo.envio_pendente {
        destino.enviar()?;
        return Ok(Desfecho::EnvioRetomado);
    }
    Ok(Desfecho::NadaAPublicar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SpiderError;
    use crate::exportacao::{index_json, Impressao, LoteriaNoIndice};
    use std::fs;

    #[derive(Default)]
    struct Falso {
        preparo: Option<Preparo>,
        chamadas: Vec<String>,
        registra: bool,
        falha_envio: bool,
    }

    impl Destino for Falso {
        fn preparar(&mut self) -> Result<Preparo> {
            self.chamadas.push("preparar".into());
            Ok(self.preparo.clone().unwrap())
        }
        fn espelhar(&mut self, _: &Path, arquivos: &[String]) -> Result<()> {
            self.chamadas.push(format!("espelhar {}", arquivos.len()));
            Ok(())
        }
        fn registrar(&mut self, mensagem: &str) -> Result<bool> {
            self.chamadas.push(format!("registrar {mensagem}"));
            Ok(self.registra)
        }
        fn enviar(&mut self) -> Result<()> {
            self.chamadas.push("enviar".into());
            if self.falha_envio {
                Err(SpiderError::PublicacaoEnvioFalhou("x".into()))
            } else {
                Ok(())
            }
        }
    }

    /// Saída com a quina (3 arquivos) e a lotofacil `null`.
    fn saida_de_teste(gerado_em: &str, historico: &[u8]) -> (std::path::PathBuf, Vec<u8>) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let saida = std::env::temp_dir().join(format!(
            "spider_pub_unit_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&saida);
        let pasta = saida.join("v1").join("quina");
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join("ultimo.json"), b"u").unwrap();
        fs::write(pasta.join("tabela_geral.json"), b"t").unwrap();
        fs::write(pasta.join("historico.json"), historico).unwrap();
        let entrada = LoteriaNoIndice {
            ultimo_concurso: 7140,
            data: "2026-10-11".into(),
            arquivos: [
                Impressao::de(b"u"),
                Impressao::de(b"t"),
                Impressao::de(historico),
            ],
            ranking_taxa: None,
            ranking_posicoes: None,
        };
        let indice = index_json(gerado_em, &[("lotofacil", None), ("quina", Some(entrada))]);
        fs::write(saida.join("v1").join("index.json"), &indice).unwrap();
        (saida, indice.into_bytes())
    }

    #[test]
    fn destino_vazio_publica() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        let mut d = Falso {
            preparo: Some(Preparo {
                index_atual: None,
                envio_pendente: false,
            }),
            registra: true,
            ..Falso::default()
        };
        let r = publicar(&mut d, &saida).unwrap();
        assert_eq!(
            r,
            Desfecho::Publicado {
                mensagem: "Dados: quina 7140".into()
            }
        );
        assert_eq!(
            d.chamadas,
            [
                "preparar",
                "espelhar 4",
                "registrar Dados: quina 7140",
                "enviar"
            ]
        );
        let _ = fs::remove_dir_all(&saida);
    }

    #[test]
    fn so_gerado_em_diferente_nao_mexe_em_nada() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        let (outra, antigo) = saida_de_teste("2026-10-01T10:00:00Z", b"h");
        let mut d = Falso {
            preparo: Some(Preparo {
                index_atual: Some(antigo),
                envio_pendente: false,
            }),
            registra: true,
            ..Falso::default()
        };
        assert_eq!(publicar(&mut d, &saida).unwrap(), Desfecho::NadaAPublicar);
        assert_eq!(d.chamadas, ["preparar"]);
        let _ = fs::remove_dir_all(&saida);
        let _ = fs::remove_dir_all(&outra);
    }

    #[test]
    fn sem_mudanca_mas_com_envio_pendente_reenvia() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        let (outra, antigo) = saida_de_teste("2026-10-01T10:00:00Z", b"h");
        let mut d = Falso {
            preparo: Some(Preparo {
                index_atual: Some(antigo),
                envio_pendente: true,
            }),
            ..Falso::default()
        };
        assert_eq!(publicar(&mut d, &saida).unwrap(), Desfecho::EnvioRetomado);
        assert_eq!(d.chamadas, ["preparar", "enviar"]);
        let _ = fs::remove_dir_all(&saida);
        let _ = fs::remove_dir_all(&outra);
    }

    #[test]
    fn saida_adulterada_e_recusada_antes_de_tocar_no_destino() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        fs::write(saida.join("v1/quina/historico.json"), b"adulterado").unwrap();
        let mut d = Falso::default();
        let e = publicar(&mut d, &saida).unwrap_err();
        assert!(matches!(e, SpiderError::PublicacaoSaidaInvalida { .. }));
        assert!(d.chamadas.is_empty());
        let _ = fs::remove_dir_all(&saida);
    }

    #[test]
    fn mudanca_sem_diferenca_a_registrar_nao_cria_commit_vazio() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        let mut d = Falso {
            preparo: Some(Preparo {
                index_atual: Some(b"lixo".to_vec()),
                envio_pendente: false,
            }),
            registra: false,
            ..Falso::default()
        };
        assert_eq!(publicar(&mut d, &saida).unwrap(), Desfecho::NadaAPublicar);
        assert!(!d.chamadas.iter().any(|c| c == "enviar"));
        let _ = fs::remove_dir_all(&saida);
    }

    #[test]
    fn falha_no_envio_vira_erro() {
        let (saida, _) = saida_de_teste("2026-10-12T10:00:00Z", b"h");
        let mut d = Falso {
            preparo: Some(Preparo {
                index_atual: None,
                envio_pendente: false,
            }),
            registra: true,
            falha_envio: true,
            ..Falso::default()
        };
        assert!(matches!(
            publicar(&mut d, &saida),
            Err(SpiderError::PublicacaoEnvioFalhou(_))
        ));
        let _ = fs::remove_dir_all(&saida);
    }
}
