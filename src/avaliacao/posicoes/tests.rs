use super::*;
use crate::avaliacao::tests::{serie_aleatoria, Gerador, MINI};
use crate::avaliacao::{calibrar, taxa_do_acaso};
use crate::config;
use crate::motor::{FeaturesDezena, Pesos};

fn cfg(slug: &str) -> &'static LoteriaConfig {
    config::buscar(slug).expect("loteria configurada")
}

// ------------------------------------------------------------ montagem

/// Componentes com os três primeiros dados e o resto constante (0).
fn comp3(c0: Option<i64>, c1: Option<i64>, c2: Option<i64>) -> [Option<i64>; QTD_COMPONENTES] {
    let mut c = [Some(0); QTD_COMPONENTES];
    c[0] = c0;
    c[1] = c1;
    c[2] = c2;
    c
}

/// Um concurso da MINI: `posicoes[d-1]`, dezenas que saíram (`None` = ainda
/// não sorteado) e os valores dos componentes 0 e 2 (o 1 é sempre 5).
fn concurso_mini_pos(
    concurso: u32,
    posicoes: [u32; 4],
    saidas: Option<[u32; 2]>,
    c0: [Option<i64>; 4],
    c2: [Option<i64>; 4],
) -> Vec<ObservacaoPosicoes> {
    (1..=4_u32)
        .map(|d| ObservacaoPosicoes {
            concurso,
            dezena: d,
            posicao: posicoes[(d - 1) as usize],
            saiu: saidas.map(|s| s.contains(&d)),
            componentes: comp3(c0[(d - 1) as usize], Some(5), c2[(d - 1) as usize]),
        })
        .collect()
}

fn algum<const L: usize>(v: [i64; L]) -> [Option<i64>; L] {
    v.map(Some)
}

/// Série de referência da MINI (K = 2 de N = 4, m = 100), conferida à mão em
/// `mini_valores_a_mao`:
///
/// | concurso | posições | saíram | componente 0 | componente 2 |
/// |---|---|---|---|---|
/// | 1 | 1,2,3,4 | 1 e 2 | 10,20,30,40 | 1,NULL,3,3 |
/// | 2 | 2,1,3,4 | 2 e 3 | 30,10,20,20 | NULL ×4 |
/// | 3 | 1,2,3,4 | 1 e 4 | 7,7,7,7 | NULL,9,NULL,NULL |
/// | 4 | 4,3,2,1 | — | 1,NULL,2,3 | −4,−4,8,−9 |
///
/// O componente 1 vale 5 em todas as dezenas (constante) e os componentes 3 a
/// 8 valem 0.
fn serie_mini() -> Vec<ObservacaoPosicoes> {
    [
        concurso_mini_pos(
            1,
            [1, 2, 3, 4],
            Some([1, 2]),
            algum([10, 20, 30, 40]),
            [Some(1), None, Some(3), Some(3)],
        ),
        concurso_mini_pos(
            2,
            [2, 1, 3, 4],
            Some([2, 3]),
            algum([30, 10, 20, 20]),
            [None; 4],
        ),
        concurso_mini_pos(
            3,
            [1, 2, 3, 4],
            Some([1, 4]),
            algum([7, 7, 7, 7]),
            [None, Some(9), None, None],
        ),
        concurso_mini_pos(
            4,
            [4, 3, 2, 1],
            None,
            [Some(1), None, Some(2), Some(3)],
            algum([-4, -4, 8, -9]),
        ),
    ]
    .concat()
}

/// Série sintética válida com componentes aleatórios (valores pequenos, para
/// dar empates; alguns `NULL`; alguns concursos com um componente constante).
fn serie_sintetica(
    cfg: &LoteriaConfig,
    concursos: u32,
    semente: u64,
    com_snapshot: bool,
) -> Vec<ObservacaoPosicoes> {
    let mut g = Gerador(semente.wrapping_mul(2).wrapping_add(1));
    serie_aleatoria(cfg, concursos, semente, com_snapshot)
        .into_iter()
        .map(|o| {
            let mut componentes = [None; QTD_COMPONENTES];
            for (c, valor) in componentes.iter_mut().enumerate() {
                let sorteio = g.proximo();
                *valor = if (o.concurso as usize + c).is_multiple_of(11) {
                    Some(3) // componente constante neste concurso
                } else if sorteio.is_multiple_of(7) {
                    None
                } else {
                    Some((sorteio >> 8) as i64 % 5 - 2)
                };
            }
            ObservacaoPosicoes {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                saiu: o.saiu,
                componentes,
            }
        })
        .collect()
}

fn linha_de(linhas: &[LinhaTaxas], concurso: u32, dezena: u32) -> &LinhaTaxas {
    linhas
        .iter()
        .find(|l| l.concurso == concurso && l.dezena == dezena)
        .expect("linha presente")
}

fn do_concurso(obs: &[ObservacaoPosicoes], concurso: u32) -> Vec<ObservacaoPosicoes> {
    obs.iter()
        .filter(|o| o.concurso == concurso)
        .copied()
        .collect()
}

fn perto(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
}

// ------------------------------------------------------- faixas (tercis)

#[test]
fn tercil_de_quatro_valores_distintos_n_nao_divisivel_por_3() {
    // n = 4: menores 0,1,2,3 -> ⌊3·menores/4⌋ = 0,0,1,2.
    assert_eq!(
        faixas_do_componente(&algum([10, 20, 30, 40])),
        [Faixa::Baixo, Faixa::Baixo, Faixa::Medio, Faixa::Alto]
    );
}

#[test]
fn tercil_de_cinco_e_sete_valores() {
    // n = 5: menores 0..=4 -> 0,0,1,1,2 (15/5 = 3 nunca acontece: menores ≤ 4).
    assert_eq!(
        faixas_do_componente(&algum([1, 2, 3, 4, 5])),
        [
            Faixa::Baixo,
            Faixa::Baixo,
            Faixa::Medio,
            Faixa::Medio,
            Faixa::Alto
        ]
    );
    // n = 7: 3·menores/7 para menores 0..=6 -> 0,0,0,1,1,2,2.
    assert_eq!(
        faixas_do_componente(&algum([1, 2, 3, 4, 5, 6, 7])),
        [
            Faixa::Baixo,
            Faixa::Baixo,
            Faixa::Baixo,
            Faixa::Medio,
            Faixa::Medio,
            Faixa::Alto,
            Faixa::Alto
        ]
    );
}

