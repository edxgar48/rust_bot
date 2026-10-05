//! Registro do uso do conjunto trancado (`sync avaliar --trancado`, 1c.5).
//!
//! O trancado (`docs/PLANEJAMENTO.md`, 1c, "Conjunto final trancado") é
//! usado **uma única vez** por perfil. O registro de cada uso fica num
//! arquivo por loteria, `registros/trancado_<loteria>.json`, **versionado no
//! git** (decisão de 2026-10-05): não numa tabela do banco, que é derivado e
//! pode ser apagado e refeito — o registro não pode sumir junto.
//!
//! Formato determinístico: chaves em ordem fixa, uma avaliação por linha
//! (o diff do git mostra exatamente o que entrou), ordem por
//! (`avaliado_em`, `perfil`, `hash_toml`, `ate_concurso`), fim de linha LF
//! (`registros/*.json text eol=lf` no `.gitattributes`).
//!
//! A pasta fica ao lado do banco ([`pasta_padrao`]), não no diretório de
//! trabalho. Quem decide recusar ou repetir, e recarrega o registro logo
//! antes de gravar, é `orquestracao::avaliar_perfil_e_formatar`; aqui só há
//! leitura, gravação e contagem.

use crate::error::{Result, SpiderError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Nome da pasta padrão dos registros, ao lado do banco (ver [`pasta_padrao`]).
pub const PASTA_PADRAO: &str = "registros";

/// Pasta padrão dos registros de um banco: `registros/` no mesmo diretório
/// do arquivo do banco. Ancorar no banco, e não no diretório de trabalho,
/// impede que rodar de outra pasta com `--db C:\...\resultados_quina.db`
/// encontre um registro vazio, avalie o trancado de novo e grave o uso fora
/// do git. Os bancos reais ficam na raiz do repositório, então o padrão é
/// `registros/` na raiz. Banco sem diretório (`resultados_quina.db`, relativo
/// ao diretório de trabalho) dá `registros` relativo ao mesmo diretório.
pub fn pasta_padrao(banco: &Path) -> PathBuf {
    match banco.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(PASTA_PADRAO),
        _ => PathBuf::from(PASTA_PADRAO),
    }
}

/// Um uso do conjunto trancado. Campo desconhecido numa entrada é recusa,
/// como no arquivo: um registro editado à mão com chave a mais (ou com erro
/// de digitação) não pode passar despercebido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AvaliacaoTrancado {
    /// Nome do perfil (`perfis.nome`).
    pub perfil: String,
    /// SHA-256 do `.toml` do perfil (o mesmo de `perfis.hash_toml`).
    pub hash_toml: String,
    /// Último concurso sorteado avaliado (o trancado vai do corte + 1 até ele).
    pub ate_concurso: u32,
    /// Momento da avaliação, ISO-8601 UTC com segundos
    /// (`2026-10-05T18:30:00Z`), como `perfis.criado_em`.
    pub avaliado_em: String,
}

impl AvaliacaoTrancado {
    /// Mesmo perfil, com o mesmo conteúdo.
    pub fn mesmo_perfil(&self, perfil: &str, hash_toml: &str) -> bool {
        self.perfil == perfil && self.hash_toml == hash_toml
    }
}

/// Formato do arquivo em disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Arquivo {
    loteria: String,
    avaliacoes: Vec<AvaliacaoTrancado>,
}

/// `<dir>/trancado_<slug>.json`.
pub fn caminho(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("trancado_{slug}.json"))
}

/// O registro como foi lido, com a informação de o arquivo existir ou não
/// (para quem avalia o trancado ver sempre o que foi consultado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroLido {
    /// As avaliações registradas, em ordem ([`ordenar`]).
    pub avaliacoes: Vec<AvaliacaoTrancado>,
    /// `false` = arquivo inexistente (equivale a nenhum uso).
    pub existia: bool,
}

/// Qual arquivo de registro foi lido e quantos usos (de qualquer perfil) ele
/// tinha: vai nas mensagens, para quem avalia o trancado ver sempre o que foi
/// consultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroConsultado {
    /// Caminho absoluto do arquivo.
    pub caminho: PathBuf,
    pub usos: usize,
}

