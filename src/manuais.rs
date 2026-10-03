//! Cache de concursos inseridos à mão (`sync inserir`) para fechar buracos.
//!
//! Cada loteria tem um arquivo `cache/manuais_<loteria>.json`, versionado no
//! git: é dado digitado, que não dá para baixar de novo. As entradas ficam em
//! ordem de data de sorteio (e concurso, para desempate), o que deixa o
//! arquivo previsível e fácil de revisar.
//!
//! O cache tem dois usos além do registro:
//! - **reaplicar**: `importar`/`baixar` regravam as entradas que a planilha
//!   ainda não traz, então um banco recriado do zero recupera os manuais;
//! - **conciliar**: quando a planilha passa a trazer o concurso, a entrada
//!   igual é removida do cache (resolvida) e a diferente gera aviso.

use crate::error::{Result, SpiderError};
use crate::model::Resultado;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// Um concurso inserido à mão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaManual {
    pub concurso: u32,
    /// Data ISO `AAAA-MM-DD`.
    pub data_sorteio: String,
    pub dezenas: Vec<u32>,
}

impl EntradaManual {
    pub fn para_resultado(&self, loteria: &str) -> Resultado {
        Resultado {
            loteria: loteria.to_string(),
            concurso: self.concurso,
            data_sorteio: self.data_sorteio.clone(),
            dezenas: self.dezenas.clone(),
        }
    }

    /// Mesmo sorteio, ignorando a ordem das dezenas (a planilha traz a ordem
    /// do sorteio; quem digita costuma usar ordem crescente).
    fn mesmo_sorteio(&self, r: &Resultado) -> bool {
        let a: BTreeSet<u32> = self.dezenas.iter().copied().collect();
        let b: BTreeSet<u32> = r.dezenas.iter().copied().collect();
        self.data_sorteio == r.data_sorteio && a == b
    }
}

/// Formato do arquivo em disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Arquivo {
    loteria: String,
    entradas: Vec<EntradaManual>,
}

pub fn caminho(dir: &Path, slug: &str) -> PathBuf {
    dir.join(format!("manuais_{slug}.json"))
}

/// Lê o cache; arquivo inexistente equivale a cache vazio.
pub fn carregar(caminho: &Path, slug: &str) -> Result<Vec<EntradaManual>> {
    let texto = match std::fs::read_to_string(caminho) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(origem) => {
            return Err(SpiderError::Io {
                caminho: caminho.to_path_buf(),
                origem,
            })
        }
    };
    let invalido = |motivo: String| SpiderError::CacheManualInvalido {
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
    // O arquivo pode ter sido editado à mão: normaliza a ordem das dezenas e
    // recusa concurso repetido aqui, com uma mensagem que aponta o cache.
    let mut vistos = BTreeSet::new();
    let mut entradas = arquivo.entradas;
    for e in &mut entradas {
        if !vistos.insert(e.concurso) {
            return Err(invalido(format!(
                "o concurso {} aparece mais de uma vez",
                e.concurso
            )));
        }
        e.dezenas.sort_unstable();
    }
    Ok(entradas)
}

/// Grava o cache ordenado por (data, concurso). Cache vazio apaga o arquivo,
/// para não deixar arquivos sem conteúdo no repositório.
pub fn salvar(caminho: &Path, slug: &str, entradas: &[EntradaManual]) -> Result<()> {
    let io = |origem| SpiderError::Io {
        caminho: caminho.to_path_buf(),
        origem,
    };
    if entradas.is_empty() {
        return match std::fs::remove_file(caminho) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        };
    }
    // Uma entrada por linha: o arquivo é lido e revisado por gente, e o diff
    // do git mostra exatamente qual concurso entrou ou saiu.
    let linhas: Vec<String> = ordenar(entradas.to_vec())
        .iter()
        .map(|e| para_json(e).map(|j| format!("    {j}")))
        .collect::<Result<_>>()?;
    let texto = format!(
        "{{\n  \"loteria\": {},\n  \"entradas\": [\n{}\n  ]\n}}\n",
        para_json(&slug)?,
        linhas.join(",\n")
    );
    if let Some(dir) = caminho.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    std::fs::write(caminho, texto).map_err(io)
}

