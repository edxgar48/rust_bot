use super::*;
use crate::config::{self, Volante};
use crate::error::SpiderError;

/// Loteria fictícia (2 de 1..=4, acaso = 1/2) para conferir à mão.
pub(super) const MINI: LoteriaConfig = LoteriaConfig {
    slug: "mini",
    nome_exibicao: "Mini",
    pagina_url: "",
    seletor_botao_download: "",
    seletor_concurso_data: "",
    seletor_dezenas: "",
    qtd_dezenas: 2,
    dezena_min: 1,
    dezena_max: 4,
    volante: Volante {
        linhas: 2,
        colunas: 2,
    },
    coluna_concurso: 0,
    coluna_data: 1,
    primeira_coluna_dezena: 2,
    // Corte fictício da busca: os testes do walk-forward usam séries maiores.
    ultimo_concurso_busca: 20,
};

pub(super) fn obs(concurso: u32, dezena: u32, posicao: u32, saiu: Option<bool>) -> Observacao {
    Observacao {
        concurso,
        dezena,
        posicao,
        saiu,
    }
}

/// Concurso da MINI a partir de `posicoes[d-1]` e das dezenas que saíram
/// (`None` = ainda não sorteado).
pub(super) fn concurso_mini(
    concurso: u32,
    posicoes: [u32; 4],
    saidas: Option<[u32; 2]>,
) -> Vec<Observacao> {
    (1..=4)
        .map(|d| {
            obs(
                concurso,
                d,
                posicoes[(d - 1) as usize],
                saidas.map(|s| s.contains(&d)),
            )
        })
        .collect()
}

/// Série de referência, conferida à mão nos testes abaixo:
/// - 1: posições 1,2,3,4 (dezenas 1..4); saíram 1 e 2 (posições 1 e 2);
/// - 2: posições 2,1,3,4; saíram 2 e 3 (posições 1 e 3);
/// - 3: posições 1,2,3,4; saíram 1 e 4 (posições 1 e 4);
/// - 4: snapshot ainda não sorteado, posições 4,3,2,1.
pub(super) fn serie_mini() -> Vec<Observacao> {
    [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [2, 1, 3, 4], Some([2, 3])),
        concurso_mini(3, [1, 2, 3, 4], Some([1, 4])),
        concurso_mini(4, [4, 3, 2, 1], None),
    ]
    .concat()
}

fn p(probs: &[ProbabilidadeDezena], concurso: u32, dezena: u32) -> f64 {
    probs
        .iter()
        .find(|x| x.concurso == concurso && x.dezena == dezena)
        .map(|x| x.probabilidade)
        .expect("linha presente")
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

macro_rules! assert_perto {
    ($a:expr, $b:expr) => {{
        let (a, b): (f64, f64) = ($a, $b);
        assert!(perto(a, b), "{} = {a:?}, esperado {b:?}", stringify!($a));
    }};
}

/// Gerador determinístico (xorshift64), sem dependência nova.
pub(super) struct Gerador(pub(super) u64);

impl Gerador {
    pub(super) fn proximo(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub(super) fn embaralhar<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.proximo() % (i as u64 + 1)) as usize;
            v.swap(i, j);
        }
    }
}

/// Série sintética válida: posições e saídas embaralhadas a cada concurso;
/// o último fica sem resultado se `com_snapshot`.
pub(super) fn serie_aleatoria(
    cfg: &LoteriaConfig,
    concursos: u32,
    semente: u64,
    com_snapshot: bool,
) -> Vec<Observacao> {
    let mut g = Gerador(semente);
    let n = cfg.qtd_dezenas_possiveis();
    let mut saida = Vec::new();
    for c in 1..=concursos {
        let mut posicoes: Vec<u32> = (1..=n).collect();
        g.embaralhar(&mut posicoes);
        let mut dezenas: Vec<u32> = cfg.dezenas().collect();
        g.embaralhar(&mut dezenas);
        let sorteadas = &dezenas[..cfg.qtd_dezenas];
        let sorteado = !(com_snapshot && c == concursos);
        for (i, d) in cfg.dezenas().enumerate() {
            saida.push(obs(
                c,
                d,
                posicoes[i],
                sorteado.then(|| sorteadas.contains(&d)),
            ));
        }
    }
    saida
}

// ---------------------------------------------------------------- acaso

#[test]
fn taxa_do_acaso_vem_do_config() {
    let esperado = [
        ("lotofacil", 15.0 / 25.0),
        ("megasena", 6.0 / 60.0),
        ("quina", 5.0 / 80.0),
    ];
    for (slug, taxa) in esperado {
        let cfg = config::buscar(slug).expect("loteria configurada");
        assert_eq!(taxa_do_acaso(cfg), taxa, "{slug}");
    }
    assert_eq!(taxa_do_acaso(&MINI), 0.5);
}

// ----------------------------------------------------------- calibração

