"""Fixtures dos testes da ``ml/``.

- ``FIXTURES``: dataset pequeno gerado pelo próprio ``sync dataset`` num
  banco sintético (``ml/ferramentas/gerar_fixture.py``), com as referências
  do Rust;
- :func:`escrever_dataset`: datasets sintéticos mínimos no formato do
  ``sync dataset`` (CSV ou Parquet + manifesto), para os testes de recusa.
"""

from __future__ import annotations

import hashlib
import json
import random
from pathlib import Path

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from rust_spider_ml.dataset import COLUNAS, impressao_resultados

FIXTURES = Path(__file__).resolve().parent / "fixtures"
RAIZ_REPO = Path(__file__).resolve().parents[2]

#: Tabela de cortes dos datasets sintéticos (loteria fictícia `teste`, corte
#: padrão de :func:`escrever_dataset`). Passada explicitamente a
#: ``ler_dataset(..., cortes=CORTES_TESTE)``: o padrão de produção
#: (``CORTES_BUSCA``) continua recusando a loteria `teste`. A fixture do
#: ``sync dataset`` (Lotofácil, corte 3416) é lida sempre com o padrão.
CORTES_TESTE = {"teste": 100}


def linhas_sinteticas(
    t: int = 8, n: int = 6, k: int = 2, janelas=(0, 10), semente: int = 1
) -> list[dict]:
    """Linhas (concurso, dezena, janela) com sorteios e features aleatórios
    (o Python não recalcula features; só lê)."""
    g = random.Random(semente)
    linhas = []
    for c in range(1, t + 1):
        sorteadas = set(g.sample(range(1, n + 1), k))
        for d in range(1, n + 1):
            for j in janelas:
                def opc():
                    return None if g.random() < 0.3 else g.randint(0, 9)

                linhas.append(
                    {
                        "concurso": c,
                        "data": f"2020-01-{c:02d}",
                        "dezena": d,
                        "janela": j,
                        "trecho": "busca",
                        "frequencia": g.randint(0, 9),
                        "maior_espaco": opc(),
                        "maior_espaco_ocorrencias": opc(),
                        "menor_espaco": opc(),
                        "menor_espaco_ocorrencias": opc(),
                        "soma_variacoes": opc(),
                        "espaco_final": opc(),
                        "repeticoes_total": g.randint(0, 9),
                        "repeticao_atual": g.randint(0, 3),
                        "saiu": int(d in sorteadas),
                        "feature_version": 1,
                    }
                )
    return linhas


def _csv(linhas: list[dict]) -> bytes:
    texto = ",".join(COLUNAS) + "\n"
    for l in linhas:
        texto += ",".join("" if l[c] is None else str(l[c]) for c in COLUNAS) + "\n"
    return texto.encode("utf-8")


def _parquet(linhas: list[dict], metadados: dict[str, str]) -> bytes:
    campos = [
        pa.field(c, pa.string() if c in ("data", "trecho") else pa.int32()) for c in COLUNAS
    ]
    esquema = pa.schema(campos, metadata=metadados)
    tabela = pa.Table.from_pylist(linhas, schema=esquema)
    saida = pa.BufferOutputStream()
    pq.write_table(tabela, saida, compression="none")
    return saida.getvalue().to_pybytes()


def escrever_dataset(
    pasta: Path,
    linhas: list[dict] | None = None,
    *,
    formato: str = "csv",
    loteria: str = "teste",
    corte: int = 100,
    manifesto_extra: dict | None = None,
    metadados_extra: dict | None = None,
    nome_manifesto: str | None = None,
) -> Path:
    """Grava arquivo + manifesto coerentes a partir de ``linhas``; depois
    aplica ``manifesto_extra`` (para quebrar um campo de propósito)."""
    linhas = linhas_sinteticas() if linhas is None else linhas
    pasta.mkdir(parents=True, exist_ok=True)
    versao = linhas[0]["feature_version"]
    arquivo = f"features_v{versao}.{formato}"
    metadados = {"loteria": loteria, "feature_version": str(versao), "trecho": "busca"}
    metadados.update(metadados_extra or {})
    dados = _csv(linhas) if formato == "csv" else _parquet(linhas, metadados)
    (pasta / arquivo).write_bytes(dados)

    concursos = sorted({l["concurso"] for l in linhas})
    janela0 = min(l["janela"] for l in linhas)
    datas = {l["concurso"]: l["data"] for l in linhas}
    sorteadas = {c: [] for c in concursos}
    for l in linhas:
        if l["janela"] == janela0 and l["saiu"] == 1:
            sorteadas[l["concurso"]].append(l["dezena"])
    impressao = impressao_resultados(
        loteria,
        np.array(concursos),
        tuple(datas[c] for c in concursos),
        [sorteadas[c] for c in concursos],
    )
    manifesto = {
        "loteria": loteria,
        "feature_version": versao,
        "formato": formato,
        "arquivo": arquivo,
        "colunas": list(COLUNAS),
        "primeiro_concurso": concursos[0],
        "ultimo_concurso": concursos[-1],
        "ultimo_concurso_busca": corte,
        "trecho": "busca",
        "concursos": len(concursos),
        "linhas": len(linhas),
        "sha256": hashlib.sha256(dados).hexdigest(),
        "impressao_resultados_banco": impressao,
        "impressao_resultados_incluidos": impressao,
    }
    manifesto.update(manifesto_extra or {})
    caminho = pasta / (nome_manifesto or f"manifesto_{formato}.json")
    caminho.write_text(json.dumps(manifesto, indent=2), encoding="utf-8")
    return caminho


@pytest.fixture
def fixture_lotofacil() -> Path:
    return FIXTURES / "lotofacil"
