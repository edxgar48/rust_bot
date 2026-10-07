"""Leitura de perfis ``.toml`` (mesmas regras de ``src/perfil.rs``)."""

from __future__ import annotations

import json

import pytest
from conftest import FIXTURES, RAIZ_REPO

from rust_spider_ml.dataset import COMPONENTES
from rust_spider_ml.perfil import ErroPerfil, carregar_perfil, interpretar_perfil

BASE = (RAIZ_REPO / "perfis" / "lotofacil" / "padrao_v1.toml").read_bytes().decode("utf-8")


def test_padrao_v1_e_hash_igual_ao_do_rust():
    p = carregar_perfil(RAIZ_REPO / "perfis" / "lotofacil" / "padrao_v1.toml")
    assert (p.nome, p.versao, p.loteria, p.janela, p.normalizacao) == (
        "padrao_v1",
        1,
        "lotofacil",
        50,
        "zscore",
    )
    assert p.pesos == (1.0, 1.0, 0.3, 0.5, 0.3, -0.8, 1.2, 0.7, 0.4)
    ref = json.loads((FIXTURES / "lotofacil" / "referencia_padrao_v1.json").read_text("utf-8"))
    assert p.hash_toml == ref["hash_toml"]


def test_hash_ignora_bom_e_crlf():
    a = interpretar_perfil(BASE)
    b = interpretar_perfil("﻿" + BASE.replace("\n", "\r\n"))
    assert a.hash_toml == b.hash_toml


@pytest.mark.parametrize(
    "troca, erro",
    [
        (("janela = 50", "janela = 20"), "janela"),
        (('normalizacao = "zscore"', 'normalizacao = "rank"'), "normalizacao"),
        (("versao = 1", "versao = 2"), "sufixo"),
        (("repeticao_atual           = 0.4\n", ""), "faltam os pesos"),
        (("[pesos]", "extra = 1\n[pesos]"), "desconhecida"),
        (("[pesos]", "[pesos]\noutro = 1.0"), "desconhecido"),
        (("frequencia                = 1.0", "frequencia = inf"), "finito"),
    ],
)
def test_validacao_rigida(troca, erro):
    with pytest.raises(ErroPerfil, match=erro):
        interpretar_perfil(BASE.replace(*troca))


def test_pasta_e_nome_conferidos(tmp_path):
    pasta = tmp_path / "quina"
    pasta.mkdir()
    (pasta / "padrao_v1.toml").write_text(BASE, encoding="utf-8")
    with pytest.raises(ErroPerfil, match="pasta"):
        carregar_perfil(pasta / "padrao_v1.toml")


def test_ordem_dos_pesos_e_a_dos_componentes():
    p = interpretar_perfil(
        BASE.replace("frequencia                = 1.0", "frequencia                = 9.0")
    )
    assert p.pesos[COMPONENTES.index("frequencia")] == 9.0
