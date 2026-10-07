"""Leitura do dataset pelo manifesto: estrutura e recusas."""

from __future__ import annotations

import json

import numpy as np
import pytest
from conftest import escrever_dataset, linhas_sinteticas

from rust_spider_ml.dataset import COMPONENTES, ErroDataset, ler_dataset


# --- fixture gerada pelo `sync dataset` -------------------------------------


def test_csv_e_parquet_da_fixture_dao_a_mesma_estrutura(fixture_lotofacil):
    csv = ler_dataset(fixture_lotofacil / "manifesto_csv.json")
    par = ler_dataset(fixture_lotofacil / "manifesto_parquet.json")
    assert (csv.formato, par.formato) == ("csv", "parquet")
    for ds in (csv, par):
        assert ds.loteria == "lotofacil"
        assert ds.feature_version == 1
        assert ds.corte == 3416
        assert ds.janelas == (0, 10, 50, 100)
        assert ds.concursos.tolist() == list(range(1, 41))
        assert ds.dezenas.tolist() == list(range(1, 26))
        # K e N saem do dataset, não de tabela.
        assert (ds.k_sorteadas, ds.n_dezenas) == (15, 25)
        assert (ds.saiu.sum(axis=1) == 15).all()
    np.testing.assert_array_equal(csv.saiu, par.saiu)
    assert csv.datas == par.datas
    for j in csv.janelas:
        a, b = csv.componentes_da_janela(j), par.componentes_da_janela(j)
        assert a.shape == (40, 25, len(COMPONENTES))
        np.testing.assert_array_equal(np.isnan(a), np.isnan(b))
        np.testing.assert_array_equal(np.nan_to_num(a, nan=-1), np.nan_to_num(b, nan=-1))


def test_fixture_tem_null_no_inicio(fixture_lotofacil):
    ds = ler_dataset(fixture_lotofacil / "manifesto_csv.json")
    comp = ds.componentes_da_janela(0)
    # Concurso 1: nada antes → espaços NULL; frequência 0 (obrigatória).
    assert np.isnan(comp[0, :, COMPONENTES.index("espaco_final")]).all()
    assert (comp[0, :, COMPONENTES.index("frequencia")] == 0).all()


def test_feature_version_esperada(fixture_lotofacil):
    ler_dataset(fixture_lotofacil / "manifesto_csv.json", feature_version=1)
    with pytest.raises(ErroDataset, match="feature_version 1, esperada 2"):
        ler_dataset(fixture_lotofacil / "manifesto_csv.json", feature_version=2)


# --- sintéticos --------------------------------------------------------------


@pytest.mark.parametrize("formato", ["csv", "parquet"])
def test_sintetico_le_nos_dois_formatos(tmp_path, formato):
    linhas = linhas_sinteticas()
    ds = ler_dataset(escrever_dataset(tmp_path, linhas, formato=formato))
    assert (ds.n_dezenas, ds.k_sorteadas) == (6, 2)
    assert ds.janelas == (0, 10)
    # Um valor conferido campo a campo, inclusive NULL → NaN.
    l = next(x for x in linhas if (x["concurso"], x["dezena"], x["janela"]) == (3, 4, 10))
    comp = ds.componentes_da_janela(10)[2, 3]
    for i, c in enumerate(COMPONENTES):
        assert (np.isnan(comp[i]) if l[c] is None else comp[i] == l[c]), c


def test_recusa_sha256_diferente(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"sha256": "0" * 64})
    with pytest.raises(ErroDataset, match="SHA-256"):
        ler_dataset(m)


def test_recusa_arquivo_alterado_depois_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path)
    arquivo = tmp_path / "features_v1.csv"
    arquivo.write_bytes(arquivo.read_bytes() + b"\n")
    with pytest.raises(ErroDataset, match="SHA-256"):
        ler_dataset(m)


def test_recusa_numero_de_linhas(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"linhas": 1})
    with pytest.raises(ErroDataset, match="linha"):
        ler_dataset(m)


def test_recusa_ultimo_concurso_acima_do_corte(tmp_path):
    m = escrever_dataset(tmp_path, corte=7)
    with pytest.raises(ErroDataset, match="acima do corte"):
        ler_dataset(m)


