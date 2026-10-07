"""Leitura de perfis de pesos ``perfis/<loteria>/<nome>.toml``.

Mesmas regras do Rust (``src/perfil.rs``; ``docs/PLANEJAMENTO.md``, "Perfil
de pesos"): os 9 pesos obrigatórios e finitos, chave desconhecida é erro,
``janela`` em {0, 10, 50, 100}, ``normalizacao`` ``zscore`` ou ``minmax``,
``versao`` igual ao sufixo ``_v<N>`` do nome, e o hash SHA-256 do texto sem
BOM e com fim de linha LF. Aqui só se lê; quem registra é o ``sync indice``.
"""

from __future__ import annotations

import hashlib
import math
import re
import tomllib
from dataclasses import dataclass
from pathlib import Path

from .dataset import COMPONENTES

JANELAS: tuple[int, ...] = (0, 10, 50, 100)
NORMALIZACOES: tuple[str, ...] = ("zscore", "minmax")
_CHAVES = {"nome", "versao", "loteria", "janela", "normalizacao", "pesos"}
_SUFIXO = re.compile(r"^(.+)_v([1-9][0-9]*)$")


class ErroPerfil(Exception):
    """Perfil ``.toml`` inválido."""


@dataclass(frozen=True)
class Perfil:
    nome: str
    versao: int
    loteria: str
    janela: int
    normalizacao: str
    #: Os 9 pesos na ordem de :data:`COMPONENTES` (a ordem da soma).
    pesos: tuple[float, ...]
    #: SHA-256 do texto com LF, como em ``perfis.hash_toml``.
    hash_toml: str


def normalizar_texto(texto: str) -> str:
    """Tira o BOM e troca CRLF/CR por LF (o texto de onde sai o hash)."""
    if texto.startswith("﻿"):
        texto = texto[1:]
    return texto.replace("\r\n", "\n").replace("\r", "\n")


def _inteiro(valor, campo: str) -> int:
    if isinstance(valor, bool) or not isinstance(valor, int):
        raise ErroPerfil(f"campo `{campo}` deveria ser inteiro, veio {valor!r}")
    return valor


def interpretar_perfil(texto: str, origem: str = "<texto>") -> Perfil:
    texto = normalizar_texto(texto)
    try:
        dados = tomllib.loads(texto)
    except tomllib.TOMLDecodeError as e:
        raise ErroPerfil(f"{origem}: TOML inválido ({e})") from e

    def erro(motivo: str) -> ErroPerfil:
        return ErroPerfil(f"{origem}: {motivo}")

    desconhecidas = sorted(set(dados) - _CHAVES)
    faltando = sorted(_CHAVES - set(dados))
    if desconhecidas:
        raise erro(f"chave(s) desconhecida(s) {desconhecidas}")
    if faltando:
        raise erro(f"faltam as chaves {faltando}")

    nome = dados["nome"]
    loteria = dados["loteria"]
    normalizacao = dados["normalizacao"]
    if not isinstance(nome, str) or not isinstance(loteria, str):
        raise erro("`nome` e `loteria` devem ser texto")
    versao = _inteiro(dados["versao"], "versao")
    janela = _inteiro(dados["janela"], "janela")
    sufixo = _SUFIXO.match(nome)
    if sufixo is None:
        raise erro(f"`nome` {nome!r} não termina com o sufixo de versão `_v<N>`")
    if int(sufixo.group(2)) != versao:
        raise erro(f"`versao` {versao} não bate com o sufixo do nome {nome!r}")
    if janela not in JANELAS:
        raise erro(f"`janela` {janela} fora de {list(JANELAS)}")
    if normalizacao not in NORMALIZACOES:
        raise erro(f"`normalizacao` {normalizacao!r} fora de {list(NORMALIZACOES)}")

    tabela = dados["pesos"]
    if not isinstance(tabela, dict):
        raise erro("`pesos` deveria ser uma tabela")
    desconhecidos = sorted(set(tabela) - set(COMPONENTES))
    ausentes = [c for c in COMPONENTES if c not in tabela]
    if desconhecidos:
        raise erro(f"peso(s) desconhecido(s) {desconhecidos}")
    if ausentes:
        raise erro(f"faltam os pesos {ausentes} (peso 0 é escrito por extenso)")
    pesos = []
    for c in COMPONENTES:
        v = tabela[c]
        if isinstance(v, bool) or not isinstance(v, (int, float)):
            raise erro(f"peso `{c}` deveria ser número, veio {v!r}")
        v = float(v)
        if not math.isfinite(v):
            raise erro(f"peso `{c}` = {v} não é finito")
        pesos.append(v)

    return Perfil(
        nome=nome,
        versao=versao,
        loteria=loteria,
        janela=janela,
        normalizacao=normalizacao,
        pesos=tuple(pesos),
        hash_toml=hashlib.sha256(texto.encode("utf-8")).hexdigest(),
    )


def carregar_perfil(caminho: Path | str) -> Perfil:
    """Lê ``perfis/<loteria>/<nome>.toml`` conferindo pasta e nome do arquivo."""
    caminho = Path(caminho)
    try:
        texto = caminho.read_bytes().decode("utf-8")
    except FileNotFoundError as e:
        raise ErroPerfil(f"perfil não encontrado: {caminho}") from e
    perfil = interpretar_perfil(texto, str(caminho))
    if caminho.stem != perfil.nome:
        raise ErroPerfil(f"{caminho}: `nome` {perfil.nome!r} diferente do nome do arquivo")
    if caminho.parent.name != perfil.loteria:
        raise ErroPerfil(
            f"{caminho}: `loteria` {perfil.loteria!r} diferente da pasta {caminho.parent.name!r}"
        )
    return perfil
