//! Conferências feitas à mão dos casos de conformidade: os números abaixo
//! saem de contas no papel (descritas em cada teste), não do motor. O
//! esperado versionado é conferido contra o motor em `tests/conformidade.rs`.

use super::*;

fn calculado(arquivo: &str) -> Calculado {
    let caso = casos()
        .into_iter()
        .find(|c| c.arquivo == arquivo)
        .unwrap_or_else(|| panic!("caso {arquivo} não existe"));
    calcular(&caso).expect("o caso calcula")
}

fn feat(c: &Calculado, concurso: u32, dezena: u32, janela: u32) -> FeaturesDezena {
    c.features
        .iter()
        .find(|f| (f.concurso, f.dezena, f.janela) == (concurso, dezena, janela))
        .unwrap_or_else(|| panic!("sem features de ({concurso}, {dezena}, {janela})"))
        .clone()
}

/// (totais, posições, variações) das dezenas de um concurso, em ordem de dezena.
fn indice(c: &Calculado, concurso: u32) -> (Vec<f64>, Vec<u32>, Vec<Option<i64>>) {
    let linhas: Vec<&IndiceDezena> = c.indice.iter().filter(|i| i.concurso == concurso).collect();
    (
        linhas.iter().map(|i| i.total).collect(),
        linhas.iter().map(|i| i.posicao).collect(),
        linhas.iter().map(|i| i.variacao_posicao).collect(),
    )
}

/// Componentes de uma linha na ordem de `NOMES_PESOS`.
fn componentes(f: &FeaturesDezena) -> [Option<u32>; 9] {
    [
        Some(f.frequencia),
        f.maior_espaco,
        f.maior_espaco_ocorrencias,
        f.menor_espaco,
        f.menor_espaco_ocorrencias,
        f.soma_variacoes,
        f.espaco_final,
        Some(f.repeticoes_total),
        Some(f.repeticao_atual),
    ]
}

const N: Option<u32> = None;

#[test]
fn nomes_dos_pesos_acompanham_o_motor_e_as_colunas() {
    let nomes: Vec<&str> = pesos([0.0; 9]).componentes().iter().map(|c| c.0).collect();
    assert_eq!(nomes, NOMES_PESOS);
    assert_eq!(COLUNAS_FEATURES[3..12], NOMES_PESOS);
}

#[test]
fn golden_do_plano() {
    let c = calculado("01_golden_plano.json");
    // Aparições 3, 4, 8, 9, 10, 15; t = 20. Espaços 0, 3, 0, 0, 4:
    // maior 4 (1x), menor 0 (3x), variações 3+3+0+4 = 10, final 19−15 = 4,
    // repetições 3 (pares 3-4, 8-9, 9-10).
    let total = componentes(&feat(&c, 20, 7, 0));
    assert_eq!(
        total,
        [s(6), s(4), s(1), s(0), s(3), s(10), s(4), s(3), s(0)]
    );
    // Janela 50 em t = 20 cobre o histórico inteiro.
    assert_eq!(componentes(&feat(&c, 20, 7, 50)), total);
    // Janela 10 (concursos 10 … 19): aparições 10 e 15, um espaço de 4.
    assert_eq!(
        componentes(&feat(&c, 20, 7, 10)),
        [s(2), s(4), s(1), s(4), s(1), N, s(4), s(0), s(0)]
    );
    // O alvo: saiu em 3, e o snapshot de 20 ainda não foi sorteado.
    assert_eq!(feat(&c, 3, 7, 0).saiu, Some(true));
    assert_eq!(feat(&c, 5, 7, 0).saiu, Some(false));
    assert_eq!(feat(&c, 20, 7, 0).saiu, None);
}

