"""Leitura e conferência dos vetores de ``testes/conformidade/``.

Cada JSON traz a entrada (resultados ou features), o perfil e o esperado do
Rust. Aqui as features vêm da implementação de referência
(:mod:`referencia_features`) e o índice de :mod:`indice`; a comparação é
exata: inteiros e posições iguais, ``total`` igual bit a bit (o mesmo
``f64``). Formato em ``src/conformidade.rs``.
"""

from __future__ import annotations

import json
import struct
from pathlib import Path

import numpy as np

from .indice import calcular_indice, variacao_posicao
from .referencia_features import COLUNAS_FEATURES, features_de_resultados

VERSAO_FORMATO = 1
COLUNAS_INDICE = ("concurso", "dezena", "total", "posicao", "variacao_posicao")
#: Componentes na ordem dos pesos e das colunas 3 … 11 de ``COLUNAS_FEATURES``.
COMPONENTES = COLUNAS_FEATURES[3:12]


class ErroConformidade(Exception):
    """Vetor malformado ou de formato desconhecido."""


def ler_caso(caminho: Path) -> dict:
    """Lê o JSON de um caso e confere o formato."""
    caso = json.loads(Path(caminho).read_text(encoding="utf-8"))
    if caso.get("versao_formato") != VERSAO_FORMATO:
        raise ErroConformidade(f"{caminho}: versao_formato {caso.get('versao_formato')!r}")
    if tuple(caso["entrada"].get("colunas_features", COLUNAS_FEATURES)) != COLUNAS_FEATURES:
        raise ErroConformidade(f"{caminho}: colunas de features inesperadas")
    if tuple(caso["esperado"]["colunas_indice"]) != COLUNAS_INDICE:
        raise ErroConformidade(f"{caminho}: colunas do índice inesperadas")
    return caso


def features_do_caso(caso: dict) -> list[tuple]:
    """Features que a referência em Python calcula (resultados) ou a entrada
    do próprio caso (features), como tuplas de ``COLUNAS_FEATURES``."""
    entrada = caso["entrada"]
    if entrada["tipo"] == "resultados":
        q = caso["loteria"]
        return features_de_resultados(entrada["resultados"], q["dezena_min"], q["dezena_max"])
    if entrada["tipo"] == "features":
        return [tuple(l) for l in entrada["features"]]
    raise ErroConformidade(f"tipo de entrada desconhecido: {entrada['tipo']!r}")


def indice_de_features(features: list[tuple], perfil: dict) -> list[tuple]:
    """Índice (``COLUNAS_INDICE``) das linhas da janela do perfil, ordenadas
    por (concurso, dezena). Normalização por concurso, como no Rust."""
    janela = perfil["janela"]
    linhas = sorted((l for l in features if l[2] == janela), key=lambda l: (l[0], l[1]))
    if not linhas:
        raise ErroConformidade(f"nenhuma linha da janela {janela}")
    concursos = sorted({l[0] for l in linhas})
    dezenas = sorted({l[1] for l in linhas})
    if len(linhas) != len(concursos) * len(dezenas):
        raise ErroConformidade("a série de features não é completa")
    if concursos != list(range(concursos[0], concursos[0] + len(concursos))):
        raise ErroConformidade("buraco entre os concursos das features")
    valores = np.array(
        [[np.nan if x is None else x for x in l[3:12]] for l in linhas], dtype=np.float64
    ).reshape(len(concursos), len(dezenas), 9)
    pesos = [perfil["pesos"][nome] for nome in COMPONENTES]
    totais, posicoes = calcular_indice(valores, perfil["normalizacao"], pesos)
    variacoes = variacao_posicao(posicoes)
    saida = []
    for ti, concurso in enumerate(concursos):
        for di, dezena in enumerate(dezenas):
            variacao = None if ti == 0 else int(variacoes[ti - 1, di])
            saida.append((concurso, dezena, float(totais[ti, di]), int(posicoes[ti, di]), variacao))
    return saida


def filtrar(linhas: list[tuple], conferidos: list[int] | None) -> list[tuple]:
    """Só as linhas dos concursos conferidos (``None`` = todas)."""
    if conferidos is None:
        return list(linhas)
    permitidos = set(conferidos)
    return [l for l in linhas if l[0] in permitidos]


def bits(x: float) -> bytes:
    """Os 8 bytes do ``f64`` (``0.0`` e ``-0.0`` diferem)."""
    return struct.pack("<d", x)


def diferencas(obtido: list[tuple], esperado: list, colunas: tuple[str, ...]) -> list[str]:
    """Descrição das diferenças entre linhas; vazio = iguais. Floats
    comparados bit a bit, o resto por igualdade exata (e mesmo tipo: um
    ``True`` não passa por ``1``)."""
    if len(obtido) != len(esperado):
        return [f"{len(obtido)} linhas, esperadas {len(esperado)}"]
    problemas = []
    for o, e in zip(obtido, esperado):
        e = tuple(e)
        if len(o) != len(e):
            problemas.append(f"linha {e[:2]}: {len(o)} colunas, esperadas {len(e)}")
            continue
        for nome, vo, ve in zip(colunas, o, e):
            if isinstance(ve, float) or isinstance(vo, float):
                igual = isinstance(vo, float) and isinstance(ve, float) and bits(vo) == bits(ve)
            else:
                igual = vo == ve and type(vo) is type(ve)
            if not igual:
                problemas.append(f"linha {e[:2]} coluna {nome}: obtido {vo!r}, esperado {ve!r}")
    return problemas