#[test]
fn calibracao_confere_com_a_conta_a_mao() {
    let probs = calibrar(&MINI, &serie_mini()).expect("série válida");
    assert_eq!(probs.len(), 16);

    // p_t(r) = (s_r·N + m·K) / ((c + m)·N), com N = 4, K = 2, m = 100.
    // Concurso 1: nada antes → 200/400 = 1/2 (o acaso).
    for d in 1..=4 {
        assert_eq!(p(&probs, 1, d), 0.5, "concurso 1, dezena {d}");
    }
    // Antes do 2: c = 1, saídas por posição (1,1,0,0) → (204, 204, 200, 200)/404.
    // Dezena 1 está na posição 2, dezena 2 na 1, 3 na 3, 4 na 4.
    assert_eq!(p(&probs, 2, 1), 204.0 / 404.0);
    assert_eq!(p(&probs, 2, 2), 204.0 / 404.0);
    assert_eq!(p(&probs, 2, 3), 200.0 / 404.0);
    assert_eq!(p(&probs, 2, 4), 200.0 / 404.0);
    // Antes do 3: c = 2, saídas por posição (2,1,1,0) → (208, 204, 204, 200)/408.
    assert_eq!(p(&probs, 3, 1), 208.0 / 408.0);
    assert_eq!(p(&probs, 3, 2), 204.0 / 408.0);
    assert_eq!(p(&probs, 3, 3), 204.0 / 408.0);
    assert_eq!(p(&probs, 3, 4), 200.0 / 408.0);
    // Antes do 4: c = 3, saídas por posição (3,1,1,1) → (212, 204, 204, 204)/412.
    // Posições do 4: dezena 1 → 4, 2 → 3, 3 → 2, 4 → 1.
    assert_eq!(p(&probs, 4, 1), 204.0 / 412.0);
    assert_eq!(p(&probs, 4, 2), 204.0 / 412.0);
    assert_eq!(p(&probs, 4, 3), 204.0 / 412.0);
    assert_eq!(p(&probs, 4, 4), 212.0 / 412.0);
    // `saiu` copiado da entrada.
    assert!(probs
        .iter()
        .filter(|x| x.concurso == 4)
        .all(|x| x.saiu.is_none()));
    assert_eq!(probs[1].saiu, Some(true));
}

#[test]
fn primeiro_concurso_recebe_o_acaso_em_cada_loteria() {
    for cfg in config::LOTERIAS {
        let serie = serie_aleatoria(cfg, 3, 7, false);
        let probs = calibrar(cfg, &serie).expect("série válida");
        let acaso = taxa_do_acaso(cfg);
        for x in probs.iter().filter(|x| x.concurso == 1) {
            assert_eq!(x.probabilidade, acaso, "{} dezena {}", cfg.slug, x.dezena);
        }
    }
}

#[test]
fn probabilidades_de_cada_concurso_somam_k() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let serie = serie_aleatoria(cfg, 300, 11 + i as u64, true);
        let probs = calibrar(cfg, &serie).expect("série válida");
        for grupo in probs.chunk_by(|a, b| a.concurso == b.concurso) {
            let soma: f64 = grupo.iter().map(|x| x.probabilidade).sum();
            assert!(
                (soma - cfg.qtd_dezenas as f64).abs() < 1e-9,
                "{} concurso {}: soma {soma}",
                cfg.slug,
                grupo[0].concurso
            );
            assert!(grupo
                .iter()
                .all(|x| x.probabilidade > 0.0 && x.probabilidade < 1.0));
        }
    }
}

#[test]
fn snapshot_sem_resultado_nao_entra_na_contagem() {
    // Mesma série com o 4 sorteado e um 5 depois: as probabilidades do 5
    // contam o 4; sem o 4 sorteado, um concurso a mais nem existiria.
    // Aqui: o snapshot (4) não altera nenhuma probabilidade de 1..=3.
    let sem_snapshot: Vec<Observacao> = serie_mini()
        .into_iter()
        .filter(|o| o.concurso < 4)
        .collect();
    let com = calibrar(&MINI, &serie_mini()).expect("série válida");
    let sem = calibrar(&MINI, &sem_snapshot).expect("série válida");
    assert_eq!(&com[..12], &sem[..]);
}

#[test]
fn ponto_no_tempo_alterar_t_e_seguintes_nao_muda_p_t() {
    let cfg = config::buscar("lotofacil").expect("lotofacil configurada");
    let original = serie_aleatoria(cfg, 60, 42, true);
    let base = calibrar(cfg, &original).expect("série válida");
    for t in [1_u32, 2, 30, 59, 60] {
        // Troca posições e saídas de t em diante por outra série válida.
        let outra = serie_aleatoria(cfg, 60, 4242 + u64::from(t), true);
        let alterada: Vec<Observacao> = original
            .iter()
            .filter(|o| o.concurso < t)
            .chain(outra.iter().filter(|o| o.concurso >= t))
            .copied()
            .collect();
        assert_ne!(alterada, original);
        let novo = calibrar(cfg, &alterada).expect("série válida");

        for c in 1..=t {
            // p_t como função da posição: igual até t, inclusive.
            let por_posicao = |probs: &[ProbabilidadeDezena]| {
                let mut v: Vec<(u32, u64)> = probs
                    .iter()
                    .filter(|x| x.concurso == c)
                    .map(|x| (x.posicao, x.probabilidade.to_bits()))
                    .collect();
                v.sort_unstable();
                v
            };
            assert_eq!(
                por_posicao(&base),
                por_posicao(&novo),
                "t = {t}, concurso {c}"
            );
        }
        // Antes de t, linha a linha (as posições também não mudaram).
        let antes = |probs: &[ProbabilidadeDezena]| -> Vec<ProbabilidadeDezena> {
            probs.iter().filter(|x| x.concurso < t).cloned().collect()
        };
        assert_eq!(antes(&base), antes(&novo), "t = {t}");
    }
}

#[test]
fn truncar_a_serie_nao_muda_o_passado() {
    let cfg = config::buscar("quina").expect("quina configurada");
    let serie = serie_aleatoria(cfg, 80, 3, true);
    let completo = calibrar(cfg, &serie).expect("série válida");
    for corte in [1_u32, 40, 79] {
        let parcial: Vec<Observacao> = serie
            .iter()
            .filter(|o| o.concurso <= corte)
            .copied()
            .collect();
        let truncado = calibrar(cfg, &parcial).expect("série válida");
        assert_eq!(&completo[..truncado.len()], &truncado[..], "corte {corte}");
    }
}