#[test]
fn bordas_de_null() {
    let c = calculado("02_bordas_null.json");
    // t = 1: sem história.
    for d in 1..=5 {
        for j in [0, 10, 50, 100] {
            assert_eq!(
                componentes(&feat(&c, 1, d, j)),
                [s(0), N, N, N, N, N, N, s(0), s(0)],
                "dezena {d} janela {j}"
            );
        }
    }
    // A dezena 5 nunca sai.
    for t in 1..=8 {
        assert_eq!(
            componentes(&feat(&c, t, 5, 0)),
            [s(0), N, N, N, N, N, N, s(0), s(0)]
        );
    }
    // Dezena 3, t = 3: uma aparição (concurso 2) → sem espaços.
    assert_eq!(
        componentes(&feat(&c, 3, 3, 0)),
        [s(1), N, N, N, N, N, s(0), s(0), s(1)]
    );
    // Dezena 3, t = 7: aparições 2 e 6 → um espaço de 3, sem variação.
    assert_eq!(
        componentes(&feat(&c, 7, 3, 0)),
        [s(2), s(3), s(1), s(3), s(1), N, s(0), s(0), s(1)]
    );
    // Dezena 1, t = 8: aparições 1, 2, 4, 5, 7 → espaços 0, 1, 0, 1:
    // maior 1 (2x), menor 0 (2x), variações 1+1+1 = 3, final 0, repetições
    // 2 (pares 1-2 e 4-5), atual 1 (o 6 não saiu).
    assert_eq!(
        componentes(&feat(&c, 8, 1, 0)),
        [s(5), s(1), s(2), s(0), s(2), s(3), s(0), s(2), s(1)]
    );
}

#[test]
fn fronteiras_das_janelas() {
    let c = calculado("03_janelas_fronteiras.json");
    // Dezena 1 sai em 1, 40 e 41. Janela 10: t = 50 → 40 … 49 (as duas
    // aparições), t = 51 → 41 … 50 (só a 41), t = 52 → 42 … 51 (nenhuma).
    assert_eq!(
        componentes(&feat(&c, 50, 1, 10)),
        [s(2), s(0), s(1), s(0), s(1), N, s(8), s(1), s(0)]
    );
    assert_eq!(
        componentes(&feat(&c, 51, 1, 10)),
        [s(1), N, N, N, N, N, s(9), s(0), s(0)]
    );
    // Sem aparição na janela, o espaço final continua contando do histórico.
    assert_eq!(
        componentes(&feat(&c, 52, 1, 10)),
        [s(0), N, N, N, N, N, s(10), s(0), s(0)]
    );
    // Janela 50: t = 90 → 40 … 89; t = 91 → 41 … 90; t = 92 → 42 … 91.
    assert_eq!(feat(&c, 90, 1, 50).frequencia, 2);
    assert_eq!(feat(&c, 91, 1, 50).frequencia, 1);
    assert_eq!(feat(&c, 92, 1, 50).frequencia, 0);
    assert_eq!(feat(&c, 92, 1, 50).espaco_final, Some(50));
    // Janela 100: t = 140 → 40 … 139; t = 141 → 41 … 140.
    assert_eq!(feat(&c, 140, 1, 100).frequencia, 2);
    assert_eq!(feat(&c, 141, 1, 100).frequencia, 1);
    assert_eq!(feat(&c, 141, 1, 100).espaco_final, Some(99));
    // Janela total em t = 141: aparições 1, 40, 41 → espaços 38 e 0.
    assert_eq!(
        componentes(&feat(&c, 141, 1, 0)),
        [s(3), s(38), s(1), s(0), s(1), s(38), s(99), s(1), s(0)]
    );
    // Dezena 2 sai em 50, 90 e 140, sempre em t − 1 das fronteiras.
    assert_eq!(
        componentes(&feat(&c, 51, 2, 10)),
        [s(1), N, N, N, N, N, s(0), s(0), s(1)]
    );
    assert_eq!(
        componentes(&feat(&c, 91, 2, 50)),
        [s(2), s(39), s(1), s(39), s(1), N, s(0), s(0), s(1)]
    );
    // Espaços 39 e 49: maior 49, menor 39, variação 10.
    assert_eq!(
        componentes(&feat(&c, 141, 2, 100)),
        [s(3), s(49), s(1), s(39), s(1), s(10), s(0), s(0), s(1)]
    );
}

