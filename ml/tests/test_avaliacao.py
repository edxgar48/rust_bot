"""Calibração, métricas, dobras e top-k (regras da 1c)."""

from __future__ import annotations

import json
import math

import numpy as np
import pytest
from conftest import CORTES_TESTE, FIXTURES, escrever_dataset, linhas_sinteticas

from rust_spider_ml.avaliacao import (
    ErroAvaliacao,
    avaliar_perfil,
    brier,
    calibrar,
    comparar,
    dividir_em_dobras,
    dobras_da_busca,
    hipergeometrica,
    log_loss,
    metricas_por_concurso,
)
from rust_spider_ml.dataset import ler_dataset
from rust_spider_ml.perfil import interpretar_perfil

# --- calibração ---------------------------------------------------------------


def test_calibracao_a_mao():
    # N = 2, K = 1, m = 100. Concurso 0: acaso 1/2. Concurso 1: a posição 1
    # saiu no concurso 0 → (1·2 + 100)/(101·2) = 102/202; posição 2 → 100/202.
    pos = np.array([[1, 2], [2, 1], [1, 2]])
    saiu = np.array([[1, 0], [1, 0], [0, 1]])
    p = calibrar(pos, saiu, 1)
    assert p[0].tolist() == [0.5, 0.5]
    # Concurso 1: dezena 1 está na posição 2 (0 saídas), dezena 2 na 1.
    assert p[1].tolist() == [100 / 202, 102 / 202]
    # Concurso 2: posição 1 saiu 1 vez (conc. 0), posição 2 saiu 1 vez
    # (conc. 1, dezena 1 na posição 2) → as duas (1·2 + 100)/(102·2).
    assert p[2].tolist() == [102 / 204, 102 / 204]


def test_calibracao_soma_k_e_primeiro_recebe_o_acaso():
    g = np.random.default_rng(7)
    t, n, k = 30, 10, 3
    pos = np.array([g.permutation(n) + 1 for _ in range(t)])
    saiu = np.zeros((t, n), dtype=np.int8)
    for i in range(t):
        saiu[i, g.choice(n, k, replace=False)] = 1
    p = calibrar(pos, saiu, k)
    assert (p[0] == k / n).all()
    np.testing.assert_allclose(p.sum(axis=1), k, rtol=0, atol=1e-12)


def test_calibracao_so_usa_o_passado():
    g = np.random.default_rng(3)
    t, n, k = 20, 8, 2
    pos = np.array([g.permutation(n) + 1 for _ in range(t)])
    saiu = np.zeros((t, n), dtype=np.int8)
    for i in range(t):
        saiu[i, g.choice(n, k, replace=False)] = 1
    base = calibrar(pos, saiu, k)
    # Mudar o sorteio de t = 12 (e todo o futuro: sorteios e posições)
    # não muda nenhuma probabilidade até t = 12. As posições de 12 ficam: o
    # índice de 12 vem das features de 12, que só olham até 11.
    saiu2, pos2 = saiu.copy(), pos.copy()
    for i in range(12, t):
        saiu2[i] = np.roll(saiu2[i], 1)
        if i > 12:
            pos2[i] = pos2[i][::-1]
    alterado = calibrar(pos2, saiu2, k)
    np.testing.assert_array_equal(alterado[:13], base[:13])


# --- métricas -----------------------------------------------------------------


def test_log_loss_limita_p_e_brier_nao():
    saiu = np.array([1, 0])
    assert log_loss(np.array([0.0, 1.0]), saiu).tolist() == [
        -math.log(1e-15),
        -math.log(1.0 - (1.0 - 1e-15)),
    ]
    assert brier(np.array([0.0, 1.0]), saiu).tolist() == [1.0, 1.0]
    assert log_loss(np.array([0.25]), np.array([0])).tolist() == [-math.log(0.75)]


def test_metricas_acaso_iguais_ao_perfil_quando_p_e_k_sobre_n():
    saiu = np.array([[1, 0, 0, 0], [0, 0, 1, 0]])
    m = metricas_por_concurso(np.full((2, 4), 0.25), saiu, 1)
    assert m["log_loss_perfil"].tolist() == m["log_loss_acaso"].tolist()
    assert m["brier_perfil"].tolist() == m["brier_acaso"].tolist()


def test_comparar_ep_amostral_e_sem_ep_com_um_concurso():
    c = comparar(np.array([1.0, 2.0, 3.0]), np.array([1.0, 1.0, 1.0]))
    assert (c.perfil, c.acaso, c.diferenca) == (2.0, 1.0, 1.0)
    assert c.erro_padrao == pytest.approx(1.0 / math.sqrt(3), abs=1e-15)
    assert c.ic95 == (1.0 - 1.96 * c.erro_padrao, 1.0 + 1.96 * c.erro_padrao)
    um = comparar(np.array([0.5]), np.array([0.25]))
    assert um.erro_padrao is None and um.ic95 is None