#[test]
fn metricas_com_corte_menor_sao_prefixo_das_com_corte_maior() {
    // Pega o que os testes de corte acima não pegam: uma estatística do
    // trecho inteiro até o corte (ex.: normalizar pelo máximo do trecho)
    // mudaria as métricas dos concursos já avaliados ao subir o corte.
    // Cortes 79 e 80: o último sorteado e o próprio snapshot.
    for (slug, semente) in [("quina", 3_u64), ("lotofacil", 11), ("megasena", 29)] {
        let cfg = config::buscar(slug).expect("loteria configurada");
        let serie = serie_aleatoria(cfg, 80, semente, true);
        let maior = metricas_por_concurso(cfg, &serie, u32::MAX).expect("série válida");
        assert_eq!(maior.len(), 78, "{slug}: do 2 ao 79");
        for corte in [2_u32, 3, 40, 79, 80] {
            let menor = metricas_por_concurso(cfg, &serie, corte).expect("série válida");
            assert_eq!(
                &maior[..menor.len()],
                &menor[..],
                "{slug}, corte {corte}: não é prefixo"
            );
            assert_eq!(
                menor.last().map(|m| m.concurso),
                Some(corte.min(79)),
                "{slug}, corte {corte}"
            );
        }
    }
}

#[test]
fn calibracao_e_deterministica_e_nao_depende_da_ordem_da_entrada() {
    let cfg = config::buscar("megasena").expect("megasena configurada");
    let serie = serie_aleatoria(cfg, 50, 99, true);
    let a = calibrar(cfg, &serie).expect("série válida");
    let b = calibrar(cfg, &serie).expect("série válida");
    assert_eq!(a, b);

    let mut embaralhada = serie.clone();
    Gerador(5).embaralhar(&mut embaralhada);
    let c = calibrar(cfg, &embaralhada).expect("série válida");
    assert_eq!(a, c);
    assert!(a
        .windows(2)
        .all(|w| (w[0].concurso, w[0].dezena) < (w[1].concurso, w[1].dezena)));
}

#[test]
fn entrada_vazia_devolve_vazio() {
    assert!(calibrar(&MINI, &[]).expect("vazio é válido").is_empty());
}

// --------------------------------------------------------------- métricas

#[test]
fn log_loss_e_brier_contra_valores_de_referencia() {
    // Referências calculadas à parte (Python 3, math.log).
    assert_perto!(log_loss(0.5, true), std::f64::consts::LN_2);
    assert_perto!(log_loss(0.5, false), std::f64::consts::LN_2);
    assert_perto!(log_loss(0.6, true), 0.5108256237659907);
    assert_perto!(log_loss(0.6, false), 0.916290731874155);
    assert_eq!(brier(0.6, true), (0.4_f64) * (0.4_f64));
    assert_eq!(brier(0.6, false), 0.36);
    assert_eq!(brier(0.0, false), 0.0);
    assert_eq!(brier(1.0, false), 1.0);
}

#[test]
fn log_loss_limita_p() {
    let teto = -LIMITE_PROBABILIDADE.ln();
    assert_perto!(log_loss(0.0, true), teto);
    assert_perto!(log_loss(-1.0, true), teto);
    assert_perto!(
        log_loss(1.0, false),
        -(1.0 - (1.0 - LIMITE_PROBABILIDADE)).ln()
    );
    assert!(log_loss(1.0, false).is_finite());
    assert!(log_loss(1.0, true) >= 0.0 && log_loss(1.0, true) < 1e-14);
    assert!(log_loss(0.0, false).abs() < 1e-14);
}

#[test]
fn acaso_por_concurso_e_a_entropia_de_k_sobre_n() {
    // Com exatamente K saídas, a média do acaso no concurso é a entropia
    // binária H(K/N) no log loss e (K/N)(1 − K/N) no Brier.
    // Referências calculadas à parte (Python 3): −(q ln q + (1−q) ln(1−q)).
    let esperado = [
        ("lotofacil", 0.6730116670092565, 0.24),
        ("megasena", 0.3250829733914482, 0.09),
        ("quina", 0.2337916587064593, 0.05859375),
    ];
    for (slug, entropia, brier_acaso) in esperado {
        let cfg = config::buscar(slug).expect("loteria configurada");
        let serie = serie_aleatoria(cfg, 5, 17, false);
        let metricas = metricas_por_concurso(cfg, &serie, u32::MAX).expect("série válida");
        assert_eq!(metricas.len(), 4, "{slug}");
        for m in &metricas {
            assert_perto!(m.log_loss_acaso, entropia);
            assert_perto!(m.brier_acaso, brier_acaso);
        }
    }
}

/// Médias à mão dos concursos 2 e 3 da `serie_mini` (perfil; acaso = 1/2).
pub(super) fn metricas_mini_a_mao() -> [(f64, f64); 2] {
    // Concurso 2: p = 204/404 (dezenas 1 e 2), 200/404 (3 e 4);
    // saíram 2 e 3. Pares: 1 → 1 − 204/404 = 200/404; 2 → 204/404;
    // 3 → 200/404; 4 → 1 − 200/404 = 204/404.
    let ll2 = -(2.0 * (200.0_f64 / 404.0).ln() + 2.0 * (204.0_f64 / 404.0).ln()) / 4.0;
    let br2 = (2.0 * 204.0_f64.powi(2) + 2.0 * 200.0_f64.powi(2)) / (4.0 * 404.0_f64.powi(2));
    // Concurso 3: p = 208, 204, 204, 200 (/408); saíram 1 e 4.
    // Acertos: 1 → 208/408; 2 → 204/408; 3 → 204/408; 4 → 200/408.
    let ll3 =
        -((208.0_f64 / 408.0).ln() + 2.0 * (204.0_f64 / 408.0).ln() + (200.0_f64 / 408.0).ln())
            / 4.0;
    let br3 = (200.0_f64.powi(2) + 2.0 * 204.0_f64.powi(2) + 208.0_f64.powi(2))
        / (4.0 * 408.0_f64.powi(2));
    [(ll2, br2), (ll3, br3)]
}

