"""Vetores de conformidade Rust ↔ Python (1.5.4).

Os mesmos JSON de ``testes/conformidade/`` que o ``cargo test`` confere
contra o motor são conferidos aqui contra a referência em Python:

- resultados → features (inteiros exatos, ``NULL`` = ``None``);
- features → índice (``total`` igual bit a bit, posição e variação exatas).

Divergência: o Rust é a referência; corrige-se o Python.
"""

from __future__ import annotations

import copy
import struct

import pytest
from conftest import RAIZ_REPO

from rust_spider_ml.conformidade import (
    COLUNAS_INDICE,
    ErroConformidade,
    diferencas,
    features_do_caso,
    filtrar,
    indice_de_features,
    ler_caso,
)
from rust_spider_ml.referencia_features import (
    COLUNAS_FEATURES,
    FEATURE_VERSION,
    componentes,
    features_de_resultados,
)

PASTA = RAIZ_REPO / "testes" / "conformidade"
ARQUIVOS = sorted(PASTA.glob("*.json"))
IDS = [a.name for a in ARQUIVOS]


def _casos(entrada: str | None = None) -> list:
    casos = []
    for arquivo in ARQUIVOS:
        caso = ler_caso(arquivo)
        if entrada is None or caso["entrada"]["tipo"] == entrada:
            casos.append(pytest.param(caso, id=arquivo.name))
    return casos


def test_ha_vetores_dos_dois_tipos():
    assert len(ARQUIVOS) >= 10, f"vetores em {PASTA}"
    tipos = {ler_caso(a)["entrada"]["tipo"] for a in ARQUIVOS}
    assert tipos == {"resultados", "features"}


@pytest.mark.parametrize("caso", _casos("resultados"))
def test_resultados_para_features(caso):
    obtido = filtrar(features_do_caso(caso), caso["esperado"]["concursos_conferidos"])
    dif = diferencas(obtido, caso["esperado"]["features"], COLUNAS_FEATURES)
    assert not dif, dif[:5]


@pytest.mark.parametrize("caso", _casos())
def test_features_para_indice(caso):
    # Resultados: o índice sai das features da referência (cadeia completa);
    # features: sai da própria entrada.
    indice = indice_de_features(features_do_caso(caso), caso["perfil"])
    obtido = filtrar(indice, caso["esperado"]["concursos_conferidos"])
    dif = diferencas(obtido, caso["esperado"]["indice"], COLUNAS_INDICE)
    assert not dif, dif[:5]


def _casos_completos():
    """Casos de resultados cujo esperado traz todos os concursos (sem filtro)."""
    return [
        pytest.param(p.values[0], id=p.id)
        for p in _casos("resultados")
        if p.values[0]["esperado"]["concursos_conferidos"] is None
    ]


@pytest.mark.parametrize("caso", _casos_completos())
def test_indice_a_partir_das_features_esperadas_do_rust(caso):
    """Só o passo features → índice, partindo das features que o Rust
    calculou, sem passar pela referência das features."""
    esperado = [tuple(l) for l in caso["esperado"]["features"]]
    indice = indice_de_features(esperado, caso["perfil"])
    dif = diferencas(indice, caso["esperado"]["indice"], COLUNAS_INDICE)
    assert not dif, dif[:5]


def test_total_e_comparado_bit_a_bit():
    obtido = [(1, 1, 0.3, 1, None)]
    assert not diferencas(obtido, [[1, 1, 0.3, 1, None]], COLUNAS_INDICE)
    proximo = struct.unpack("<d", struct.pack("<q", struct.unpack("<q", struct.pack("<d", 0.3))[0] + 1))[0]
    assert diferencas(obtido, [[1, 1, proximo, 1, None]], COLUNAS_INDICE), "1 ulp de diferença"
    # 0.0 e -0.0 são iguais como número, mas não nos bits.
    assert diferencas([(1, 1, 0.0, 1, None)], [[1, 1, -0.0, 1, None]], COLUNAS_INDICE)
    # Posição é inteiro: 1.0 não vale por 1.
    assert diferencas([(1, 1, 0.3, 1, None)], [[1, 1, 0.3, 1.0, None]], COLUNAS_INDICE)