#[test]
fn componentes_constantes_em_zscore() {
    let c = calculado("04_componente_constante.json");
    // Concurso 1: freq [2,2,4,4] → z [−1,−1,1,1]; menor_espaco só {0, 2} →
    // [−1, 0, 1, 0]; espaco_final [0,4,0,4] → [−1,1,−1,1]; o resto é
    // constante, de valor único ou NULL → 0. Pesos 1, 0,5 e −0,25.
    let (totais, pos, var) = indice(&c, 1);
    assert_eq!(totais, [-1.25, -1.25, 1.75, 0.75]);
    assert_eq!(pos, [3, 4, 1, 2]); // empate de −1,25: a dezena 1 na frente
    assert_eq!(var, [None; 4]);
    // Concurso 2: só a frequência varia, [2,4,4,2] → [−1,1,1,−1].
    let (totais, pos, var) = indice(&c, 2);
    assert_eq!(totais, [-1.0, 1.0, 1.0, -1.0]);
    assert_eq!(pos, [3, 1, 2, 4]);
    assert_eq!(var, [Some(0), Some(3), Some(-1), Some(-2)]);
    // Concurso 3: tudo constante → totais 0, posição pela dezena.
    let (totais, pos, var) = indice(&c, 3);
    assert_eq!(totais, [0.0; 4]);
    assert_eq!(pos, [1, 2, 3, 4]);
    assert_eq!(var, [Some(2), Some(-1), Some(-1), Some(0)]);
}

#[test]
fn todo_null_em_minmax() {
    let c = calculado("05_todo_null.json");
    // Σ pesos = 28,25; neutro 0,5 → 14,125 (exato em binário).
    for t in [1, 2] {
        let (totais, pos, var) = indice(&c, t);
        assert_eq!(totais, [14.125; 3]);
        assert_eq!(pos, [1, 2, 3]);
        assert_eq!(
            var,
            if t == 1 {
                vec![None; 3]
            } else {
                vec![Some(0); 3]
            }
        );
    }
}

#[test]
fn empate_exato_com_peso_negativo() {
    let c = calculado("06_empate_exato.json");
    // Minmax, total = freq_norm − espaco_final_norm.
    let (totais, pos, var) = indice(&c, 1);
    assert_eq!(totais, [1.0, 0.0, 0.0, 1.0, -1.0]);
    assert_eq!(pos, [1, 3, 4, 2, 5]);
    assert_eq!(var, [None; 5]);
    let (totais, pos, var) = indice(&c, 2);
    assert_eq!(totais, [-1.0, 1.0, 1.0, -1.0, 1.0]);
    assert_eq!(pos, [4, 1, 2, 5, 3]);
    assert_eq!(var, [Some(-3), Some(2), Some(2), Some(-3), Some(2)]);
    // Constantes: 0,5 − 0,5 = 0 para todas.
    let (totais, pos, var) = indice(&c, 3);
    assert_eq!(totais, [0.0; 5]);
    assert_eq!(pos, [1, 2, 3, 4, 5]);
    assert_eq!(var, [Some(3), Some(-1), Some(-1), Some(1), Some(-2)]);
}

#[test]
fn quase_empate_a_1e9() {
    let c = calculado("07_quase_empate_1e9.json");
    // 0,3 + 4e-10 → 0,3 (empata com a dezena 1); 0,3 + 6e-10 → 0,300000001;
    // 0,3 − 5,1e-10 → 0,299999999; −4e-10 → 0 (sem sinal) e empata com a 6.
    let (totais, pos, var) = indice(&c, 1);
    assert_eq!(totais, [0.3, 0.3, 0.300_000_001, 0.299_999_999, 0.0, 0.0]);
    assert!(totais[4].is_sign_positive(), "zero sem sinal");
    assert_eq!(pos, [2, 3, 1, 4, 5, 6]);
    assert_eq!(var, [None; 6]);
    let (totais, pos, var) = indice(&c, 2);
    assert_eq!(totais, [0.3, 0.3, 0.299_999_999, 0.300_000_001, 0.0, 0.0]);
    assert_eq!(pos, [2, 3, 4, 1, 5, 6]);
    assert_eq!(var, [Some(0), Some(0), Some(-3), Some(3), Some(0), Some(0)]);
}

#[test]
fn variacao_no_primeiro_concurso() {
    let c = calculado("10_variacao_primeiro_concurso.json");
    let (totais, pos, var) = indice(&c, 1);
    assert_eq!(totais, [0.0; 4]);
    assert_eq!(pos, [1, 2, 3, 4]);
    assert_eq!(var, [None; 4]);
    // t = 2: freq [0,0,1,1] → [−1,−1,1,1]; espaço final só {0, 0} → 0;
    // repetição atual [0,0,1,1] → [−1,−1,1,1]; pesos 1, −1, 0,5.
    let (totais, pos, var) = indice(&c, 2);
    assert_eq!(totais, [-1.5, -1.5, 1.5, 1.5]);
    assert_eq!(pos, [3, 4, 1, 2]);
    assert_eq!(var, [Some(-2), Some(-2), Some(2), Some(2)]);
}