#[test]
fn metricas_por_concurso_conferem_com_a_conta_a_mao() {
    let metricas = metricas_por_concurso(&MINI, &serie_mini(), u32::MAX).expect("série válida");
    // 1 fica de fora (primeiro do índice) e 4 também (não sorteado).
    assert_eq!(
        metricas.iter().map(|m| m.concurso).collect::<Vec<_>>(),
        [2, 3]
    );
    let [(ll2, br2), (ll3, br3)] = metricas_mini_a_mao();
    assert_perto!(metricas[0].log_loss_perfil, ll2);
    assert_perto!(metricas[0].brier_perfil, br2);
    assert_perto!(metricas[1].log_loss_perfil, ll3);
    assert_perto!(metricas[1].brier_perfil, br3);
    for m in &metricas {
        assert_perto!(m.log_loss_acaso, std::f64::consts::LN_2);
        assert_eq!(m.brier_acaso, 0.25);
    }
    // Os números da conta, para quem for conferir com calculadora.
    assert_perto!(br2, 81616.0 / 326432.0);
    assert_perto!(br3, 166496.0 / 665856.0);
}

#[test]
fn resumo_confere_com_a_conta_a_mao() {
    let resumo = avaliar(&MINI, &serie_mini(), u32::MAX).expect("série válida");
    assert_eq!(resumo.concursos_avaliados, 2);
    assert_eq!((resumo.primeiro_concurso, resumo.ultimo_concurso), (2, 3));

    let [(ll2, br2), (ll3, br3)] = metricas_mini_a_mao();
    for (comparacao, x2, x3, acaso) in [
        (&resumo.log_loss, ll2, ll3, std::f64::consts::LN_2),
        (&resumo.brier, br2, br3, 0.25),
    ] {
        let (d2, d3) = (x2 - acaso, x3 - acaso);
        assert_perto!(comparacao.perfil, (x2 + x3) / 2.0);
        assert_perto!(comparacao.acaso, acaso);
        assert_perto!(comparacao.diferenca, (d2 + d3) / 2.0);
        // T = 2: desvio amostral = |d2 − d3| / √2, EP = desvio / √2 = |d2 − d3| / 2.
        let ep = comparacao.erro_padrao.expect("T = 2 tem erro padrão");
        assert_perto!(ep, (d2 - d3).abs() / 2.0);
        let ic = comparacao.ic95.expect("T = 2 tem intervalo");
        assert_perto!(ic.inferior, (d2 + d3) / 2.0 - 1.96 * ep);
        assert_perto!(ic.superior, (d2 + d3) / 2.0 + 1.96 * ep);
    }
}

#[test]
fn resumo_com_tres_concursos_a_mao() {
    // d_t = 0,1; 0,2; 0,3 → média 0,2; desvio amostral 0,1; EP = 0,1/√3.
    let serie: Vec<MetricasConcurso> = [(10, 0.6), (11, 0.7), (12, 0.8)]
        .into_iter()
        .map(|(concurso, perfil)| MetricasConcurso {
            concurso,
            log_loss_perfil: perfil,
            log_loss_acaso: 0.5,
            brier_perfil: 0.25,
            brier_acaso: 0.25,
        })
        .collect();
    let resumo = resumir(&serie).expect("série não vazia");
    assert_eq!(resumo.concursos_avaliados, 3);
    assert_eq!((resumo.primeiro_concurso, resumo.ultimo_concurso), (10, 12));
    assert_perto!(resumo.log_loss.perfil, 0.7);
    assert_perto!(resumo.log_loss.acaso, 0.5);
    assert_perto!(resumo.log_loss.diferenca, 0.2);
    let ep = resumo.log_loss.erro_padrao.expect("T = 3");
    assert_perto!(ep, 0.1 / 3.0_f64.sqrt());
    assert_perto!(ep, 0.05773502691896258);
    let ic = resumo.log_loss.ic95.expect("T = 3");
    assert_perto!(ic.inferior, 0.2 - 1.96 * 0.05773502691896258);
    assert_perto!(ic.superior, 0.2 + 1.96 * 0.05773502691896258);
}

#[test]
fn acaso_contra_si_mesmo_da_diferenca_zero() {
    let serie: Vec<MetricasConcurso> = (1..=5)
        .map(|c| MetricasConcurso {
            concurso: c,
            log_loss_perfil: 0.3 + f64::from(c) / 100.0,
            log_loss_acaso: 0.3 + f64::from(c) / 100.0,
            brier_perfil: 0.09,
            brier_acaso: 0.09,
        })
        .collect();
    let resumo = resumir(&serie).expect("série não vazia");
    for c in [&resumo.log_loss, &resumo.brier] {
        assert_eq!(c.perfil, c.acaso);
        assert_eq!(c.diferenca, 0.0);
        assert_eq!(c.erro_padrao, Some(0.0));
        assert_eq!(
            c.ic95,
            Some(Intervalo {
                inferior: 0.0,
                superior: 0.0
            })
        );
    }
}

#[test]
fn um_concurso_avaliado_fica_sem_erro_padrao() {
    // Corte 2: só o concurso 2 é avaliado (o 1 é o primeiro do índice).
    let resumo = avaliar(&MINI, &serie_mini(), 2).expect("série válida");
    assert_eq!(resumo.concursos_avaliados, 1);
    assert_eq!((resumo.primeiro_concurso, resumo.ultimo_concurso), (2, 2));
    let [(ll2, br2), _] = metricas_mini_a_mao();
    assert_perto!(resumo.log_loss.perfil, ll2);
    assert_perto!(resumo.brier.perfil, br2);
    for c in [&resumo.log_loss, &resumo.brier] {
        assert_eq!(c.erro_padrao, None);
        assert_eq!(c.ic95, None);
    }
}

