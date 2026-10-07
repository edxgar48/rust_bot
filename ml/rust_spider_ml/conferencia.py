"""Conferência Python × Rust da avaliação de um perfil.

A referência é o JSON do exemplo ``examples/referencia_avaliacao.rs``: o
mesmo walk-forward e veredito do ``sync avaliar``, com cada ``f64`` em
precisão total (a CLI imprime só 6 casas). O Rust é a referência; uma
divergência é bug do Python.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

from .avaliacao import AvaliacaoFaixa, AvaliacaoPerfil, Comparacao

#: Tolerância absoluta da conferência (``docs/PLANEJAMENTO.md``, 1.5.2).
TOLERANCIA = 1e-12


class DivergenciaRust(Exception):
    """Python e Rust discordam além da tolerância (ou em valor exato)."""


def ler_referencia(caminho: Path | str) -> dict:
    return json.loads(Path(caminho).read_text(encoding="utf-8"))


def _diferencas_comparacao(nome: str, py: Comparacao, rs: dict, saida: dict[str, float]) -> None:
    pares = [
        ("perfil", py.perfil, rs["perfil"]),
        ("acaso", py.acaso, rs["acaso"]),
        ("diferenca", py.diferenca, rs["diferenca"]),
    ]
    if (py.erro_padrao is None) != (rs["erro_padrao"] is None):
        raise DivergenciaRust(f"{nome}: erro padrão presente só de um lado")
    if py.erro_padrao is not None:
        pares += [
            ("erro_padrao", py.erro_padrao, rs["erro_padrao"]),
            ("ic95_inferior", py.ic95[0], rs["ic95"]["inferior"]),
            ("ic95_superior", py.ic95[1], rs["ic95"]["superior"]),
        ]
    for campo, a, b in pares:
        chave = f"{nome}.{campo}"
        saida[chave] = max(saida.get(chave, 0.0), abs(a - b))


def _conferir_faixa(
    rotulo: str, py: AvaliacaoFaixa, rs: dict, diferencas: dict[str, float]
) -> None:
    faixa = (rs["faixa"]["primeiro"], rs["faixa"]["ultimo"])
    if (py.primeiro, py.ultimo) != faixa:
        raise DivergenciaRust(f"{rotulo}: faixa {py.primeiro}–{py.ultimo} × Rust {faixa}")
    met, topk = rs["metricas"], rs["topk"]
    if py.concursos_avaliados != met["concursos_avaliados"]:
        raise DivergenciaRust(f"{rotulo}: T {py.concursos_avaliados} × Rust {met['concursos_avaliados']}")
    if py.topk.k != topk["k"] or py.topk.total_acertos != topk["total_acertos"]:
        raise DivergenciaRust(
            f"{rotulo}: top-{py.topk.k} com {py.topk.total_acertos} acertos × Rust "
            f"top-{topk['k']} com {topk['total_acertos']}"
        )
    obs_py = [(f.acertos, f.observados) for f in py.topk.distribuicao]
    obs_rs = [(f["acertos"], f["observados"]) for f in topk["distribuicao"]]
    if obs_py != obs_rs:
        raise DivergenciaRust(f"{rotulo}: distribuição de acertos {obs_py} × Rust {obs_rs}")
    esperados = max(
        abs(f.esperados - g["esperados"]) for f, g in zip(py.topk.distribuicao, topk["distribuicao"])
    )
    diferencas["topk.distribuicao_esperados"] = max(
        diferencas.get("topk.distribuicao_esperados", 0.0), esperados
    )
    _diferencas_comparacao("log_loss", py.log_loss, met["log_loss"], diferencas)
    _diferencas_comparacao("brier", py.brier, met["brier"], diferencas)
    _diferencas_comparacao("topk", py.topk.acertos, topk["acertos"], diferencas)


def conferir(avaliacao: AvaliacaoPerfil, referencia: dict) -> dict[str, float]:
    """Confere a avaliação Python com a referência do Rust.

    Valores exatos (faixas das dobras, T, k, acertos, distribuição observada,
    perfil, hash) têm de bater; para os ``float`` devolve a maior diferença
    absoluta por métrica (dobras e trecho inteiro). Levanta
    :class:`DivergenciaRust` se algo exato divergir.
    """
    p = avaliacao.perfil
    for campo, py in [
        ("loteria", avaliacao.loteria),
        ("perfil", p.nome),
        ("hash_toml", p.hash_toml),
        ("janela", p.janela),
        ("normalizacao", p.normalizacao),
        ("k", avaliacao.k),
        ("corte", avaliacao.corte),
    ]:
        if referencia[campo] != py:
            raise DivergenciaRust(f"{campo}: {py!r} × Rust {referencia[campo]!r}")
    dobras_rs = referencia["walk_forward"]["dobras"]
    if len(dobras_rs) != len(avaliacao.dobras):
        raise DivergenciaRust(f"{len(avaliacao.dobras)} dobras × Rust {len(dobras_rs)}")
    diferencas: dict[str, float] = {}
    for i, (py, rs) in enumerate(zip(avaliacao.dobras, dobras_rs), start=1):
        _conferir_faixa(f"dobra {i}", py, rs, diferencas)
    _conferir_faixa("trecho inteiro", avaliacao.total, referencia["veredito"]["avaliacao"], diferencas)
    return diferencas


def maior_diferenca(diferencas: dict[str, float]) -> float:
    return max(diferencas.values(), default=0.0)


def dentro_da_tolerancia(diferencas: dict[str, float], tolerancia: float = TOLERANCIA) -> bool:
    return all(math.isfinite(d) and d <= tolerancia for d in diferencas.values())
