//! Exportação para o app (Fase 2): os quatro JSON do contrato `v1`.
//!
//! Este módulo é puro: recebe o que a `orquestracao` já leu e conferiu
//! ([`DadosExportacao`]) e devolve bytes. Quem fala com o disco é [`disco`];
//! quem fala com o banco é a `orquestracao`. Mesmos dados → mesmos bytes: o
//! único campo que muda a cada execução é `gerado_em` do `index.json`.
//!
//! Layout (relativo à pasta de saída, p.ex. `publico/`):
//!
//! ```text
//! v1/index.json
//! v1/<loteria>/ultimo.json
//! v1/<loteria>/tabela_geral.json
//! v1/<loteria>/historico.json
//! v1/<loteria>/ranking_taxa.json   (opcional)
//! v1/<loteria>/ranking_posicoes.json   (opcional)
//! ```
//!
//! Os campos e a ordem em que aparecem são o contrato `v1`: depois dele só
//! se acrescenta campo opcional (ver `docs/PLANEJAMENTO.md`, Fase 2). O
//! formato dos bytes está em [`json`].

pub mod disco;
pub mod json;

use crate::avaliacao::{Direcao, Grau};
use crate::dataset::sha256_hex;
use crate::error::{Result, SpiderError};
use crate::motor::Normalizacao;
use crate::orquestracao::{
    DadosExportacao, LinhaTabela, MotivoPosicoesOmitido, MotivoRankingOmitido,
    PosicoesParaExportar, PosicoesProximo, RankingParaExportar, RankingProximo,
};
use json::Valor;

/// Versão do esquema dos arquivos, presente em todos eles. Constante única:
/// muda só junto com a pasta (`v2/`).
pub const SCHEMA_VERSION: u32 = 1;

/// Nome da pasta do contrato dentro da saída.
pub const PASTA_VERSAO: &str = "v1";

/// Nome do arquivo de índice, na raiz de `v1/`.
pub const ARQUIVO_INDEX: &str = "index.json";

/// Casas decimais fixas do `total` na tabela geral (o índice já guarda o
/// total arredondado a 1e-9).
pub const CASAS_TOTAL: usize = 9;

// ---------------------------------------------------------------------------
// Valores de texto do contrato
// ---------------------------------------------------------------------------
//
// Identificadores próprios do JSON `v1`, sem acento nem espaço (decisão de
// 2026-10-12): não reaproveitam os rótulos de exibição do CSV e do relatório
// (`Grau::nome`, `Direcao::nome`), para que mudar um rótulo não mude o
// contrato em silêncio. Os testes fixam a tabela inteira.

/// `evidencia_grau` no JSON.
pub const fn grau_no_contrato(grau: Grau) -> &'static str {
    match grau {
        Grau::MuitoFraca => "muito_fraca",
        Grau::Fraca => "fraca",
        Grau::Moderada => "moderada",
        Grau::Forte => "forte",
        Grau::MuitoForte => "muito_forte",
    }
}

/// `evidencia_direcao` no JSON.
pub const fn direcao_no_contrato(direcao: Direcao) -> &'static str {
    match direcao {
        Direcao::Vies => "vies",
        Direcao::Acaso => "acaso",
    }
}

/// `normalizacao` no JSON.
pub const fn normalizacao_no_contrato(normalizacao: Normalizacao) -> &'static str {
    match normalizacao {
        Normalizacao::Zscore => "zscore",
        Normalizacao::Minmax => "minmax",
    }
}

/// Um dos arquivos de cada loteria: os três obrigatórios ([`Arquivo::TODOS`])
/// e os opcionais [`Arquivo::RankingTaxa`] e [`Arquivo::RankingPosicoes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arquivo {
    Ultimo,
    TabelaGeral,
    Historico,
    /// Opcional: só existe quando `ranking_taxa` está em dia. Fica **fora** de
    /// [`Arquivo::TODOS`], e a chave some do `index.json` quando falta.
    RankingTaxa,
    /// Opcional: só existe quando `posicao_*` está em dia (tarefa 2.6). Também
    /// fora de [`Arquivo::TODOS`], e a chave some do `index.json` quando falta.
    RankingPosicoes,
}