#[test]
fn sem_concursos_para_avaliar_e_erro() {
    for corte in [0_u32, 1] {
        let erro = avaliar(&MINI, &serie_mini(), corte).expect_err("nada para avaliar");
        assert!(
            matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
            "corte {corte}: {erro}"
        );
    }
    // Só o primeiro e o snapshot: nada sorteado depois do primeiro.
    let curta: Vec<Observacao> = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(2, [1, 2, 3, 4], None),
    ]
    .concat();
    assert!(matches!(
        avaliar(&MINI, &curta, u32::MAX),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
    assert!(matches!(
        resumir(&[]),
        Err(SpiderError::AvaliacaoSemConcursos(_))
    ));
}

#[test]
fn nada_acima_do_corte_e_lido() {
    // Bagunçar os concursos acima do corte (até deixá-los inválidos) não
    // muda a avaliação até o corte.
    let cfg = config::buscar("lotofacil").expect("lotofacil configurada");
    let serie = serie_aleatoria(cfg, 40, 8, true);
    let corte = 25;
    let base = avaliar(cfg, &serie, corte).expect("série válida");
    assert_eq!(base.concursos_avaliados, 24);
    assert_eq!((base.primeiro_concurso, base.ultimo_concurso), (2, 25));

    let mut alterada: Vec<Observacao> = serie
        .iter()
        .filter(|o| o.concurso <= corte)
        .copied()
        .collect();
    alterada.extend(
        serie_aleatoria(cfg, 40, 1234, false)
            .into_iter()
            .filter(|o| o.concurso > corte),
    );
    // Concurso acima do corte inválido: todas as dezenas "saíram".
    alterada.extend((1..=25).map(|d| obs(50, d, d, Some(true))));
    assert_eq!(
        avaliar(cfg, &alterada, corte).expect("até o corte é válido"),
        base
    );
}

#[test]
fn avaliacao_comeca_no_segundo_concurso_do_indice_mesmo_sem_comecar_no_1() {
    // Entrada começando no 3: o 3 recebe o acaso e não é avaliado.
    let serie: Vec<Observacao> = [
        concurso_mini(3, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(4, [2, 1, 3, 4], Some([2, 3])),
        concurso_mini(5, [1, 2, 3, 4], Some([1, 4])),
    ]
    .concat();
    let metricas = metricas_por_concurso(&MINI, &serie, u32::MAX).expect("série válida");
    assert_eq!(
        metricas.iter().map(|m| m.concurso).collect::<Vec<_>>(),
        [4, 5]
    );
    let [(ll2, br2), (ll3, br3)] = metricas_mini_a_mao();
    assert_perto!(metricas[0].log_loss_perfil, ll2);
    assert_perto!(metricas[0].brier_perfil, br2);
    assert_perto!(metricas[1].log_loss_perfil, ll3);
    assert_perto!(metricas[1].brier_perfil, br3);
}

#[test]
fn avaliacao_e_deterministica() {
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let serie = serie_aleatoria(cfg, 200, 31 + i as u64, true);
        let a = avaliar(cfg, &serie, 150).expect("série válida");
        let b = avaliar(cfg, &serie, 150).expect("série válida");
        assert_eq!(a, b, "{}", cfg.slug);
        assert_eq!(
            serde_json::to_string(&a).expect("serializa"),
            serde_json::to_string(&b).expect("serializa")
        );
        let mut embaralhada = serie.clone();
        Gerador(77).embaralhar(&mut embaralhada);
        assert_eq!(
            avaliar(cfg, &embaralhada, 150).expect("série válida"),
            a,
            "{}",
            cfg.slug
        );
    }
}

// ------------------------------------------------------------- validação

fn erro_de(serie: &[Observacao]) -> String {
    match calibrar(&MINI, serie) {
        Err(SpiderError::AvaliacaoEntradaInvalida(detalhe)) => detalhe,
        outro => panic!("esperado AvaliacaoEntradaInvalida, veio {outro:?}"),
    }
}

#[test]
fn recusa_buraco_entre_concursos() {
    let serie = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(3, [1, 2, 3, 4], Some([1, 2])),
    ]
    .concat();
    assert!(
        erro_de(&serie).contains("faltam o concurso 2"),
        "{}",
        erro_de(&serie)
    );
}

#[test]
fn recusa_dezena_repetida_faltando_ou_fora_da_faixa() {
    let mut repetida = serie_mini();
    repetida.push(obs(2, 3, 3, Some(false)));
    assert!(erro_de(&repetida).contains("dezena 3 repetida"));

    let faltando: Vec<Observacao> = serie_mini()
        .into_iter()
        .filter(|o| !(o.concurso == 2 && o.dezena == 4))
        .collect();
    assert!(
        erro_de(&faltando).contains("faltam as dezenas 4"),
        "{}",
        erro_de(&faltando)
    );

    let mut fora = serie_mini();
    fora.iter_mut()
        .filter(|o| o.concurso == 3 && o.dezena == 4)
        .for_each(|o| o.dezena = 5);
    assert!(
        erro_de(&fora).contains("dezena 5 fora da faixa"),
        "{}",
        erro_de(&fora)
    );
}