/// [`ler`] só com as avaliações.
pub fn carregar(caminho: &Path, slug: &str) -> Result<Vec<AvaliacaoTrancado>> {
    ler(caminho, slug).map(|r| r.avaliacoes)
}

/// Lê o registro; arquivo inexistente equivale a nenhum uso (`existia =
/// false`). Arquivo ilegível, de outra loteria, com campo desconhecido (no
/// arquivo ou numa entrada) ou com campo vazio é
/// [`SpiderError::RegistroTrancadoInvalido`]: o registro protege o trancado,
/// então na dúvida nada é avaliado.
pub fn ler(caminho: &Path, slug: &str) -> Result<RegistroLido> {
    let texto = match std::fs::read_to_string(caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RegistroLido {
                avaliacoes: Vec::new(),
                existia: false,
            })
        }
        Err(origem) => {
            return Err(SpiderError::Io {
                caminho: caminho.to_path_buf(),
                origem,
            })
        }
    };
    let invalido = |motivo: String| SpiderError::RegistroTrancadoInvalido {
        caminho: caminho.to_path_buf(),
        motivo,
    };
    let arquivo: Arquivo = serde_json::from_str(&texto).map_err(|e| invalido(e.to_string()))?;
    if arquivo.loteria != slug {
        return Err(invalido(format!(
            "é da loteria `{}`, esperada `{slug}`",
            arquivo.loteria
        )));
    }
    if let Some(a) = arquivo
        .avaliacoes
        .iter()
        .find(|a| a.perfil.is_empty() || a.hash_toml.is_empty() || a.avaliado_em.is_empty())
    {
        return Err(invalido(format!(
            "avaliação com campo vazio: {a:?} (perfil, hash_toml e avaliado_em são obrigatórios)"
        )));
    }
    Ok(RegistroLido {
        avaliacoes: ordenar(arquivo.avaliacoes),
        existia: true,
    })
}

/// Grava o registro inteiro, em ordem, criando a pasta se preciso. Escreve
/// num arquivo temporário ao lado e renomeia: uma falha no meio não deixa o
/// registro pela metade.
pub fn salvar(caminho: &Path, slug: &str, avaliacoes: &[AvaliacaoTrancado]) -> Result<()> {
    let io = |origem| SpiderError::Io {
        caminho: caminho.to_path_buf(),
        origem,
    };
    let linhas: Vec<String> = ordenar(avaliacoes.to_vec())
        .iter()
        .map(|a| para_json(a).map(|j| format!("    {j}")))
        .collect::<Result<_>>()?;
    let corpo = if linhas.is_empty() {
        "[]".to_string()
    } else {
        format!("[\n{}\n  ]", linhas.join(",\n"))
    };
    let texto = format!(
        "{{\n  \"loteria\": {},\n  \"avaliacoes\": {corpo}\n}}\n",
        para_json(&slug)?
    );
    if let Some(dir) = caminho.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let temporario = caminho.with_extension("json.tmp");
    std::fs::write(&temporario, texto).map_err(|origem| SpiderError::Io {
        caminho: temporario.clone(),
        origem,
    })?;
    std::fs::rename(&temporario, caminho).map_err(io)
}

fn para_json<T: Serialize>(valor: &T) -> Result<String> {
    serde_json::to_string(valor).map_err(|e| SpiderError::LinhaInvalida(e.to_string()))
}

/// Ordem do arquivo: momento da avaliação (ISO-8601 em UTC ordena como
/// texto), depois perfil, hash e concurso, para empates.
pub fn ordenar(mut avaliacoes: Vec<AvaliacaoTrancado>) -> Vec<AvaliacaoTrancado> {
    avaliacoes.sort_by(|a, b| {
        (&a.avaliado_em, &a.perfil, &a.hash_toml, a.ate_concurso).cmp(&(
            &b.avaliado_em,
            &b.perfil,
            &b.hash_toml,
            b.ate_concurso,
        ))
    });
    avaliacoes
}

