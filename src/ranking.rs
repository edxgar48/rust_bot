//! CSV e relatório em texto do `sync ranking` (Fase 1f, tarefa 1f.3).
//!
//! O CSV segue o formato da planilha de `tabela`: separador `;`, vírgula
//! decimal, UTF-8 **com BOM** (o Excel pt-BR assume Windows-1252 sem ele) e
//! fim de linha `\n`. Traz o ranking do próximo concurso, uma linha por
//! dezena, da melhor para a pior pela nota A (a principal); desempate pela
//! menor dezena. Colunas, como em `ranking_taxa`: `taxa_posicao` é a nota A e
//! `nota_b` é a nota B (**experimental**). Floats com [`CASAS_NOTA`] casas
//! fixas (a ordenação compara as notas arredondadas a 1e-9, então 9 casas
//! reproduzem a ordem), sem `-0,000000000`.
//!
//! Mesmo relatório → mesmos bytes, tanto no CSV quanto no texto: ordem
//! explícita em tudo e nenhuma data nem tempo de execução (estes ficam por
//! conta da CLI, fora do texto).

use crate::avaliacao::{
    self, Direcao, EsperadoPriori, Grade, Grau, LinhaRanking, CONCURSOS_FICTICIOS, QTD_PRIORIS,
};
use crate::config::LoteriaConfig;
use crate::motor::JANELA_TOTAL;
use crate::orquestracao::RelatorioRanking;
use std::fmt::Write;

/// Cabeçalho do CSV, na ordem das colunas.
pub const CABECALHO: &str = "loteria;perfil;concurso;dezena;posicao_indice;taxa_posicao;\
nota_b;posicao_ranking_a;posicao_ranking_b";

/// Marca de ordem de bytes UTF-8 (`EF BB BF`) que abre o arquivo.
pub const BOM: &str = "\u{FEFF}";

/// Casas decimais das notas no CSV.
pub const CASAS_NOTA: usize = 9;

/// Quantas dezenas cada ranking mostra no texto (o CSV traz todas).
pub const TOPO_NO_TEXTO: usize = 10;

/// Nome do arquivo: `ranking_<loteria>_<perfil>_<concurso>.csv`, com o
/// concurso do ranking (o próximo, ainda não sorteado).
pub fn nome_arquivo(r: &RelatorioRanking) -> String {
    format!(
        "ranking_{}_{}_{}.csv",
        r.loteria, r.perfil, r.proximo_concurso
    )
}

/// As linhas do próximo concurso, da melhor para a pior pela nota A.
fn ordenadas_por_a(r: &RelatorioRanking) -> Vec<&LinhaRanking> {
    let mut v: Vec<&LinhaRanking> = r.proximo.iter().collect();
    v.sort_by_key(|l| (l.posicao_ranking_a, l.dezena));
    v
}

/// As linhas do próximo concurso, da melhor para a pior pela nota B.
fn ordenadas_por_b(r: &RelatorioRanking) -> Vec<&LinhaRanking> {
    let mut v: Vec<&LinhaRanking> = r.proximo.iter().collect();
    v.sort_by_key(|l| (l.posicao_ranking_b, l.dezena));
    v
}

/// O CSV inteiro: [`BOM`], cabeçalho e uma linha por dezena do próximo
/// concurso, em ordem da nota A.
pub fn formatar_csv(r: &RelatorioRanking) -> String {
    let mut saida = String::from(BOM);
    saida.push_str(CABECALHO);
    saida.push('\n');
    for l in ordenadas_por_a(r) {
        // `write!` num `String` não falha.
        let _ = writeln!(
            saida,
            "{};{};{};{};{};{};{};{};{}",
            r.loteria,
            r.perfil,
            l.concurso,
            l.dezena,
            l.posicao_indice,
            decimal(l.nota_a),
            decimal(l.nota_b),
            l.posicao_ranking_a,
            l.posicao_ranking_b,
        );
    }
    saida
}

/// Nota com [`CASAS_NOTA`] casas e vírgula decimal, sem `-0,000000000`.
fn decimal(valor: f64) -> String {
    let texto = format!("{valor:.CASAS_NOTA$}");
    let texto = if texto
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        texto.trim_start_matches('-').to_string()
    } else {
        texto
    };
    texto.replace('.', ",")
}

fn linha(s: &mut String, texto: impl AsRef<str>) {
    s.push_str(texto.as_ref());
    s.push('\n');
}

fn descrever_janela(janela: u32) -> String {
    if janela == JANELA_TOTAL {
        "total".to_string()
    } else {
        janela.to_string()
    }
}

fn formatar_desvio(desvio: f64) -> String {
    format!("{:.1} %", desvio * 100.0)
}

