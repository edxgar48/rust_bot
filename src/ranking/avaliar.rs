//! Relatório em texto do `sync ranking --avaliar` (Fase 1f, tarefa 1f.4).
//!
//! Mostra sempre: os concursos avaliados, a métrica do ranking, a do acaso, a
//! diferença e a incerteza, por dobra de teste (2 a 5) e no trecho inteiro, com
//! o aviso de que é descrição do passado e não previsão. Determinístico: mesma
//! entrada, mesmos bytes (nenhum tempo de execução no texto; este fica por conta
//! da CLI).

use crate::avaliacao::{
    Cauda, Comparacao, DobraDeTeste, Intervalo, PorTrecho, ResumoAvaliacao, ResumoTopK, ALFA,
    BLOCO_BOOTSTRAP, CONCURSOS_FICTICIOS,
};
use crate::config::LoteriaConfig;
use crate::motor::JANELA_TOTAL;
use crate::orquestracao::{descrever_faixa, RelatorioAvaliacaoRanking};
use std::fmt::Write;

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

fn formatar_intervalo(ic: Intervalo, numero: impl Fn(f64) -> String) -> String {
    format!("[{}, {}]", numero(ic.inferior), numero(ic.superior))
}

/// p-valor com 4 casas, ou em notação científica abaixo de 0,001.
fn formatar_p(p: f64) -> String {
    if p >= 1e-3 {
        format!("{p:.4}")
    } else {
        format!("{p:.3e}")
    }
}

/// Cauda do top-k: como [`formatar_p`], ou pela potência de 10 quando a
/// probabilidade é pequena demais para o `f64`.
fn formatar_cauda(c: Cauda) -> String {
    if c.probabilidade >= f64::MIN_POSITIVE {
        formatar_p(c.probabilidade)
    } else {
        format!("10^{:.1}", c.log10)
    }
}

/// Tabela de log loss ou Brier: uma linha por dobra de teste e o total.
fn tabela_metrica(
    s: &mut String,
    rotulo: &str,
    dobras: &[DobraDeTeste],
    trecho: &PorTrecho<ResumoAvaliacao>,
    metrica: fn(&ResumoAvaliacao) -> &Comparacao,
) {
    let _ = writeln!(
        s,
        "  {:>8}  {:>11}  {:>5}  {:>10}  {:>10}  {:>11}  {:>10}  IC 95% (normal)",
        "dobra", "concursos", "T", rotulo, "acaso", "diferença", "EP"
    );
    let linhas = dobras
        .iter()
        .zip(&trecho.dobras)
        .map(|(d, r)| (d.dobra.to_string(), r))
        .chain(std::iter::once(("total".to_string(), &trecho.total)));
    for (nome, r) in linhas {
        let c = metrica(r);
        let _ = writeln!(
            s,
            "  {:>8}  {:>11}  {:>5}  {:>10.6}  {:>10.6}  {:>11}  {:>10}  {}",
            nome,
            descrever_faixa(r.primeiro_concurso, r.ultimo_concurso),
            r.concursos_avaliados,
            c.perfil,
            c.acaso,
            format!("{:+.3e}", c.diferenca),
            c.erro_padrao
                .map_or_else(|| "—".to_string(), |ep| format!("{ep:.3e}")),
            c.ic95.map_or_else(
                || "—".to_string(),
                |ic| formatar_intervalo(ic, |x| format!("{x:+.3e}"))
            )
        );
    }
}