#[test]
fn tercil_de_n_divisivel_por_3() {
    // n = 6: menores 0..=5 -> 0,0,1,1,2,2.
    assert_eq!(
        faixas_do_componente(&algum([9, 8, 7, 6, 5, 4])),
        [
            Faixa::Alto,
            Faixa::Alto,
            Faixa::Medio,
            Faixa::Medio,
            Faixa::Baixo,
            Faixa::Baixo
        ],
        "a ordem dos valores na entrada não importa"
    );
}

#[test]
fn dois_valores_distintos_so_dao_baixo_e_medio() {
    // n = 2: menores 0 e 1 -> 0 e ⌊3/2⌋ = 1. Consequência da fórmula do plano.
    assert_eq!(
        faixas_do_componente(&algum([5, 9])),
        [Faixa::Baixo, Faixa::Medio]
    );
}

#[test]
fn empatados_caem_na_mesma_faixa() {
    // Valores 1,2,2,2,9 (n = 5): menores = 0,1,1,1,4 -> 0,0,0,0,2. Os três
    // empatados ficam juntos mesmo ocupando as ordens 2, 3 e 4.
    assert_eq!(
        faixas_do_componente(&algum([2, 1, 2, 9, 2])),
        [
            Faixa::Baixo,
            Faixa::Baixo,
            Faixa::Baixo,
            Faixa::Alto,
            Faixa::Baixo
        ]
    );
    // Empate no topo: 1,5,5,5 (n = 4): menores 0,1,1,1 -> 0,0,0,0.
    assert_eq!(
        faixas_do_componente(&algum([5, 1, 5, 5])),
        [Faixa::Baixo; 4]
    );
}

#[test]
fn componente_constante_fica_todo_em_medio() {
    assert_eq!(
        faixas_do_componente(&algum([7, 7, 7, 7])),
        [Faixa::Medio; 4]
    );
    // A fórmula sozinha daria "baixo" (menores = 0): o caso é tratado à parte.
    assert_eq!(faixas_do_componente(&algum([0, 0])), [Faixa::Medio; 2]);
    // Uma única dezena com valor: constante.
    assert_eq!(
        faixas_do_componente(&[None, Some(4), None]),
        [Faixa::Nulo, Faixa::Medio, Faixa::Nulo]
    );
    // Constante entre os não NULL, com NULL no meio.
    assert_eq!(
        faixas_do_componente(&[Some(-3), None, Some(-3)]),
        [Faixa::Medio, Faixa::Nulo, Faixa::Medio]
    );
}

#[test]
fn nulo_e_faixa_propria_e_nao_conta_em_n() {
    // 1,NULL,2,3: n = 3 (o NULL não conta): menores 0,_,1,2 -> 0,_,1,2.
    assert_eq!(
        faixas_do_componente(&[Some(1), None, Some(2), Some(3)]),
        [Faixa::Baixo, Faixa::Nulo, Faixa::Medio, Faixa::Alto]
    );
    // Todos NULL: todos na faixa nula.
    assert_eq!(faixas_do_componente(&[None; 5]), [Faixa::Nulo; 5]);
    assert!(faixas_do_componente(&[]).is_empty());
}

#[test]
fn valores_negativos_e_extremos() {
    assert_eq!(
        faixas_do_componente(&[Some(i64::MIN), Some(0), Some(i64::MAX)]),
        [Faixa::Baixo, Faixa::Medio, Faixa::Alto]
    );
    assert_eq!(
        faixas_do_componente(&algum([-4, -4, 8, -9])),
        [Faixa::Baixo, Faixa::Baixo, Faixa::Alto, Faixa::Baixo]
    );
}

#[test]
fn faixa_indice_e_nome() {
    for (i, f) in Faixa::TODAS.iter().enumerate() {
        assert_eq!(f.indice(), i);
    }
    assert_eq!(Faixa::Baixo.nome(), "baixo");
    assert_eq!(Faixa::Medio.nome(), "médio");
    assert_eq!(Faixa::Alto.nome(), "alto");
    assert_eq!(Faixa::Nulo.nome(), "nulo");
}

// --------------------------------------------------- ordem dos componentes

#[test]
fn componentes_e_nomes_seguem_a_ordem_dos_pesos_do_motor() {
    // Cada campo recebe um valor distinto (1..=9) e o peso de mesmo nome o
    // mesmo número: a posição i de `componentes_de` tem de ter o valor do peso
    // i, e o nome tem de ser o mesmo.
    let f = FeaturesDezena {
        concurso: 10,
        dezena: 1,
        janela: 50,
        frequencia: 1,
        maior_espaco: Some(2),
        maior_espaco_ocorrencias: Some(3),
        menor_espaco: Some(4),
        menor_espaco_ocorrencias: Some(5),
        soma_variacoes: Some(6),
        espaco_final: Some(7),
        repeticoes_total: 8,
        repeticao_atual: 9,
        saiu: Some(true),
    };
    let pesos = Pesos {
        frequencia: 1.0,
        maior_espaco: 2.0,
        maior_espaco_ocorrencias: 3.0,
        menor_espaco: 4.0,
        menor_espaco_ocorrencias: 5.0,
        soma_variacoes: 6.0,
        espaco_final: 7.0,
        repeticoes_total: 8.0,
        repeticao_atual: 9.0,
    };
    let valores = ObservacaoPosicoes::componentes_de(&f);
    for (i, (nome, peso)) in pesos.componentes().iter().enumerate() {
        assert_eq!(NOMES_COMPONENTES[i], *nome, "nome do componente {i}");
        assert_eq!(valores[i], Some(*peso as i64), "valor do componente {nome}");
    }
}

