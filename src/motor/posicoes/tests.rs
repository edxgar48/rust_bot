use super::*;
use crate::config::{HoraLocal, Volante};
use crate::motor::JANELA_TOTAL;

/// MINI do golden da 1g: 2 de 1..=5.
const MINI: LoteriaConfig = LoteriaConfig {
    slug: "mini",
    nome_exibicao: "Mini",
    pagina_url: "",
    seletor_botao_download: "",
    seletor_concurso_data: "",
    seletor_dezenas: "",
    qtd_dezenas: 2,
    dezena_min: 1,
    dezena_max: 5,
    volante: Volante {
        linhas: 1,
        colunas: 5,
    },
    coluna_concurso: 0,
    coluna_data: 1,
    primeira_coluna_dezena: 2,
    ultimo_concurso_busca: 0,
    perfil_ativo: "mini_v1",
    dias_sorteio: &[],
    procurar_a_partir_das: HoraLocal { hora: 0, minuto: 0 },
    sorteios_especiais: &[],
};

fn res(concurso: u32, dezenas: &[u32]) -> Resultado {
    Resultado {
        loteria: MINI.slug.to_string(),
        concurso,
        data_sorteio: "2020-01-01".to_string(),
        dezenas: dezenas.to_vec(),
    }
}

/// Linhas do índice de um concurso: `posicoes[i]` é a posição da dezena `i + 1`.
/// O `total` não importa para as marcas.
fn ind(concurso: u32, posicoes: &[u32]) -> Vec<IndiceDezena> {
    (1..)
        .zip(posicoes)
        .map(|(dezena, &posicao)| IndiceDezena {
            concurso,
            dezena,
            total: 0.0,
            posicao,
            variacao_posicao: None,
        })
        .collect()
}

/// Sorteios do golden da 1g (`docs/PLANEJAMENTO.md`).
fn sorteios() -> Vec<Resultado> {
    vec![res(1, &[1, 2]), res(2, &[2, 3]), res(3, &[1, 5])]
}

/// Índice das dezenas do golden: concursos 1 a 3 e o snapshot do 4.
fn indice_golden() -> Vec<IndiceDezena> {
    [
        ind(1, &[1, 2, 3, 4, 5]),
        ind(2, &[2, 1, 3, 4, 5]),
        ind(3, &[3, 2, 1, 5, 4]),
        ind(4, &[2, 4, 3, 5, 1]),
    ]
    .concat()
}

/// Só a frequência pesa: o índice das posições vira a ordem da frequência.
fn pesos_frequencia() -> Pesos {
    Pesos {
        frequencia: 1.0,
        maior_espaco: 0.0,
        maior_espaco_ocorrencias: 0.0,
        menor_espaco: 0.0,
        menor_espaco_ocorrencias: 0.0,
        soma_variacoes: 0.0,
        espaco_final: 0.0,
        repeticoes_total: 0.0,
        repeticao_atual: 0.0,
    }
}

fn calcular_golden(
    resultados: &[Resultado],
    indice: &[IndiceDezena],
) -> (EstadoMotor, Vec<FeaturesDezena>, Vec<IndiceDezena>) {
    calcular_posicoes(
        &MINI,
        resultados,
        indice,
        JANELA_TOTAL,
        Normalizacao::Zscore,
        &pesos_frequencia(),
    )
    .expect("golden válido")
}

fn linha(f: &[FeaturesDezena], concurso: u32, posicao: u32, janela: u32) -> &FeaturesDezena {
    f.iter()
        .find(|l| l.concurso == concurso && l.dezena == posicao && l.janela == janela)
        .expect("linha presente")
}

#[test]
fn config_das_posicoes_tem_n_posicoes_e_k_marcas() {
    let cfg = config_posicoes(&MINI);
    assert_eq!((cfg.dezena_min, cfg.dezena_max), (1, 5));
    assert_eq!(cfg.qtd_dezenas, 2);
    assert_eq!(cfg.slug, MINI.slug);
}

