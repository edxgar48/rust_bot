"""Leitura do dataset pelo manifesto: estrutura e recusas."""

from __future__ import annotations

import hashlib
import json
import re
import shutil

import numpy as np
import pyarrow.parquet as pq
import pytest
from conftest import CORTES_TESTE, FIXTURES, RAIZ_REPO, escrever_dataset, linhas_sinteticas

from rust_spider_ml import dataset as dataset_mod
from rust_spider_ml.dataset import COMPONENTES, CORTES_BUSCA, ErroDataset, ler_dataset


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
    ds = ler_dataset(escrever_dataset(tmp_path, linhas, formato=formato), cortes=CORTES_TESTE)
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
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_arquivo_alterado_depois_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path)
    arquivo = tmp_path / "features_v1.csv"
    arquivo.write_bytes(arquivo.read_bytes() + b"\n")
    with pytest.raises(ErroDataset, match="SHA-256"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_numero_de_linhas(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"linhas": 1})
    with pytest.raises(ErroDataset, match="linha"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_ultimo_concurso_acima_do_corte(tmp_path):
    m = escrever_dataset(tmp_path, corte=7)
    with pytest.raises(ErroDataset, match="último concurso 8 acima do corte da busca 7"):
        ler_dataset(m, cortes={"teste": 7})


# --- tabela de cortes (reforço do corte no Python, começo da 1.5.3) ---------


def test_tabela_de_cortes_igual_a_do_rust():
    # `src/config.rs` (a fonte) e `dobras_rust.json` (gerado pelo Rust).
    texto = (RAIZ_REPO / "src" / "config.rs").read_text(encoding="utf-8")
    do_config = {
        slug: int(corte)
        for slug, corte in re.findall(
            r'slug: "(\w+)",.*?ultimo_concurso_busca: (\d+),', texto, flags=re.S
        )
    }
    assert do_config == dict(CORTES_BUSCA)
    dobras = json.loads((FIXTURES / "dobras_rust.json").read_text(encoding="utf-8"))
    assert {k: v["corte"] for k, v in dobras.items()} == dict(CORTES_BUSCA)
    assert dict(CORTES_BUSCA) == {"lotofacil": 3416, "megasena": 2759, "quina": 6421}


def test_tabela_injetada_nao_troca_corte_de_loteria_real(tmp_path, fixture_lotofacil):
    with pytest.raises(ErroDataset, match="tabela de cortes injetada muda o corte de `lotofacil`"):
        ler_dataset(fixture_lotofacil / "manifesto_csv.json", cortes={"lotofacil": 10**9})
    # Acrescentar loteria fictícia (e repetir o corte real) é aceito.
    ds = ler_dataset(
        fixture_lotofacil / "manifesto_csv.json", cortes={"lotofacil": 3416, "teste": 100}
    )
    assert ds.corte == 3416


def test_tabela_de_cortes_nao_muda_em_tempo_de_execucao():
    with pytest.raises(TypeError):
        CORTES_BUSCA["lotofacil"] = 9999  # type: ignore[index]


def _copiar_fixture(origem, destino, **alteracoes):
    destino.mkdir()
    for nome in ("features_v1.csv", "features_v1.parquet"):
        shutil.copyfile(origem / nome, destino / nome)
    for nome in ("manifesto_csv.json", "manifesto_parquet.json"):
        dados = json.loads((origem / nome).read_text(encoding="utf-8"))
        dados.update(alteracoes)
        (destino / nome).write_text(json.dumps(dados, indent=2), encoding="utf-8")
    return destino


@pytest.mark.parametrize("formato", ["csv", "parquet"])
@pytest.mark.parametrize("corte", [3415, 3417, 2759])
def test_recusa_corte_do_manifesto_diferente_da_tabela(tmp_path, fixture_lotofacil, formato, corte):
    # Caminho de produção (tabela padrão): a fixture real da Lotofácil com o
    # corte adulterado no manifesto.
    pasta = _copiar_fixture(fixture_lotofacil, tmp_path / "f", ultimo_concurso_busca=corte)
    with pytest.raises(
        ErroDataset, match=f"corte da busca {corte} no manifesto, mas o corte de `lotofacil` é 3416"
    ):
        ler_dataset(pasta / f"manifesto_{formato}.json")


def test_recusa_corte_diferente_da_tabela_injetada(tmp_path):
    m = escrever_dataset(tmp_path, corte=99)
    with pytest.raises(ErroDataset, match="corte da busca 99 no manifesto, mas o corte de `teste` é 100"):
        ler_dataset(m, cortes=CORTES_TESTE)


@pytest.mark.parametrize("loteria", ["teste", "lotomania", "LOTOFACIL"])
def test_recusa_loteria_desconhecida(tmp_path, loteria):
    # Sem `cortes`, vale a tabela de produção: loteria fora dela é recusada,
    # mesmo com o manifesto todo coerente.
    m = escrever_dataset(tmp_path, loteria=loteria)
    with pytest.raises(ErroDataset, match=f"loteria `{loteria}` desconhecida"):
        ler_dataset(m)


def test_caminho_normal_com_a_tabela_de_producao(tmp_path, fixture_lotofacil):
    pasta = _copiar_fixture(fixture_lotofacil, tmp_path / "f")
    for formato in ("csv", "parquet"):
        ds = ler_dataset(pasta / f"manifesto_{formato}.json")
        assert ds.corte == CORTES_BUSCA["lotofacil"] == ds.manifesto["ultimo_concurso_busca"]
        assert ds.ultimo_concurso <= ds.corte


# --- trancado: recusado antes de abrir/decodificar o arquivo de dados -------


def _sem_decodificar(monkeypatch):
    """Faz qualquer decodificação do arquivo de dados falhar o teste."""

    def proibido(*_args, **_kwargs):
        raise AssertionError("o arquivo de dados foi decodificado antes da recusa")

    monkeypatch.setattr(dataset_mod, "_ler_tabela", proibido)


@pytest.mark.parametrize(
    "extra, nome_manifesto",
    [
        ({}, "manifesto_trancado_csv.json"),
        ({"arquivo": "features_v1_trancado.csv"}, None),
        ({"trecho": "busca+trancado"}, None),
        ({"trecho": "trancado"}, None),
    ],
)
def test_marcas_de_trancado_no_manifesto_recusadas_sem_abrir_o_arquivo(
    tmp_path, extra, nome_manifesto
):
    m = escrever_dataset(tmp_path, manifesto_extra=extra, nome_manifesto=nome_manifesto)
    # Sem o arquivo de dados: se a recusa viesse depois de abri-lo, o erro
    # seria "arquivo de dados não encontrado".
    (tmp_path / "features_v1.csv").unlink()
    with pytest.raises(ErroDataset, match="TRANCADO|só o trecho `busca`"):
        ler_dataset(m, cortes=CORTES_TESTE)


def _linhas_com_trancado():
    linhas = linhas_sinteticas()
    for l in linhas:
        if l["concurso"] == 8:
            l["trecho"] = "trancado"
    return linhas


@pytest.mark.parametrize("formato", ["csv", "parquet"])
def test_recusa_arquivo_trancado_com_manifesto_adulterado_antes_de_decodificar(
    tmp_path, monkeypatch, formato
):
    # Manifesto forjado (diz `busca`, SHA-256 refeito) sobre um arquivo com
    # linhas do trancado: recusado só pelo rodapé (Parquet) ou pelos bytes (CSV).
    m = escrever_dataset(tmp_path, _linhas_com_trancado(), formato=formato)
    _sem_decodificar(monkeypatch)
    with pytest.raises(ErroDataset, match=r"TRANCADO não é lido \(recusado antes de decodificar\)"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_parquet_acima_do_corte_antes_de_decodificar(tmp_path, monkeypatch):
    # O manifesto mente o último concurso (≤ corte), mas o rodapé mostra o 8.
    m = escrever_dataset(tmp_path, formato="parquet", corte=7, manifesto_extra={"ultimo_concurso": 7})
    _sem_decodificar(monkeypatch)
    with pytest.raises(ErroDataset, match="concurso 8 acima do corte da busca 7"):
        ler_dataset(m, cortes={"teste": 7})


def test_recusa_parquet_sem_estatisticas(tmp_path, monkeypatch):
    m = escrever_dataset(tmp_path, formato="parquet")
    # Regrava o Parquet sem estatísticas e refaz o SHA-256 do manifesto.
    arquivo = tmp_path / "features_v1.parquet"
    tabela = pq.read_table(arquivo)
    pq.write_table(tabela, arquivo, compression="none", write_statistics=False)
    dados = json.loads(m.read_text(encoding="utf-8"))
    dados["sha256"] = hashlib.sha256(arquivo.read_bytes()).hexdigest()
    m.write_text(json.dumps(dados), encoding="utf-8")
    _sem_decodificar(monkeypatch)
    with pytest.raises(ErroDataset, match="sem estatísticas da coluna `trecho`"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_linha_do_trancado_ainda_recusada_depois_de_decodificar(tmp_path, monkeypatch):
    # Defesa em profundidade: sem a checagem prévia, a conferência linha a
    # linha continua recusando.
    monkeypatch.setattr(dataset_mod, "_recusar_trancado_sem_decodificar", lambda *a: None)
    with pytest.raises(ErroDataset, match="trecho diferente de `busca`.*concurso 8"):
        ler_dataset(escrever_dataset(tmp_path, _linhas_com_trancado()), cortes=CORTES_TESTE)


def test_recusa_feature_version_da_coluna_diferente_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"feature_version": 2})
    with pytest.raises(ErroDataset, match="feature_version"):
        ler_dataset(m, cortes=CORTES_TESTE)


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
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)