#[test]
fn componentes_de_preserva_null() {
    let f = FeaturesDezena {
        concurso: 1,
        dezena: 1,
        janela: 50,
        frequencia: 0,
        maior_espaco: None,
        maior_espaco_ocorrencias: None,
        menor_espaco: None,
        menor_espaco_ocorrencias: None,
        soma_variacoes: None,
        espaco_final: None,
        repeticoes_total: 0,
        repeticao_atual: 0,
        saiu: None,
    };
    assert_eq!(
        ObservacaoPosicoes::componentes_de(&f),
        [
            Some(0),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(0),
            Some(0)
        ]
    );
}

// --------------------------------------------------- valores à mão (MINI)

/// Cálculo à mão (K = 2, N = 4, m = 100, K/N = 1/2); `taxa = (s·4 + 200) /
/// ((total + 100)·4)`, escrita com os números do enunciado.
#[test]
fn mini_valores_a_mao() {
    let serie = calcular_taxas(&MINI, &serie_mini()).expect("série válida");
    let l = &serie.linhas;
    assert_eq!(l.len(), 16);

    // ---- concurso 1: nada no passado, tudo = K/N.
    let acaso = taxa_do_acaso(&MINI);
    assert_eq!(acaso, 0.5);
    for d in 1..=4 {
        let x = linha_de(l, 1, d);
        assert_eq!(x.taxa_posicao, acaso);
        assert_eq!(x.taxas_faixa, [acaso; QTD_COMPONENTES]);
    }
    // Faixas do concurso 1: componente 0 = 10,20,30,40 -> baixo,baixo,médio,alto;
    // componente 1 constante -> médio; componente 2 = 1,NULL,3,3 -> baixo,nulo,
    // médio,médio (n = 3, os dois 3 empatados).
    let faixas = |c: u32, d: u32, i: usize| linha_de(l, c, d).faixas[i];
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(1, d, 0)),
        [Faixa::Baixo, Faixa::Baixo, Faixa::Medio, Faixa::Alto]
    );
    assert_eq!([1, 2, 3, 4].map(|d| faixas(1, d, 1)), [Faixa::Medio; 4]);
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(1, d, 2)),
        [Faixa::Baixo, Faixa::Nulo, Faixa::Medio, Faixa::Medio]
    );

    // ---- concurso 2. Posições antes: saídas {1:1, 2:1, 3:0, 4:0}, c = 1.
    // Dezenas 1..4 estão nas posições 2,1,3,4.
    assert_eq!(linha_de(l, 2, 1).taxa_posicao, 204.0 / 404.0);
    assert_eq!(linha_de(l, 2, 2).taxa_posicao, 204.0 / 404.0);
    assert_eq!(linha_de(l, 2, 3).taxa_posicao, 200.0 / 404.0);
    assert_eq!(linha_de(l, 2, 4).taxa_posicao, 200.0 / 404.0);
    // Componente 0 do concurso 1: baixo 2 saídas/2 pares, médio 0/1, alto 0/1.
    // Concurso 2 (30,10,20,20): alto, baixo, baixo, baixo (os dois 20 empatados).
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(2, d, 0)),
        [Faixa::Alto, Faixa::Baixo, Faixa::Baixo, Faixa::Baixo]
    );
    assert_eq!(linha_de(l, 2, 1).taxas_faixa[0], 200.0 / 404.0); // alto 0/1
    for d in 2..=4 {
        assert_eq!(linha_de(l, 2, d).taxas_faixa[0], 208.0 / 408.0); // baixo 2/2
    }
    // Componente 1 (constante, médio): 2 saídas em 4 pares.
    for d in 1..=4 {
        assert_eq!(linha_de(l, 2, d).taxas_faixa[1], 208.0 / 416.0);
    }
    // Componente 2: concurso 2 todo NULL -> faixa nula, que no concurso 1 teve a
    // dezena 2 (saiu): 1 saída em 1 par.
    for d in 1..=4 {
        assert_eq!(faixas(2, d, 2), Faixa::Nulo);
        assert_eq!(linha_de(l, 2, d).taxas_faixa[2], 204.0 / 404.0);
    }

    // ---- concurso 3. Posições: pos1 saiu em 1 e 2 (2), pos2 em 1 (1),
    // pos3 em 2 (1), pos4 em 0; c = 2.
    assert_eq!(linha_de(l, 3, 1).taxa_posicao, 208.0 / 408.0);
    assert_eq!(linha_de(l, 3, 2).taxa_posicao, 204.0 / 408.0);
    assert_eq!(linha_de(l, 3, 3).taxa_posicao, 204.0 / 408.0);
    assert_eq!(linha_de(l, 3, 4).taxa_posicao, 200.0 / 408.0);
    // Componente 0 constante (7,7,7,7): todas em médio, que tinha 0 saídas em 1
    // par (a dezena 3 do concurso 1).
    for d in 1..=4 {
        assert_eq!(faixas(3, d, 0), Faixa::Medio);
        assert_eq!(linha_de(l, 3, d).taxas_faixa[0], 200.0 / 404.0);
    }
    // Componente 2 (NULL,9,NULL,NULL): a única dezena com valor é constante ->
    // médio; as demais, nulo. Médio: 0 saídas em 2 pares (dezenas 3 e 4 do
    // concurso 1); nulo: 3 saídas em 5 pares (dezena 2 do 1 e as 4 do 2, com
    // 2 e 3 saindo).
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(3, d, 2)),
        [Faixa::Nulo, Faixa::Medio, Faixa::Nulo, Faixa::Nulo]
    );
    assert_eq!(linha_de(l, 3, 2).taxas_faixa[2], 200.0 / 408.0);
    assert_eq!(linha_de(l, 3, 1).taxas_faixa[2], 212.0 / 420.0);

    // ---- concurso 4 (snapshot, saiu = None). Posições após 3 concursos:
    // pos1 3, pos2 1, pos3 1, pos4 1; dezenas 1..4 estão nas posições 4,3,2,1.
    assert!(l
        .iter()
        .filter(|x| x.concurso == 4)
        .all(|x| x.saiu.is_none()));
    assert_eq!(linha_de(l, 4, 1).taxa_posicao, 204.0 / 412.0);
    assert_eq!(linha_de(l, 4, 2).taxa_posicao, 204.0 / 412.0);
    assert_eq!(linha_de(l, 4, 3).taxa_posicao, 204.0 / 412.0);
    assert_eq!(linha_de(l, 4, 4).taxa_posicao, 212.0 / 412.0);
    // Componente 0 do snapshot (1,NULL,2,3): baixo, nulo, médio, alto. Antes:
    // baixo 4/5, médio 2/5 (dezena 3 do 1, mais as 4 do 3 com 1 e 4 saindo),
    // alto 0/2, nulo 0/0.
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(4, d, 0)),
        [Faixa::Baixo, Faixa::Nulo, Faixa::Medio, Faixa::Alto]
    );
    assert_eq!(linha_de(l, 4, 1).taxas_faixa[0], 216.0 / 420.0);
    assert_eq!(linha_de(l, 4, 2).taxas_faixa[0], 200.0 / 400.0);
    assert_eq!(linha_de(l, 4, 3).taxas_faixa[0], 208.0 / 420.0);
    assert_eq!(linha_de(l, 4, 4).taxas_faixa[0], 200.0 / 408.0);
    // Componente 2 do snapshot (−4,−4,8,−9): n = 4 -> baixo, baixo, alto, baixo.
    // Baixo: só a dezena 1 do concurso 1 (saiu), 1/1; alto: 0/0.
    assert_eq!(
        [1, 2, 3, 4].map(|d| faixas(4, d, 2)),
        [Faixa::Baixo, Faixa::Baixo, Faixa::Alto, Faixa::Baixo]
    );
    assert_eq!(linha_de(l, 4, 1).taxas_faixa[2], 204.0 / 404.0);
    assert_eq!(linha_de(l, 4, 3).taxas_faixa[2], 200.0 / 400.0);
    // Componente 1 (constante): 6 saídas em 12 pares = exatamente o acaso.
    for d in 1..=4 {
        assert_eq!(linha_de(l, 4, d).taxas_faixa[1], 224.0 / 448.0);
    }
}