def test_recusa_linha_do_trancado(tmp_path):
    linhas = linhas_sinteticas()
    for l in linhas:
        if l["concurso"] == 8:
            l["trecho"] = "trancado"
    with pytest.raises(ErroDataset, match="trecho diferente de `busca`.*concurso 8"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_manifesto_trancado(tmp_path):
    m = escrever_dataset(tmp_path, nome_manifesto="manifesto_trancado_csv.json")
    with pytest.raises(ErroDataset, match="TRANCADO"):
        ler_dataset(m)


def test_recusa_manifesto_que_aponta_para_arquivo_trancado(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"arquivo": "features_v1_trancado.csv"})
    with pytest.raises(ErroDataset, match="TRANCADO"):
        ler_dataset(m)


def test_recusa_trecho_busca_mais_trancado(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"trecho": "busca+trancado"})
    with pytest.raises(ErroDataset, match="só o trecho `busca`"):
        ler_dataset(m)


def test_recusa_feature_version_da_coluna_diferente_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"feature_version": 2})
    with pytest.raises(ErroDataset, match="feature_version"):
        ler_dataset(m)


def test_recusa_k_diferente_entre_concursos(tmp_path):
    linhas = linhas_sinteticas()
    for l in linhas:
        if l["concurso"] == 5 and l["saiu"] == 0:
            alvo = l["dezena"]
            break
    for l in linhas:
        if l["concurso"] == 5 and l["dezena"] == alvo:
            l["saiu"] = 1
    with pytest.raises(ErroDataset, match="concurso 5: 3 dezena"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_concurso_sem_todas_as_dezenas(tmp_path):
    linhas = [l for l in linhas_sinteticas() if not (l["concurso"] == 4 and l["dezena"] == 6)]
    with pytest.raises(ErroDataset, match="todas as dezenas"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_buraco(tmp_path):
    linhas = [l for l in linhas_sinteticas() if l["concurso"] != 4]
    with pytest.raises(ErroDataset, match="buraco entre os concursos 3 e 5"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_linhas_fora_de_ordem(tmp_path):
    linhas = linhas_sinteticas()
    linhas[0], linhas[1] = linhas[1], linhas[0]
    with pytest.raises(ErroDataset, match="fora de ordem"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_saiu_diferente_entre_janelas(tmp_path):
    linhas = linhas_sinteticas()
    # Troca o `saiu` de uma dezena só na janela 10, mantendo K na janela 0.
    for l in linhas:
        if (l["concurso"], l["dezena"], l["janela"]) == (2, 1, 10):
            l["saiu"] = 1 - l["saiu"]
    with pytest.raises(ErroDataset, match="entre as janelas"):
        ler_dataset(escrever_dataset(tmp_path, linhas))


def test_recusa_impressao_diferente(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"impressao_resultados_incluidos": "fnv1a64:8:0"})
    with pytest.raises(ErroDataset, match="impressão"):
        ler_dataset(m)


def test_recusa_colunas_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"colunas": ["concurso"]})
    with pytest.raises(ErroDataset, match="colunas"):
        ler_dataset(m)


def test_recusa_campo_faltando_no_manifesto(tmp_path):
    m = escrever_dataset(tmp_path)
    dados = json.loads(m.read_text(encoding="utf-8"))
    del dados["sha256"]
    m.write_text(json.dumps(dados), encoding="utf-8")
    with pytest.raises(ErroDataset, match="faltam os campos"):
        ler_dataset(m)


def test_recusa_metadado_do_parquet_diferente(tmp_path):
    m = escrever_dataset(tmp_path, formato="parquet", metadados_extra={"trecho": "busca+trancado"})
    with pytest.raises(ErroDataset, match="metadado `trecho`"):
        ler_dataset(m)


def test_recusa_dataset_que_nao_comeca_no_concurso_1(tmp_path):
    linhas = linhas_sinteticas()
    for l in linhas:
        l["concurso"] += 10
    with pytest.raises(ErroDataset, match="começa no concurso 11"):
        ler_dataset(escrever_dataset(tmp_path, linhas))
