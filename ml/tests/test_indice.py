"""Índice em Python: os goldens de ``src/motor/indice.rs`` e bordas."""

from __future__ import annotations

import numpy as np
import pytest

from rust_spider_ml.dataset import COMPONENTES
from rust_spider_ml.indice import ErroIndice, arredondar, calcular_indice, normalizar

N = np.nan
# Mesmos pesos diádicos do golden do Rust (`PESOS_GOLDEN`).
PESOS_GOLDEN = [1.0, 0.5, 0.75, 0.25, 0.5, -2.0, -0.125, 3.0, 0.0]


def golden() -> np.ndarray:
    """Concursos 10 e 11 do golden do Rust, dezenas 1..=4, janela 0."""
    c10 = {
        "frequencia": [1, 1, 3, 3],
        "maior_espaco": [N, 2, 4, N],
        "maior_espaco_ocorrencias": [1, 1, 1, 1],
        "menor_espaco": [2, 0, 2, 0],
        "menor_espaco_ocorrencias": [N, N, N, N],
        "soma_variacoes": [N, N, N, 7],
        "espaco_final": [6, 2, 2, 6],
        "repeticoes_total": [5, 5, 5, 5],
        "repeticao_atual": [0, 1, 0, 1],
    }
    c11 = {
        "frequencia": [3, 3, 1, 1],
        "maior_espaco": [N] * 4,
        "maior_espaco_ocorrencias": [N] * 4,
        "menor_espaco": [3] * 4,
        "menor_espaco_ocorrencias": [N] * 4,
        "soma_variacoes": [N] * 4,
        "espaco_final": [0] * 4,
        "repeticoes_total": [0] * 4,
        "repeticao_atual": [2] * 4,
    }
    return np.array(
        [[[c[nome][d] for nome in COMPONENTES] for d in range(4)] for c in (c10, c11)],
        dtype=np.float64,
    )


def test_golden_zscore():
    totais, pos = calcular_indice(golden(), "zscore", PESOS_GOLDEN)
    assert totais.tolist() == [[-0.875, -1.625, 1.875, 0.625], [1.0, 1.0, -1.0, -1.0]]
    # Empate no concurso 11 → menor dezena à frente.
    assert pos.tolist() == [[3, 4, 1, 2], [1, 2, 3, 4]]


def test_golden_minmax():
    totais, pos = calcular_indice(golden(), "minmax", PESOS_GOLDEN)
    assert totais.tolist() == [[1.5, 1.125, 2.875, 2.25], [2.4375, 2.4375, 1.4375, 1.4375]]
    assert pos.tolist() == [[3, 4, 1, 2], [1, 2, 3, 4]]


def test_ruido_menor_que_1e9_vira_empate():
    # d1 = 0,3; d2 = 0,1 + 0,2 = 0,30000000000000004 → arredondados iguais;
    # empate → d1 à frente (como no Rust).
    comp = np.zeros((1, 3, 9))
    comp[0, :, 0] = [1, 0, 0]
    comp[0, :, 1] = [0, 1, 0]
    comp[0, :, 2] = [0, 1, 0]
    pesos = [0.3, 0.1, 0.2, 0, 0, 0, 0, 0, 0]
    totais, pos = calcular_indice(comp, "minmax", pesos)
    assert totais[0, 0] == totais[0, 1] == 0.3
    assert pos.tolist() == [[1, 2, 3]]


def test_zero_negativo_empata_com_zero():
    # d1: −0,1 − 0,2 + 0,3 ≈ −5,55e−17 → −0,0 após arredondar → 0,0.
    comp = np.zeros((1, 2, 9))
    comp[0, :, 0] = [1, 0]
    comp[0, :, 1] = [1, 0]
    comp[0, :, 2] = [1, 0]
    pesos = [-0.1, -0.2, 0.3, 0, 0, 0, 0, 0, 0]
    totais, pos = calcular_indice(comp, "minmax", pesos)
    assert totais.tolist() == [[0.0, 0.0]]
    assert not np.signbit(totais).any()
    assert pos.tolist() == [[1, 2]]


def test_arredondar_meio_para_longe_do_zero():
    # np.round daria 2 e −2 (meio para o par); f64::round dá 3 e −3.
    assert arredondar(np.array([2.5e-9, -2.5e-9, 1.5e-9])).tolist() == [3e-9, -3e-9, 2e-9]


def test_normalizar_bordas():
    v = np.array(
        [
            [N, N, N],  # todo NULL → neutro
            [4, 4, 4],  # constante → neutro
            [N, 7, N],  # um valor só → neutro
            [0, N, 2],  # NULL recebe o neutro; estatísticas sem ele
        ]
    )
    z = normalizar(v, "zscore")
    assert z[:3].tolist() == [[0.0] * 3] * 3
    assert z[3].tolist() == [-1.0, 0.0, 1.0]
    mm = normalizar(v, "minmax")
    assert mm[:3].tolist() == [[0.5] * 3] * 3
    assert mm[3].tolist() == [0.0, 0.5, 1.0]


def test_zscore_desvio_populacional():
    z = normalizar(np.array([[1.0, 2.0, 3.0, 4.0]]), "zscore")
    desvio = np.sqrt(1.25)  # ÷N, não ÷(N−1)
    np.testing.assert_array_equal(z[0], (np.array([1, 2, 3, 4]) - 2.5) / desvio)


def test_normalizacao_por_concurso_nao_olha_outros_concursos():
    # Mudar um concurso não muda o índice de outro (sem estatística global).
    comp = golden()
    base, pos_base = calcular_indice(comp, "zscore", PESOS_GOLDEN)
    outro = comp.copy()
    outro[1] *= 7
    t2, p2 = calcular_indice(outro, "zscore", PESOS_GOLDEN)
    assert t2[0].tolist() == base[0].tolist()
    assert p2[0].tolist() == pos_base[0].tolist()


def test_pesos_negativos_invertem_a_ordem():
    comp = np.zeros((1, 3, 9))
    comp[0, :, 0] = [1, 2, 3]
    _, pos = calcular_indice(comp, "zscore", [1, 0, 0, 0, 0, 0, 0, 0, 0])
    _, neg = calcular_indice(comp, "zscore", [-1, 0, 0, 0, 0, 0, 0, 0, 0])
    assert pos.tolist() == [[3, 2, 1]]
    assert neg.tolist() == [[1, 2, 3]]


def test_recusa_pesos_nao_finitos_e_forma_errada():
    with pytest.raises(ErroIndice, match="não finitos"):
        calcular_indice(golden(), "zscore", [np.inf] + PESOS_GOLDEN[1:])
    with pytest.raises(ErroIndice, match="forma"):
        calcular_indice(golden(), "zscore", PESOS_GOLDEN[:8])
    with pytest.raises(ErroIndice, match="desconhecida"):
        calcular_indice(golden(), "rank", PESOS_GOLDEN)
