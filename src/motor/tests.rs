use super::*;
use crate::config::{self, Volante};
use crate::error::SpiderError;

fn lotofacil() -> &'static LoteriaConfig {
    config::buscar("lotofacil").expect("lotofacil configurada")
}

fn quina() -> &'static LoteriaConfig {
    config::buscar("quina").expect("quina configurada")
}

fn megasena() -> &'static LoteriaConfig {
    config::buscar("megasena").expect("megasena configurada")
}

/// Loteria pequena (2 de 1..=6) para ter muitas repetições e sequências.
const MINI: LoteriaConfig = LoteriaConfig {
    slug: "mini",
    nome_exibicao: "Mini",
    pagina_url: "",
    seletor_botao_download: "",
    seletor_concurso_data: "",
    seletor_dezenas: "",
    qtd_dezenas: 2,
    dezena_min: 1,
    dezena_max: 6,
    volante: Volante {
        linhas: 1,
        colunas: 6,
    },
    coluna_concurso: 0,
    coluna_data: 1,
    primeira_coluna_dezena: 2,
};

fn res(cfg: &LoteriaConfig, concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: cfg.slug.to_string(),
        concurso,
        data_sorteio: "2020-01-01".to_string(),
        dezenas: dezenas.to_vec(),
    }
}

/// Lotofácil 1..=n em que a dezena 25 sai exatamente nos concursos de
/// `aparicoes_25`; as outras 14 (ou 15) são sempre as menores.
fn lotofacil_com_25_em(n: u32, aparicoes_25: &[u32]) -> Vec<Resultado> {
    let cfg = lotofacil();
    (1..=n)
        .map(|c| {
            let dezenas: Vec<u32> = if aparicoes_25.contains(&c) {
                (1..=14).chain([25]).collect()
            } else {
                (1..=15).collect()
            };
            res(cfg, c, &dezenas)
        })
        .collect()
}

/// Gerador determinístico (xorshift64) para históricos sintéticos, sem
/// dependência nova.
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
}

fn historico_aleatorio(cfg: &LoteriaConfig, n: u32, semente: u64) -> Vec<Resultado> {
    let mut g = Gerador(semente);
    let faixa: Vec<u32> = cfg.dezenas().collect();
    (1..=n)
        .map(|c| {
            let mut pool = faixa.clone();
            let mut dezenas = Vec::with_capacity(cfg.qtd_dezenas);
            for _ in 0..cfg.qtd_dezenas {
                let i = (g.proximo() % pool.len() as u64) as usize;
                dezenas.push(pool.swap_remove(i));
            }
            dezenas.sort_unstable();
            res(cfg, c, &dezenas)
        })
        .collect()
}

fn linha(features: &[FeaturesDezena], concurso: u32, dezena: u32, janela: u32) -> &FeaturesDezena {
    features
        .iter()
        .find(|f| f.concurso == concurso && f.dezena == dezena && f.janela == janela)
        .expect("linha presente")
}

/// Implementação direta das definições do plano, sem estado, para conferir
/// o motor: monta a lista de aparições da janela e calcula cada componente.
fn oraculo(historico: &[Resultado], t: u32, dezena: u32, janela: u32) -> FeaturesDezena {
    let saiu_em = |c: u32| historico[(c - 1) as usize].dezenas.contains(&dezena);
    let inicio = if janela == JANELA_TOTAL {
        1
    } else {
        (i64::from(t) - i64::from(janela)).max(1) as u32
    };
    let aparicoes: Vec<u32> = (inicio..t).filter(|&c| saiu_em(c)).collect();
    let espacos: Vec<u32> = aparicoes.windows(2).map(|p| p[1] - p[0] - 1).collect();
    let maior = espacos.iter().copied().max();
    let menor = espacos.iter().copied().min();
    let contar = |v: Option<u32>| v.map(|v| espacos.iter().filter(|&&e| e == v).count() as u32);
    let soma_variacoes = (aparicoes.len() >= 3)
        .then(|| espacos.windows(2).map(|p| p[1].abs_diff(p[0])).sum::<u32>());
    let ultima_total = (1..t).rev().find(|&c| saiu_em(c));
    let mut repeticao_atual = 0;
    let mut c = t - 1;
    while c >= inicio && c >= 1 && saiu_em(c) {
        repeticao_atual += 1;
        c -= 1;
    }
    FeaturesDezena {
        concurso: t,
        dezena,
        janela,
        frequencia: aparicoes.len() as u32,
        maior_espaco: maior,
        maior_espaco_ocorrencias: contar(maior),
        menor_espaco: menor,
        menor_espaco_ocorrencias: contar(menor),
        soma_variacoes,
        espaco_final: ultima_total.map(|u| t - 1 - u),
        repeticoes_total: espacos.iter().filter(|&&e| e == 0).count() as u32,
        repeticao_atual,
        saiu: (t as usize <= historico.len()).then(|| saiu_em(t)),
    }
}