impl Arquivo {
    /// Os três arquivos obrigatórios, na ordem em que aparecem no
    /// `index.json` (o opcional, `RankingTaxa`, vem depois deles).
    pub const TODOS: [Arquivo; 3] = [Arquivo::Ultimo, Arquivo::TabelaGeral, Arquivo::Historico];

    /// Chave do arquivo no `index.json`.
    pub const fn chave(self) -> &'static str {
        match self {
            Arquivo::Ultimo => "ultimo",
            Arquivo::TabelaGeral => "tabela_geral",
            Arquivo::Historico => "historico",
            Arquivo::RankingTaxa => "ranking_taxa",
            Arquivo::RankingPosicoes => "ranking_posicoes",
        }
    }

    /// Nome do arquivo dentro da pasta da loteria.
    pub const fn nome(self) -> &'static str {
        match self {
            Arquivo::Ultimo => "ultimo.json",
            Arquivo::TabelaGeral => "tabela_geral.json",
            Arquivo::Historico => "historico.json",
            Arquivo::RankingTaxa => "ranking_taxa.json",
            Arquivo::RankingPosicoes => "ranking_posicoes.json",
        }
    }

    /// Caminho relativo a `v1/` (separador `/`, em qualquer sistema).
    pub fn caminho(self, slug: &str) -> String {
        format!("{slug}/{}", self.nome())
    }
}

/// Os arquivos de uma loteria, já em bytes: os três obrigatórios e, se o
/// ranking estava em dia, o `ranking_taxa.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivosLoteria {
    pub slug: String,
    pub ultimo: Vec<u8>,
    pub tabela_geral: Vec<u8>,
    pub historico: Vec<u8>,
    /// `Some` só com o ranking em dia; `None` = não gravar o arquivo (e apagar
    /// um antigo) e deixar a chave fora do `index.json`.
    pub ranking_taxa: Option<Vec<u8>>,
    /// Por que `ranking_taxa` é `None` (sempre `Some` quando é `None`).
    pub ranking_omitido: Option<MotivoRankingOmitido>,
    /// `Some` só com as posições em dia; `None` = não gravar o
    /// `ranking_posicoes.json` (e apagar um antigo) e deixar a chave fora do
    /// `index.json`.
    pub ranking_posicoes: Option<Vec<u8>>,
    /// Por que `ranking_posicoes` é `None` (sempre `Some` quando é `None`).
    pub posicoes_omitido: Option<MotivoPosicoesOmitido>,
}

impl ArquivosLoteria {
    /// Bytes de um arquivo; o `RankingTaxa` omitido dá uma fatia vazia (quem
    /// grava confere [`ArquivosLoteria::ranking_taxa`] antes).
    pub fn bytes(&self, arquivo: Arquivo) -> &[u8] {
        match arquivo {
            Arquivo::Ultimo => &self.ultimo,
            Arquivo::TabelaGeral => &self.tabela_geral,
            Arquivo::Historico => &self.historico,
            Arquivo::RankingTaxa => self.ranking_taxa.as_deref().unwrap_or_default(),
            Arquivo::RankingPosicoes => self.ranking_posicoes.as_deref().unwrap_or_default(),
        }
    }
}

/// Gera os arquivos de uma loteria (o `ranking_taxa.json` só se estiver em dia).
pub fn gerar_loteria(dados: &DadosExportacao) -> Result<ArquivosLoteria> {
    let (ranking_taxa, ranking_omitido) = match &dados.ranking {
        RankingParaExportar::EmDia(r) => (Some(ranking_taxa_json(dados, r)?.into_bytes()), None),
        RankingParaExportar::Omitido(m) => (None, Some(m.clone())),
    };
    let (ranking_posicoes, posicoes_omitido) = match &dados.posicoes {
        PosicoesParaExportar::EmDia(p) => {
            (Some(ranking_posicoes_json(dados, p)?.into_bytes()), None)
        }
        PosicoesParaExportar::Omitido(m) => (None, Some(m.clone())),
    };
    Ok(ArquivosLoteria {
        slug: dados.tabela.loteria.clone(),
        ultimo: ultimo_json(dados)?.into_bytes(),
        tabela_geral: tabela_geral_json(dados)?.into_bytes(),
        historico: historico_json(dados)?.into_bytes(),
        ranking_taxa,
        ranking_omitido,
        ranking_posicoes,
        posicoes_omitido,
    })
}