/// Quantos perfis distintos — pares (`perfil`, `hash_toml`) — já usaram o
/// trancado: o número de comparações da correção de Bonferroni.
pub fn perfis_distintos(avaliacoes: &[AvaliacaoTrancado]) -> usize {
    avaliacoes
        .iter()
        .map(|a| (a.perfil.as_str(), a.hash_toml.as_str()))
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(perfil: &str, hash: &str, ate: u32, quando: &str) -> AvaliacaoTrancado {
        AvaliacaoTrancado {
            perfil: perfil.to_string(),
            hash_toml: hash.to_string(),
            ate_concurso: ate,
            avaliado_em: quando.to_string(),
        }
    }

    fn pasta_temporaria(nome: &str) -> PathBuf {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("relógio")
            .as_nanos();
        std::env::temp_dir().join(format!("rust_spider_registros_{nome}_{unico}"))
    }

    #[test]
    fn arquivo_inexistente_e_registro_vazio() {
        let pasta = pasta_temporaria("vazio");
        assert_eq!(
            carregar(&caminho(&pasta, "quina"), "quina").expect("vazio"),
            []
        );
        assert_eq!(
            ler(&caminho(&pasta, "quina"), "quina").expect("vazio"),
            RegistroLido {
                avaliacoes: Vec::new(),
                existia: false
            }
        );
        assert!(!pasta.exists(), "ler não cria nada");
    }

    #[test]
    fn ler_distingue_arquivo_inexistente_de_registro_vazio() {
        let pasta = pasta_temporaria("existia");
        let arquivo = caminho(&pasta, "quina");
        assert!(!ler(&arquivo, "quina").expect("inexistente").existia);
        salvar(&arquivo, "quina", &[]).expect("grava vazio");
        assert_eq!(
            ler(&arquivo, "quina").expect("vazio"),
            RegistroLido {
                avaliacoes: Vec::new(),
                existia: true
            }
        );
        let uma = vec![a("p_v1", "h1", 330, "2026-10-06T12:00:00Z")];
        salvar(&arquivo, "quina", &uma).expect("grava um uso");
        assert_eq!(
            ler(&arquivo, "quina").expect("um uso"),
            RegistroLido {
                avaliacoes: uma,
                existia: true
            }
        );
        std::fs::remove_dir_all(&pasta).ok();
    }

    #[test]
    fn entrada_com_campo_desconhecido_e_recusada() {
        // Entrada válida mais uma chave: `deny_unknown_fields` na struct da
        // entrada, não só no arquivo. Sem a chave extra, a mesma entrada é lida.
        let pasta = pasta_temporaria("campo_extra");
        std::fs::create_dir_all(&pasta).expect("pasta");
        let arquivo = caminho(&pasta, "quina");
        let entrada = "\"perfil\": \"p_v1\", \"hash_toml\": \"h1\", \"ate_concurso\": 7134, \"avaliado_em\": \"2026-10-06T12:00:00Z\"";
        std::fs::write(
            &arquivo,
            format!("{{\"loteria\": \"quina\", \"avaliacoes\": [{{{entrada}}}]}}"),
        )
        .expect("escreve");
        assert_eq!(
            carregar(&arquivo, "quina").expect("entrada válida"),
            vec![a("p_v1", "h1", 7134, "2026-10-06T12:00:00Z")]
        );
        for extra in ["\"k\": 15", "\"perfi\": \"p_v1\"", "\"comentario\": null"] {
            std::fs::write(
                &arquivo,
                format!("{{\"loteria\": \"quina\", \"avaliacoes\": [{{{entrada}, {extra}}}]}}"),
            )
            .expect("escreve");
            let erro = carregar(&arquivo, "quina").expect_err("campo desconhecido");
            assert!(
                matches!(&erro, SpiderError::RegistroTrancadoInvalido { motivo, .. }
                    if motivo.contains("unknown field")),
                "{extra}: {erro:?}"
            );
        }
        std::fs::remove_dir_all(&pasta).ok();
    }

    #[test]
    fn grava_em_ordem_com_lf_e_le_de_volta() {
        let pasta = pasta_temporaria("ida_e_volta").join("sub");
        let arquivo = caminho(&pasta, "quina");
        let avaliacoes = vec![
            a("b_v1", "ff", 7001, "2026-11-01T00:00:00Z"),
            a("a_v1", "00", 7000, "2026-10-05T18:30:00Z"),
        ];
        salvar(&arquivo, "quina", &avaliacoes).expect("grava");
        let texto = std::fs::read_to_string(&arquivo).expect("lê");
        assert_eq!(
            texto,
            "{\n  \"loteria\": \"quina\",\n  \"avaliacoes\": [\n    \
             {\"perfil\":\"a_v1\",\"hash_toml\":\"00\",\"ate_concurso\":7000,\"avaliado_em\":\"2026-10-05T18:30:00Z\"},\n    \
             {\"perfil\":\"b_v1\",\"hash_toml\":\"ff\",\"ate_concurso\":7001,\"avaliado_em\":\"2026-11-01T00:00:00Z\"}\n  ]\n}\n"
        );
        assert!(!texto.contains('\r'));
        assert_eq!(
            carregar(&arquivo, "quina").expect("lê"),
            ordenar(avaliacoes.clone())
        );
        // Gravar de novo o que foi lido dá os mesmos bytes.
        salvar(&arquivo, "quina", &carregar(&arquivo, "quina").expect("lê")).expect("regrava");
        assert_eq!(std::fs::read_to_string(&arquivo).expect("lê"), texto);
        assert!(!arquivo.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(pasta.parent().expect("pai")).ok();
    }

    #[test]
    fn registro_sem_avaliacoes_tem_lista_vazia() {
        let pasta = pasta_temporaria("lista_vazia");
        let arquivo = caminho(&pasta, "megasena");
        salvar(&arquivo, "megasena", &[]).expect("grava");
        assert_eq!(
            std::fs::read_to_string(&arquivo).expect("lê"),
            "{\n  \"loteria\": \"megasena\",\n  \"avaliacoes\": []\n}\n"
        );
        assert_eq!(carregar(&arquivo, "megasena").expect("lê"), []);
        std::fs::remove_dir_all(&pasta).ok();
    }

    #[test]
    fn arquivo_invalido_ou_de_outra_loteria_e_erro() {
        let pasta = pasta_temporaria("invalido");
        std::fs::create_dir_all(&pasta).expect("pasta");
        let arquivo = caminho(&pasta, "quina");
        for texto in [
            "não é json",
            "{\"loteria\": \"megasena\", \"avaliacoes\": []}",
            "{\"loteria\": \"quina\"}",
            "{\"loteria\": \"quina\", \"avaliacoes\": [], \"extra\": 1}",
            "{\"loteria\": \"quina\", \"avaliacoes\": [{\"perfil\": \"\", \"hash_toml\": \"a\", \"ate_concurso\": 1, \"avaliado_em\": \"x\"}]}",
        ] {
            std::fs::write(&arquivo, texto).expect("escreve");
            assert!(
                matches!(
                    carregar(&arquivo, "quina"),
                    Err(SpiderError::RegistroTrancadoInvalido { .. })
                ),
                "{texto}"
            );
        }
        std::fs::remove_dir_all(&pasta).ok();
    }

    #[test]
    fn pasta_padrao_fica_ao_lado_do_banco() {
        assert_eq!(
            pasta_padrao(Path::new("C:/repo/resultados_quina.db")),
            Path::new("C:/repo").join("registros")
        );
        assert_eq!(
            pasta_padrao(Path::new("dados/sub/resultados_megasena.db")),
            Path::new("dados/sub").join("registros")
        );
        // Banco relativo sem diretório: `registros` no mesmo diretório que
        // ele (o de trabalho), nunca `/registros`.
        assert_eq!(
            pasta_padrao(Path::new("resultados_lotofacil.db")),
            PathBuf::from("registros")
        );
        // Só o caminho é calculado: nada é criado.
        let pasta = pasta_temporaria("padrao");
        let padrao = pasta_padrao(&pasta.join("resultados_quina.db"));
        assert_eq!(padrao, pasta.join("registros"));
        assert!(!pasta.exists());
    }

    #[test]
    fn perfis_distintos_conta_pares_perfil_hash() {
        assert_eq!(perfis_distintos(&[]), 0);
        let v = [
            a("p_v1", "h1", 10, "1"),
            a("p_v1", "h1", 12, "2"),
            a("p_v1", "h2", 12, "3"),
            a("q_v1", "h1", 12, "4"),
        ];
        assert_eq!(perfis_distintos(&v), 3);
        assert!(v[0].mesmo_perfil("p_v1", "h1"));
        assert!(!v[0].mesmo_perfil("p_v1", "h2"));
    }
}