/// Tabela do top-k: acertos por concurso e o p-valor exato da 1c.2.
fn tabela_topk(s: &mut String, dobras: &[DobraDeTeste], trecho: &PorTrecho<ResumoTopK>) {
    let _ = writeln!(
        s,
        "  {:>8}  {:>11}  {:>5}  {:>7}  {:>7}  {:>9}  {:>7}  {:>19}  p exato",
        "dobra", "concursos", "T", "ranking", "acaso", "diferença", "EP", "IC 95% (normal)"
    );
    let linhas = dobras
        .iter()
        .zip(&trecho.dobras)
        .map(|(d, r)| (d.dobra.to_string(), r))
        .chain(std::iter::once(("total".to_string(), &trecho.total)));
    for (nome, r) in linhas {
        let c = &r.acertos;
        let _ = writeln!(
            s,
            "  {:>8}  {:>11}  {:>5}  {:>7.4}  {:>7.4}  {:>9}  {:>7}  {:>19}  {}",
            nome,
            descrever_faixa(r.primeiro_concurso, r.ultimo_concurso),
            r.concursos_avaliados,
            c.perfil,
            c.acaso,
            format!("{:+.4}", c.diferenca),
            c.erro_padrao
                .map_or_else(|| "—".to_string(), |ep| format!("{ep:.4}")),
            c.ic95.map_or_else(
                || "—".to_string(),
                |ic| formatar_intervalo(ic, |x| format!("{x:+.4}"))
            ),
            formatar_cauda(r.p_valor)
        );
    }
}

/// Distribuição de acertos do top-k no trecho inteiro, observado × acaso.
fn distribuicao(s: &mut String, k: u32, total: &ResumoTopK) {
    linha(
        s,
        format!(
            "Distribuição de acertos do top-{k} no trecho inteiro (concursos; observado × acaso):"
        ),
    );
    for f in &total.distribuicao {
        let _ = writeln!(
            s,
            "  {:>2} acerto(s): {:>6} × {:>9.1}",
            f.acertos, f.observados, f.esperados
        );
    }
}