#[test]
fn mini_contagens_finais_a_mao() {
    let serie = calcular_taxas(&MINI, &serie_mini()).expect("série válida");
    let a = &serie.acumuladas;
    // O snapshot não entra: 3 concursos sorteados.
    assert_eq!(a.concursos_incorporados(), 3);
    assert_eq!(a.ultimo_concurso(), Some(3));
    let pos = |r: u32| a.contagem_posicao(r).expect("posição válida");
    assert_eq!(
        [1, 2, 3, 4].map(|r| (pos(r).saidas, pos(r).total)),
        [(3, 3), (1, 3), (1, 3), (1, 3)]
    );
    // Cada concurso sorteado soma K = 2 saídas.
    assert_eq!([1, 2, 3, 4].map(|r| pos(r).saidas).iter().sum::<u64>(), 6);

    let faixa = |c: usize, f: Faixa| {
        let x = a.contagem_faixa(c, f).expect("componente válido");
        (x.saidas, x.total)
    };
    // Componente 0: baixo 4/5, médio 2/5, alto 0/2, nulo 0/0.
    assert_eq!(faixa(0, Faixa::Baixo), (4, 5));
    assert_eq!(faixa(0, Faixa::Medio), (2, 5));
    assert_eq!(faixa(0, Faixa::Alto), (0, 2));
    assert_eq!(faixa(0, Faixa::Nulo), (0, 0));
    // Componente 1 (constante): médio 6/12.
    assert_eq!(faixa(1, Faixa::Medio), (6, 12));
    assert_eq!(faixa(1, Faixa::Baixo), (0, 0));
    // Componente 2: baixo 1/1, médio 0/3 (3 e 4 do 1, mais a 2 do 3), nulo 5/8.
    assert_eq!(faixa(2, Faixa::Baixo), (1, 1));
    assert_eq!(faixa(2, Faixa::Medio), (0, 3));
    assert_eq!(faixa(2, Faixa::Nulo), (5, 8));
    // Fora do domínio.
    assert!(a.contagem_posicao(0).is_none());
    assert!(a.contagem_posicao(5).is_none());
    assert!(a.contagem_faixa(QTD_COMPONENTES, Faixa::Baixo).is_none());
    assert!(a.taxa_posicao(0).is_none());
    assert!(a.taxa_faixa(9, Faixa::Medio).is_none());
    // Taxas finais = as que um 5º concurso veria.
    assert_eq!(a.taxa_posicao(1), Some(212.0 / 412.0));
    assert_eq!(a.taxa_faixa(0, Faixa::Baixo), Some(216.0 / 420.0));
    assert_eq!(a.taxa_faixa(0, Faixa::Nulo), Some(0.5));
}

#[test]
fn estado_vazio_da_o_acaso_em_tudo() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let a = TaxasAcumuladas::novas(cfg);
        assert_eq!(a.ultimo_concurso(), None);
        assert_eq!(a.concursos_incorporados(), 0);
        let acaso = taxa_do_acaso(cfg);
        assert_eq!(a.taxa_posicao(1), Some(acaso), "{slug}");
        assert_eq!(
            a.taxa_posicao(cfg.qtd_dezenas_possiveis()),
            Some(acaso),
            "{slug}"
        );
        for f in Faixa::TODAS {
            assert_eq!(a.taxa_faixa(4, f), Some(acaso), "{slug}");
        }
    }
}

// ----------------------------------------------------------- ponto no tempo

const T: u32 = 12;

