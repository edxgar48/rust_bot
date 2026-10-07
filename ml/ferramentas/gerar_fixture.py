"""Regenera as fixtures de ``ml/tests/fixtures/`` com o próprio Rust.

1. Cria, numa pasta temporária, um banco sintético da Lotofácil com
   ``CONCURSOS`` concursos sorteados por um gerador com semente fixa (só a
   tabela ``resultados``, com o esquema de ``src/db.rs``).
2. Roda a CLI de verdade: ``sync features``, ``sync indice`` (``padrao_v1``
   de ``perfis/`` e ``teste_minmax_v1`` de ``ml/tests/fixtures/perfis/``),
   ``sync dataset`` em CSV e Parquet e ``sync avaliar`` (texto da CLI).
3. Grava a referência em precisão total de cada perfil com
   ``cargo run --example referencia_avaliacao``.
4. Com ``--bancos PASTA`` (CÓPIAS dos bancos reais, nunca os originais),
   grava também ``dobras_rust.json``: os limites das 5 dobras que o Rust usa
   em cada loteria real (só o trecho de busca; o trancado não é lido).

Uso, a partir da raiz do repositório::

    ml/.venv/Scripts/python ml/ferramentas/gerar_fixture.py [--bancos PASTA]

Mesmos binários → mesmos bytes (o banco é sintético e determinístico).
"""

from __future__ import annotations

import argparse
import json
import random
import shutil
import sqlite3
import subprocess
import sys
import tempfile
from datetime import date, timedelta
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
FIXTURES = RAIZ / "ml" / "tests" / "fixtures"
PERFIS_TESTE = FIXTURES / "perfis"
LOTERIA = "lotofacil"
DEZENAS = range(1, 26)
SORTEADAS = 15
CONCURSOS = 40
SEMENTE = 20261006
#: (perfil, pasta de perfis)
PERFIS = [("padrao_v1", RAIZ / "perfis"), ("teste_minmax_v1", PERFIS_TESTE)]
LOTERIAS_REAIS = ("lotofacil", "megasena", "quina")


def rodar(*args: str) -> str:
    r = subprocess.run(
        ["cargo", "run", "--release", "--quiet", *args],
        cwd=RAIZ,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    if r.returncode != 0:
        sys.exit(f"falhou: cargo run {' '.join(args)}\n{r.stdout}\n{r.stderr}")
    return r.stdout


def criar_banco(caminho: Path) -> None:
    gerador = random.Random(SEMENTE)
    con = sqlite3.connect(caminho)
    with con:
        con.execute(
            """CREATE TABLE IF NOT EXISTS resultados (
            loteria      TEXT NOT NULL,
            concurso     INTEGER NOT NULL,
            data_sorteio TEXT NOT NULL,
            dezenas      TEXT NOT NULL,
            PRIMARY KEY (loteria, concurso)
        );"""
        )
        inicio = date(2020, 1, 2)
        for c in range(1, CONCURSOS + 1):
            dezenas = sorted(gerador.sample(list(DEZENAS), SORTEADAS))
            con.execute(
                "INSERT INTO resultados VALUES (?, ?, ?, ?)",
                (
                    LOTERIA,
                    c,
                    (inicio + timedelta(days=3 * (c - 1))).isoformat(),
                    json.dumps(dezenas, separators=(",", ":")),
                ),
            )
    con.close()


def gerar_sintetica() -> None:
    destino = FIXTURES / LOTERIA
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        banco = tmp / f"resultados_{LOTERIA}.db"
        criar_banco(banco)
        rodar("--bin", "sync", "--", "features", LOTERIA, "--db", str(banco))
        for perfil, pasta in PERFIS:
            rodar(
                "--bin", "sync", "--", "indice", LOTERIA,
                "--perfil", perfil, "--perfis", str(pasta), "--db", str(banco),
            )
        for formato in ("csv", "parquet"):
            rodar(
                "--bin", "sync", "--", "dataset", LOTERIA, "--formato", formato,
                "--saida", str(tmp / "datasets"), "--db", str(banco),
            )
        if destino.exists():
            shutil.rmtree(destino)
        shutil.copytree(tmp / "datasets" / LOTERIA, destino)
        for perfil, pasta in PERFIS:
            texto = rodar(
                "--bin", "sync", "--", "avaliar", LOTERIA,
                "--perfil", perfil, "--perfis", str(pasta), "--db", str(banco),
                "--registros", str(tmp / "registros"),
            )
            (destino / f"avaliar_{perfil}.txt").write_bytes(texto.encode("utf-8"))
            json_ = rodar(
                "--example", "referencia_avaliacao", "--",
                LOTERIA, perfil, str(banco), str(pasta),
            )
            (destino / f"referencia_{perfil}.json").write_bytes(json_.encode("utf-8"))


def gerar_dobras(bancos: Path) -> None:
    dobras = {}
    for loteria in LOTERIAS_REAIS:
        ref = json.loads(
            rodar(
                "--example", "referencia_avaliacao", "--",
                loteria, "padrao_v1", str(bancos / f"resultados_{loteria}.db"),
            )
        )
        dobras[loteria] = {
            "corte": ref["corte"],
            "primeiro_indice": ref["primeiro_indice"],
            "dobras": [[d["faixa"]["primeiro"], d["faixa"]["ultimo"]] for d in ref["walk_forward"]["dobras"]],
        }
    texto = json.dumps(dobras, indent=2, ensure_ascii=False) + "\n"
    (FIXTURES / "dobras_rust.json").write_bytes(texto.encode("utf-8"))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bancos", type=Path, help="pasta com CÓPIAS dos resultados_<loteria>.db reais")
    args = ap.parse_args()
    gerar_sintetica()
    if args.bancos:
        gerar_dobras(args.bancos)


if __name__ == "__main__":
    main()
