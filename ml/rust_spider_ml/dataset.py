"""Leitura do dataset do ``sync dataset`` (1.5.1), sempre pelo manifesto.

Regras (``docs/PLANEJAMENTO.md``, 1.5.2, decisões de 2026-10-06):

- o ponto de entrada é o manifesto ``manifesto_<ext>.json``; o arquivo de
  dados é o que ele aponta, na mesma pasta;
- confere SHA-256, número de linhas, ``feature_version``, último concurso
  ≤ corte e a impressão dos resultados incluídos (recalculada a partir das
  dezenas com ``saiu`` = 1);
- recusa qualquer manifesto ou arquivo ``_trancado`` e qualquer linha com
  ``trecho != busca``. A leitura do trancado não existe neste pacote;
- Parquet e CSV produzem a mesma estrutura (:class:`Dataset`);
- K (dezenas sorteadas) e N (dezenas possíveis) saem do próprio dataset,
  sem tabela de loterias no Python.

Nada é recalculado: os componentes são o snapshot de antes de cada concurso
gravado pelo Rust (ponto no tempo garantido lá).
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import pyarrow as pa
import pyarrow.csv as pa_csv
import pyarrow.parquet as pq

COLUNAS: tuple[str, ...] = (
    "concurso",
    "data",
    "dezena",
    "janela",
    "trecho",
    "frequencia",
    "maior_espaco",
    "maior_espaco_ocorrencias",
    "menor_espaco",
    "menor_espaco_ocorrencias",
    "soma_variacoes",
    "espaco_final",
    "repeticoes_total",
    "repeticao_atual",
    "saiu",
    "feature_version",
)

#: Os 9 componentes, na ordem de ``Pesos::componentes`` do Rust (a ordem da
#: soma do índice).
COMPONENTES: tuple[str, ...] = COLUNAS[5:14]

#: Componentes que podem ser ``NULL`` (``OPTIONAL`` no Parquet).
OPCIONAIS: frozenset[str] = frozenset(
    {
        "maior_espaco",
        "maior_espaco_ocorrencias",
        "menor_espaco",
        "menor_espaco_ocorrencias",
        "soma_variacoes",
        "espaco_final",
    }
)

FORMATOS: tuple[str, ...] = ("csv", "parquet")

TRECHO_BUSCA = "busca"

_CAMPOS_MANIFESTO: dict[str, type] = {
    "loteria": str,
    "feature_version": int,
    "formato": str,
    "arquivo": str,
    "colunas": list,
    "primeiro_concurso": int,
    "ultimo_concurso": int,
    "ultimo_concurso_busca": int,
    "trecho": str,
    "concursos": int,
    "linhas": int,
    "sha256": str,
    "impressao_resultados_banco": str,
    "impressao_resultados_incluidos": str,
}


class ErroDataset(Exception):
    """Dataset ou manifesto recusado: regenere com ``sync dataset``."""


@dataclass(frozen=True)
class Dataset:
    """Dataset pivotado: um concurso por linha e uma dezena por coluna.

    - ``concursos``: ``int64[T]``, seguidos (sem buraco), todos da busca;
    - ``dezenas``: ``int64[N]``, em ordem crescente (a mesma em todo concurso);
    - ``saiu``: ``int8[T, N]`` (0/1), o rótulo do próprio concurso;
    - ``componentes[janela]``: ``float64[T, N, 9]`` na ordem de
      :data:`COMPONENTES`, com ``NaN`` no lugar de ``NULL`` (os valores são
      inteiros pequenos, exatos em ``float64``).
    """

    loteria: str
    feature_version: int
    formato: str
    corte: int
    concursos: np.ndarray
    datas: tuple[str, ...]
    dezenas: np.ndarray
    janelas: tuple[int, ...]
    saiu: np.ndarray
    componentes: dict[int, np.ndarray] = field(repr=False)
    manifesto: dict = field(repr=False)

    @property
    def n_dezenas(self) -> int:
        """N: dezenas possíveis por concurso."""
        return int(self.dezenas.size)

    @property
    def k_sorteadas(self) -> int:
        """K: dezenas sorteadas por concurso (igual em todos, conferido)."""
        return int(self.saiu[0].sum())

    @property
    def primeiro_concurso(self) -> int:
        return int(self.concursos[0])

    @property
    def ultimo_concurso(self) -> int:
        return int(self.concursos[-1])

    def componentes_da_janela(self, janela: int) -> np.ndarray:
        if janela not in self.componentes:
            raise ErroDataset(
                f"janela {janela} não está no dataset (há {list(self.janelas)})"
            )
        return self.componentes[janela]


def impressao_resultados(
    loteria: str, concursos: np.ndarray, datas: tuple[str, ...], sorteadas: list[list[int]]
) -> str:
    """Mesma impressão de ``orquestracao::impressao_resultados`` (FNV-1a 64
    das linhas ``loteria|concurso|data|d1,d2,…\\n``, dezenas em ordem)."""
    base = 0xCBF29CE484222325
    primo = 0x00000100000001B3
    mascara = (1 << 64) - 1
    h = base
    for concurso, data, dezenas in zip(concursos.tolist(), datas, sorteadas):
        lista = ",".join(str(d) for d in sorted(dezenas))
        for b in f"{loteria}|{concurso}|{data}|{lista}\n".encode("utf-8"):
            h ^= b
            h = (h * primo) & mascara
    return f"fnv1a64:{len(datas)}:{h:016x}"


def _eh_trancado(nome: str) -> bool:
    return "trancado" in nome.lower()


def ler_manifesto(caminho: Path) -> dict:
    """Lê e confere a forma do manifesto (sem abrir o arquivo de dados)."""
    caminho = Path(caminho)
    if _eh_trancado(caminho.name):
        raise ErroDataset(
            f"{caminho.name}: manifesto do conjunto TRANCADO; a busca de pesos e o "
            "baseline nunca leem o trancado"
        )
    try:
        manifesto = json.loads(caminho.read_text(encoding="utf-8"))
    except FileNotFoundError as e:
        raise ErroDataset(f"manifesto não encontrado: {caminho}") from e
    except json.JSONDecodeError as e:
        raise ErroDataset(f"{caminho.name}: JSON inválido ({e})") from e
    if not isinstance(manifesto, dict):
        raise ErroDataset(f"{caminho.name}: o manifesto não é um objeto JSON")

    faltando = [c for c in _CAMPOS_MANIFESTO if c not in manifesto]
    if faltando:
        raise ErroDataset(f"{caminho.name}: faltam os campos {faltando}")
    for campo, tipo in _CAMPOS_MANIFESTO.items():
        valor = manifesto[campo]
        if not isinstance(valor, tipo) or (tipo is int and isinstance(valor, bool)):
            raise ErroDataset(
                f"{caminho.name}: campo `{campo}` deveria ser {tipo.__name__}, veio {valor!r}"
            )

    formato = manifesto["formato"]
    if formato not in FORMATOS:
        raise ErroDataset(f"{caminho.name}: formato `{formato}` desconhecido")
    if caminho.name != f"manifesto_{formato}.json":
        raise ErroDataset(
            f"{caminho.name}: o manifesto do formato {formato} se chama manifesto_{formato}.json"
        )
    arquivo = manifesto["arquivo"]
    if _eh_trancado(arquivo):
        raise ErroDataset(f"{caminho.name}: aponta para {arquivo}, arquivo do TRANCADO")
    if Path(arquivo).name != arquivo or not arquivo.endswith(f".{formato}"):
        raise ErroDataset(
            f"{caminho.name}: `arquivo` deve ser um nome .{formato} na mesma pasta, veio {arquivo!r}"
        )
    if manifesto["trecho"] != TRECHO_BUSCA:
        raise ErroDataset(
            f"{caminho.name}: trecho `{manifesto['trecho']}`; só o trecho `busca` é lido "
            "(o trancado fica fora da busca de pesos)"
        )
    if tuple(manifesto["colunas"]) != COLUNAS:
        raise ErroDataset(
            f"{caminho.name}: colunas {manifesto['colunas']} diferentes das esperadas {list(COLUNAS)}"
        )
    if manifesto["ultimo_concurso"] > manifesto["ultimo_concurso_busca"]:
        raise ErroDataset(
            f"{caminho.name}: último concurso {manifesto['ultimo_concurso']} acima do corte da "
            f"busca {manifesto['ultimo_concurso_busca']}"
        )
    return manifesto


def _ler_tabela(bytes_: bytes, formato: str, manifesto: dict) -> pa.Table:
    if formato == "parquet":
        arquivo = pq.ParquetFile(pa.BufferReader(bytes_))
        kv = {
            k.decode("utf-8"): v.decode("utf-8")
            for k, v in (arquivo.metadata.metadata or {}).items()
        }
        esperado = {
            "loteria": manifesto["loteria"],
            "feature_version": str(manifesto["feature_version"]),
            "trecho": manifesto["trecho"],
        }
        for chave, valor in esperado.items():
            if kv.get(chave) != valor:
                raise ErroDataset(
                    f"metadado `{chave}` do Parquet = {kv.get(chave)!r}, manifesto diz {valor!r}"
                )
        return arquivo.read()
    tipos = {
        c: (pa.string() if c in ("data", "trecho") else pa.int32()) for c in COLUNAS
    }
    try:
        return pa_csv.read_csv(
            pa.BufferReader(bytes_),
            read_options=pa_csv.ReadOptions(use_threads=False),
            parse_options=pa_csv.ParseOptions(delimiter=","),
            convert_options=pa_csv.ConvertOptions(
                column_types=tipos,
                null_values=[""],
                strings_can_be_null=False,
            ),
        )
    except pa.ArrowInvalid as e:
        raise ErroDataset(f"CSV inválido: {e}") from e


def _inteiros(tabela: pa.Table, coluna: str) -> np.ndarray:
    col = tabela.column(coluna)
    if col.null_count:
        raise ErroDataset(f"coluna `{coluna}` com {col.null_count} valor(es) ausente(s)")
    return col.to_numpy().astype(np.int64)


def _opcional(tabela: pa.Table, coluna: str) -> np.ndarray:
    col = tabela.column(coluna).cast(pa.float64())
    return col.to_numpy(zero_copy_only=False)  # NULL vira NaN


def ler_dataset(caminho_manifesto: Path | str, feature_version: int | None = None) -> Dataset:
    """Lê o dataset descrito pelo manifesto, conferindo tudo antes de usar.

    ``feature_version``, se informada, tem de bater com a do manifesto.
    """
    caminho_manifesto = Path(caminho_manifesto)
    manifesto = ler_manifesto(caminho_manifesto)
    nome = caminho_manifesto.name
    versao = manifesto["feature_version"]
    if feature_version is not None and versao != feature_version:
        raise ErroDataset(
            f"{nome}: feature_version {versao}, esperada {feature_version}; regenere o dataset"
        )

    caminho_dados = caminho_manifesto.parent / manifesto["arquivo"]
    try:
        bytes_ = caminho_dados.read_bytes()
    except FileNotFoundError as e:
        raise ErroDataset(f"arquivo de dados não encontrado: {caminho_dados}") from e
    sha = hashlib.sha256(bytes_).hexdigest()
    if sha != manifesto["sha256"]:
        raise ErroDataset(
            f"{manifesto['arquivo']}: SHA-256 {sha} não bate com o manifesto ({manifesto['sha256']})"
        )

    tabela = _ler_tabela(bytes_, manifesto["formato"], manifesto)
    if tuple(tabela.column_names) != COLUNAS:
        raise ErroDataset(f"colunas do arquivo {tabela.column_names} diferentes de {list(COLUNAS)}")
    if tabela.num_rows != manifesto["linhas"]:
        raise ErroDataset(
            f"{tabela.num_rows} linha(s) no arquivo, o manifesto diz {manifesto['linhas']}"
        )
    if tabela.num_rows == 0:
        raise ErroDataset("dataset sem linhas")

    trecho = tabela.column("trecho").to_numpy(zero_copy_only=False)
    fora = np.flatnonzero(trecho != TRECHO_BUSCA)
    if fora.size:
        concurso = tabela.column("concurso")[int(fora[0])].as_py()
        raise ErroDataset(
            f"{fora.size} linha(s) com trecho diferente de `busca` (a primeira no concurso "
            f"{concurso}); o trancado não é lido"
        )

    concurso = _inteiros(tabela, "concurso")
    dezena = _inteiros(tabela, "dezena")
    janela = _inteiros(tabela, "janela")
    saiu = _inteiros(tabela, "saiu")
    versoes = _inteiros(tabela, "feature_version")
    if np.any(versoes != versao):
        raise ErroDataset(f"coluna feature_version diferente de {versao} (manifesto)")
    if np.any((saiu != 0) & (saiu != 1)):
        raise ErroDataset("coluna `saiu` com valor fora de 0/1")

    # Ordem estritamente crescente de (concurso, dezena, janela): sem linha
    # repetida, na ordem da chave de `features_dezena`.
    chave = np.stack([concurso, dezena, janela], axis=1)
    passo = chave[1:] - chave[:-1]
    crescente = (passo[:, 0] > 0) | (
        (passo[:, 0] == 0) & ((passo[:, 1] > 0) | ((passo[:, 1] == 0) & (passo[:, 2] > 0)))
    )
    if not np.all(crescente):
        i = int(np.flatnonzero(~crescente)[0])
        raise ErroDataset(
            f"linhas fora de ordem ou repetidas: {tuple(chave[i])} antes de {tuple(chave[i + 1])}"
        )

    concursos = np.unique(concurso)
    dezenas = np.unique(dezena)
    janelas = np.unique(janela)
    t, n, j = concursos.size, dezenas.size, janelas.size
    # Linhas únicas (ordem estrita) dentro da grade concursos × dezenas ×
    # janelas: com t·n·j linhas, a grade está completa.
    if tabela.num_rows != t * n * j:
        raise ErroDataset(
            f"{tabela.num_rows} linha(s), mas {t} concurso(s) × {n} dezena(s) × {j} janela(s) "
            f"= {t * n * j}: algum concurso não tem todas as dezenas e janelas"
        )
    if np.any(np.diff(concursos) != 1):
        i = int(np.flatnonzero(np.diff(concursos) != 1)[0])
        raise ErroDataset(f"buraco entre os concursos {concursos[i]} e {concursos[i + 1]}")
    if (int(concursos[0]), int(concursos[-1]), t) != (
        manifesto["primeiro_concurso"],
        manifesto["ultimo_concurso"],
        manifesto["concursos"],
    ):
        raise ErroDataset(
            f"concursos {concursos[0]}..={concursos[-1]} ({t}) no arquivo, o manifesto diz "
            f"{manifesto['primeiro_concurso']}..={manifesto['ultimo_concurso']} "
            f"({manifesto['concursos']})"
        )
    if int(concursos[-1]) > manifesto["ultimo_concurso_busca"]:
        raise ErroDataset("concurso acima do corte da busca")

    forma = (t, n, j)
    saiu_tnj = saiu.reshape(forma)
    if np.any(saiu_tnj != saiu_tnj[:, :, :1]):
        raise ErroDataset("`saiu` diferente entre as janelas de uma mesma dezena")
    saiu_tn = saiu_tnj[:, :, 0].astype(np.int8)
    por_concurso = saiu_tn.sum(axis=1)
    k = int(por_concurso[0])
    if not 1 <= k < n:
        raise ErroDataset(f"concurso {concursos[0]}: {k} dezena(s) sorteada(s) entre {n}")
    diferentes = np.flatnonzero(por_concurso != k)
    if diferentes.size:
        i = int(diferentes[0])
        raise ErroDataset(
            f"concurso {concursos[i]}: {por_concurso[i]} dezena(s) com saiu = 1, mas o "
            f"concurso {concursos[0]} tem {k} (K tem de ser o mesmo em todos)"
        )

    data_col = tabela.column("data").to_numpy(zero_copy_only=False).reshape(t, n * j)
    if np.any(data_col != data_col[:, :1]):
        raise ErroDataset("data diferente entre as linhas de um mesmo concurso")
    datas = tuple(str(d) for d in data_col[:, 0])

    # A impressão do manifesto cobre os concursos 1 … último incluído.
    if int(concursos[0]) != 1:
        raise ErroDataset(
            f"o dataset começa no concurso {concursos[0]}; a impressão dos resultados cobre "
            "desde o concurso 1"
        )
    sorteadas = [dezenas[linha == 1].tolist() for linha in saiu_tn]
    impressao = impressao_resultados(manifesto["loteria"], concursos, datas, sorteadas)
    if impressao != manifesto["impressao_resultados_incluidos"]:
        raise ErroDataset(
            f"impressão dos resultados lidos ({impressao}) não bate com a do manifesto "
            f"({manifesto['impressao_resultados_incluidos']})"
        )

    componentes = np.stack(
        [
            (_opcional(tabela, c) if c in OPCIONAIS else _inteiros(tabela, c).astype(np.float64))
            for c in COMPONENTES
        ],
        axis=-1,
    ).reshape(t, n, j, len(COMPONENTES))
    por_janela = {
        int(jj): np.ascontiguousarray(componentes[:, :, i, :]) for i, jj in enumerate(janelas)
    }

    return Dataset(
        loteria=manifesto["loteria"],
        feature_version=versao,
        formato=manifesto["formato"],
        corte=manifesto["ultimo_concurso_busca"],
        concursos=concursos.astype(np.int64),
        datas=datas,
        dezenas=dezenas.astype(np.int64),
        janelas=tuple(int(x) for x in janelas),
        saiu=saiu_tn,
        componentes=por_janela,
        manifesto=manifesto,
    )