fn para_json<T: Serialize>(valor: &T) -> Result<String> {
    serde_json::to_string(valor).map_err(|e| SpiderError::LinhaInvalida(e.to_string()))
}

/// Ordem do arquivo: data de sorteio e, em caso de empate, concurso.
pub fn ordenar(mut entradas: Vec<EntradaManual>) -> Vec<EntradaManual> {
    entradas.sort_by(|a, b| {
        (a.data_sorteio.as_str(), a.concurso).cmp(&(b.data_sorteio.as_str(), b.concurso))
    });
    entradas
}

/// Acrescenta `nova`, substituindo uma entrada anterior do mesmo concurso.
/// As dezenas são guardadas em ordem crescente.
pub fn incluir(entradas: &mut Vec<EntradaManual>, mut nova: EntradaManual) {
    nova.dezenas.sort_unstable();
    entradas.retain(|e| e.concurso != nova.concurso);
    entradas.push(nova);
}

/// O que a conciliação fez com uma entrada manual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conciliacao {
    /// A fonte oficial trouxe o mesmo sorteio: a entrada sai do cache.
    Resolvida { concurso: u32 },
    /// A fonte oficial trouxe um sorteio diferente: a entrada fica no cache
    /// para conferência, e o dado oficial é o que vai para o banco.
    Divergente {
        manual: EntradaManual,
        oficial: Resultado,
    },
    /// O concurso já está no banco com outro conteúdo: a entrada manual não
    /// é reaplicada (nunca sobrescreve o banco) e fica no cache para conferência.
    DifereDoBanco {
        manual: EntradaManual,
        banco: Resultado,
    },
}

impl fmt::Display for Conciliacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Conciliacao::Resolvida { concurso } => write!(
                f,
                "concurso {concurso}: inserção manual confirmada pela fonte oficial e \
                 removida do cache"
            ),
            Conciliacao::Divergente { manual, oficial } => write!(
                f,
                "aviso: concurso {}: inserção manual ({}, {:?}) difere da fonte oficial \
                 ({}, {:?}); o banco fica com o oficial — confira e remova a entrada do cache",
                manual.concurso,
                manual.data_sorteio,
                manual.dezenas,
                oficial.data_sorteio,
                oficial.dezenas
            ),
            Conciliacao::DifereDoBanco { manual, banco } => write!(
                f,
                "aviso: concurso {}: inserção manual ({}, {:?}) difere do que já está no \
                 banco ({}, {:?}); o banco não foi alterado — confira e corrija o cache",
                manual.concurso,
                manual.data_sorteio,
                manual.dezenas,
                banco.data_sorteio,
                banco.dezenas
            ),
        }
    }
}

/// Concilia o cache com resultados oficiais recém-obtidos. Devolve as
/// entradas que continuam no cache e o que aconteceu com cada concurso que a
/// fonte oficial também trouxe.
pub fn conciliar(
    entradas: &[EntradaManual],
    oficiais: &[Resultado],
) -> (Vec<EntradaManual>, Vec<Conciliacao>) {
    let mut restantes = Vec::new();
    let mut ocorrencias = Vec::new();
    for e in entradas {
        match oficiais.iter().find(|r| r.concurso == e.concurso) {
            Some(r) if e.mesmo_sorteio(r) => ocorrencias.push(Conciliacao::Resolvida {
                concurso: e.concurso,
            }),
            Some(r) => {
                ocorrencias.push(Conciliacao::Divergente {
                    manual: e.clone(),
                    oficial: r.clone(),
                });
                restantes.push(e.clone());
            }
            None => restantes.push(e.clone()),
        }
    }
    (restantes, ocorrencias)
}