/// Relatório em texto do `sync ranking --avaliar`: A (principal) e B
/// (experimental) na régua da 1c e da 1e, dobras de teste 2 a 5.
pub fn formatar_avaliacao(cfg: &LoteriaConfig, r: &RelatorioAvaliacaoRanking) -> String {
    let av = &r.avaliacao;
    let mut s = String::new();

    linha(
        &mut s,
        format!(
            "Avaliação do ranking por taxa histórica da {} ({}) com o perfil {} (versão {}, janela \
             {}, {}).",
            cfg.nome_exibicao,
            r.loteria,
            r.perfil,
            r.versao,
            descrever_janela(r.janela),
            r.normalizacao
        ),
    );
    linha(&mut s, format!("  Hash do perfil: {}", r.hash_toml));
    linha(
        &mut s,
        "  AVISO: isto é a descrição do passado, não uma previsão. Os sorteios são independentes: o \
         resultado esperado de qualquer ranking é empatar com o acaso, e qualquer ganho aparente é \
         suspeita de vazamento ou de sobreajuste até prova em contrário. Nada aqui diz o que vai sair.",
    );
    linha(
        &mut s,
        format!(
            "  Trecho: busca, walk-forward; dobras de teste {} a {}, concursos {} ({} avaliados). \
             A dobra 1 não é avaliada (só treina a calibração).",
            av.dobras.first().map_or(0, |d| d.dobra),
            av.dobras.last().map_or(0, |d| d.dobra),
            descrever_faixa(av.faixa.primeiro, av.faixa.ultimo),
            av.concursos
        ),
    );
    linha(
        &mut s,
        format!(
            "  Corte da busca: concurso {} (o trancado, do {} em diante, não entra nesta avaliação, \
             que não tem `--trancado`; só as conferências de integridade o leem).",
            av.corte,
            u64::from(av.corte) + 1
        ),
    );
    linha(
        &mut s,
        format!(
            "  Índice: a calibração começa no concurso {} com m = {} concursos fictícios, e esse \
             primeiro concurso não é avaliado.",
            r.primeiro_concurso, CONCURSOS_FICTICIOS
        ),
    );
    let padrao = if av.k as usize == cfg.qtd_dezenas {
        " (padrão: K da loteria)"
    } else {
        ""
    };
    linha(&mut s, format!("  Top-k: k = {}{padrao}.", av.k));

    // --- A ---------------------------------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        "== Ordem de A (principal): dezenas ordenadas pela taxa histórica da posição ==",
    );
    linha(
        &mut s,
        "  A é a calibração da 1c (uma taxa por posição, só com o passado): o log loss e o Brier \
         de A são idênticos aos do perfil no `sync avaliar` e por isso não se repetem aqui. \
         Abaixo só o que é novo: o top-k pela ordem de A e a calibração linear sobre ela.",
    );
    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "Top-{} pela ordem de A (acertos por concurso; maior é melhor; ao acaso k·K/N = \
             {:.4}):",
            av.k, av.a.topk.total.acertos.acaso
        ),
    );
    tabela_topk(&mut s, &av.dobras, &av.a.topk);
    linha(&mut s, "");
    distribuicao(&mut s, av.k, &av.a.topk.total);
    linha(&mut s, "");
    linha(
        &mut s,
        "Log loss com a calibração linear (1e) sobre a ordem de A (só informativo: sem bootstrap; \
         diferença < 0 = ranking melhor que o acaso):",
    );
    tabela_metrica(
        &mut s,
        "ordem A",
        &av.dobras,
        &av.a.linear,
        |r: &ResumoAvaliacao| &r.log_loss,
    );

    // --- B ---------------------------------------------------------------
    linha(&mut s, "");
    linha(
        &mut s,
        "== Ordem de B (EXPERIMENTAL): dezenas ordenadas pelo naive Bayes sobre a posição e as 9 \
         faixas ==",
    );
    linha(
        &mut s,
        "  EXPERIMENTAL: B soma ln(taxa / (K/N)) sobre a posição e as faixas dos componentes, que \
         são correlacionados; B conta a mesma informação mais de uma vez e nunca é a nota \
         principal. B é um escore, não uma probabilidade: a probabilidade vem da calibração pela \
         posição da 1c (uma taxa por posição em B, m = 100) sobre a ordem de B.",
    );
    linha(&mut s, "");
    linha(
        &mut s,
        "Log loss, calibração da 1c sobre a ordem de B (menor é melhor; diferença < 0 = B melhor \
         que o acaso):",
    );
    tabela_metrica(
        &mut s,
        "ordem B",
        &av.dobras,
        &av.b.posicao,
        |r: &ResumoAvaliacao| &r.log_loss,
    );
    linha(&mut s, "");
    linha(
        &mut s,
        "Brier, calibração da 1c sobre a ordem de B (menor é melhor; diferença < 0 = B melhor que \
         o acaso):",
    );
    tabela_metrica(
        &mut s,
        "ordem B",
        &av.dobras,
        &av.b.posicao,
        |r: &ResumoAvaliacao| &r.brier,
    );
    linha(&mut s, "");
    linha(
        &mut s,
        "Log loss com a calibração linear (1e) sobre a ordem de B (só informativo: sem bootstrap; \
         diferença < 0 = B melhor que o acaso):",
    );
    tabela_metrica(
        &mut s,
        "ordem B",
        &av.dobras,
        &av.b.linear,
        |r: &ResumoAvaliacao| &r.log_loss,
    );
    linha(&mut s, "");
    linha(
        &mut s,
        format!(
            "Top-{} pela ordem de B (acertos por concurso; maior é melhor; ao acaso k·K/N = {:.4}):",
            av.k, av.b.topk.total.acertos.acaso
        ),
    );
    tabela_topk(&mut s, &av.dobras, &av.b.topk);
    linha(&mut s, "");
    distribuicao(&mut s, av.k, &av.b.topk.total);
    linha(&mut s, "");
    escrever_veredito_de_b(&mut s, r);
    if r.perfil != cfg.perfil_ativo {
        linha(&mut s, "");
        linha(
            &mut s,
            format!(
                "ATENÇÃO: o perfil {} não é o ativo ({}). Se ele saiu de uma busca sobre estas \
                 mesmas dobras (como o busca_v1), os p-valores saem otimistas: o perfil foi \
                 escolhido vendo os sorteios deste trecho.",
                r.perfil, cfg.perfil_ativo
            ),
        );
    }

    linha(&mut s, "");
    linha(
        &mut s,
        "Aviso: o IC normal e o EP das tabelas ignoram a correlação serial da calibração acumulada \
         (tendem a sair estreitos); o veredito de B usa o bootstrap por blocos. O p exato do \
         top-k só vale se o perfil e o k foram fixados sem ver os sorteios deste trecho (aqui, o \
         perfil ativo e k = K, sem busca).",
    );
    linha(
        &mut s,
        "Aviso: esta saída traz vários p-valores (top-k de A e de B, bootstrap de B, mais os de \
         cada dobra) e nenhum é corrigido por múltiplas comparações: com tantos testes, um p ≤ \
         0,05 solto aparece por acaso. Só vale como indício se vier junto de um veredito de \
         SUPERA e se confirmar fora deste trecho; o conjunto trancado não é usado aqui.",
    );
    s
}