def test_hipergeometrica_lotofacil_tabelada():
    pmf = hipergeometrica(25, 15, 15)
    assert len(pmf) == 16
    assert sum(pmf) == pytest.approx(1.0, abs=1e-15)
    assert sum(i * p for i, p in enumerate(pmf)) == pytest.approx(9.0, abs=1e-12)
    # P(15 acertos) = 1 / C(25, 15).
    assert pmf[15] == 1 / math.comb(25, 15)
    assert pmf[:5] == [0.0] * 5  # mínimo = 15 + 15 − 25 = 5


# --- dobras -------------------------------------------------------------------


def test_dividir_em_dobras_sobra_na_ultima():
    assert dividir_em_dobras(2, 40) == [(2, 8), (9, 15), (16, 22), (23, 29), (30, 40)]
    assert dividir_em_dobras(1, 5) == [(1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]
    with pytest.raises(ErroAvaliacao, match="menos que as 5 dobras"):
        dividir_em_dobras(2, 5)
    with pytest.raises(ErroAvaliacao):
        dividir_em_dobras(1, 10, 0)


def test_dobras_iguais_aos_limites_reais_do_rust():
    # Vetor gerado pelo exemplo `referencia_avaliacao` nos bancos reais
    # (cópias), só com os limites: do 2º concurso do índice ao corte.
    vetor = json.loads((FIXTURES / "dobras_rust.json").read_text(encoding="utf-8"))
    assert set(vetor) == {"lotofacil", "megasena", "quina"}
    for loteria, v in vetor.items():
        dobras = dividir_em_dobras(v["primeiro_indice"] + 1, v["corte"])
        assert [list(d) for d in dobras] == v["dobras"], loteria


def test_dobras_da_busca_da_fixture_iguais_as_do_rust(fixture_lotofacil):
    ds = ler_dataset(fixture_lotofacil / "manifesto_parquet.json")
    ref = json.loads((fixture_lotofacil / "referencia_padrao_v1.json").read_text(encoding="utf-8"))
    rust = [(d["faixa"]["primeiro"], d["faixa"]["ultimo"]) for d in ref["walk_forward"]["dobras"]]
    assert dobras_da_busca(ds) == rust


# --- avaliação de ponta a ponta (sintético) ----------------------------------

PERFIL_TESTE = """
nome = "padrao_v1"
versao = 1
loteria = "teste"
janela = 10
normalizacao = "zscore"

[pesos]
frequencia = 1.0
maior_espaco = 1.0
maior_espaco_ocorrencias = 0.3
menor_espaco = 0.5
menor_espaco_ocorrencias = 0.3
soma_variacoes = -0.8
espaco_final = 1.2
repeticoes_total = 0.7
repeticao_atual = 0.4
"""


def test_avaliacao_sintetica_e_vazamento(tmp_path):
    linhas = linhas_sinteticas(t=12)
    ds = ler_dataset(escrever_dataset(tmp_path / "a", linhas), cortes=CORTES_TESTE)
    perfil = interpretar_perfil(PERFIL_TESTE)
    av = avaliar_perfil(ds, perfil)
    assert [(d.primeiro, d.ultimo) for d in av.dobras] == [(2, 3), (4, 5), (6, 7), (8, 9), (10, 12)]
    assert (av.total.primeiro, av.total.ultimo, av.total.concursos_avaliados) == (2, 12, 11)
    assert av.total.topk.acertos.acaso == pytest.approx(2 * 2 / 6, abs=1e-15)
    assert av.k == 2 and av.n_dezenas == 6

    # Mudar o futuro (features e sorteios a partir do concurso 9) não muda
    # as dobras que terminam antes dele.
    futuro = linhas_sinteticas(t=12, semente=99)
    misturado = [l if l["concurso"] < 9 else f for l, f in zip(linhas, futuro)]
    ds2 = ler_dataset(escrever_dataset(tmp_path / "b", misturado), cortes=CORTES_TESTE)
    av2 = avaliar_perfil(ds2, perfil)
    assert av2.dobras[:3] == av.dobras[:3]


def test_recusa_perfil_de_outra_loteria_e_k_invalido(tmp_path):
    ds = ler_dataset(escrever_dataset(tmp_path, linhas_sinteticas()), cortes=CORTES_TESTE)
    outro = interpretar_perfil(PERFIL_TESTE.replace('"teste"', '"quina"'))
    with pytest.raises(ErroAvaliacao, match="loteria"):
        avaliar_perfil(ds, outro)
    with pytest.raises(ErroAvaliacao, match="k = 6"):
        avaliar_perfil(ds, interpretar_perfil(PERFIL_TESTE), k=6)