fn troca_saiu(obs: &[ObservacaoPosicoes], concurso: u32) -> Vec<ObservacaoPosicoes> {
    // Tira uma dezena que saiu e põe uma que não saiu (continua com K saídas).
    let mut v = obs.to_vec();
    let saiu = v
        .iter()
        .position(|o| o.concurso == concurso && o.saiu == Some(true))
        .expect("alguma saiu");
    let nao = v
        .iter()
        .position(|o| o.concurso == concurso && o.saiu == Some(false))
        .expect("alguma não saiu");
    v[saiu].saiu = Some(false);
    v[nao].saiu = Some(true);
    v
}

#[test]
fn ponto_no_tempo_o_futuro_nao_muda_o_passado() {
    let cfg = cfg("lotofacil");
    let a = serie_sintetica(cfg, 30, 7, true);
    let outro_futuro = serie_sintetica(cfg, 30, 99, true);
    let b: Vec<_> = a
        .iter()
        .filter(|o| o.concurso <= T)
        .chain(outro_futuro.iter().filter(|o| o.concurso > T))
        .copied()
        .collect();
    assert_ne!(a, b, "o futuro mudou");
    let ra = calcular_taxas(cfg, &a).unwrap();
    let rb = calcular_taxas(cfg, &b).unwrap();
    let ate_t = |s: &SerieTaxas| -> Vec<LinhaTaxas> {
        s.linhas
            .iter()
            .filter(|l| l.concurso <= T)
            .cloned()
            .collect()
    };
    assert_eq!(ate_t(&ra), ate_t(&rb), "concursos ≤ t idênticos");
    // E o futuro mesmo mudou (o teste enxerga diferença).
    assert_ne!(ra.linhas, rb.linhas);
}

#[test]
fn ponto_no_tempo_o_resultado_de_t_nao_muda_as_taxas_de_t() {
    let cfg = cfg("lotofacil");
    let a = serie_sintetica(cfg, 30, 21, true);
    let b = troca_saiu(&a, T);
    assert_ne!(a, b);
    let ra = calcular_taxas(cfg, &a).unwrap();
    let rb = calcular_taxas(cfg, &b).unwrap();
    // Antes de t: idêntico.
    let antes = |s: &SerieTaxas| -> Vec<LinhaTaxas> {
        s.linhas
            .iter()
            .filter(|l| l.concurso < T)
            .cloned()
            .collect()
    };
    assert_eq!(antes(&ra), antes(&rb));
    // Em t: as taxas e faixas são as mesmas; só o `saiu` copiado difere.
    let em_t = |s: &SerieTaxas| -> Vec<LinhaTaxas> {
        s.linhas
            .iter()
            .filter(|l| l.concurso == T)
            .cloned()
            .map(|mut l| {
                l.saiu = None;
                l
            })
            .collect()
    };
    assert_eq!(em_t(&ra), em_t(&rb));
    // Sensibilidade: o resultado de t chega às taxas de t+1.
    let seguinte = |s: &SerieTaxas| -> Vec<LinhaTaxas> {
        s.linhas
            .iter()
            .filter(|l| l.concurso == T + 1)
            .cloned()
            .collect()
    };
    assert_ne!(seguinte(&ra), seguinte(&rb));
}

#[test]
fn ponto_no_tempo_pelo_estado_incremental() {
    // Estado com 1..t−1: `linhas(t)` não muda com o `saiu` de t e não altera o
    // estado; só `incorporar(t)` o altera.
    let cfg = cfg("megasena");
    let serie = serie_sintetica(cfg, 15, 5, false);
    let mut acum = TaxasAcumuladas::novas(cfg);
    for c in 1..T {
        acum.incorporar(&do_concurso(&serie, c)).unwrap();
    }
    let antes = format!("{acum:?}");
    let t = do_concurso(&serie, T);
    let t_alterado = do_concurso(&troca_saiu(&serie, T), T);
    let sem_saiu = |linhas: Vec<LinhaTaxas>| -> Vec<LinhaTaxas> {
        linhas
            .into_iter()
            .map(|mut l| {
                l.saiu = None;
                l
            })
            .collect()
    };
    let la = acum.linhas(&t).unwrap();
    let lb = acum.linhas(&t_alterado).unwrap();
    assert_eq!(sem_saiu(la.clone()), sem_saiu(lb));
    assert_eq!(format!("{acum:?}"), antes, "`linhas` não altera o estado");
    acum.incorporar(&t).unwrap();
    assert_ne!(format!("{acum:?}"), antes);
    // Agora o estado já viu t: `linhas(t)` é recusado (as taxas de t não podem
    // incluir t), e só o t+1 é aceito.
    assert!(acum.linhas(&t).is_err());
    assert!(acum.linhas(&do_concurso(&serie, T + 1)).is_ok());
}

// ------------------------------------------------- incremental = completo

#[test]
fn incremental_em_partes_igual_ao_completo() {
    for (slug, concursos) in [("lotofacil", 25_u32), ("megasena", 20), ("quina", 18)] {
        let cfg = cfg(slug);
        let com_snapshot = serie_sintetica(cfg, concursos + 1, 3, true);
        let completo = calcular_taxas(cfg, &com_snapshot).unwrap();
        // Concurso a concurso (incluindo o snapshot ao fim), pelo estado.
        let mut acum = TaxasAcumuladas::novas(cfg);
        let mut linhas: Vec<LinhaTaxas> = Vec::new();
        for c in 1..=concursos + 1 {
            let grupo = do_concurso(&com_snapshot, c);
            linhas.extend(acum.linhas(&grupo).unwrap());
            if grupo[0].saiu.is_some() {
                acum.incorporar(&grupo).unwrap();
            }
        }
        assert_eq!(linhas, completo.linhas, "{slug}");
        assert_eq!(
            format!("{acum:?}"),
            format!("{:?}", completo.acumuladas),
            "{slug}"
        );
    }
}

