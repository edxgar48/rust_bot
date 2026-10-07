"""Python × Rust na fixture gerada pelo ``sync dataset``.

Duas referências, ambas geradas por ``ml/ferramentas/gerar_fixture.py``:

- ``referencia_<perfil>.json``: o walk-forward e o veredito do Rust em
  precisão total (``examples/referencia_avaliacao.rs``) → tolerância 1e-12;
- ``avaliar_<perfil>.txt``: a saída da CLI ``sync avaliar`` → as tabelas
  impressas têm de sair iguais, texto a texto, com a mesma formatação.
"""

from __future__ import annotations

import re

import pytest
from conftest import FIXTURES, RAIZ_REPO

from rust_spider_ml.avaliacao import avaliar_perfil
from rust_spider_ml.conferencia import (
    TOLERANCIA,
    conferir,
    dentro_da_tolerancia,
    ler_referencia,
    maior_diferenca,
)
from rust_spider_ml.dataset import ler_dataset
from rust_spider_ml.perfil import carregar_perfil

PERFIS = {
    "padrao_v1": RAIZ_REPO / "perfis" / "lotofacil" / "padrao_v1.toml",
    "teste_minmax_v1": FIXTURES / "perfis" / "lotofacil" / "teste_minmax_v1.toml",
}


def _avaliar(formato: str, perfil: str):
    ds = ler_dataset(FIXTURES / "lotofacil" / f"manifesto_{formato}.json")
    return avaliar_perfil(ds, carregar_perfil(PERFIS[perfil]))


@pytest.mark.parametrize("formato", ["csv", "parquet"])
@pytest.mark.parametrize("perfil", sorted(PERFIS))
def test_igual_a_referencia_do_rust(formato, perfil):
    av = _avaliar(formato, perfil)
    ref = ler_referencia(FIXTURES / "lotofacil" / f"referencia_{perfil}.json")
    dif = conferir(av, ref)
    assert dif, "nenhuma métrica conferida"
    assert dentro_da_tolerancia(dif), f"maior diferença {maior_diferenca(dif)} > {TOLERANCIA}"


def _rust_exp(x: float) -> str:
    """``{:+.3e}`` do Rust: expoente sem ``+`` nem zeros à esquerda."""
    mantissa, exp = f"{x:+.3e}".split("e")
    return f"{mantissa}e{int(exp)}"


@pytest.mark.parametrize("perfil", sorted(PERFIS))
def test_tabelas_iguais_a_saida_do_sync_avaliar(perfil):
    av = _avaliar("parquet", perfil)
    texto = (FIXTURES / "lotofacil" / f"avaliar_{perfil}.txt").read_text(encoding="utf-8")
    secoes = re.split(r"\n\n", texto)
    tabela = {
        nome: next(s for s in secoes if s.startswith(prefixo)).splitlines()[2:]
        for nome, prefixo in [("log_loss", "Log loss"), ("brier", "Brier"), ("topk", "Top-")]
    }
    faixas = list(av.dobras) + [av.total]
    rotulos = [str(i) for i in range(1, len(av.dobras) + 1)] + ["total"]
    for metrica in ("log_loss", "brier", "topk"):
        linhas = tabela[metrica]
        assert len(linhas) == len(faixas)
        for rotulo, f, linha in zip(rotulos, faixas, linhas):
            campos = linha.split()
            assert campos[0] == rotulo
            assert campos[1] == f"{f.primeiro}–{f.ultimo}"
            assert int(campos[2]) == f.concursos_avaliados
            if metrica == "topk":
                c = f.topk.acertos
                esperado = [f"{c.perfil:.4f}", f"{c.acaso:.4f}", f"{c.diferenca:+.4f}", f"{c.erro_padrao:.4f}"]
            else:
                c = getattr(f, metrica)
                esperado = [
                    f"{c.perfil:.6f}",
                    f"{c.acaso:.6f}",
                    _rust_exp(c.diferenca),
                    _rust_exp(c.erro_padrao).lstrip("+"),
                ]
            assert campos[3:7] == esperado, (perfil, metrica, rotulo)