fn escrever_veredito_de_b(s: &mut String, r: &RelatorioAvaliacaoRanking) {
    let av = &r.avaliacao;
    linha(
        s,
        format!(
            "Veredito de B no trecho inteiro (dobras de teste, concursos {}, {} avaliados):",
            descrever_faixa(av.faixa.primeiro, av.faixa.ultimo),
            av.concursos
        ),
    );
    match &av.b.bootstrap {
        None => linha(
            s,
            format!(
                "  Sem bootstrap: o trecho tem {} concursos, menos que os {} (dois blocos de {}) \
                 que o bootstrap por blocos exige.",
                av.concursos, av.b.concursos_minimos, BLOCO_BOOTSTRAP
            ),
        ),
        Some(b) => {
            linha(
                s,
                format!(
                    "  Bootstrap circular por blocos de {} concursos, {} reamostragens, semente {}.",
                    b.log_loss.bloco, b.log_loss.reamostragens, b.log_loss.semente
                ),
            );
            linha(
                s,
                format!(
                    "  Log loss, B − acaso: média {:+.3e}, IC 95% percentil {}, p unilateral (B \
                     melhor) = {}.",
                    b.log_loss.media,
                    formatar_intervalo(b.log_loss.ic95, |x| format!("{x:+.3e}")),
                    formatar_p(b.log_loss.p_valor)
                ),
            );
            linha(
                s,
                format!(
                    "  Brier (só informativo): média {:+.3e}, IC 95% percentil {}, p = {}.",
                    b.brier.media,
                    formatar_intervalo(b.brier.ic95, |x| format!("{x:+.3e}")),
                    formatar_p(b.brier.p_valor)
                ),
            );
        }
    }
    let topk = &av.b.topk.total;
    linha(
        s,
        format!(
            "  Top-{} de B (p exato unilateral, trecho inteiro): {} acertos em {} concursos (ao \
             acaso, {:.1} em média), p = {}.",
            av.k,
            topk.total_acertos,
            topk.concursos_avaliados,
            topk.acertos.acaso * topk.concursos_avaliados as f64,
            formatar_cauda(topk.p_valor)
        ),
    );
    let ta = &av.a.topk.total;
    linha(
        s,
        format!(
            "  Top-{} de A (p exato unilateral, trecho inteiro): {} acertos em {} concursos (ao \
             acaso, {:.1} em média), p = {}.",
            av.k,
            ta.total_acertos,
            ta.concursos_avaliados,
            ta.acertos.acaso * ta.concursos_avaliados as f64,
            formatar_cauda(ta.p_valor)
        ),
    );
    match (&av.b.bootstrap, av.b.supera_o_acaso()) {
        (Some(b), Some(true)) => {
            linha(
                s,
                format!(
                    "=> B SUPERA o acaso no log loss: p = {} ≤ α = {ALFA}.",
                    formatar_p(b.log_loss.p_valor)
                ),
            );
            linha(
                s,
                "   Ganho sobre sorteios independentes é suspeito por princípio: confira \
                 vazamento (ponto no tempo) e sobreajuste antes de acreditar.",
            );
        }
        (Some(b), _) => linha(
            s,
            format!(
                "=> B NÃO supera o acaso no log loss: p = {} > α = {ALFA}.",
                formatar_p(b.log_loss.p_valor)
            ),
        ),
        (None, _) => linha(s, "=> SEM VEREDITO: trecho curto demais para o bootstrap."),
    }
}

#[cfg(test)]
mod tests;