#[test]
fn marcas_do_golden() {
    let marcas = marcar_historico(&MINI, &sorteios(), &indice_golden()).unwrap();
    let posicoes: Vec<(u32, &[u32])> = marcas
        .iter()
        .map(|m| (m.concurso, m.dezenas.as_slice()))
        .collect();
    assert_eq!(
        posicoes,
        [(1, &[1, 2][..]), (2, &[1, 3][..]), (3, &[3, 4][..])]
    );
    // Data e loteria acompanham o resultado original.
    assert_eq!(marcas[0].loteria, "mini");
    assert_eq!(marcas[0].data_sorteio, "2020-01-01");
}

#[test]
fn features_das_posicoes_em_t4_janela_total() {
    let (_, features, _) = calcular_golden(&sorteios(), &indice_golden());
    // (posição, frequencia, maior (oc.), menor (oc.), soma, espaco_final,
    //  repeticoes_total, repeticao_atual)
    type Esperado = (
        u32,
        u32,
        Option<(u32, u32)>,
        Option<(u32, u32)>,
        Option<u32>,
        Option<u32>,
        u32,
        u32,
    );
    let esperado: [Esperado; 5] = [
        (1, 2, Some((0, 1)), Some((0, 1)), None, Some(1), 1, 0),
        (2, 1, None, None, None, Some(2), 0, 0),
        (3, 2, Some((0, 1)), Some((0, 1)), None, Some(0), 1, 2),
        (4, 1, None, None, None, Some(0), 0, 1),
        (5, 0, None, None, None, None, 0, 0),
    ];
    for (pos, freq, maior, menor, soma, final_, rep_total, rep_atual) in esperado {
        let f = linha(&features, 4, pos, JANELA_TOTAL);
        assert_eq!(f.frequencia, freq, "posição {pos}: frequencia");
        assert_eq!(
            f.maior_espaco.zip(f.maior_espaco_ocorrencias),
            maior,
            "posição {pos}: maior espaço"
        );
        assert_eq!(
            f.menor_espaco.zip(f.menor_espaco_ocorrencias),
            menor,
            "posição {pos}: menor espaço"
        );
        assert_eq!(f.soma_variacoes, soma, "posição {pos}: soma_variacoes");
        assert_eq!(f.espaco_final, final_, "posição {pos}: espaco_final");
        assert_eq!(
            f.repeticoes_total, rep_total,
            "posição {pos}: repeticoes_total"
        );
        assert_eq!(
            f.repeticao_atual, rep_atual,
            "posição {pos}: repeticao_atual"
        );
        assert_eq!(f.saiu, None, "t=4 ainda não foi sorteado");
    }
}

#[test]
fn indice_das_posicoes_ordena_por_g_1() {
    let (_, _, indice) = calcular_golden(&sorteios(), &indice_golden());
    // Em t=4, com só a frequência pesando: frequências [2, 1, 2, 1, 0];
    // empate → menor posição à frente.
    let mut g_1: Vec<(u32, u32)> = indice
        .iter()
        .filter(|l| l.concurso == 4)
        .map(|l| (l.posicao, l.dezena))
        .collect();
    g_1.sort_unstable();
    assert_eq!(g_1, [(1, 1), (2, 3), (3, 2), (4, 4), (5, 5)]);
    // 4 concursos × 5 posições.
    assert_eq!(indice.len(), 20);
}

#[test]
fn traduz_g_1_em_dezenas_pelo_indice_do_proximo() {
    // `g_1` do exemplo do golden: posição 3 = 1, 1 = 2, 4 = 3, 2 = 4, 5 = 5.
    let g_1_posicoes = ind(4, &[2, 4, 1, 3, 5]);
    let ordem = dezenas_via_posicao(&MINI, 4, &indice_golden(), &g_1_posicoes).unwrap();
    let dezenas: Vec<u32> = ordem.iter().map(|d| d.dezena).collect();
    assert_eq!(dezenas, [3, 5, 2, 1, 4]);
    assert_eq!(
        ordem[0],
        DezenaViaPosicao {
            dezena: 3,
            posicao: 3,
            g_1: 1
        }
    );
    assert_eq!(
        ordem[1],
        DezenaViaPosicao {
            dezena: 5,
            posicao: 1,
            g_1: 2
        }
    );
}