def test_recusa_concurso_sem_todas_as_dezenas(tmp_path):
    linhas = [l for l in linhas_sinteticas() if not (l["concurso"] == 4 and l["dezena"] == 6)]
    with pytest.raises(ErroDataset, match="todas as dezenas"):
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)


def test_recusa_buraco(tmp_path):
    linhas = [l for l in linhas_sinteticas() if l["concurso"] != 4]
    with pytest.raises(ErroDataset, match="buraco entre os concursos 3 e 5"):
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)


def test_recusa_linhas_fora_de_ordem(tmp_path):
    linhas = linhas_sinteticas()
    linhas[0], linhas[1] = linhas[1], linhas[0]
    with pytest.raises(ErroDataset, match="fora de ordem"):
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)


def test_recusa_saiu_diferente_entre_janelas(tmp_path):
    linhas = linhas_sinteticas()
    # Troca o `saiu` de uma dezena só na janela 10, mantendo K na janela 0.
    for l in linhas:
        if (l["concurso"], l["dezena"], l["janela"]) == (2, 1, 10):
            l["saiu"] = 1 - l["saiu"]
    with pytest.raises(ErroDataset, match="entre as janelas"):
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)


def test_recusa_impressao_diferente(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"impressao_resultados_incluidos": "fnv1a64:8:0"})
    with pytest.raises(ErroDataset, match="impressão"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_colunas_do_manifesto(tmp_path):
    m = escrever_dataset(tmp_path, manifesto_extra={"colunas": ["concurso"]})
    with pytest.raises(ErroDataset, match="colunas"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_campo_faltando_no_manifesto(tmp_path):
    m = escrever_dataset(tmp_path)
    dados = json.loads(m.read_text(encoding="utf-8"))
    del dados["sha256"]
    m.write_text(json.dumps(dados), encoding="utf-8")
    with pytest.raises(ErroDataset, match="faltam os campos"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_metadado_do_parquet_diferente(tmp_path):
    m = escrever_dataset(tmp_path, formato="parquet", metadados_extra={"trecho": "busca+trancado"})
    with pytest.raises(ErroDataset, match="metadado `trecho`"):
        ler_dataset(m, cortes=CORTES_TESTE)


def test_recusa_dataset_que_nao_comeca_no_concurso_1(tmp_path):
    linhas = linhas_sinteticas()
    for l in linhas:
        l["concurso"] += 10
    with pytest.raises(ErroDataset, match="começa no concurso 11"):
        ler_dataset(escrever_dataset(tmp_path, linhas), cortes=CORTES_TESTE)