#[test]
fn recusa_posicoes_que_nao_sao_permutacao() {
    let repetida = [concurso_mini(1, [1, 1, 3, 4], Some([1, 2]))].concat();
    assert!(erro_de(&repetida).contains("posição 1 repetida"));
    let zero = [concurso_mini(1, [0, 2, 3, 4], Some([1, 2]))].concat();
    assert!(erro_de(&zero).contains("posição 0 fora de 1..=4"));
    let grande = [concurso_mini(1, [1, 2, 3, 5], Some([1, 2]))].concat();
    assert!(erro_de(&grande).contains("posição 5 fora de 1..=4"));
}

#[test]
fn recusa_quantidade_errada_de_saidas() {
    let mut tres = serie_mini();
    tres.iter_mut()
        .filter(|o| o.concurso == 2 && o.dezena == 4)
        .for_each(|o| o.saiu = Some(true));
    assert!(
        erro_de(&tres).contains("3 dezena(s) com `saiu`"),
        "{}",
        erro_de(&tres)
    );

    let mut uma = serie_mini();
    uma.iter_mut()
        .filter(|o| o.concurso == 2 && o.dezena == 3)
        .for_each(|o| o.saiu = Some(false));
    assert!(
        erro_de(&uma).contains("1 dezena(s) com `saiu`"),
        "{}",
        erro_de(&uma)
    );
}

#[test]
fn recusa_saiu_misto_ou_nulo_no_meio() {
    let mut misto = serie_mini();
    misto
        .iter_mut()
        .filter(|o| o.concurso == 2 && o.dezena == 1)
        .for_each(|o| o.saiu = None);
    assert!(erro_de(&misto).contains("NULL em 1"), "{}", erro_de(&misto));

    let mut meio = serie_mini();
    meio.iter_mut()
        .filter(|o| o.concurso == 2)
        .for_each(|o| o.saiu = None);
    assert!(
        erro_de(&meio).contains("no meio da série"),
        "{}",
        erro_de(&meio)
    );
}

// ------------------------------------------- bordas e referências à mão (2)

#[test]
fn historico_longo_com_posicoes_que_sempre_saem() {
    // MINI: N = 4, K = 2, m = 100. Em 50 concursos as posições 1 e 2 sempre
    // saem (dezena d fica na posição d); o 51 é o snapshot.
    // p_51(pos 1 ou 2) = (50·4 + 100·2) / ((50 + 100)·4) = 400/600
    // p_51(pos 3 ou 4) = (0·4  + 100·2) / ((50 + 100)·4) = 200/600
    let mut serie: Vec<Observacao> = (1..=50)
        .flat_map(|c| concurso_mini(c, [1, 2, 3, 4], Some([1, 2])))
        .collect();
    serie.extend(concurso_mini(51, [1, 2, 3, 4], None));
    let probs = calibrar(&MINI, &serie).expect("série válida");
    assert_eq!(probs.len(), 51 * 4);

    assert_eq!(p(&probs, 51, 1), 400.0 / 600.0);
    assert_eq!(p(&probs, 51, 2), 400.0 / 600.0);
    assert_eq!(p(&probs, 51, 3), 200.0 / 600.0);
    assert_eq!(p(&probs, 51, 4), 200.0 / 600.0);
    // Soma: 2·400/600 + 2·200/600 = 1200/600 = K = 2.
    let soma: f64 = probs
        .iter()
        .filter(|x| x.concurso == 51)
        .map(|x| x.probabilidade)
        .sum();
    assert_perto!(soma, 2.0);
}

#[test]
fn megasena_pequena_confere_por_fracao_inteira() {
    let cfg = config::buscar("megasena").expect("megasena configurada");
    // Concursos 1 e 2 (posição da dezena d = d) e snapshot 3, também com
    // posição = dezena. Saíram: 1: dezenas 1..=6; 2: dezenas 1, 2, 3, 7, 8, 9.
    let concurso = |c: u32, saidas: Option<[u32; 6]>| -> Vec<Observacao> {
        (1..=60)
            .map(|d| obs(c, d, d, saidas.map(|s| s.contains(&d))))
            .collect()
    };
    let serie = [
        concurso(1, Some([1, 2, 3, 4, 5, 6])),
        concurso(2, Some([1, 2, 3, 7, 8, 9])),
        concurso(3, None),
    ]
    .concat();
    let probs = calibrar(cfg, &serie).expect("série válida");
    assert_eq!(probs.len(), 180);

    // N = 60, K = 6, m = 100: p = (s·60 + 600) / ((c + 100)·60).
    // Concurso 1 (c = 0): 600/6000 = 1/10 = K/N.
    assert_eq!(p(&probs, 1, 33), 600.0 / 6000.0);
    assert_eq!(p(&probs, 1, 33), taxa_do_acaso(cfg));
    // Concurso 2 (c = 1): dezenas 1..=6 com s = 1 → 660/6060; resto 600/6060.
    assert_eq!(p(&probs, 2, 1), 660.0 / 6060.0);
    assert_eq!(p(&probs, 2, 6), 660.0 / 6060.0);
    assert_eq!(p(&probs, 2, 7), 600.0 / 6060.0);
    // Snapshot (c = 2), contagens por posição: 1,2,3 → 2; 4,5,6 → 1;
    // 7,8,9 → 1; demais → 0.
    assert_eq!(p(&probs, 3, 1), 720.0 / 6120.0);
    assert_eq!(p(&probs, 3, 3), 720.0 / 6120.0);
    assert_eq!(p(&probs, 3, 4), 660.0 / 6120.0);
    assert_eq!(p(&probs, 3, 9), 660.0 / 6120.0);
    assert_eq!(p(&probs, 3, 10), 600.0 / 6120.0);
    assert_eq!(p(&probs, 3, 60), 600.0 / 6120.0);
    // Soma: (3·720 + 6·660 + 51·600) / 6120 = 36720/6120 = 6 = K.
    let soma: f64 = probs
        .iter()
        .filter(|x| x.concurso == 3)
        .map(|x| x.probabilidade)
        .sum();
    assert_perto!(soma, 6.0);
}