#[test]
fn ponto_no_tempo_a_marca_de_t_nao_entra_nas_features_de_t() {
    let (_, features, _) = calcular_golden(&sorteios(), &indice_golden());
    // Posição 3 foi marcada em t=2 e t=3; em t=3 só a de t=2 conta.
    assert_eq!(linha(&features, 3, 3, JANELA_TOTAL).frequencia, 1);
    assert_eq!(linha(&features, 3, 3, JANELA_TOTAL).saiu, Some(true));

    // Mudar o sorteio 3 não altera nenhuma feature dos concursos ≤ 3.
    let mut outros = sorteios();
    outros[2] = res(3, &[2, 4]);
    let (_, outras, _) = calcular_golden(&outros, &indice_golden());
    let ate_3 = |f: &[FeaturesDezena]| -> Vec<FeaturesDezena> {
        f.iter()
            .filter(|l| l.concurso <= 3)
            .cloned()
            .map(|mut l| {
                l.saiu = None;
                l
            })
            .collect()
    };
    assert_eq!(ate_3(&features), ate_3(&outras));
    // E altera o snapshot de t=4 (a marca de 3 já pode ser vista).
    assert_ne!(
        linha(&features, 4, 4, JANELA_TOTAL),
        linha(&outras, 4, 4, JANELA_TOTAL)
    );
}

#[test]
fn incremental_igual_ao_completo() {
    let marcas = marcar_historico(&MINI, &sorteios(), &indice_golden()).unwrap();
    let cfg_pos = config_posicoes(&MINI);

    let mut estado = EstadoMotor::novo(&cfg_pos);
    let mut incremental = Vec::new();
    for m in &marcas {
        incremental.extend(estado.adicionar(&cfg_pos, m).unwrap());
    }
    incremental.extend(estado.snapshot());

    let (estado_completo, completo, _) = calcular_golden(&sorteios(), &indice_golden());
    assert_eq!(estado, estado_completo);
    assert_eq!(incremental, completo);
}

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
}

#[test]
fn cada_concurso_marca_exatamente_k_posicoes() {
    let mut g = Gerador(0x9E37_79B9_7F4A_7C15);
    let mut resultados = Vec::new();
    let mut indice = Vec::new();
    for concurso in 1..=40u32 {
        // Permutação aleatória de 1..=5 (Fisher-Yates).
        let mut posicoes = [1u32, 2, 3, 4, 5];
        for i in (1..posicoes.len()).rev() {
            posicoes.swap(i, (g.proximo() % (i as u64 + 1)) as usize);
        }
        indice.extend(ind(concurso, &posicoes));
        let a = (g.proximo() % 5) as u32 + 1;
        let b = (a + (g.proximo() % 4) as u32) % 5 + 1;
        resultados.push(res(concurso, &[a.min(b), a.max(b)]));
    }
    let marcas = marcar_historico(&MINI, &resultados, &indice).unwrap();
    assert_eq!(marcas.len(), 40);
    for m in &marcas {
        assert_eq!(m.dezenas.len(), MINI.qtd_dezenas, "concurso {}", m.concurso);
        assert!(
            m.dezenas.windows(2).all(|w| w[0] < w[1]),
            "ordenadas e distintas"
        );
        assert!(m.dezenas.iter().all(|p| (1..=5).contains(p)));
    }
}

#[test]
fn indice_sem_um_concurso_e_erro() {
    let sem_o_2: Vec<IndiceDezena> = indice_golden()
        .into_iter()
        .filter(|l| l.concurso != 2)
        .collect();
    let erro = marcar_historico(&MINI, &sorteios(), &sem_o_2).unwrap_err();
    assert!(
        matches!(&erro, SpiderError::MotorIndiceEntradaInvalida(m) if m.contains("concurso 2")),
        "{erro}"
    );
}