#[test]
fn processar_em_duas_chamadas_independentes_da_o_mesmo_estado() {
    // Sem snapshot: primeira parte via `calcular_taxas`, o resto incorporado a
    // mão sobre o estado devolvido.
    let cfg = cfg("lotofacil");
    let serie = serie_sintetica(cfg, 24, 11, false);
    let completo = calcular_taxas(cfg, &serie).unwrap();
    for corte in [1_u32, 7, 23] {
        let parte1: Vec<_> = serie
            .iter()
            .filter(|o| o.concurso <= corte)
            .copied()
            .collect();
        let mut acum = calcular_taxas(cfg, &parte1).unwrap().acumuladas;
        assert_eq!(acum.ultimo_concurso(), Some(corte));
        let mut linhas2 = Vec::new();
        for c in corte + 1..=24 {
            let g = do_concurso(&serie, c);
            linhas2.extend(acum.linhas(&g).unwrap());
            acum.incorporar(&g).unwrap();
        }
        let esperado: Vec<_> = completo
            .linhas
            .iter()
            .filter(|l| l.concurso > corte)
            .cloned()
            .collect();
        assert_eq!(linhas2, esperado, "corte {corte}");
        assert_eq!(
            format!("{acum:?}"),
            format!("{:?}", completo.acumuladas),
            "corte {corte}"
        );
    }
}

#[test]
fn entrada_em_qualquer_ordem_da_a_mesma_saida() {
    let cfg = cfg("quina");
    let serie = serie_sintetica(cfg, 12, 17, true);
    let referencia = calcular_taxas(cfg, &serie).unwrap();
    let mut embaralhada = serie.clone();
    Gerador(12345).embaralhar(&mut embaralhada);
    assert_ne!(embaralhada, serie);
    let saida = calcular_taxas(cfg, &embaralhada).unwrap();
    assert_eq!(saida.linhas, referencia.linhas);
    assert_eq!(
        format!("{:?}", saida.acumuladas),
        format!("{:?}", referencia.acumuladas)
    );
    // Determinismo: de novo, o mesmo.
    assert_eq!(
        calcular_taxas(cfg, &serie).unwrap().linhas,
        referencia.linhas
    );
    // Ordem das linhas: (concurso, dezena).
    assert!(referencia
        .linhas
        .windows(2)
        .all(|p| (p[0].concurso, p[0].dezena) < (p[1].concurso, p[1].dezena)));
}

// ------------------------------------------------ referência independente

/// Faixa pela definição, sem ordenação nem busca binária: conta as menores uma
/// a uma. Independente de `faixas_do_componente`.
fn faixa_ingenua(valores: &[Option<i64>], i: usize) -> Faixa {
    let Some(v) = valores[i] else {
        return Faixa::Nulo;
    };
    let presentes: Vec<i64> = valores.iter().filter_map(|x| *x).collect();
    if presentes.iter().all(|&x| x == v) {
        return Faixa::Medio;
    }
    let n = presentes.len();
    let menores = presentes.iter().filter(|&&x| x < v).count();
    let tercil = (3 * menores) / n;
    [Faixa::Baixo, Faixa::Medio, Faixa::Alto][tercil]
}

/// Recontagem do zero, por concurso: para cada `t`, percorre todos os
/// concursos sorteados `s < t` do dado cru e aplica a fórmula do plano em
/// `f64` (`(saídas + m·K/N) / (total + m)`), sem estado incremental, sem
/// aritmética inteira e sem reaproveitar código de produção.
fn referencia_ingenua(cfg: &LoteriaConfig, obs: &[ObservacaoPosicoes]) -> Vec<LinhaTaxas> {
    let n = f64::from(cfg.qtd_dezenas_possiveis());
    let k = cfg.qtd_dezenas as f64;
    let m = f64::from(CONCURSOS_FICTICIOS);
    let max = obs.iter().map(|o| o.concurso).max().unwrap_or(0);
    let por_concurso = |c: u32| -> Vec<ObservacaoPosicoes> {
        let mut v: Vec<_> = obs.iter().filter(|o| o.concurso == c).copied().collect();
        v.sort_by_key(|o| o.dezena);
        v
    };
    let faixas = |g: &[ObservacaoPosicoes]| -> Vec<[Faixa; QTD_COMPONENTES]> {
        (0..g.len())
            .map(|i| {
                std::array::from_fn(|c| {
                    let valores: Vec<Option<i64>> = g.iter().map(|o| o.componentes[c]).collect();
                    faixa_ingenua(&valores, i)
                })
            })
            .collect()
    };

    let mut saida = Vec::new();
    let primeiro = obs.iter().map(|o| o.concurso).min().unwrap_or(1);
    for t in primeiro..=max {
        let g = por_concurso(t);
        let f = faixas(&g);
        // Contagens recomeçadas do zero com os concursos primeiro..t.
        let mut concursos = 0.0;
        let mut saidas_pos = vec![0.0; cfg.qtd_dezenas_possiveis() as usize];
        let mut saidas_fx = [[0.0_f64; QTD_FAIXAS]; QTD_COMPONENTES];
        let mut pares_fx = [[0.0_f64; QTD_FAIXAS]; QTD_COMPONENTES];
        for s in primeiro..t {
            let gs = por_concurso(s);
            let fs = faixas(&gs);
            concursos += 1.0;
            for (o, fo) in gs.iter().zip(&fs) {
                let saiu = if o.saiu == Some(true) { 1.0 } else { 0.0 };
                saidas_pos[(o.posicao - 1) as usize] += saiu;
                for c in 0..QTD_COMPONENTES {
                    pares_fx[c][fo[c].indice()] += 1.0;
                    saidas_fx[c][fo[c].indice()] += saiu;
                }
            }
        }
        for (o, fo) in g.iter().zip(&f) {
            saida.push(LinhaTaxas {
                concurso: t,
                dezena: o.dezena,
                posicao: o.posicao,
                saiu: o.saiu,
                taxa_posicao: (saidas_pos[(o.posicao - 1) as usize] + m * k / n) / (concursos + m),
                faixas: *fo,
                taxas_faixa: std::array::from_fn(|c| {
                    let i = fo[c].indice();
                    (saidas_fx[c][i] + m * k / n) / (pares_fx[c][i] + m)
                }),
            });
        }
    }
    saida
}