#[test]
fn log_loss_e_simetrico_entre_p_e_um_menos_p() {
    // −ln p (saiu) com p = 1 − q é o mesmo que −ln(1 − q) (não saiu) com q.
    for q in [0.1, 0.25, 0.5, 0.75, 0.9] {
        assert_perto!(log_loss(q, true), log_loss(1.0 - q, false));
        assert_perto!(log_loss(q, false), log_loss(1.0 - q, true));
    }
    // Referências: ln 4 e −ln(3/4) (Python 3, math.log).
    assert_perto!(log_loss(0.25, true), 1.3862943611198906);
    assert_perto!(log_loss(0.25, false), 0.2876820724517809);
    assert_perto!(log_loss(0.75, true), 0.2876820724517809);
    assert_perto!(log_loss(0.75, false), 1.3862943611198906);
}

#[test]
fn log_loss_exatamente_no_limite_nao_e_alterado() {
    // p = 1e-15 já está dentro do intervalo: −ln(1e-15) = 15·ln 10.
    let teto = 15.0 * std::f64::consts::LN_10;
    assert_perto!(log_loss(LIMITE_PROBABILIDADE, true), teto);
    assert_eq!(
        log_loss(LIMITE_PROBABILIDADE, true),
        log_loss(0.0, true),
        "abaixo do limite vira o limite"
    );
    // Não saiu com p = 1e-15: −ln(1 − 1e-15) ≈ 1e-15.
    assert!(log_loss(LIMITE_PROBABILIDADE, false) < 2e-15);
    assert!(log_loss(LIMITE_PROBABILIDADE, false) > 0.0);
}

fn metricas_com(
    concurso: u32,
    log_loss_perfil: f64,
    log_loss_acaso: f64,
    brier_perfil: f64,
    brier_acaso: f64,
) -> MetricasConcurso {
    MetricasConcurso {
        concurso,
        log_loss_perfil,
        log_loss_acaso,
        brier_perfil,
        brier_acaso,
    }
}

#[test]
fn erro_padrao_com_quatro_concursos_a_mao() {
    // Log loss: acaso 0,5 em todos; perfil 0,5 · 0,5 · 0,5 · 0,9 → d_t = 0, 0, 0, 0,4.
    // Média 0,1; soma dos quadrados dos desvios = 3·0,1² + 0,3² = 0,12;
    // variância amostral = 0,12/3 = 0,04 → desvio 0,2; EP = 0,2/√4 = 0,1.
    // Brier: acaso 0,25; perfil 0,25 · 0,25 · 0,25 · 0,29 → d_t = 0, 0, 0, 0,04
    // (escala 1/10 da anterior): média 0,01, desvio 0,02, EP 0,01.
    let serie = [
        metricas_com(1, 0.5, 0.5, 0.25, 0.25),
        metricas_com(2, 0.5, 0.5, 0.25, 0.25),
        metricas_com(3, 0.5, 0.5, 0.25, 0.25),
        metricas_com(4, 0.9, 0.5, 0.29, 0.25),
    ];
    let resumo = resumir(&serie).expect("série não vazia");
    assert_eq!(resumo.concursos_avaliados, 4);

    assert_perto!(resumo.log_loss.perfil, 0.6);
    assert_perto!(resumo.log_loss.acaso, 0.5);
    assert_perto!(resumo.log_loss.diferenca, 0.1);
    let ep = resumo.log_loss.erro_padrao.expect("T = 4");
    assert_perto!(ep, 0.1);
    // IC = 0,1 ± 1,96·0,1.
    let ic = resumo.log_loss.ic95.expect("T = 4");
    assert_perto!(ic.inferior, 0.1 - 0.196);
    assert_perto!(ic.superior, 0.1 + 0.196);

    assert_perto!(resumo.brier.perfil, 0.26);
    assert_perto!(resumo.brier.acaso, 0.25);
    assert_perto!(resumo.brier.diferenca, 0.01);
    let ep = resumo.brier.erro_padrao.expect("T = 4");
    assert_perto!(ep, 0.01);
    let ic = resumo.brier.ic95.expect("T = 4");
    assert_perto!(ic.inferior, 0.01 - 0.0196);
    assert_perto!(ic.superior, 0.01 + 0.0196);
}

#[test]
fn diferenca_negativa_perfil_melhor_que_o_acaso_mantem_o_sinal() {
    // Log loss: acaso 0,6; perfil 0,4 e 0,5 → d_t = −0,2 e −0,1.
    // Média −0,15; desvios ∓0,05 → variância amostral = 0,005/1;
    // EP = √0,005/√2 = 0,05. IC = −0,15 ± 1,96·0,05 = [−0,248, −0,052].
    let serie = [
        metricas_com(7, 0.4, 0.6, 0.20, 0.25),
        metricas_com(8, 0.5, 0.6, 0.22, 0.25),
    ];
    let resumo = resumir(&serie).expect("série não vazia");
    assert_perto!(resumo.log_loss.perfil, 0.45);
    assert_perto!(resumo.log_loss.diferenca, -0.15);
    assert!(resumo.log_loss.diferenca < 0.0);
    let ep = resumo.log_loss.erro_padrao.expect("T = 2");
    assert_perto!(ep, 0.05);
    assert!(ep >= 0.0, "o erro padrão nunca é negativo");
    let ic = resumo.log_loss.ic95.expect("T = 2");
    assert_perto!(ic.inferior, -0.15 - 1.96 * 0.05);
    assert_perto!(ic.superior, -0.15 + 1.96 * 0.05);
    assert!(ic.inferior < ic.superior && ic.superior < 0.0);
    // Brier: d_t = −0,05 e −0,03 → média −0,04.
    assert_perto!(resumo.brier.diferenca, -0.04);
}

