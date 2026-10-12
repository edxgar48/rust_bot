//! Parte pura da publicação: ler o `index.json`, decidir se algo mudou e
//! montar a mensagem do commit. Sem disco, sem `git`.

use crate::exportacao::SCHEMA_VERSION;
use serde_json::Value;
use std::collections::BTreeMap;

/// Um arquivo de dados como o `index.json` o descreve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivoNoIndice {
    /// `ultimo`, `tabela_geral` ou `historico`.
    pub chave: String,
    /// Relativo a `v1/`, separador `/`.
    pub caminho: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Uma loteria com arquivos no índice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaIndice {
    pub ultimo_concurso: u32,
    /// Em ordem alfabética da chave (a do JSON lido).
    pub arquivos: Vec<ArquivoNoIndice>,
}

/// O que a publicação precisa do `index.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndiceLido {
    pub gerado_em: String,
    /// Em ordem alfabética do slug; `None` é a loteria sem arquivos (`null`).
    pub loterias: BTreeMap<String, Option<EntradaIndice>>,
}

/// Lê um `index.json` do contrato `v1`. O erro é um texto curto para quem
/// chamar embutir na sua própria mensagem.
pub fn ler_indice(bytes: &[u8]) -> Result<IndiceLido, String> {
    let json: Value = serde_json::from_slice(bytes).map_err(|e| format!("JSON inválido ({e})"))?;
    if json["schema_version"].as_u64() != Some(u64::from(SCHEMA_VERSION)) {
        return Err(format!("schema_version diferente de {SCHEMA_VERSION}"));
    }
    let gerado_em = json["gerado_em"]
        .as_str()
        .ok_or("campo `gerado_em` ausente ou inválido")?
        .to_string();
    let loterias_json = json["loterias"]
        .as_object()
        .ok_or("campo `loterias` ausente ou inválido")?;

    let mut loterias = BTreeMap::new();
    for (slug, entrada) in loterias_json {
        if entrada.is_null() {
            loterias.insert(slug.clone(), None);
            continue;
        }
        let ultimo_concurso = entrada["ultimo_concurso"]
            .as_u64()
            .and_then(|c| u32::try_from(c).ok())
            .ok_or_else(|| format!("`{slug}`: campo `ultimo_concurso` ausente ou inválido"))?;
        let arquivos_json = entrada["arquivos"]
            .as_object()
            .ok_or_else(|| format!("`{slug}`: campo `arquivos` ausente ou inválido"))?;
        let mut arquivos = Vec::with_capacity(arquivos_json.len());
        for (chave, a) in arquivos_json {
            let texto = |campo: &str| {
                a[campo]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("`{slug}.{chave}`: campo `{campo}` ausente ou inválido"))
            };
            arquivos.push(ArquivoNoIndice {
                chave: chave.clone(),
                caminho: texto("caminho")?,
                sha256: texto("sha256")?,
                bytes: a["bytes"].as_u64().ok_or_else(|| {
                    format!("`{slug}.{chave}`: campo `bytes` ausente ou inválido")
                })?,
            });
        }
        loterias.insert(
            slug.clone(),
            Some(EntradaIndice {
                ultimo_concurso,
                arquivos,
            }),
        );
    }
    Ok(IndiceLido {
        gerado_em,
        loterias,
    })
}

impl IndiceLido {
    /// Há algo a publicar em relação a `anterior`? Conta algum hash, ou a
    /// lista de loterias e de arquivos, diferente; `gerado_em` não conta
    /// (muda a cada exportação, mesmo sem concurso novo).
    pub fn difere_de(&self, anterior: &IndiceLido) -> bool {
        fn projecao(i: &IndiceLido) -> BTreeMap<&str, Option<&[ArquivoNoIndice]>> {
            i.loterias
                .iter()
                .map(|(slug, e)| (slug.as_str(), e.as_ref().map(|e| e.arquivos.as_slice())))
                .collect()
        }
        projecao(self) != projecao(anterior)
    }

    /// Mensagem fixa do commit, com o último concurso de cada loteria que
    /// tem arquivos, em ordem alfabética: `Dados: lotofacil 3802, megasena
    /// 3069, quina 7140`. Loteria `null` no índice fica de fora.
    pub fn mensagem_commit(&self) -> String {
        let partes: Vec<String> = self
            .loterias
            .iter()
            .filter_map(|(slug, e)| e.as_ref().map(|e| format!("{slug} {}", e.ultimo_concurso)))
            .collect();
        if partes.is_empty() {
            "Dados: nenhuma loteria com arquivos".to_string()
        } else {
            format!("Dados: {}", partes.join(", "))
        }
    }
}