#[test]
fn janelas_em_ordem_crescente_e_janela_max() {
    assert!(JANELAS.windows(2).all(|p| p[0] < p[1]));
    assert!(JANELAS.contains(&JANELA_TOTAL));
    assert_eq!(JANELA_MAX, 100);
}

#[test]
fn golden_do_plano() {
    // docs/PLANEJAMENTO.md, "Exemplo de referência".
    let historico = lotofacil_com_25_em(20, &[3, 4, 8, 9, 10, 15]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    let esperado = FeaturesDezena {
        concurso: 20,
        dezena: 25,
        janela: JANELA_TOTAL,
        frequencia: 6,
        maior_espaco: Some(4),
        maior_espaco_ocorrencias: Some(1),
        menor_espaco: Some(0),
        menor_espaco_ocorrencias: Some(3),
        soma_variacoes: Some(10),
        espaco_final: Some(4),
        repeticoes_total: 3,
        repeticao_atual: 0,
        saiu: Some(false),
    };
    assert_eq!(linha(&features, 20, 25, JANELA_TOTAL), &esperado);
    // Janelas de 50 e 100 cobrem o histórico inteiro aqui.
    for janela in [50, 100] {
        let f = linha(&features, 20, 25, janela);
        assert_eq!(
            f,
            &FeaturesDezena {
                janela,
                ..esperado.clone()
            }
        );
    }
}

#[test]
fn golden_na_janela_de_10() {
    // t = 20, J = 10 → concursos 10..=19: aparições 10 e 15, espaço [4].
    let historico = lotofacil_com_25_em(20, &[3, 4, 8, 9, 10, 15]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    assert_eq!(
        linha(&features, 20, 25, 10),
        &FeaturesDezena {
            concurso: 20,
            dezena: 25,
            janela: 10,
            frequencia: 2,
            maior_espaco: Some(4),
            maior_espaco_ocorrencias: Some(1),
            menor_espaco: Some(4),
            menor_espaco_ocorrencias: Some(1),
            soma_variacoes: None,
            espaco_final: Some(4),
            repeticoes_total: 0,
            repeticao_atual: 0,
            saiu: Some(false),
        }
    );
}

#[test]
fn golden_nas_fronteiras_das_janelas_de_50_e_100() {
    // t = 151: J = 50 → 101..=150; J = 100 → 51..=150. Aparições exatamente
    // em t−101, t−100, t−51 e t−50: cada fronteira deixa uma dentro e a
    // vizinha fora, então um erro de um concurso muda a frequência.
    let historico = lotofacil_com_25_em(151, &[50, 51, 100, 101]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    let esperado = |janela, frequencia, espacos: Option<(u32, u32, u32, u32)>, soma, repeticoes| {
        FeaturesDezena {
            concurso: 151,
            dezena: 25,
            janela,
            frequencia,
            maior_espaco: espacos.map(|e| e.0),
            maior_espaco_ocorrencias: espacos.map(|e| e.1),
            menor_espaco: espacos.map(|e| e.2),
            menor_espaco_ocorrencias: espacos.map(|e| e.3),
            soma_variacoes: soma,
            espaco_final: Some(150 - 101),
            repeticoes_total: repeticoes,
            repeticao_atual: 0,
            saiu: Some(false),
        }
    };
    // J = 50: só 101 (t−50); 100 (t−51) fica fora.
    assert_eq!(
        linha(&features, 151, 25, 50),
        &esperado(50, 1, None, None, 0)
    );
    // J = 100: 51 (t−100), 100 e 101; 50 (t−101) fica fora. Espaços [48, 0].
    assert_eq!(
        linha(&features, 151, 25, 100),
        &esperado(100, 3, Some((48, 1, 0, 1)), Some(48), 1)
    );
    // Total: 50, 51, 100 e 101. Espaços [0, 48, 0].
    assert_eq!(
        linha(&features, 151, 25, JANELA_TOTAL),
        &esperado(JANELA_TOTAL, 4, Some((48, 1, 0, 2)), Some(96), 2)
    );
    // J = 10: nenhuma aparição, mas `espaco_final` não é limitado pela janela.
    assert_eq!(
        linha(&features, 151, 25, 10),
        &esperado(10, 0, None, None, 0)
    );
}

#[test]
fn primeiro_concurso_nao_tem_historico() {
    let historico = lotofacil_com_25_em(1, &[1]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    assert_eq!(features.len(), 25 * JANELAS.len());
    for f in &features {
        assert_eq!(f.concurso, 1);
        assert_eq!(f.frequencia, 0);
        assert_eq!(f.maior_espaco, None);
        assert_eq!(f.maior_espaco_ocorrencias, None);
        assert_eq!(f.menor_espaco, None);
        assert_eq!(f.menor_espaco_ocorrencias, None);
        assert_eq!(f.soma_variacoes, None);
        assert_eq!(f.espaco_final, None);
        assert_eq!(f.repeticoes_total, 0);
        assert_eq!(f.repeticao_atual, 0);
        assert_eq!(f.saiu, Some(f.dezena <= 14 || f.dezena == 25));
    }
}

#[test]
fn dezena_que_nunca_saiu_tem_espaco_final_nulo() {
    // A dezena 25 nunca sai antes de 30.
    let historico = lotofacil_com_25_em(30, &[30]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    for &janela in &JANELAS {
        let f = linha(&features, 30, 25, janela);
        assert_eq!(f.frequencia, 0);
        assert_eq!(f.espaco_final, None);
        assert_eq!(f.maior_espaco, None);
        assert_eq!(f.soma_variacoes, None);
        assert_eq!(f.saiu, Some(true));
    }
}

#[test]
fn espaco_final_nao_e_limitado_pela_janela() {
    // Única aparição no 2; em t = 40 a janela de 10 está vazia, mas o
    // espaco_final continua contando desde o 2.
    let historico = lotofacil_com_25_em(40, &[2]);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    let f = linha(&features, 40, 25, 10);
    assert_eq!(f.frequencia, 0);
    assert_eq!(f.espaco_final, Some(37));
    let f = linha(&features, 40, 25, JANELA_TOTAL);
    assert_eq!(f.frequencia, 1);
    assert_eq!(f.maior_espaco, None);
    assert_eq!(f.espaco_final, Some(37));
}

#[test]
fn sequencia_maior_que_a_janela_fica_limitada_a_j() {
    // Saiu nos concursos 1..=12; em t = 13 a janela de 10 vê 3..=12.
    let historico = lotofacil_com_25_em(13, &(1..=12).collect::<Vec<_>>());
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    let f = linha(&features, 13, 25, 10);
    assert_eq!(f.repeticao_atual, 10);
    assert_eq!(f.frequencia, 10);
    // Pares de concursos seguidos dentro da janela: (3,4) … (11,12).
    assert_eq!(f.repeticoes_total, 9);
    assert_eq!(f.menor_espaco, Some(0));
    assert_eq!(f.menor_espaco_ocorrencias, Some(9));
    assert_eq!(f.soma_variacoes, Some(0));
    let f = linha(&features, 13, 25, JANELA_TOTAL);
    assert_eq!(f.repeticao_atual, 12);
    assert_eq!(f.repeticoes_total, 11);
}

#[test]
fn janela_maior_que_o_historico_usa_so_os_existentes() {
    let historico = historico_aleatorio(lotofacil(), 30, 7);
    let (_, features) = calcular(lotofacil(), &historico).unwrap();
    for t in 1..=30 {
        for dezena in lotofacil().dezenas() {
            let total = linha(&features, t, dezena, JANELA_TOTAL);
            for janela in [50, 100] {
                assert_eq!(
                    linha(&features, t, dezena, janela),
                    &FeaturesDezena {
                        janela,
                        ..total.clone()
                    }
                );
            }
        }
    }
}

#[test]
fn snapshot_do_proximo_concurso_tem_saiu_nulo() {
    let historico = historico_aleatorio(quina(), 120, 11);
    let (estado, _) = calcular(quina(), &historico[..119]).unwrap();
    let proximo = estado.snapshot();
    assert_eq!(estado.ultimo_concurso(), Some(119));
    assert_eq!(estado.proximo_concurso(), 120);
    assert_eq!(proximo.len(), 80 * JANELAS.len());
    assert!(proximo
        .iter()
        .all(|f| f.concurso == 120 && f.saiu.is_none()));

    // É o mesmo snapshot que sai ao adicionar o 120, tirando o alvo.
    let mut estado = estado;
    let com_alvo = estado.adicionar(quina(), &historico[119]).unwrap();
    let sem_alvo: Vec<_> = com_alvo
        .into_iter()
        .map(|f| FeaturesDezena { saiu: None, ..f })
        .collect();
    assert_eq!(proximo, sem_alvo);
}

#[test]
fn estado_vazio_produz_snapshot_do_concurso_1() {
    let estado = EstadoMotor::novo(quina());
    assert_eq!(estado.ultimo_concurso(), None);
    assert_eq!(estado.proximo_concurso(), 1);
    let s = estado.snapshot();
    assert_eq!(s.len(), 80 * JANELAS.len());
    assert!(s.iter().all(|f| f.concurso == 1 && f.saiu.is_none()));
}

#[test]
fn confere_com_o_oraculo() {
    for (cfg, n, semente) in [
        (&MINI, 260, 1),
        (&MINI, 150, 99),
        (quina(), 260, 3),
        (lotofacil(), 230, 5),
    ] {
        let historico = historico_aleatorio(cfg, n, semente);
        let (estado, features) = calcular(cfg, &historico).unwrap();
        let mut esperado = Vec::new();
        for t in 1..=n {
            for dezena in cfg.dezenas() {
                for &janela in &JANELAS {
                    esperado.push(oraculo(&historico, t, dezena, janela));
                }
            }
        }
        assert_eq!(features, esperado, "{} semente {semente}", cfg.slug);
        // Snapshot do próximo concurso, ainda não sorteado.
        let mut proximo = Vec::new();
        for dezena in cfg.dezenas() {
            for &janela in &JANELAS {
                proximo.push(oraculo(&historico, n + 1, dezena, janela));
            }
        }
        assert_eq!(estado.snapshot(), proximo, "{} semente {semente}", cfg.slug);
    }
}

#[test]
fn saida_ordenada_por_concurso_dezena_janela() {
    let historico = historico_aleatorio(&MINI, 40, 2);
    let (_, features) = calcular(&MINI, &historico).unwrap();
    assert_eq!(features.len(), 40 * 6 * JANELAS.len());
    let chaves: Vec<_> = features
        .iter()
        .map(|f| (f.concurso, f.dezena, f.janela))
        .collect();
    let mut ordenadas = chaves.clone();
    ordenadas.sort_unstable();
    ordenadas.dedup();
    assert_eq!(chaves, ordenadas);
}

#[test]
fn determinismo() {
    let historico = historico_aleatorio(quina(), 200, 13);
    let a = calcular(quina(), &historico).unwrap();
    let b = calcular(quina(), &historico).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        serde_json::to_string(&a.0).unwrap(),
        serde_json::to_string(&b.0).unwrap()
    );
}

#[test]
fn incremental_igual_a_completo() {
    for (cfg, semente) in [(lotofacil(), 17), (quina(), 18), (megasena(), 19)] {
        let n = 250usize;
        let historico = historico_aleatorio(cfg, n as u32, semente);
        let (estado_completo, completo) = calcular(cfg, &historico).unwrap();

        // Extremos: k = 0 (estado vazio, serializado, + tudo) e k = n (tudo +
        // nada); no meio, as fronteiras da maior janela (100/101).
        for k in [0, 1, 99, 100, 101, n - 1, n] {
            let (estado, mut parte) = calcular(cfg, &historico[..k]).unwrap();
            // Retoma de um estado serializado, como a 1a.2 faz com `motor_estado`.
            let json = serde_json::to_string(&estado).unwrap();
            let mut retomado: EstadoMotor = serde_json::from_str(&json).unwrap();
            assert_eq!(retomado, estado);
            let resto = retomado.avancar(cfg, &historico[k..]).unwrap();
            assert_eq!(
                resto.len(),
                (n - k) * cfg.qtd_dezenas_possiveis() as usize * JANELAS.len(),
                "{} corte em {k}",
                cfg.slug
            );
            parte.extend(resto);
            assert_eq!(parte, completo, "{} corte em {k}", cfg.slug);
            assert_eq!(retomado, estado_completo, "{} corte em {k}", cfg.slug);
            assert_eq!(
                retomado.snapshot(),
                estado_completo.snapshot(),
                "{} corte em {k}",
                cfg.slug
            );
        }

        // Concurso a concurso.
        let mut estado = EstadoMotor::novo(cfg);
        let mut um_a_um = Vec::new();
        for r in &historico {
            um_a_um.extend(estado.adicionar(cfg, r).unwrap());
        }
        assert_eq!(um_a_um, completo, "{}", cfg.slug);
        assert_eq!(estado, estado_completo, "{}", cfg.slug);
    }
}

#[test]
fn features_de_t_nao_dependem_de_t_nem_do_que_vem_depois() {
    // Para todo t, as features de t no histórico completo são exatamente o
    // snapshot de um estado que só viu 1 … t−1: os concursos ≥ t poderiam
    // nem existir. Pega também o vazamento que não depende do conteúdo dos
    // concursos futuros, só de quantos são (ex.: `resultados.len()`), que a
    // troca de dezenas de `alterar_o_concurso_t_…` não detecta.
    for (cfg, semente) in [
        (&MINI, 31),
        (lotofacil(), 32),
        (quina(), 33),
        (megasena(), 34),
    ] {
        let n = 130u32;
        let historico = historico_aleatorio(cfg, n, semente);
        let (estado_completo, completo) = calcular(cfg, &historico).unwrap();
        for t in 1..=n + 1 {
            let (passado, _) = calcular(cfg, &historico[..(t - 1) as usize]).unwrap();
            let esperado: Vec<FeaturesDezena> = if t <= n {
                completo
                    .iter()
                    .filter(|f| f.concurso == t)
                    .map(|f| FeaturesDezena {
                        saiu: None,
                        ..f.clone()
                    })
                    .collect()
            } else {
                estado_completo.snapshot()
            };
            assert_eq!(
                esperado.len(),
                cfg.qtd_dezenas_possiveis() as usize * JANELAS.len()
            );
            assert_eq!(passado.snapshot(), esperado, "{} t = {t}", cfg.slug);
        }
    }
}

#[test]
fn alterar_o_concurso_t_e_os_seguintes_nao_muda_as_features_de_t() {
    // Vários t, inclusive o primeiro concurso e as fronteiras da maior
    // janela (100/101). As dezenas de t … n são "giradas" (d → d + 1, com
    // volta), o que sempre muda o conjunto sorteado (a quantidade sorteada é
    // menor que a faixa, então o conjunto girado nunca é o mesmo).
    for (cfg, semente) in [
        (quina(), 21),
        (&MINI, 22),
        (lotofacil(), 23),
        (megasena(), 24),
    ] {
        let n = 150u32;
        let original = historico_aleatorio(cfg, n, semente);
        let (_, a) = calcular(cfg, &original).unwrap();
        for t in [1u32, 2, 50, 100, 101, 120, 150] {
            let mut alterado = original.clone();
            for c in t..=n {
                let r = &mut alterado[(c - 1) as usize];
                for d in &mut r.dezenas {
                    *d = if *d == cfg.dezena_max {
                        cfg.dezena_min
                    } else {
                        *d + 1
                    };
                }
                r.dezenas.sort_unstable();
            }
            let (_, b) = calcular(cfg, &alterado).unwrap();
            let sem_alvo = |v: &[FeaturesDezena]| -> Vec<FeaturesDezena> {
                v.iter()
                    .filter(|f| f.concurso <= t)
                    .map(|f| FeaturesDezena {
                        saiu: None,
                        ..f.clone()
                    })
                    .collect()
            };
            assert_eq!(sem_alvo(&a), sem_alvo(&b), "{} t = {t}", cfg.slug);
            // O alvo de t, esse sim, reflete as dezenas de t.
            let alvo = |v: &[FeaturesDezena]| -> Vec<Option<bool>> {
                v.iter()
                    .filter(|f| f.concurso == t)
                    .map(|f| f.saiu)
                    .collect()
            };
            assert_ne!(alvo(&a), alvo(&b), "{} t = {t}", cfg.slug);
            // Contraprova de que a alteração chega às features: as de t + 1
            // (que já veem t), sem o alvo — que também foi alterado —, mudam.
            // Sem isso, um teste que não alterasse nada passaria igual.
            let features_sem_alvo = |v: &[FeaturesDezena], c: u32| -> Vec<FeaturesDezena> {
                v.iter()
                    .filter(|f| f.concurso == c)
                    .map(|f| FeaturesDezena {
                        saiu: None,
                        ..f.clone()
                    })
                    .collect()
            };
            if t < n {
                assert_ne!(
                    features_sem_alvo(&a, t + 1),
                    features_sem_alvo(&b, t + 1),
                    "{} t = {t}",
                    cfg.slug
                );
            }
        }
    }
}

#[test]
fn buraco_na_numeracao_e_erro() {
    let cfg = &MINI;
    let historico = vec![
        res(cfg, 1, &[1, 2]),
        res(cfg, 2, &[1, 3]),
        res(cfg, 4, &[2, 3]),
    ];
    let erro = calcular(cfg, &historico).unwrap_err();
    assert!(matches!(
        erro,
        SpiderError::MotorBuraco {
            esperado: 3,
            recebido: 4
        }
    ));
}

#[test]
fn historico_que_nao_comeca_no_1_e_erro() {
    let cfg = &MINI;
    let historico = vec![res(cfg, 2, &[1, 2]), res(cfg, 3, &[1, 3])];
    let erro = calcular(cfg, &historico).unwrap_err();
    assert!(matches!(
        erro,
        SpiderError::MotorBuraco {
            esperado: 1,
            recebido: 2
        }
    ));
}

#[test]
fn concurso_repetido_e_erro() {
    let cfg = &MINI;
    let historico = vec![
        res(cfg, 1, &[1, 2]),
        res(cfg, 2, &[1, 3]),
        res(cfg, 2, &[1, 3]),
    ];
    let erro = calcular(cfg, &historico).unwrap_err();
    assert!(matches!(erro, SpiderError::MotorConcursoRepetido(2)));
}

#[test]
fn concurso_fora_de_ordem_e_erro() {
    let cfg = &MINI;
    let historico = vec![
        res(cfg, 1, &[1, 2]),
        res(cfg, 2, &[1, 3]),
        res(cfg, 3, &[1, 3]),
        res(cfg, 2, &[1, 3]),
    ];
    let erro = calcular(cfg, &historico).unwrap_err();
    assert!(matches!(
        erro,
        SpiderError::MotorForaDeOrdem {
            ultimo: 3,
            recebido: 2
        }
    ));
}

#[test]
fn resultado_invalido_e_erro() {
    let cfg = &MINI;
    for dezenas in [&[1, 7][..], &[2, 2], &[1, 2, 3]] {
        let historico = vec![res(cfg, 1, dezenas)];
        let erro = calcular(cfg, &historico).unwrap_err();
        assert!(
            matches!(
                erro,
                SpiderError::MotorResultadoInvalido { concurso: 1, .. }
            ),
            "{dezenas:?}: {erro}"
        );
    }
    let mut outra = res(cfg, 1, &[1, 2]);
    outra.loteria = "quina".to_string();
    assert!(matches!(
        calcular(cfg, &[outra]).unwrap_err(),
        SpiderError::MotorResultadoInvalido { .. }
    ));
}

#[test]
fn erro_no_meio_do_lote_nao_altera_o_estado() {
    let cfg = &MINI;
    let (mut estado, _) = calcular(cfg, &[res(cfg, 1, &[1, 2])]).unwrap();
    let antes = estado.clone();
    let lote = vec![
        res(cfg, 2, &[1, 3]),
        res(cfg, 3, &[1, 3]),
        res(cfg, 5, &[1, 3]),
    ];
    assert!(estado.avancar(cfg, &lote).is_err());
    assert_eq!(estado, antes);
    assert!(estado.adicionar(cfg, &res(cfg, 3, &[1, 3])).is_err());
    assert_eq!(estado, antes);
}

#[test]
fn estado_de_outra_loteria_ou_versao_e_recusado() {
    let estado = EstadoMotor::novo(quina());
    assert!(matches!(
        estado.verificar_compatibilidade(lotofacil()),
        Err(SpiderError::MotorEstadoIncompativel(_))
    ));
    let mut estado = estado;
    assert!(matches!(
        estado.adicionar(
            lotofacil(),
            &res(lotofacil(), 1, &(1..=15).collect::<Vec<_>>())
        ),
        Err(SpiderError::MotorEstadoIncompativel(_))
    ));

    let mut json: serde_json::Value = serde_json::to_value(&estado).unwrap();
    json["feature_version"] = serde_json::json!(FEATURE_VERSION + 1);
    let antigo: EstadoMotor = serde_json::from_value(json).unwrap();
    assert_eq!(antigo.feature_version(), FEATURE_VERSION + 1);
    assert!(matches!(
        antigo.verificar_compatibilidade(quina()),
        Err(SpiderError::MotorEstadoIncompativel(_))
    ));
}

#[test]
fn estado_guarda_so_as_aparicoes_da_maior_janela() {
    // A fila de aparições recentes não cresce com o histórico.
    let cfg = &MINI;
    let historico: Vec<_> = (1..=500).map(|c| res(cfg, c, &[1, 2])).collect();
    let (estado, _) = calcular(cfg, &historico).unwrap();
    let json = serde_json::to_value(&estado).unwrap();
    let recentes = json["dezenas"][0]["recentes"].as_array().unwrap();
    assert_eq!(recentes.len(), JANELA_MAX as usize);
    assert_eq!(recentes.first().unwrap(), 401);
    assert_eq!(recentes.last().unwrap(), 500);
}

#[test]
fn estado_adulterado_e_recusado() {
    let cfg = &MINI;
    let historico = historico_aleatorio(cfg, 30, 4);
    let (estado, _) = calcular(cfg, &historico).unwrap();
    assert!(estado.verificar_compatibilidade(cfg).is_ok());
    let base = serde_json::to_value(&estado).unwrap();

    // Aparição depois do último concurso incorporado (30), com fila, última
    // aparição e frequência coerentes entre si: só a guarda de "nenhuma
    // aparição depois do último concurso" pode recusar.
    let mut json = base.clone();
    let ultima = json["dezenas"][0]["total"]["ultima"].as_u64().unwrap();
    let frequencia = json["dezenas"][0]["total"]["frequencia"].as_u64().unwrap();
    json["dezenas"][0]["recentes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(31));
    json["dezenas"][0]["total"]["ultima"] = serde_json::json!(31);
    json["dezenas"][0]["total"]["frequencia"] = serde_json::json!(frequencia + 1);
    let adulterado: EstadoMotor = serde_json::from_value(json).unwrap();
    assert!(matches!(
        adulterado.verificar_compatibilidade(cfg),
        Err(SpiderError::MotorEstadoIncompativel(_))
    ));

    // Fila que não termina na última aparição do acumulado.
    let mut json = base;
    json["dezenas"][0]["total"]["ultima"] = serde_json::json!(ultima - 1);
    let adulterado: EstadoMotor = serde_json::from_value(json).unwrap();
    assert!(adulterado.verificar_compatibilidade(cfg).is_err());
}
