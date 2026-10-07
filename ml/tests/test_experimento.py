"""O experimento 001 roda na fixture, confere com o Rust e é determinístico."""

from __future__ import annotations

import importlib.util

from conftest import FIXTURES, RAIZ_REPO

CAMINHO = RAIZ_REPO / "ml" / "experimentos" / "001_baseline.py"


def _modulo():
    spec = importlib.util.spec_from_file_location("experimento_001", CAMINHO)
    modulo = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(modulo)
    return modulo


def test_001_baseline_deterministico_e_confere(capsys):
    exp = _modulo()
    args = [
        str(FIXTURES / "lotofacil" / "manifesto_parquet.json"),
        str(FIXTURES / "lotofacil" / "manifesto_csv.json"),
        "--referencia",
        str(FIXTURES / "lotofacil" / "referencia_padrao_v1.json"),
    ]
    assert exp.main(args) == 0
    primeira = capsys.readouterr().out
    assert exp.main(args) == 0
    assert capsys.readouterr().out == primeira
    assert primeira.count("Conferência com o Rust (tolerância 1e-12): OK") == 2
    assert "N = 25 dezenas, K = 15 sorteadas" in primeira