#[test]
fn posicoes_sao_permutacoes_e_variacoes_fecham() {
    for caso in casos() {
        let c = calcular(&caso).expect("calcula");
        let n = caso.loteria.dezena_max - caso.loteria.dezena_min + 1;
        let primeiro = c.indice.iter().map(|i| i.concurso).min().expect("índice");
        let ultimo = c.indice.iter().map(|i| i.concurso).max().expect("índice");
        for t in primeiro..=ultimo {
            let (_, pos, var) = indice(&c, t);
            assert_eq!(pos.len() as u32, n, "{}", caso.arquivo);
            let mut ordenadas = pos.clone();
            ordenadas.sort_unstable();
            assert_eq!(ordenadas, (1..=n).collect::<Vec<_>>(), "{}", caso.arquivo);
            if t == primeiro {
                assert!(var.iter().all(Option::is_none), "{}", caso.arquivo);
            } else {
                let (_, ant, _) = indice(&c, t - 1);
                for i in 0..pos.len() {
                    assert_eq!(
                        var[i],
                        Some(i64::from(ant[i]) - i64::from(pos[i])),
                        "{} t={t}",
                        caso.arquivo
                    );
                }
            }
        }
    }
}

/// Ponto no tempo: cortar os sorteios depois de `m` não muda as features de
/// nenhum concurso ≤ m + 1 (o snapshot de m + 1 usa só 1 … m).
#[test]
fn features_de_t_nao_dependem_de_concursos_posteriores() {
    for caso in casos() {
        let Entrada::Resultados(sorteios) = &caso.entrada else {
            continue;
        };
        let completo = calcular(&caso).expect("calcula");
        for m in [1, sorteios.len() / 2, sorteios.len() - 1] {
            let cortado = Caso {
                entrada: Entrada::Resultados(sorteios[..m].to_vec()),
                ..caso.clone()
            };
            let parcial = calcular(&cortado).expect("calcula");
            let sem_alvo = |v: &[FeaturesDezena]| -> Vec<FeaturesDezena> {
                v.iter()
                    .filter(|f| f.concurso as usize <= m + 1)
                    .map(|f| FeaturesDezena {
                        saiu: None,
                        ..f.clone()
                    })
                    .collect()
            };
            assert_eq!(
                sem_alvo(&completo.features),
                sem_alvo(&parcial.features),
                "{} cortado em {m}",
                caso.arquivo
            );
        }
    }
}

#[test]
fn flutuantes_com_17_digitos_voltam_exatos() {
    assert_eq!(flutuante(0.3), "2.9999999999999999e-1");
    assert_eq!(flutuante(14.125), "1.4125000000000000e1");
    assert_eq!(flutuante(-0.25), "-2.5000000000000000e-1");
    assert_eq!(flutuante(-0.0), "-0.0000000000000000e0");
    for x in [
        0.3,
        0.3 + 4e-10,
        0.299_999_999,
        1.0 / 3.0,
        -2.0 / 3.0,
        1e-9,
        123_456.789_012_345_67,
        f64::MIN_POSITIVE,
    ] {
        let volta: f64 = flutuante(x).parse().expect("número");
        assert_eq!(volta.to_bits(), x.to_bits(), "{x}");
    }
}

#[test]
fn arquivos_sao_unicos_e_conferidos_existem() {
    let mut nomes: Vec<&str> = casos().iter().map(|c| c.arquivo).collect();
    let antes = nomes.len();
    nomes.sort_unstable();
    nomes.dedup();
    assert_eq!(nomes.len(), antes, "nome de arquivo repetido");
    for caso in casos() {
        let c = calcular(&caso).expect("calcula");
        for conferido in caso.conferidos.iter().flatten() {
            assert!(
                c.indice.iter().any(|i| i.concurso == *conferido),
                "{}: concurso {conferido} não existe",
                caso.arquivo
            );
        }
    }
}