#[test]
fn perfil_que_acerta_sempre_as_mesmas_posicoes_vence_o_acaso_na_avaliacao_completa() {
    // MINI, posições 1 e 2 sempre saem: depois do primeiro concurso o
    // perfil dá mais que 1/2 às posições 1 e 2 e menos às outras, então
    // melhora as duas métricas em todo concurso avaliado.
    let mut serie: Vec<Observacao> = (1..=20)
        .flat_map(|c| concurso_mini(c, [1, 2, 3, 4], Some([1, 2])))
        .collect();
    serie.extend(concurso_mini(21, [1, 2, 3, 4], None));
    let resumo = avaliar(&MINI, &serie, u32::MAX).expect("série válida");
    assert_eq!(resumo.concursos_avaliados, 19);
    assert!(resumo.log_loss.diferenca < 0.0, "{:?}", resumo.log_loss);
    assert!(resumo.brier.diferenca < 0.0, "{:?}", resumo.brier);
    assert!(resumo.log_loss.perfil < resumo.log_loss.acaso);
    assert!(resumo.brier.perfil < resumo.brier.acaso);
}

#[test]
fn resumir_serie_fora_de_ordem_usa_minimo_e_maximo() {
    let serie: Vec<MetricasConcurso> = [30, 5, 17, 9]
        .into_iter()
        .map(|c| metricas_com(c, 0.7, 0.5, 0.3, 0.25))
        .collect();
    let resumo = resumir(&serie).expect("série não vazia");
    assert_eq!(resumo.concursos_avaliados, 4);
    assert_eq!((resumo.primeiro_concurso, resumo.ultimo_concurso), (5, 30));
}

#[test]
fn recusa_concurso_inteiro_repetido() {
    // O concurso 2 aparece duas vezes, com as mesmas 4 dezenas: depois da
    // ordenação por (concurso, dezena) a dezena 1 aparece em duplicata.
    let mut serie = serie_mini();
    serie.extend(concurso_mini(2, [2, 1, 3, 4], Some([2, 3])));
    let detalhe = erro_de(&serie);
    assert!(detalhe.contains("dezena 1 repetida"), "{detalhe}");
    assert!(detalhe.contains("concurso 2"), "{detalhe}");
}

#[test]
fn recusa_buraco_de_varios_concursos_e_cita_o_intervalo() {
    let serie = [
        concurso_mini(1, [1, 2, 3, 4], Some([1, 2])),
        concurso_mini(5, [1, 2, 3, 4], Some([1, 2])),
    ]
    .concat();
    // Faltam 2, 3 e 4.
    let detalhe = erro_de(&serie);
    assert!(detalhe.contains("os concursos 2..=4"), "{detalhe}");
    assert!(detalhe.contains("entre o 1 e o 5"), "{detalhe}");
}

#[test]
fn corte_no_ultimo_sorteado_ou_no_snapshot_da_o_mesmo_resultado() {
    // serie_mini: 3 é o último sorteado e 4 o snapshot. Corte 3 descarta o
    // snapshot; corte 4 o mantém, mas ele não é avaliado; sem corte, idem.
    let ate_3 = avaliar(&MINI, &serie_mini(), 3).expect("série válida");
    let ate_4 = avaliar(&MINI, &serie_mini(), 4).expect("série válida");
    let sem_corte = avaliar(&MINI, &serie_mini(), u32::MAX).expect("série válida");
    assert_eq!(ate_3, ate_4);
    assert_eq!(ate_3, sem_corte);
    assert_eq!(ate_3.concursos_avaliados, 2);
    assert_eq!((ate_3.primeiro_concurso, ate_3.ultimo_concurso), (2, 3));

    let m3 = metricas_por_concurso(&MINI, &serie_mini(), 3).expect("série válida");
    let m4 = metricas_por_concurso(&MINI, &serie_mini(), 4).expect("série válida");
    assert_eq!(m3, m4);
    // Conferido contra a conta à mão, não só entre si.
    let [(ll2, _), (ll3, _)] = metricas_mini_a_mao();
    assert_perto!(m3[0].log_loss_perfil, ll2);
    assert_perto!(m3[1].log_loss_perfil, ll3);
}

#[test]
fn entrada_so_com_o_snapshot_calibra_com_o_acaso_e_nao_avalia() {
    // Um único concurso sem resultado: N linhas com p = K/N, `saiu` None.
    let probs = calibrar(&MINI, &concurso_mini(1, [3, 1, 4, 2], None)).expect("série válida");
    assert_eq!(probs.len(), 4);
    for x in &probs {
        assert_eq!(x.probabilidade, 0.5, "dezena {}", x.dezena);
        assert_eq!(x.saiu, None);
        assert_eq!(x.concurso, 1);
    }

    // O mesmo vale para as loterias reais (N linhas, todas com K/N).
    for (i, cfg) in config::LOTERIAS.iter().enumerate() {
        let serie = serie_aleatoria(cfg, 1, 5 + i as u64, true);
        assert!(serie.iter().all(|o| o.saiu.is_none()));
        let probs = calibrar(cfg, &serie).expect("série válida");
        assert_eq!(
            probs.len(),
            cfg.qtd_dezenas_possiveis() as usize,
            "{}",
            cfg.slug
        );
        assert!(
            probs.iter().all(|x| x.probabilidade == taxa_do_acaso(cfg)),
            "{}",
            cfg.slug
        );
    }

    let erro = avaliar(&MINI, &concurso_mini(1, [1, 2, 3, 4], None), u32::MAX)
        .expect_err("nada sorteado para avaliar");
    assert!(
        matches!(erro, SpiderError::AvaliacaoSemConcursos(_)),
        "{erro}"
    );
}