fn confere_com_a_referencia(cfg: &LoteriaConfig, obs: &[ObservacaoPosicoes], rotulo: &str) {
    let incremental = calcular_taxas(cfg, obs).unwrap().linhas;
    let ingenua = referencia_ingenua(cfg, obs);
    assert_eq!(incremental.len(), ingenua.len(), "{rotulo}");
    for (a, b) in incremental.iter().zip(&ingenua) {
        let onde = format!("{rotulo}: concurso {} dezena {}", a.concurso, a.dezena);
        assert_eq!(
            (a.concurso, a.dezena, a.posicao, a.saiu),
            (b.concurso, b.dezena, b.posicao, b.saiu),
            "{onde}"
        );
        assert_eq!(a.faixas, b.faixas, "{onde}: faixas");
        assert!(perto(a.taxa_posicao, b.taxa_posicao), "{onde}: posição");
        for c in 0..QTD_COMPONENTES {
            assert!(
                perto(a.taxas_faixa[c], b.taxas_faixa[c]),
                "{onde}: componente {c}: {} x {}",
                a.taxas_faixa[c],
                b.taxas_faixa[c]
            );
        }
    }
}

#[test]
fn incremental_confere_com_a_referencia_ingenua_na_mini() {
    confere_com_a_referencia(&MINI, &serie_mini(), "mini à mão");
    for semente in [1_u64, 2, 3] {
        let s = serie_sintetica(&MINI, 40, semente, true);
        confere_com_a_referencia(&MINI, &s, &format!("mini semente {semente}"));
    }
}

#[test]
fn incremental_confere_com_a_referencia_ingenua_nas_tres_loterias() {
    for (slug, concursos) in [("lotofacil", 30_u32), ("megasena", 30), ("quina", 25)] {
        let cfg = cfg(slug);
        for semente in [4_u64, 8] {
            let s = serie_sintetica(cfg, concursos, semente, true);
            confere_com_a_referencia(cfg, &s, &format!("{slug} semente {semente}"));
        }
    }
}

#[test]
fn a_referencia_ingenua_reproduz_os_valores_a_mao() {
    // Cola entre as duas verificações: a referência ingênua também dá os
    // números calculados à mão (e não só concorda com o código de produção).
    let r = referencia_ingenua(&MINI, &serie_mini());
    assert!(perto(linha_de(&r, 2, 1).taxa_posicao, 204.0 / 404.0));
    assert!(perto(linha_de(&r, 4, 1).taxas_faixa[0], 216.0 / 420.0));
    assert!(perto(linha_de(&r, 4, 3).taxas_faixa[0], 208.0 / 420.0));
}

// -------------------------------------------------- relação com a 1c e invariantes

#[test]
fn taxa_por_posicao_e_a_da_calibracao_da_1c_bit_a_bit() {
    for slug in ["lotofacil", "megasena", "quina"] {
        let cfg = cfg(slug);
        let s = serie_sintetica(cfg, 40, 13, true);
        let para_1c: Vec<Observacao> = s
            .iter()
            .map(|o| Observacao {
                concurso: o.concurso,
                dezena: o.dezena,
                posicao: o.posicao,
                saiu: o.saiu,
            })
            .collect();
        let calibradas = calibrar(cfg, &para_1c).unwrap();
        let taxas = calcular_taxas(cfg, &s).unwrap().linhas;
        assert_eq!(calibradas.len(), taxas.len());
        for (c, t) in calibradas.iter().zip(&taxas) {
            assert_eq!(
                (c.concurso, c.dezena, c.posicao),
                (t.concurso, t.dezena, t.posicao)
            );
            assert_eq!(
                c.probabilidade, t.taxa_posicao,
                "{slug} c{} d{}",
                c.concurso, c.dezena
            );
        }
    }
}

#[test]
fn invariantes_das_contagens() {
    let cfg = cfg("lotofacil");
    let n = u64::from(cfg.qtd_dezenas_possiveis());
    let k = cfg.qtd_dezenas as u64;
    let s = serie_sintetica(cfg, 37, 29, true);
    let a = calcular_taxas(cfg, &s).unwrap().acumuladas;
    let c = a.concursos_incorporados();
    assert_eq!(c, 37 - 1, "o snapshot não entra");
    // Cada concurso sorteado soma K saídas nas posições.
    let saidas: u64 = (1..=cfg.qtd_dezenas_possiveis())
        .map(|r| a.contagem_posicao(r).unwrap().saidas)
        .sum();
    assert_eq!(saidas, k * c);
    // Em cada componente, as 4 faixas somam N pares por concurso e K saídas.
    for comp in 0..QTD_COMPONENTES {
        let (mut pares, mut sai) = (0, 0);
        for f in Faixa::TODAS {
            let x = a.contagem_faixa(comp, f).unwrap();
            assert!(x.saidas <= x.total);
            pares += x.total;
            sai += x.saidas;
        }
        assert_eq!(pares, n * c, "componente {comp}");
        assert_eq!(sai, k * c, "componente {comp}");
    }
}

#[test]
fn taxas_ficam_entre_0_e_1_e_encolhem_para_o_acaso() {
    let cfg = cfg("megasena");
    let acaso = taxa_do_acaso(cfg);
    let s = serie_sintetica(cfg, 30, 31, true);
    for l in calcular_taxas(cfg, &s).unwrap().linhas {
        assert!(l.taxa_posicao > 0.0 && l.taxa_posicao < 1.0);
        // Com m = 100 e poucos concursos, nada se afasta muito do acaso.
        assert!((l.taxa_posicao - acaso).abs() < 0.05);
        for t in l.taxas_faixa {
            assert!(t > 0.0 && t < 1.0);
        }
    }
}