/// Entradas a reaplicar junto com o lote: as que nem a fonte oficial
/// (`oficiais`) nem o banco (`existentes`) trazem — é o caso do banco
/// recriado do zero. Entrada cujo concurso já está no banco nunca o
/// sobrescreve: se o conteúdo for igual, é ignorada; se for diferente, vira
/// [`Conciliacao::DifereDoBanco`].
pub fn pendentes(
    entradas: &[EntradaManual],
    oficiais: &[Resultado],
    existentes: &[Resultado],
    slug: &str,
) -> (Vec<Resultado>, Vec<Conciliacao>) {
    let oficiais: BTreeSet<u32> = oficiais.iter().map(|r| r.concurso).collect();
    let mut reaplicar = Vec::new();
    let mut avisos = Vec::new();
    for e in entradas.iter().filter(|e| !oficiais.contains(&e.concurso)) {
        match existentes.iter().find(|r| r.concurso == e.concurso) {
            None => reaplicar.push(e.para_resultado(slug)),
            Some(r) if e.mesmo_sorteio(r) => {}
            Some(r) => avisos.push(Conciliacao::DifereDoBanco {
                manual: e.clone(),
                banco: r.clone(),
            }),
        }
    }
    (reaplicar, avisos)
}

/// Lê dezenas digitadas como `1,5,10` (aceita espaços e zeros à esquerda).
pub fn parsear_dezenas(texto: &str) -> Result<Vec<u32>> {
    texto
        .split([',', ' ', ';'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| SpiderError::LinhaInvalida(format!("dezena inválida: `{s}`")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrada(concurso: u32, data: &str, dezenas: &[u32]) -> EntradaManual {
        EntradaManual {
            concurso,
            data_sorteio: data.to_string(),
            dezenas: dezenas.to_vec(),
        }
    }

    fn oficial(concurso: u32, data: &str, dezenas: &[u32]) -> Resultado {
        entrada(concurso, data, dezenas).para_resultado("quina")
    }

    fn dir_temporario(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rust_spider_manuais_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn ordena_por_data_e_depois_concurso() {
        let ordenadas = ordenar(vec![
            entrada(12, "2026-09-30", &[1]),
            entrada(10, "2026-09-28", &[1]),
            entrada(11, "2026-09-30", &[1]),
        ]);
        let concursos: Vec<u32> = ordenadas.iter().map(|e| e.concurso).collect();
        assert_eq!(concursos, vec![10, 11, 12]);
    }

    #[test]
    fn incluir_substitui_mesmo_concurso_com_dezenas_em_ordem() {
        let mut v = vec![entrada(10, "2026-09-28", &[1, 2])];
        incluir(&mut v, entrada(10, "2026-09-28", &[4, 3]));
        assert_eq!(v, vec![entrada(10, "2026-09-28", &[3, 4])]);
    }

    #[test]
    fn conciliacao_resolve_igual_mesmo_com_dezenas_em_outra_ordem() {
        let cache = vec![entrada(10, "2026-09-28", &[1, 2, 3, 4, 5])];
        let (restantes, ocorrencias) =
            conciliar(&cache, &[oficial(10, "2026-09-28", &[5, 3, 1, 4, 2])]);
        assert!(restantes.is_empty());
        assert_eq!(ocorrencias, vec![Conciliacao::Resolvida { concurso: 10 }]);
    }

    #[test]
    fn conciliacao_mantem_e_avisa_quando_diverge() {
        let cache = vec![entrada(10, "2026-09-28", &[1, 2, 3, 4, 5])];
        let (restantes, ocorrencias) =
            conciliar(&cache, &[oficial(10, "2026-09-28", &[1, 2, 3, 4, 6])]);
        assert_eq!(restantes, cache);
        assert!(matches!(ocorrencias[..], [Conciliacao::Divergente { .. }]));
        assert!(ocorrencias[0].to_string().starts_with("aviso:"));
    }

    #[test]
    fn conciliacao_ignora_entradas_que_a_fonte_nao_trouxe() {
        let cache = vec![entrada(10, "2026-09-28", &[1, 2, 3, 4, 5])];
        let (restantes, ocorrencias) = conciliar(&cache, &[oficial(9, "2026-09-26", &[1])]);
        assert_eq!(restantes, cache);
        assert!(ocorrencias.is_empty());
    }

    #[test]
    fn pendentes_sao_as_que_nem_a_fonte_nem_o_banco_trazem() {
        let cache = vec![
            entrada(10, "2026-09-28", &[1, 2, 3, 4, 5]),
            entrada(11, "2026-09-30", &[6, 7, 8, 9, 10]),
        ];
        let (p, avisos) = pendentes(
            &cache,
            &[oficial(10, "2026-09-28", &[1, 2, 3, 4, 5])],
            &[],
            "quina",
        );
        assert_eq!(p, vec![oficial(11, "2026-09-30", &[6, 7, 8, 9, 10])]);
        assert!(avisos.is_empty());
    }

    #[test]
    fn pendente_igual_ao_banco_e_ignorada() {
        let cache = vec![entrada(11, "2026-09-30", &[6, 7, 8, 9, 10])];
        let banco = [oficial(11, "2026-09-30", &[10, 9, 8, 7, 6])];
        let (p, avisos) = pendentes(&cache, &[], &banco, "quina");
        assert!(p.is_empty());
        assert!(avisos.is_empty());
    }

    #[test]
    fn pendente_diferente_do_banco_nunca_sobrescreve() {
        // Ex.: `atualizar` gravou o oficial e um `importar` de arquivo antigo
        // não traz o concurso: a entrada manual divergente não é reaplicada.
        let cache = vec![entrada(11, "2026-09-30", &[6, 7, 8, 9, 10])];
        let banco = [oficial(11, "2026-09-30", &[6, 7, 8, 9, 11])];
        let (p, avisos) = pendentes(&cache, &[], &banco, "quina");
        assert!(p.is_empty());
        assert!(matches!(avisos[..], [Conciliacao::DifereDoBanco { .. }]));
        assert!(avisos[0].to_string().starts_with("aviso:"));
    }

    #[test]
    fn carregar_normaliza_dezenas_e_recusa_concurso_repetido() {
        let dir = dir_temporario("normaliza");
        std::fs::create_dir_all(&dir).unwrap();
        let arq = caminho(&dir, "quina");
        std::fs::write(
            &arq,
            r#"{"loteria":"quina","entradas":[{"concurso":10,"data_sorteio":"2026-09-28","dezenas":[5,1,3,2,4]}]}"#,
        )
        .unwrap();
        assert_eq!(
            carregar(&arq, "quina").unwrap(),
            vec![entrada(10, "2026-09-28", &[1, 2, 3, 4, 5])]
        );
        std::fs::write(
            &arq,
            r#"{"loteria":"quina","entradas":[{"concurso":10,"data_sorteio":"2026-09-28","dezenas":[1]},{"concurso":10,"data_sorteio":"2026-09-28","dezenas":[2]}]}"#,
        )
        .unwrap();
        let erro = carregar(&arq, "quina").unwrap_err().to_string();
        assert!(erro.contains("mais de uma vez"), "{erro}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn salvar_e_carregar_ida_e_volta_em_ordem() {
        let dir = dir_temporario("ida_volta");
        let arq = caminho(&dir, "quina");
        let entradas = vec![
            entrada(11, "2026-09-30", &[6, 7, 8, 9, 10]),
            entrada(10, "2026-09-28", &[1, 2, 3, 4, 5]),
        ];
        salvar(&arq, "quina", &entradas).unwrap();
        let lidas = carregar(&arq, "quina").unwrap();
        assert_eq!(lidas, ordenar(entradas.clone()));

        // Determinismo: salvar de novo gera os mesmos bytes.
        let antes = std::fs::read(&arq).unwrap();
        salvar(&arq, "quina", &lidas).unwrap();
        assert_eq!(std::fs::read(&arq).unwrap(), antes);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_inexistente_e_vazio_e_cache_vazio_apaga_arquivo() {
        let dir = dir_temporario("vazio");
        let arq = caminho(&dir, "quina");
        assert_eq!(carregar(&arq, "quina").unwrap(), vec![]);
        salvar(&arq, "quina", &[entrada(10, "2026-09-28", &[1])]).unwrap();
        assert!(arq.exists());
        salvar(&arq, "quina", &[]).unwrap();
        assert!(!arq.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cache_de_outra_loteria_e_recusado() {
        let dir = dir_temporario("outra");
        let arq = caminho(&dir, "quina");
        salvar(&arq, "megasena", &[entrada(10, "2026-09-28", &[1])]).unwrap();
        assert!(carregar(&arq, "quina").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parseia_dezenas_digitadas() {
        assert_eq!(parsear_dezenas("01, 5,10 ;80").unwrap(), vec![1, 5, 10, 80]);
        assert!(parsear_dezenas("1,x,3").is_err());
    }
}