/// Dezenas sempre em ordem crescente: o contrato não guarda a ordem do sorteio.
fn dezenas_ordenadas(dezenas: &[u32]) -> Vec<u32> {
    let mut ordenadas = dezenas.to_vec();
    ordenadas.sort_unstable();
    ordenadas
}

/// `ultimo.json`: o último concurso sorteado.
pub fn ultimo_json(dados: &DadosExportacao) -> Result<String> {
    let ultimo = dados
        .resultados
        .last()
        .ok_or_else(|| SpiderError::ExportacaoInvalida("sem resultados para exportar".into()))?;
    if ultimo.concurso != dados.tabela.ultimo_sorteado {
        return Err(SpiderError::ExportacaoInvalida(format!(
            "o último resultado é o concurso {}, mas a tabela diz {}",
            ultimo.concurso, dados.tabela.ultimo_sorteado
        )));
    }
    Ok(Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("loteria", Valor::texto(&dados.tabela.loteria)),
        ("concurso", Valor::inteiro(ultimo.concurso)),
        ("data", Valor::texto(&ultimo.data_sorteio)),
        (
            "dezenas",
            Valor::linha_de(&dezenas_ordenadas(&ultimo.dezenas)),
        ),
    ])
    .para_arquivo())
}

/// `historico.json`: o histórico inteiro, em ordem crescente, um concurso por
/// linha.
pub fn historico_json(dados: &DadosExportacao) -> Result<String> {
    let mut anterior = 0;
    let mut concursos = Vec::with_capacity(dados.resultados.len());
    for r in &dados.resultados {
        if r.concurso <= anterior {
            return Err(SpiderError::ExportacaoInvalida(format!(
                "histórico fora de ordem: concurso {} depois do {anterior}",
                r.concurso
            )));
        }
        anterior = r.concurso;
        concursos.push(Valor::Linha(vec![
            Valor::inteiro(r.concurso),
            Valor::texto(&r.data_sorteio),
            Valor::linha_de(&dezenas_ordenadas(&r.dezenas)),
        ]));
    }
    Ok(Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("loteria", Valor::texto(&dados.tabela.loteria)),
        ("concursos", Valor::Lista(concursos)),
    ])
    .para_arquivo())
}

/// `tabela_geral.json`: o snapshot do próximo concurso com o perfil ativo.
pub fn tabela_geral_json(dados: &DadosExportacao) -> Result<String> {
    let t = &dados.tabela;
    let dezenas = t
        .linhas
        .iter()
        .map(entrada_da_dezena)
        .collect::<Result<Vec<_>>>()?;
    Ok(Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("loteria", Valor::texto(&t.loteria)),
        ("perfil", Valor::texto(&t.perfil)),
        ("hash_toml", Valor::texto(&dados.hash_toml)),
        ("janela", Valor::inteiro(t.janela)),
        (
            "normalizacao",
            Valor::texto(normalizacao_no_contrato(dados.normalizacao)),
        ),
        ("feature_version", Valor::inteiro(dados.feature_version)),
        ("concurso", Valor::inteiro(t.concurso)),
        ("ultimo_sorteado", Valor::inteiro(t.ultimo_sorteado)),
        ("dezenas", Valor::Lista(dezenas)),
    ])
    .para_arquivo())
}