/// Relatório em texto do `sync ranking`. Determinístico: mesma entrada, mesmos
/// bytes. Mostra o topo de cada ranking, as taxas por posição e por faixa com
/// grau e direção, e a contagem de graus observada contra a esperada sob o
/// acaso, sempre com o aviso de que é descrição do passado.
pub fn formatar_relatorio(cfg: &LoteriaConfig, r: &RelatorioRanking) -> String {
    let n = cfg.qtd_dezenas_possiveis();
    let ev = &r.evidencia;
    let mut s = String::new();

    linha(
        &mut s,
        format!(
            "Ranking por taxa histórica de posição e componentes da {}: concurso {} (o próximo, \
             ainda não sorteado).",
            cfg.nome_exibicao, r.proximo_concurso
        ),
    );
    linha(
        &mut s,
        format!(
            "Perfil {} (versão {}, janela {}, {}, hash {}).",
            r.perfil,
            r.versao,
            descrever_janela(r.janela),
            r.normalizacao,
            r.hash_toml
        ),
    );
    linha(
        &mut s,
        format!(
            "Histórico: concursos {} a {} ({} sorteados). Cada taxa é encolhida em direção a K/N = \
             {}/{} = {:.4} com {} concursos fictícios (pares concurso x dezena, nas faixas).",
            r.primeiro_concurso,
            r.ultimo_concurso,
            r.concursos_sorteados,
            cfg.qtd_dezenas,
            n,
            ev.p,
            CONCURSOS_FICTICIOS
        ),
    );
    linha(
        &mut s,
        "AVISO: isto é a descrição do passado, não uma previsão. Os sorteios são independentes: o \
         resultado esperado de qualquer ranking feito com estas taxas é empatar com o acaso. Grau \
         e direção só descrevem; nenhuma nota nem ordem os usa. A descrição lê o histórico \
         inteiro, inclusive o conjunto trancado, e nada aqui avalia ou escolhe perfil.",
    );

    // --- rankings do próximo concurso ---------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "== Ranking pela nota A (principal): taxa histórica da posição da dezena no índice \
             (top {} de {}) ==",
            TOPO_NO_TEXTO.min(r.proximo.len()),
            r.proximo.len()
        ),
    );
    linha(
        &mut s,
        format!(
            "  {:>7}  {:>6}  {:>17}  {:>10}",
            "ranking", "dezena", "posição no índice", "nota A (%)"
        ),
    );
    for l in ordenadas_por_a(r).into_iter().take(TOPO_NO_TEXTO) {
        linha(
            &mut s,
            format!(
                "  {:>7}  {:>6}  {:>17}  {:>10.4}",
                l.posicao_ranking_a,
                format!("{:02}", l.dezena),
                l.posicao_indice,
                100.0 * l.nota_a
            ),
        );
    }

    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "== Ranking pela nota B (EXPERIMENTAL; top {} de {}) ==",
            TOPO_NO_TEXTO.min(r.proximo.len()),
            r.proximo.len()
        ),
    );
    linha(
        &mut s,
        "  EXPERIMENTAL: B soma ln(taxa / (K/N)) sobre a posição e as 9 faixas (naive Bayes). Os \
         componentes são correlacionados, então B conta a mesma informação mais de uma vez e \
         exagera a distância ao acaso. Nunca é a nota principal.",
    );
    linha(
        &mut s,
        format!(
            "  {:>7}  {:>6}  {:>17}  {:>10}",
            "ranking", "dezena", "posição no índice", "nota B"
        ),
    );
    for l in ordenadas_por_b(r).into_iter().take(TOPO_NO_TEXTO) {
        linha(
            &mut s,
            format!(
                "  {:>7}  {:>6}  {:>17}  {:>+10.4}",
                l.posicao_ranking_b,
                format!("{:02}", l.dezena),
                l.posicao_indice,
                l.nota_b
            ),
        );
    }
    linha(
        &mut s,
        format!(
            "  (o CSV traz as {} dezenas, ordenadas pela nota A.)",
            r.proximo.len()
        ),
    );

    // --- taxa por posição ---------------------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "== Taxa por posição do índice (histórico inteiro, {} concursos) ==",
            ev.concursos
        ),
    );
    linha(
        &mut s,
        format!(
            "  {:>7}  {:>7}  {:>9}  {:>9}  {:>9}  {:<11}  {:<16}",
            "posição",
            "saídas",
            "concursos",
            "taxa (%)",
            format!("log10 FB {}", formatar_desvio(ev.prioris[0].desvio)),
            "grau",
            "direção"
        ),
    );
    // As taxas e os textos de grau e direção são os que foram gravados; o
    // log10 FB vem da evidência, na mesma ordem (posição crescente).
    for (p, t) in ev.posicoes.iter().zip(&r.taxas_posicao) {
        let log10_fb = p.avaliacao.as_ref().map(|a| a.log10_fb[0]);
        linha(
            &mut s,
            format!(
                "  {:>7}  {:>7}  {:>9}  {:>9.4}  {:>9}  {:<11}  {:<16}",
                t.posicao,
                t.saidas,
                t.exposicoes,
                100.0 * t.taxa,
                log10_fb.map_or_else(|| "-".to_string(), |v| format!("{v:+.3}")),
                t.grau.as_deref().unwrap_or("-"),
                t.direcao.as_deref().unwrap_or("sem exposição")
            ),
        );
    }

    // --- taxa por faixa -----------------------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        "== Taxa por (componente, faixa) do tercil (histórico inteiro; \"nulo\" = componente sem \
         valor) ==",
    );
    linha(
        &mut s,
        format!(
            "  {:<26}  {:<6}  {:>8}  {:>9}  {:>9}  {:>9}  {:<11}  {:<16}",
            "componente",
            "faixa",
            "saídas",
            "pares",
            "taxa (%)",
            format!("log10 FB {}", formatar_desvio(ev.prioris[0].desvio)),
            "grau",
            "direção"
        ),
    );
    for (f, t) in ev.faixas.iter().zip(&r.taxas_faixa) {
        let log10_fb = f.avaliacao.as_ref().map(|a| a.log10_fb[0]);
        linha(
            &mut s,
            format!(
                "  {:<26}  {:<6}  {:>8}  {:>9}  {:>9.4}  {:>9}  {:<11}  {:<16}",
                t.componente,
                t.faixa,
                t.saidas,
                t.exposicoes,
                100.0 * t.taxa,
                log10_fb.map_or_else(|| "-".to_string(), |v| format!("{v:+.3}")),
                t.grau.as_deref().unwrap_or("-"),
                t.direcao.as_deref().unwrap_or("sem exposição")
            ),
        );
    }
    linha(
        &mut s,
        "  (taxa = (saídas + m·K/N) / (exposição + m): sem exposição vale K/N, e o grau fica de \
         fora da contagem.)",
    );

    // --- graus: observado x esperado ----------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "AVISO: com {} posições e {} (componente, faixa), algumas saem com grau moderado ou \
             maior só por acaso. Compare a contagem observada com a esperada sob o acaso \
             (simulação de {} réplicas, semente {}, sorteio sem reposição mantendo as posições e \
             faixas reais de cada concurso); só um excesso sobre o esperado indica algo além do \
             acaso. O fator de Bayes binomial trata como independentes os pares do mesmo \
             concurso, que não são (hipergeométrica): por isso a comparação justa é com o \
             esperado simulado.",
            r.esperado.celulas_posicoes,
            r.esperado.celulas_faixas,
            r.esperado.replicas,
            r.esperado.semente
        ),
    );
    linha(&mut s, "");
    escrever_graus(
        &mut s,
        &format!("Posições ({} células)", r.esperado.celulas_posicoes),
        &ev.observada_posicoes,
        &r.esperado.posicoes,
        &r.esperado,
    );
    linha(&mut s, "");
    escrever_graus(
        &mut s,
        &format!(
            "Faixas de componente ({} células com exposição)",
            r.esperado.celulas_faixas
        ),
        &ev.observada_faixas,
        &r.esperado.faixas,
        &r.esperado,
    );
    s
}