#[test]
fn nulo_tem_taxa_propria() {
    // Componente 0 com a dezena 4 NULL em todos os concursos: a faixa nula
    // acumula só a dezena 4, e a taxa dela não se mistura com as outras.
    let mut obs = Vec::new();
    for c in 1..=6_u32 {
        let saidas = if c % 2 == 0 { [4, 1] } else { [1, 2] };
        let mut g = concurso_mini_pos(
            c,
            [1, 2, 3, 4],
            Some(saidas),
            algum([1, 2, 3, 0]),
            algum([0, 0, 0, 0]),
        );
        g[3].componentes[0] = None;
        obs.extend(g);
    }
    let serie = calcular_taxas(&MINI, &obs).unwrap();
    let a = &serie.acumuladas;
    // Dezena 4 saiu nos concursos 2, 4 e 6: nulo = 3 saídas em 6 pares.
    let nulo = a.contagem_faixa(0, Faixa::Nulo).unwrap();
    assert_eq!((nulo.saidas, nulo.total), (3, 6));
    assert_eq!(a.taxa_faixa(0, Faixa::Nulo), Some(212.0 / 424.0));
    // As outras faixas só têm as dezenas 1..3 (3 pares por concurso).
    let total: u64 = [Faixa::Baixo, Faixa::Medio, Faixa::Alto]
        .iter()
        .map(|&f| a.contagem_faixa(0, f).unwrap().total)
        .sum();
    assert_eq!(total, 18);
    // No concurso 6 a dezena 4 está em "nulo" e as demais não.
    let l6 = serie.linhas.iter().filter(|l| l.concurso == 6);
    for l in l6 {
        assert_eq!(l.faixas[0] == Faixa::Nulo, l.dezena == 4);
    }
}

// -------------------------------------------------------------- entradas inválidas

fn erro_de(r: Result<impl std::fmt::Debug>) -> String {
    match r {
        Err(SpiderError::AvaliacaoEntradaInvalida(d)) => d,
        outro => panic!("esperado AvaliacaoEntradaInvalida, veio {outro:?}"),
    }
}

#[test]
fn entrada_vazia_devolve_vazio() {
    let s = calcular_taxas(&MINI, &[]).unwrap();
    assert!(s.linhas.is_empty());
    assert_eq!(s.acumuladas.concursos_incorporados(), 0);
}

#[test]
fn serie_com_buraco_e_recusada() {
    let mut s = serie_mini();
    s.retain(|o| o.concurso != 2);
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("faltam o concurso 2"));
}

#[test]
fn snapshot_no_meio_da_serie_e_recusado() {
    let mut s = serie_mini();
    for o in s.iter_mut().filter(|o| o.concurso == 2) {
        o.saiu = None;
    }
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("sem resultado"));
}

#[test]
fn posicao_repetida_ou_fora_da_faixa_e_recusada() {
    let mut s = serie_mini();
    s[1].posicao = 1;
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("repetida"));
    let mut s = serie_mini();
    s[0].posicao = 5;
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("fora de"));
}

#[test]
fn quantidade_de_saidas_errada_e_recusada() {
    let mut s = serie_mini();
    s[2].saiu = Some(true); // concurso 1 passa a ter 3 saídas
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("sorteia 2"));
}

#[test]
fn dezena_repetida_ou_faltando_e_recusada() {
    let mut s = serie_mini();
    s.pop();
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("faltam as dezenas"));
    let mut s = serie_mini();
    s[1].dezena = 1;
    assert!(erro_de(calcular_taxas(&MINI, &s)).contains("repetida"));
}

#[test]
fn incorporar_exige_concurso_seguinte_e_sorteado() {
    let s = serie_mini();
    let mut a = TaxasAcumuladas::novas(&MINI);
    // O primeiro pode ser qualquer um (a série pode começar em outro número).
    a.incorporar(&do_concurso(&s, 1)).unwrap();
    // Buraco: pula o 2.
    let antes = format!("{a:?}");
    assert!(erro_de(a.incorporar(&do_concurso(&s, 3))).contains("esperado o concurso 2"));
    assert!(erro_de(a.linhas(&do_concurso(&s, 3))).contains("esperado o concurso 2"));
    // Repetido.
    assert!(erro_de(a.incorporar(&do_concurso(&s, 1))).contains("esperado o concurso 2"));
    // Snapshot não entra nas contagens (nem pulando para ele).
    a.incorporar(&do_concurso(&s, 2)).unwrap();
    a.incorporar(&do_concurso(&s, 3)).unwrap();
    assert!(erro_de(a.incorporar(&do_concurso(&s, 4))).contains("sem resultado"));
    // `linhas` aceita o snapshot.
    assert_eq!(a.linhas(&do_concurso(&s, 4)).unwrap().len(), 4);
    // Erros não mexem no estado.
    let mut b = TaxasAcumuladas::novas(&MINI);
    b.incorporar(&do_concurso(&s, 1)).unwrap();
    assert_eq!(format!("{b:?}"), antes);
}

#[test]
fn concurso_vazio_ou_misturado_e_recusado() {
    let a = TaxasAcumuladas::novas(&MINI);
    assert!(erro_de(a.linhas(&[])).contains("nenhuma dezena"));
    let s = serie_mini();
    let mut misturado = do_concurso(&s, 1);
    misturado[3].concurso = 2;
    assert!(erro_de(a.linhas(&misturado)).contains("misturam"));
}

#[test]
fn serie_pode_comecar_em_qualquer_concurso() {
    // Como na 1c, o primeiro concurso da entrada recebe a taxa do acaso.
    let mut s = serie_mini();
    for o in &mut s {
        o.concurso += 100;
    }
    let r = calcular_taxas(&MINI, &s).unwrap();
    assert_eq!(linha_de(&r.linhas, 101, 1).taxa_posicao, 0.5);
    assert_eq!(r.acumuladas.ultimo_concurso(), Some(103));
    let sem_deslocar = calcular_taxas(&MINI, &serie_mini()).unwrap();
    let valores = |l: &[LinhaTaxas]| -> Vec<(f64, [f64; QTD_COMPONENTES])> {
        l.iter().map(|x| (x.taxa_posicao, x.taxas_faixa)).collect()
    };
    assert_eq!(valores(&r.linhas), valores(&sem_deslocar.linhas));
}