fn entrada_da_dezena(l: &LinhaTabela) -> Result<Valor> {
    let f = &l.features;
    Ok(Valor::objeto(vec![
        ("dezena", Valor::inteiro(l.dezena)),
        ("posicao", Valor::inteiro(l.posicao)),
        ("total", Valor::fixo(l.total, CASAS_TOTAL)?),
        (
            "variacao_posicao",
            l.variacao_posicao.map_or(Valor::Nulo, Valor::Inteiro),
        ),
        ("frequencia", Valor::inteiro(f.frequencia)),
        ("maior_espaco", Valor::opcional(f.maior_espaco)),
        (
            "maior_espaco_ocorrencias",
            Valor::opcional(f.maior_espaco_ocorrencias),
        ),
        ("menor_espaco", Valor::opcional(f.menor_espaco)),
        (
            "menor_espaco_ocorrencias",
            Valor::opcional(f.menor_espaco_ocorrencias),
        ),
        ("soma_variacoes", Valor::opcional(f.soma_variacoes)),
        ("espaco_final", Valor::opcional(f.espaco_final)),
        ("repeticoes_total", Valor::inteiro(f.repeticoes_total)),
        ("repeticao_atual", Valor::inteiro(f.repeticao_atual)),
        (
            "evidencia_grau",
            Valor::opcional_texto(l.evidencia.map(|e| grau_no_contrato(e.grau))),
        ),
        (
            "evidencia_direcao",
            Valor::opcional_texto(l.evidencia.map(|e| direcao_no_contrato(e.direcao))),
        ),
    ]))
}

/// Casas decimais fixas das notas e da diferença para o acaso no
/// `ranking_taxa.json` (as mesmas 9 do `total`).
pub const CASAS_NOTA: usize = 9;