#[test]
fn posicoes_que_nao_sao_permutacao_sao_erro() {
    // Posição 2 repetida no concurso 1.
    let repetida = ind(1, &[1, 2, 2, 4, 5]);
    let erro = marcar_historico(&MINI, &sorteios()[..1], &repetida).unwrap_err();
    assert!(
        matches!(erro, SpiderError::MotorIndiceEntradaInvalida(_)),
        "{erro}"
    );

    // Posição fora de 1..=N.
    let fora = ind(1, &[1, 2, 3, 4, 6]);
    let erro = marcar_historico(&MINI, &sorteios()[..1], &fora).unwrap_err();
    assert!(
        matches!(erro, SpiderError::MotorIndiceEntradaInvalida(_)),
        "{erro}"
    );

    // Dezena faltando.
    let mut faltando = ind(1, &[1, 2, 3, 4, 5]);
    faltando.pop();
    let erro = marcar_historico(&MINI, &sorteios()[..1], &faltando).unwrap_err();
    assert!(
        matches!(erro, SpiderError::MotorIndiceEntradaInvalida(_)),
        "{erro}"
    );
}

#[test]
fn sorteio_com_dezena_fora_da_faixa_e_erro() {
    let erro = marcar(&MINI, &res(1, &[1, 9]), &[1, 2, 3, 4, 5]).unwrap_err();
    assert!(
        matches!(
            erro,
            SpiderError::MotorResultadoInvalido { concurso: 1, .. }
        ),
        "{erro}"
    );
}

#[test]
fn traducao_sem_o_concurso_e_erro() {
    let g_1 = ind(4, &[2, 4, 1, 3, 5]);
    let erro = dezenas_via_posicao(&MINI, 9, &indice_golden(), &g_1).unwrap_err();
    assert!(
        matches!(erro, SpiderError::MotorIndiceEntradaInvalida(_)),
        "{erro}"
    );
}

#[test]
fn ponto_no_tempo_o_indice_de_t_ou_depois_nao_muda_as_features_de_t() {
    let (_, base, _) = calcular_golden(&sorteios(), &indice_golden());
    let ate = |f: &[FeaturesDezena], limite: u32| -> Vec<FeaturesDezena> {
        f.iter()
            .filter(|l| l.concurso <= limite)
            .cloned()
            .map(|mut l| {
                // `saiu` de `limite` depende da marca desse concurso.
                if l.concurso == limite {
                    l.saiu = None;
                }
                l
            })
            .collect()
    };

    // Trocar o índice do concurso 3 mexe só a partir do snapshot de t=4.
    let mut outro_3 = indice_golden();
    outro_3.retain(|l| l.concurso != 3);
    outro_3.extend(ind(3, &[5, 4, 3, 2, 1]));
    let (_, f3, _) = calcular_golden(&sorteios(), &outro_3);
    assert_eq!(ate(&base, 3), ate(&f3, 3));
    assert_ne!(base, f3);

    // O índice do snapshot (concurso 4) não entra em nenhuma feature.
    let mut outro_4 = indice_golden();
    outro_4.retain(|l| l.concurso != 4);
    outro_4.extend(ind(4, &[5, 4, 3, 2, 1]));
    let (_, f4, _) = calcular_golden(&sorteios(), &outro_4);
    assert_eq!(base, f4);
}

#[test]
fn retomar_do_estado_no_meio_igual_ao_completo() {
    let cfg_pos = config_posicoes(&MINI);
    let indice = indice_golden();

    let mut estado = EstadoMotor::novo(&cfg_pos);
    let antigas = marcar_historico(&MINI, &sorteios()[..2], &indice).unwrap();
    let mut incremental = estado.avancar(&cfg_pos, &antigas).unwrap();
    // O concurso novo chega depois, com as marcas só dele.
    let novas = marcar_historico(&MINI, &sorteios()[2..], &indice).unwrap();
    incremental.extend(estado.avancar(&cfg_pos, &novas).unwrap());
    incremental.extend(estado.snapshot());

    let (estado_completo, completo, _) = calcular_golden(&sorteios(), &indice);
    assert_eq!(estado, estado_completo);
    assert_eq!(incremental, completo);
}

#[test]
fn calcular_posicoes_e_deterministico() {
    let a = calcular_golden(&sorteios(), &indice_golden());
    let b = calcular_golden(&sorteios(), &indice_golden());
    assert_eq!(a, b);
}
