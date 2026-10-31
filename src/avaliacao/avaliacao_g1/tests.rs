use super::*;
use crate::avaliacao::tests::{Gerador, MINI};
use crate::motor::JANELAS;

/// A MINI (2 de 1..=4) com um corte da busca depois do aquecimento: a série é
/// curta e os testes rodam rápido, mesmo sem otimização.
const CFG: LoteriaConfig = LoteriaConfig {
    ultimo_concurso_busca: 400,
    ..MINI
};

const K: u32 = 2;

/// Série aleatória até `concursos` (inclusive o trancado): em cada concurso, as
/// duas ordens são permutações independentes e K dezenas saem.
fn serie(concursos: u32, semente: u64) -> Vec<LinhaG1> {
    let mut g = Gerador(semente);
    let n = CFG.qtd_dezenas_possiveis();
    let mut saida = Vec::new();
    for c in 1..=concursos {
        let mut a: Vec<u32> = (1..=n).collect();
        let mut b: Vec<u32> = (1..=n).collect();
        let mut dezenas: Vec<u32> = CFG.dezenas().collect();
        g.embaralhar(&mut a);
        g.embaralhar(&mut b);
        g.embaralhar(&mut dezenas);
        let sorteadas = &dezenas[..CFG.qtd_dezenas];
        for (i, d) in CFG.dezenas().enumerate() {
            saida.push(LinhaG1 {
                concurso: c,
                dezena: d,
                posicao_ranking_a: a[i],
                g_1: b[i],
                saiu: sorteadas.contains(&d),
            });
        }
    }
    saida
}

#[test]
fn aquecimento_e_a_maior_janela_finita_do_motor() {
    assert_eq!(Some(&AQUECIMENTO), JANELAS.iter().max());
}

#[test]
fn a_serie_avaliada_comeca_depois_do_aquecimento() {
    let s = serie(CFG.ultimo_concurso_busca, 7);
    let r = avaliar_g1(&CFG, &s, K).expect("avalia");
    assert_eq!(r.aquecimento, 100);
    assert_eq!(r.corte, CFG.ultimo_concurso_busca);
    assert_eq!(r.dobras.len(), 4);
    assert_eq!(
        r.dobras.iter().map(|d| d.dobra).collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    // As dobras de teste vêm depois do aquecimento e da dobra 1.
    assert!(r.faixa.primeiro > AQUECIMENTO + 1);
    assert_eq!(r.faixa.ultimo, CFG.ultimo_concurso_busca);
    assert_eq!(r.concursos as u64, r.faixa.tamanho());
}

#[test]
fn aquecimento_e_trancado_nao_entram_em_nenhum_numero() {
    let base = serie(CFG.ultimo_concurso_busca + 40, 11);
    let referencia = avaliar_g1(&CFG, &base, K).expect("base");

    // Trocar tudo o que está no aquecimento e acima do corte não muda nada,
    // nem a validade da série (o aquecimento e o trancado nem são validados).
    let mut g = Gerador(99);
    let n = CFG.qtd_dezenas_possiveis();
    let mut mexida = base.clone();
    for l in &mut mexida {
        if l.concurso <= AQUECIMENTO || l.concurso > CFG.ultimo_concurso_busca {
            l.saiu = g.proximo().is_multiple_of(2);
            l.g_1 = (g.proximo() % u64::from(n)) as u32 + 1;
            l.posicao_ranking_a = (g.proximo() % u64::from(n)) as u32 + 1;
        }
    }
    assert_eq!(avaliar_g1(&CFG, &mexida, K).expect("mexida"), referencia);

    // Já trocar o que fica muda: o teste não passa por entrada que não conta.
    // Troca quem saiu entre duas dezenas do mesmo concurso (K continua 2).
    let alvo = AQUECIMENTO + 150;
    let mut dentro = base;
    let (sai, nao_sai): (Vec<usize>, Vec<usize>) = (0..dentro.len())
        .filter(|&i| dentro[i].concurso == alvo)
        .partition(|&i| dentro[i].saiu);
    dentro[sai[0]].saiu = false;
    dentro[nao_sai[0]].saiu = true;
    assert_ne!(avaliar_g1(&CFG, &dentro, K).expect("dentro"), referencia);
}

#[test]
fn g_1_igual_a_nota_a_da_o_mesmo_resultado_nas_duas_ordens() {
    let mut s = serie(CFG.ultimo_concurso_busca, 3);
    for l in &mut s {
        l.g_1 = l.posicao_ranking_a;
    }
    let r = avaliar_g1(&CFG, &s, K).expect("avalia");
    assert_eq!(r.g, r.a);
}

#[test]
fn g_1_e_avaliada_pela_regua_da_1f4_no_recorte_sem_aquecimento() {
    // A ordem G-1 sai exatamente como a "ordem B" de `avaliar_ranking` sobre as
    // mesmas linhas sem o aquecimento.
    let s = serie(CFG.ultimo_concurso_busca, 5);
    let r = avaliar_g1(&CFG, &s, K).expect("avalia");

    let recorte: Vec<LinhaG1> = s
        .iter()
        .filter(|l| l.concurso > AQUECIMENTO)
        .copied()
        .collect();
    let direto = avaliar_ranking(
        &CFG,
        &como_ranking(&recorte, |l| l.g_1, |l| l.posicao_ranking_a),
        K,
    )
    .expect("direto");
    assert_eq!(r.g, direto.b);
    assert_eq!(r.dobras, direto.dobras);
    // As duas ordens diferem entre si (séries independentes).
    assert_ne!(r.g, r.a);
}

#[test]
fn sem_sinal_o_veredito_nao_supera_o_acaso() {
    // Média sobre sementes: com ordens e sorteios independentes, o p do
    // bootstrap não pode ser sistematicamente pequeno. Cinco sementes, no
    // máximo uma abaixo de α (esperado 0,25 de 5; folga para a variância).
    let abaixo = (1..=5_u64)
        .filter(|&semente| {
            let s = serie(CFG.ultimo_concurso_busca, semente);
            avaliar_g1(&CFG, &s, K)
                .expect("avalia")
                .g
                .supera_o_acaso()
                .unwrap_or(false)
        })
        .count();
    assert!(
        abaixo <= 1,
        "{abaixo} de 5 séries sem sinal superaram o acaso"
    );
}

#[test]
fn k_invalido_e_trecho_curto_sao_erros_do_avaliar_ranking() {
    let s = serie(CFG.ultimo_concurso_busca, 1);
    assert!(avaliar_g1(&CFG, &s, 0).is_err());
    assert!(avaliar_g1(&CFG, &s, CFG.qtd_dezenas_possiveis()).is_err());
    // Só o aquecimento: nada para avaliar.
    let curto = serie(AQUECIMENTO, 1);
    assert!(avaliar_g1(&CFG, &curto, K).is_err());
}