/// `ranking_taxa.json`: o ranking por taxa do próximo concurso, uma entrada por
/// dezena em ordem crescente de dezena. Não calcula nada: `diferenca_acaso` é
/// a subtração trivial `nota_a − K/N` sobre valores já gravados.
pub fn ranking_taxa_json(dados: &DadosExportacao, ranking: &RankingProximo) -> Result<String> {
    let t = &dados.tabela;
    let acaso = f64::from(ranking.qtd_sorteadas) / f64::from(ranking.qtd_dezenas);
    let mut linhas = ranking.linhas.iter().collect::<Vec<_>>();
    linhas.sort_by_key(|l| l.dezena);
    let dezenas = linhas
        .into_iter()
        .map(|l| {
            if l.concurso != t.concurso {
                return Err(SpiderError::ExportacaoInvalida(format!(
                    "o ranking traz o concurso {} para a dezena {}, mas o snapshot é do {}",
                    l.concurso, l.dezena, t.concurso
                )));
            }
            Ok(Valor::objeto(vec![
                ("dezena", Valor::inteiro(l.dezena)),
                ("posicao_indice", Valor::inteiro(l.posicao_indice)),
                ("nota_a", Valor::fixo(l.taxa_posicao, CASAS_NOTA)?),
                (
                    "diferenca_acaso",
                    Valor::fixo(l.taxa_posicao - acaso, CASAS_NOTA)?,
                ),
                ("nota_b", Valor::fixo(l.nota_b, CASAS_NOTA)?),
                ("posicao_a", Valor::inteiro(l.posicao_ranking_a)),
                ("posicao_b", Valor::inteiro(l.posicao_ranking_b)),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("loteria", Valor::texto(&t.loteria)),
        ("perfil", Valor::texto(&t.perfil)),
        ("hash_toml", Valor::texto(&dados.hash_toml)),
        ("janela", Valor::inteiro(t.janela)),
        (
            "normalizacao",
            Valor::texto(normalizacao_no_contrato(dados.normalizacao)),
        ),
        ("feature_version", Valor::inteiro(dados.feature_version)),
        ("concurso", Valor::inteiro(t.concurso)),
        ("ultimo_sorteado", Valor::inteiro(t.ultimo_sorteado)),
        ("taxa_acaso", Valor::fixo(acaso, CASAS_NOTA)?),
        ("dezenas", Valor::Lista(dezenas)),
    ])
    .para_arquivo())
}

/// `ranking_posicoes.json`: o G-1 do próximo concurso, uma entrada por dezena em
/// ordem crescente de dezena. Não calcula nada: só serializa o que
/// `ler_posicoes_para_exportar` leu e conferiu. Descreve o passado (a série de
/// ocupação das posições da tabela de força), não prevê sorteio.
pub fn ranking_posicoes_json(
    dados: &DadosExportacao,
    posicoes: &PosicoesProximo,
) -> Result<String> {
    let t = &dados.tabela;
    let mut linhas = posicoes.linhas.iter().collect::<Vec<_>>();
    linhas.sort_by_key(|l| l.dezena);
    if linhas.len() != t.linhas.len() {
        return Err(SpiderError::ExportacaoInvalida(format!(
            "as posições trazem {} dezena(s), mas a tabela geral tem {}",
            linhas.len(),
            t.linhas.len()
        )));
    }
    // `g_1` é uma ordem 1..=N sem repetição.
    let mut g_1: Vec<u32> = linhas.iter().map(|l| l.g_1).collect();
    g_1.sort_unstable();
    if g_1.iter().zip(1u32..).any(|(&g, esperado)| g != esperado) {
        return Err(SpiderError::ExportacaoInvalida(
            "o g_1 das posições não é uma ordem 1..=N sem repetição".into(),
        ));
    }
    let dezenas = linhas
        .into_iter()
        .map(|l| {
            let da_tabela = t.linhas.iter().find(|x| x.dezena == l.dezena);
            if da_tabela.map(|x| x.posicao) != Some(l.posicao_indice) {
                return Err(SpiderError::ExportacaoInvalida(format!(
                    "a dezena {} está na posição {} nas posições e em outra na tabela geral",
                    l.dezena, l.posicao_indice
                )));
            }
            Ok(Valor::objeto(vec![
                ("dezena", Valor::inteiro(l.dezena)),
                ("posicao_indice", Valor::inteiro(l.posicao_indice)),
                ("g_1", Valor::inteiro(l.g_1)),
                (
                    "variacao_g_1",
                    l.variacao_g_1.map_or(Valor::Nulo, Valor::Inteiro),
                ),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("loteria", Valor::texto(&t.loteria)),
        ("perfil", Valor::texto(&t.perfil)),
        ("hash_toml", Valor::texto(&dados.hash_toml)),
        ("janela", Valor::inteiro(t.janela)),
        (
            "normalizacao",
            Valor::texto(normalizacao_no_contrato(dados.normalizacao)),
        ),
        ("feature_version", Valor::inteiro(dados.feature_version)),
        ("concurso", Valor::inteiro(t.concurso)),
        ("ultimo_sorteado", Valor::inteiro(t.ultimo_sorteado)),
        ("perfil_posicoes", Valor::texto(&posicoes.perfil_posicoes)),
        (
            "hash_toml_posicoes",
            Valor::texto(&posicoes.hash_toml_posicoes),
        ),
        ("janela_posicoes", Valor::inteiro(posicoes.janela_posicoes)),
        (
            "normalizacao_posicoes",
            Valor::texto(normalizacao_no_contrato(posicoes.normalizacao_posicoes)),
        ),
        ("dezenas", Valor::Lista(dezenas)),
    ])
    .para_arquivo())
}

/// SHA-256 (hex minúsculo) e tamanho em bytes de um arquivo exportado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impressao {
    pub sha256: String,
    pub bytes: u64,
}

impl Impressao {
    pub fn de(bytes: &[u8]) -> Impressao {
        Impressao {
            sha256: sha256_hex(bytes),
            bytes: bytes.len() as u64,
        }
    }
}

/// O que o `index.json` diz de uma loteria que tem arquivos na saída.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoteriaNoIndice {
    pub ultimo_concurso: u32,
    /// Data do último concurso (`AAAA-MM-DD`), a de `ultimo.json`.
    pub data: String,
    /// Na ordem de [`Arquivo::TODOS`].
    pub arquivos: [Impressao; 3],
    /// O `ranking_taxa.json`, se está no disco; `None` = a chave fica fora do
    /// `index.json`.
    pub ranking_taxa: Option<Impressao>,
    /// O `ranking_posicoes.json`, se está no disco; `None` = a chave fica fora
    /// do `index.json`. Vem depois do `ranking_taxa`.
    pub ranking_posicoes: Option<Impressao>,
}

/// `index.json`: uma entrada por loteria, na ordem dada (a do config);
/// `None` (loteria sem arquivos na saída) vira `null`.
pub fn index_json(gerado_em: &str, loterias: &[(&str, Option<LoteriaNoIndice>)]) -> String {
    let entradas = loterias
        .iter()
        .map(|(slug, entrada)| {
            let valor = entrada.as_ref().map_or(Valor::Nulo, |e| {
                let opcional = e.ranking_taxa.as_ref().map(|i| (&Arquivo::RankingTaxa, i));
                let opcional_posicoes = e
                    .ranking_posicoes
                    .as_ref()
                    .map(|i| (&Arquivo::RankingPosicoes, i));
                let arquivos = Arquivo::TODOS
                    .iter()
                    .zip(&e.arquivos)
                    .chain(opcional)
                    .chain(opcional_posicoes)
                    .map(|(a, i)| {
                        (
                            a.chave(),
                            Valor::objeto(vec![
                                ("caminho", Valor::Texto(a.caminho(slug))),
                                ("sha256", Valor::texto(&i.sha256)),
                                ("bytes", Valor::Inteiro(i.bytes as i64)),
                            ]),
                        )
                    })
                    .collect();
                Valor::objeto(vec![
                    ("ultimo_concurso", Valor::inteiro(e.ultimo_concurso)),
                    ("data", Valor::texto(&e.data)),
                    ("arquivos", Valor::objeto(arquivos)),
                ])
            });
            (slug.to_string(), valor)
        })
        .collect();
    Valor::objeto(vec![
        ("schema_version", Valor::inteiro(SCHEMA_VERSION)),
        ("gerado_em", Valor::texto(gerado_em)),
        ("loterias", Valor::objeto_de(entradas)),
    ])
    .para_arquivo()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avaliacao::{Classificacao, Direcao, Grau};
    use crate::motor::FeaturesDezena;
    use crate::orquestracao::{DadosExportacao, TabelaConcurso};
    use crate::Resultado;

    fn dados() -> DadosExportacao {
        let features = FeaturesDezena {
            concurso: 3,
            dezena: 7,
            janela: 10,
            frequencia: 1,
            maior_espaco: None,
            maior_espaco_ocorrencias: None,
            menor_espaco: Some(0),
            menor_espaco_ocorrencias: Some(2),
            soma_variacoes: None,
            espaco_final: Some(1),
            repeticoes_total: 0,
            repeticao_atual: 0,
            saiu: None,
        };
        let resultado = |c: u32, d: &str, dz: &[u32]| Resultado {
            loteria: "quina".into(),
            concurso: c,
            data_sorteio: d.into(),
            dezenas: dz.to_vec(),
        };
        DadosExportacao {
            tabela: TabelaConcurso {
                loteria: "quina".into(),
                perfil: "padrao_v1".into(),
                janela: 10,
                concurso: 3,
                ultimo_sorteado: 2,
                data_sorteio: None,
                linhas: vec![LinhaTabela {
                    dezena: 7,
                    posicao: 1,
                    total: -0.000_000_000_4,
                    variacao_posicao: None,
                    saiu: None,
                    features,
                    evidencia: Some(Classificacao {
                        grau: Grau::MuitoFraca,
                        direcao: Direcao::Vies,
                    }),
                }],
            },
            hash_toml: "abc".into(),
            normalizacao: Normalizacao::Zscore,
            feature_version: 1,
            ranking: RankingParaExportar::Omitido(MotivoRankingOmitido::SemTabela),
            posicoes: PosicoesParaExportar::Omitido(MotivoPosicoesOmitido::SemTabela),
            resultados: vec![
                resultado(1, "2020-01-02", &[5, 3, 9, 1, 7]),
                resultado(2, "2020-01-04", &[8, 2, 4, 6, 10]),
            ],
        }
    }

    #[test]
    fn ultimo_json_confere_byte_a_byte() {
        assert_eq!(
            ultimo_json(&dados()).unwrap(),
            "{\n  \"schema_version\": 1,\n  \"loteria\": \"quina\",\n  \"concurso\": 2,\n  \
             \"data\": \"2020-01-04\",\n  \"dezenas\": [2, 4, 6, 8, 10]\n}\n"
        );
    }

    #[test]
    fn historico_json_tem_um_concurso_por_linha_com_dezenas_ordenadas() {
        assert_eq!(
            historico_json(&dados()).unwrap(),
            "{\n  \"schema_version\": 1,\n  \"loteria\": \"quina\",\n  \"concursos\": [\n    \
             [1, \"2020-01-02\", [1, 3, 5, 7, 9]],\n    [2, \"2020-01-04\", [2, 4, 6, 8, 10]]\n  ]\n}\n"
        );
    }

    #[test]
    fn historico_fora_de_ordem_e_recusado() {
        let mut d = dados();
        d.resultados.reverse();
        assert!(matches!(
            historico_json(&d),
            Err(SpiderError::ExportacaoInvalida(_))
        ));
    }

    #[test]
    fn tabela_geral_tem_nove_casas_e_null_explicito() {
        let t = tabela_geral_json(&dados()).unwrap();
        assert!(t.contains("\"hash_toml\": \"abc\",\n  \"janela\": 10,\n  \"normalizacao\": \"zscore\",\n  \"feature_version\": 1,\n  \"concurso\": 3,\n  \"ultimo_sorteado\": 2,\n"));
        assert!(t.contains("      \"total\": 0.000000000,\n      \"variacao_posicao\": null,\n"));
        assert!(t.contains("\"maior_espaco\": null,"));
        assert!(t.contains("\"soma_variacoes\": null,"));
        assert!(t.contains("\"evidencia_grau\": \"muito_fraca\",\n"));
        assert!(t.contains("\"evidencia_direcao\": \"vies\"\n"));
        assert!(t.is_ascii(), "o JSON v1 não tem acento");
    }

    #[test]
    fn valores_de_texto_do_contrato_ficam_fixos() {
        // Contrato v1: trocar qualquer um destes valores quebra o app.
        let graus: Vec<&str> = Grau::TODOS.iter().map(|&g| grau_no_contrato(g)).collect();
        assert_eq!(
            graus,
            ["muito_fraca", "fraca", "moderada", "forte", "muito_forte"]
        );
        assert_eq!(direcao_no_contrato(Direcao::Vies), "vies");
        assert_eq!(direcao_no_contrato(Direcao::Acaso), "acaso");
        assert_eq!(normalizacao_no_contrato(Normalizacao::Zscore), "zscore");
        assert_eq!(normalizacao_no_contrato(Normalizacao::Minmax), "minmax");
    }

    #[test]
    fn index_tem_null_para_loteria_sem_arquivos() {
        let entrada = LoteriaNoIndice {
            ultimo_concurso: 2,
            data: "2020-01-04".into(),
            arquivos: [
                Impressao::de(b"a"),
                Impressao::de(b"bb"),
                Impressao::de(b"ccc"),
            ],
            ranking_taxa: None,
            ranking_posicoes: None,
        };
        let j = index_json(
            "2026-10-12T10:00:00Z",
            &[("lotofacil", None), ("quina", Some(entrada))],
        );
        assert!(j.starts_with(
            "{\n  \"schema_version\": 1,\n  \"gerado_em\": \"2026-10-12T10:00:00Z\",\n  \"loterias\": {\n    \"lotofacil\": null,\n    \"quina\": {\n      \"ultimo_concurso\": 2,"
        ));
        assert!(j.contains("\"caminho\": \"quina/tabela_geral.json\""));
        assert!(j.contains("\"bytes\": 2"));
    }
}