/// Caminho relativo seguro para o `index.json` apontar: sem barra
/// invertida, `:`, componente vazio, `.` ou `..` (não deixa a cópia sair de
/// `v1/`).
pub fn caminho_seguro(caminho: &str) -> bool {
    !caminho.is_empty()
        && !caminho.contains('\\')
        && !caminho.contains(':')
        && caminho
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exportacao::{index_json, Impressao, LoteriaNoIndice};

    fn entrada(concurso: u32, texto: &[u8]) -> LoteriaNoIndice {
        LoteriaNoIndice {
            ultimo_concurso: concurso,
            data: "2026-10-11".into(),
            arquivos: [
                Impressao::de(texto),
                Impressao::de(b"tabela"),
                Impressao::de(b"historico"),
            ],
        }
    }

    fn indice(gerado_em: &str, texto: &[u8]) -> IndiceLido {
        let json = index_json(
            gerado_em,
            &[
                ("lotofacil", Some(entrada(3802, texto))),
                ("megasena", None),
                ("quina", Some(entrada(7140, b"q"))),
            ],
        );
        ler_indice(json.as_bytes()).unwrap()
    }

    #[test]
    fn le_o_index_que_o_exportar_grava() {
        let i = indice("2026-10-12T10:00:00Z", b"u");
        assert_eq!(i.gerado_em, "2026-10-12T10:00:00Z");
        assert_eq!(i.loterias.len(), 3);
        assert!(i.loterias["megasena"].is_none());
        let l = i.loterias["lotofacil"].as_ref().unwrap();
        assert_eq!(l.ultimo_concurso, 3802);
        assert_eq!(l.arquivos.len(), 3);
        assert_eq!(l.arquivos[0].chave, "historico");
        assert_eq!(l.arquivos[0].caminho, "lotofacil/historico.json");
    }

    #[test]
    fn so_gerado_em_diferente_nao_e_mudanca() {
        let a = indice("2026-10-12T10:00:00Z", b"u");
        let b = indice("2026-10-13T10:00:00Z", b"u");
        assert!(!a.difere_de(&b));
    }

    #[test]
    fn hash_diferente_e_mudanca() {
        let a = indice("2026-10-12T10:00:00Z", b"u");
        let b = indice("2026-10-12T10:00:00Z", b"outro");
        assert!(a.difere_de(&b));
    }

    #[test]
    fn loteria_a_mais_ou_a_menos_e_mudanca() {
        let a = indice("2026-10-12T10:00:00Z", b"u");
        let mut b = a.clone();
        b.loterias.remove("quina");
        assert!(a.difere_de(&b));
        let mut c = a.clone();
        c.loterias.insert("quina".into(), None);
        assert!(a.difere_de(&c));
    }

    #[test]
    fn mensagem_lista_as_loterias_com_arquivos_e_omite_null() {
        let i = indice("2026-10-12T10:00:00Z", b"u");
        assert_eq!(i.mensagem_commit(), "Dados: lotofacil 3802, quina 7140");
        let vazio = IndiceLido {
            gerado_em: "x".into(),
            loterias: BTreeMap::new(),
        };
        assert_eq!(
            vazio.mensagem_commit(),
            "Dados: nenhuma loteria com arquivos"
        );
    }

    #[test]
    fn indice_invalido_e_recusado() {
        assert!(ler_indice(b"nao e json").is_err());
        assert!(ler_indice(br#"{"schema_version": 2, "gerado_em": "x", "loterias": {}}"#).is_err());
        assert!(ler_indice(br#"{"schema_version": 1, "loterias": {}}"#).is_err());
        assert!(
            ler_indice(br#"{"schema_version": 1, "gerado_em": "x", "loterias": {"q": {}}}"#)
                .is_err()
        );
    }

    #[test]
    fn caminhos_perigosos_nao_passam() {
        assert!(caminho_seguro("quina/ultimo.json"));
        for ruim in ["", "../x", "a/../b", "/abs", "a//b", "a\\b", "C:/x", "./a"] {
            assert!(!caminho_seguro(ruim), "{ruim}");
        }
    }
}