@pytest.mark.parametrize("caso", _casos())
def test_o_json_e_o_formato_conhecido(caso):
    assert caso["feature_version"] == FEATURE_VERSION
    assert caso["descricao"]
    perfil = caso["perfil"]
    assert perfil["normalizacao"] in {"zscore", "minmax"}
    assert perfil["janela"] in {0, 10, 50, 100}
    assert len(perfil["pesos"]) == 9
    assert all(isinstance(v, float) for v in perfil["pesos"].values())
    q = caso["loteria"]
    assert 1 <= q["qtd_dezenas"] <= q["dezena_max"] - q["dezena_min"] + 1
    if caso["entrada"]["tipo"] == "resultados":
        assert all(len(set(d)) == q["qtd_dezenas"] for d in caso["entrada"]["resultados"])


def test_formato_desconhecido_e_recusado(tmp_path):
    caso = copy.deepcopy(ler_caso(ARQUIVOS[0]))
    caso["versao_formato"] = 2
    import json

    arquivo = tmp_path / "v2.json"
    arquivo.write_text(json.dumps(caso), encoding="utf-8")
    with pytest.raises(ErroConformidade):
        ler_caso(arquivo)


# --- Referência das features: contas feitas à mão, sem o JSON -------------


def test_golden_do_plano_na_referencia():
    # Aparições 3, 4, 8, 9, 10, 15; t = 20 (docs/PLANEJAMENTO.md).
    aparicoes = [3, 4, 8, 9, 10, 15]
    assert componentes(aparicoes, 20, 0) == (6, 4, 1, 0, 3, 10, 4, 3, 0)
    # Janela 10 = concursos 10 … 19: aparições 10 e 15, um espaço de 4.
    assert componentes(aparicoes, 20, 10) == (2, 4, 1, 4, 1, None, 4, 0, 0)
    # Sem história: tudo NULL, exceto os contadores.
    assert componentes([], 1, 0) == (0, None, None, None, None, None, None, 0, 0)
    assert componentes([4], 5, 0) == (1, None, None, None, None, None, 0, 0, 1)


def test_fronteiras_da_janela_na_referencia():
    aparicoes = [1, 40, 41]
    # Janela 10: t = 51 vê 41 … 50 (só a 41); t = 52 vê 42 … 51 (nenhuma).
    assert componentes(aparicoes, 51, 10)[0] == 1
    assert componentes(aparicoes, 52, 10)[0] == 0
    # O espaço final não é limitado pela janela.
    assert componentes(aparicoes, 52, 10)[6] == 10
    assert componentes(aparicoes, 50, 10)[0] == 2


def test_alvo_e_snapshot_seguinte():
    linhas = features_de_resultados([[1, 2], [2, 3]], 1, 3)
    assert {l[0] for l in linhas} == {1, 2, 3}
    saiu = {(l[0], l[1]): l[12] for l in linhas if l[2] == 0}
    assert saiu[(1, 1)] == 1 and saiu[(1, 3)] == 0
    assert saiu[(2, 3)] == 1 and saiu[(2, 1)] == 0
    assert all(v is None for (c, _), v in saiu.items() if c == 3)


def test_referencia_recusa_aparicao_do_futuro():
    with pytest.raises(ValueError):
        componentes([3, 20], 20, 0)


@pytest.mark.parametrize("caso", _casos("resultados"))
def test_ponto_no_tempo_features_de_t_nao_veem_o_futuro(caso):
    """Alterar ou cortar os sorteios a partir de m + 1 não muda as features
    dos concursos ≤ m + 1 (o alvo `saiu` de m + 1 é que pode mudar)."""
    q = caso["loteria"]
    sorteios = caso["entrada"]["resultados"]
    completo = features_do_caso(caso)
    m = len(sorteios) // 2
    alterado = [list(d) for d in sorteios]
    faixa = list(range(q["dezena_min"], q["dezena_max"] + 1))
    for i in range(m, len(alterado)):
        # outro sorteio qualquer, válido, só nos concursos m + 1 em diante
        alterado[i] = faixa[i % len(faixa) :] + faixa[: i % len(faixa)]
        alterado[i] = alterado[i][: q["qtd_dezenas"]]
    outro = features_de_resultados(alterado, q["dezena_min"], q["dezena_max"])

    def sem_alvo(linhas):
        return [l[:-1] for l in linhas if l[0] <= m + 1]

    assert sem_alvo(completo) == sem_alvo(outro)