/// Tabela "observadas / esperadas sob o acaso" por grau e direção, para cada
/// priori (a primeira é a principal).
fn escrever_graus(
    s: &mut String,
    titulo: &str,
    observada: &[Grade<usize>; QTD_PRIORIS],
    esperado: &[EsperadoPriori],
    simulacao: &avaliacao::EsperadoTaxas,
) {
    linha(
        s,
        format!(
            "{titulo}: observadas / esperadas sob o acaso (média ± erro padrão de {} réplicas, \
             {} concursos sorteados):",
            simulacao.replicas, simulacao.concursos
        ),
    );
    for (k, esp) in esperado.iter().enumerate() {
        let rotulo = if k == 0 { " (principal)" } else { "" };
        linha(
            s,
            format!("  Priori {}{rotulo}:", formatar_desvio(esp.desvio)),
        );
        linha(
            s,
            format!(
                "    {:<12}  {:>24}  {:>24}",
                "grau", "a favor de viés", "a favor do acaso"
            ),
        );
        for grau in Grau::TODOS {
            let celula = |direcao| {
                format!(
                    "{:>3} / {:>7.2} ± {:.2}",
                    observada[k].do_grau(grau, direcao),
                    esp.media.do_grau(grau, direcao),
                    esp.erro_padrao.do_grau(grau, direcao)
                )
            };
            linha(
                s,
                format!(
                    "    {:<12}  {:>24}  {:>24}",
                    grau.nome(),
                    celula(Direcao::Vies),
                    celula(Direcao::Acaso)
                ),
            );
        }
        linha(
            s,
            format!(
                "    Moderada ou maior (as duas direções): {} observadas contra {:.2} esperadas \
                 sob o acaso.",
                observada[k].a_partir_de(Grau::Moderada),
                esp.media.a_partir_de(Grau::Moderada)
            ),
        );
    }
}

#[cfg(test)]
mod tests;
