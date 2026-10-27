use super::*;
use crate::avaliacao::{
    avaliar_ranking, calcular_taxas, ranquear, ObservacaoPosicoes, QTD_COMPONENTES,
};
use crate::config;

/// Gerador determinístico (xorshift64), sem dependência nova.
struct Gerador(u64);

impl Gerador {
    fn proximo(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn embaralhar<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.proximo() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Relatório de uma loteria com `concursos` sorteios sintéticos (posições e
/// sorteios aleatórios, o que torna a avaliação um caso de acaso).
fn relatorio(slug: &str, concursos: u32) -> (&'static LoteriaConfig, RelatorioAvaliacaoRanking) {
    let cfg = config::buscar(slug).expect("loteria configurada");
    let mut g = Gerador(0x9E37_79B9_7F4A_7C15 ^ u64::from(concursos));
    let n = cfg.qtd_dezenas_possiveis();
    let mut obs = Vec::new();
    for c in 1..=concursos {
        let mut posicoes: Vec<u32> = (1..=n).collect();
        g.embaralhar(&mut posicoes);
        let mut dezenas: Vec<u32> = cfg.dezenas().collect();
        g.embaralhar(&mut dezenas);
        let sorteadas = &dezenas[..cfg.qtd_dezenas];
        for (i, d) in cfg.dezenas().enumerate() {
            obs.push(ObservacaoPosicoes {
                concurso: c,
                dezena: d,
                posicao: posicoes[i],
                saiu: Some(sorteadas.contains(&d)),
                componentes: [Some(i64::from(c % 3)); QTD_COMPONENTES],
            });
        }
    }
    let taxas = calcular_taxas(cfg, &obs).expect("série válida");
    let ranking = ranquear(cfg, &taxas.linhas).expect("linhas válidas");
    let k = cfg.qtd_dezenas as u32;
    let avaliacao = avaliar_ranking(cfg, &ranking, k).expect("avaliação");
    (
        cfg,
        RelatorioAvaliacaoRanking {
            loteria: cfg.slug.to_string(),
            perfil: "padrao_v1".to_string(),
            versao: 1,
            janela: 50,
            normalizacao: "zscore".to_string(),
            hash_toml: "abc123".to_string(),
            primeiro_concurso: 1,
            ultimo_concurso: concursos,
            corte: cfg.ultimo_concurso_busca,
            avaliacao,
        },
    )
}

#[test]
fn perfil_que_nao_e_o_ativo_leva_aviso_de_p_otimista() {
    let (cfg, mut r) = relatorio("lotofacil", 260);
    let ativo = formatar_avaliacao(cfg, &r);
    assert!(!ativo.contains("não é o ativo"), "{ativo}");
    r.perfil = "busca_v1".to_string();
    let texto = formatar_avaliacao(cfg, &r);
    assert!(
        texto.contains("o perfil busca_v1 não é o ativo (padrao_v1)"),
        "{texto}"
    );
    assert!(texto.contains("otimistas"), "{texto}");
}

#[test]
fn relatorio_traz_o_necessario_com_o_aviso_de_nao_previsao() {
    let (cfg, r) = relatorio("lotofacil", 260);
    let texto = formatar_avaliacao(cfg, &r);
    for trecho in [
        "Avaliação do ranking por taxa histórica da Lotofácil (lotofacil) com o perfil padrao_v1",
        "Hash do perfil: abc123",
        "AVISO: isto é a descrição do passado, não uma previsão",
        "dobras de teste 2 a 5",
        "A dobra 1 não é avaliada",
        "Corte da busca: concurso 3416 (o trancado, do 3417 em diante, não entra nesta avaliação",
        "Top-k: k = 15 (padrão: K da loteria)",
        "== Ordem de A (principal)",
        "idênticos aos do perfil no `sync avaliar` e por isso não se repetem",
        "Top-15 pela ordem de A",
        "Log loss com a calibração linear (1e) sobre a ordem de A",
        "== Ordem de B (EXPERIMENTAL)",
        "Log loss, calibração da 1c sobre a ordem de B",
        "Brier, calibração da 1c sobre a ordem de B",
        "Log loss com a calibração linear (1e) sobre a ordem de B",
        "Top-15 pela ordem de B",
        "Distribuição de acertos do top-15",
        "Veredito de B no trecho inteiro",
        "Bootstrap circular por blocos de 50 concursos, 10000 reamostragens, semente 20261005",
        "múltiplas comparações",
        "concursos",
        "diferença",
        "IC 95% (normal)",
    ] {
        assert!(texto.contains(trecho), "falta `{trecho}` em:\n{texto}");
    }
    // Cada tabela tem as dobras 2 a 5 e o total (6 tabelas: top-k de
    // A e de B, log loss linear de A, log loss, Brier e linear de B).
    assert_eq!(texto.matches("\n     total  ").count(), 6, "{texto}");
    for dobra in 2..=5 {
        assert_eq!(
            texto.matches(&format!("\n{dobra:>10}  ")).count(),
            6,
            "dobra {dobra}:\n{texto}"
        );
    }
    // A dobra 1 não aparece em nenhuma tabela.
    assert_eq!(texto.matches("\n         1  ").count(), 0, "{texto}");
    // Acaso e diferença estão sempre ao lado do valor do ranking.
    assert!(texto.contains("acaso"), "{texto}");
    // Termina com o aviso, sempre.
    assert!(texto.ends_with('\n'));
}

#[test]
fn relatorio_curto_nao_tem_veredito_e_diz_por_que() {
    // 60 concursos: as dobras de teste somam bem menos que 100 concursos.
    let (cfg, r) = relatorio("megasena", 60);
    assert!(r.avaliacao.b.bootstrap.is_none());
    let texto = formatar_avaliacao(cfg, &r);
    assert!(texto.contains("Sem bootstrap: o trecho tem"), "{texto}");
    assert!(texto.contains("=> SEM VEREDITO"), "{texto}");
    assert!(texto.contains("Top-6 pela ordem de A"), "{texto}");
    assert!(!texto.contains("Bootstrap circular por blocos"), "{texto}");
}

#[test]
fn relatorio_e_deterministico() {
    let (cfg, a) = relatorio("quina", 260);
    let (_, b) = relatorio("quina", 260);
    assert_eq!(formatar_avaliacao(cfg, &a), formatar_avaliacao(cfg, &b));
    // Nenhum tempo de execução no texto.
    assert!(!formatar_avaliacao(cfg, &a).contains("Tempo:"));
}

#[test]
fn veredito_diz_nao_supera_ou_supera_conforme_o_p() {
    let (cfg, mut r) = relatorio("lotofacil", 260);
    let boot = r.avaliacao.b.bootstrap.as_mut().expect("bootstrap");
    boot.log_loss.p_valor = 0.5;
    let texto = formatar_avaliacao(cfg, &r);
    assert!(
        texto.contains("=> B NÃO supera o acaso no log loss: p = 0.5000 > α"),
        "{texto}"
    );
    assert!(!texto.contains("SUPERA o acaso"), "{texto}");

    let boot = r.avaliacao.b.bootstrap.as_mut().expect("bootstrap");
    boot.log_loss.p_valor = 0.001_5;
    let texto = formatar_avaliacao(cfg, &r);
    assert!(
        texto.contains("=> B SUPERA o acaso no log loss: p = 0.0015 ≤ α"),
        "{texto}"
    );
    assert!(texto.contains("suspeito por princípio"), "{texto}");
}
